//! A turn's looks at its pane through herdr (docs/agent-control.md, 「Reading is done only over the
//! socket」, 「The order for putting in one turn and waiting for it to end」): what the agent in the pane
//! is doing, what its screen shows, and the keys and words put into it. A pane is gone only on herdr's
//! own word, look after look; a herdr that answers neither question is its own answer, never a pane
//! gone.

use std::time::Duration;

use herdr::{AGENT_BLOCKED, Fault, Herdr};
use policy::resident_turn::RESIDENT_TURN;
use serde_json::{Value, json};

/// What one look at a pane read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Look {
  /// What herdr says the agent in the pane is doing: `working`, `idle`, `done`, `blocked`, or
  /// `unknown` for a pane with no agent detected in it yet.
  Status(String),
  /// herdr said, look after look, that the pane is not there.
  Gone,
  /// herdr answered none of the look, and why its call said it did not.
  Silent(String),
}

impl Look {
  pub fn is(&self, status: &str) -> bool {
    matches!(self, Self::Status(now) if now == status)
  }

  /// At its input box: idle when it has just come up, done once it has finished a turn.
  pub fn at_rest(&self) -> bool {
    self.is("idle") || self.is("done")
  }

  /// The word a log line names the state by.
  pub fn word(&self) -> &str {
    match self {
      Self::Status(now) => now,
      Self::Gone => "gone",
      Self::Silent(_) => "herdr-silent",
    }
  }
}

/// How a pane found missing is confirmed gone: looks in a row, each herdr answering without it.
#[derive(Clone, Copy, Debug)]
pub struct GoneConfirm {
  pub looks: u32,
  pub gap: Duration,
}

impl Default for GoneConfirm {
  fn default() -> Self {
    Self {
      looks: RESIDENT_TURN.gone_looks,
      gap: Duration::from_millis(RESIDENT_TURN.gone_gap_ms),
    }
  }
}

/// Whether a refusal is the one that is not a fault: an agent waiting on a person refused the words with
/// its pane left as it was.
pub fn is_blocked(fault: &Fault) -> bool {
  fault.code == AGENT_BLOCKED
}

// One of a look's list calls, waited on until herdr answers it: a stall under load is waited out. A
// refusal or a broken connection is herdr's answer as it stands.
async fn list_call(herdr: &Herdr, method: &str) -> Result<Value, Fault> {
  herdr.call(method, json!({}), None).await
}

/// Whether herdr lists a pane, or none where it did not answer.
pub async fn pane_listed(herdr: &Herdr, pane: &str) -> Option<bool> {
  let answer = list_call(herdr, "pane.list").await.ok()?;

  Some(
    answer.get("panes").and_then(Value::as_array).is_some_and(|panes| {
      panes.iter().any(|one| one.get("pane_id").and_then(Value::as_str) == Some(pane))
    }),
  )
}

async fn look_at(herdr: &Herdr, pane: &str) -> Look {
  let answer = match list_call(herdr, "agent.list").await {
    Ok(answer) => answer,
    Err(fault) => return Look::Silent(format!("agent.list: {}: {}", fault.code, fault.message)),
  };
  let found = answer.get("agents").and_then(Value::as_array).and_then(|agents| {
    agents.iter().find(|agent| agent.get("pane_id").and_then(Value::as_str) == Some(pane))
  });

  if let Some(found) = found {
    return Look::Status(found.get("agent_status").and_then(Value::as_str).unwrap_or("unknown").to_owned());
  }

  // A pane with no agent detected in it yet is not a pane that has gone: the program is still drawing
  // itself. A pane herdr no longer lists at all is.
  match pane_listed(herdr, pane).await {
    None => Look::Silent("pane.list did not answer".to_owned()),
    Some(true) => Look::Status("unknown".to_owned()),
    Some(false) => Look::Gone,
  }
}

/// Whether herdr reads the agent in a pane working now, one look: a word typed into it then joins its turn
/// rather than opening one of its own.
pub async fn working(herdr: &Herdr, pane: &str) -> bool {
  look_at(herdr, pane).await.is("working")
}

/// What the agent in a pane is doing; gone only where herdr has said so look after look.
pub async fn status_of(herdr: &Herdr, pane: &str, confirm: GoneConfirm) -> Look {
  let mut now = look_at(herdr, pane).await;
  let mut look = 1;

  while now == Look::Gone && look < confirm.looks {
    tokio::time::sleep(confirm.gap).await;
    now = look_at(herdr, pane).await;
    look += 1;
  }

  now
}

/// What a person looking at the pane would read: always the read, never the scroll, since the program
/// draws on the alternate screen.
pub async fn screen_of(herdr: &Herdr, pane: &str) -> String {
  herdr.read_pane(pane, herdr::ReadSource::Visible, None).await.unwrap_or_default()
}

/// Keys pressed in a pane, as a person at its keyboard would.
pub async fn send_keys(herdr: &Herdr, pane: &str, keys: &[&str]) -> Result<Value, Fault> {
  herdr.call("pane.send_keys", json!({ "pane_id": pane, "keys": keys }), None).await
}

/// Words put into a Claude Code program as one message: the lead typed, then the words pasted after it
/// and submitted with it. The program records what arrives by paste as pasted material and acts on an
/// instruction inside it only where the person's own words ask it to, and typed text is cut once it is
/// long, so the words go in pasted behind a typed line that vouches for them. A round's lead is nothing,
/// and its head line alone is typed before the paste.
pub async fn prompt_with_lead(herdr: &Herdr, pane: &str, lead: &str, text: &str) -> Result<Value, Fault> {
  if !lead.is_empty() {
    herdr
      .call("pane.send_text", json!({ "pane_id": pane, "text": format!("{lead} ") }), None)
      .await?;
  }

  herdr.call("agent.prompt", json!({ "target": pane, "text": text }), None).await
}

/// A command of the program's own put in as the whole of what it is told.
pub async fn prompt(herdr: &Herdr, pane: &str, text: &str) -> Result<Value, Fault> {
  herdr.call("agent.prompt", json!({ "target": pane, "text": text }), None).await
}

const CURSOR_MARK: &str = "❯";

// A choice the program puts on its screen whose answer is settled before it is asked, read by the words
// its rows show: the heading that names it, where its own rows do not, a row it offers, and the row to
// choose. The keys move the cursor the program marks down to the row to choose and confirm it, counted off
// the screen.
struct SettledChoice {
  heading: Option<&'static str>,
  offered: &'static str,
  chosen: &'static str,
}

impl SettledChoice {
  fn keys(&self, screen: &str) -> Option<Vec<&'static str>> {
    let rows: Vec<&str> = screen.split('\n').collect();

    if let Some(heading) = self.heading
      && !rows.iter().any(|row| row.contains(heading))
    {
      return None;
    }

    let offered = rows.iter().position(|row| row.contains(self.offered))?;
    let chosen = rows.iter().position(|row| row.contains(self.chosen))?;
    let first = offered.min(chosen);
    let cursor = (first..=chosen).find(|at| rows[*at].trim_start().starts_with(CURSOR_MARK))?;
    let mut keys = vec!["down"; chosen - cursor];

    keys.push("enter");

    Some(keys)
  }
}

// The folder-trust question: the cursor stands on refusing, which ends the program.
const FOLDER_TRUST: SettledChoice = SettledChoice {
  heading: None,
  offered: "No, exit",
  chosen: "Yes, I trust this folder",
};

// The program's offer of a code-intelligence plugin: declined for every plugin, so it is never offered
// again (docs/agent-control.md, 「A turn's end stops at a plugin recommendation」).
const PLUGIN_OFFER: SettledChoice = SettledChoice {
  heading: Some("LSP plugin recommendation"),
  offered: "Yes, install",
  chosen: "Disable all LSP recommendations",
};

/// The keys that accept the folder-trust question a program asks the first time it comes up in a
/// directory nobody has accepted, read off the screen — or none, where the screen is not that question.
pub fn folder_trust_keys(screen: &str) -> Option<Vec<&'static str>> {
  FOLDER_TRUST.keys(screen)
}

/// The keys that decline the program's plugin offer for good, read off the screen — or none, where the
/// screen is not that offer.
pub fn plugin_offer_keys(screen: &str) -> Option<Vec<&'static str>> {
  PLUGIN_OFFER.keys(screen)
}
