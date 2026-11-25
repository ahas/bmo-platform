//! A turn of a surface that carries its own turns (docs/agent-control.md, 「The interactive surface a
//! provider declares」): the program stood in the agent's pane by its command, the turn put in through
//! the surface's door, and read back until it has ended. Which door a provider's turns go through is
//! its own declaration; the lines of a provider whose surface carries none put their turns through
//! Claude Code's transcript door themselves.

use std::collections::BTreeMap;
use std::time::Duration;

use agent_host::place::ResidentCommand;
use policy::providers::{Resident, Turns};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::execution::{Identity, Row, cancelled, publish_envelope};
use super::guard::socket_refusal;
use super::record::Reply;
use super::stand::Interrupt;
use super::turn::Watch;
use super::{endpoint, inbox, rollout};
use crate::now_ms;
use crate::places::Ground;
use crate::places::home::Home;

/// The run a turn is, in the fields a door reads of it; the run as its caller handed it rides beside
/// it for a door that hands it on whole.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
  pub line: String,
  pub model: String,
  #[serde(default)]
  pub effort: String,
  #[serde(default)]
  pub tier: Option<Value>,
  pub prompt: String,
  pub cwd: String,
  #[serde(default)]
  pub env: BTreeMap<String, String>,
  #[serde(default)]
  pub resume_session_id: Option<String>,
}

/// The pace a caller sets over the door's own.
#[derive(Clone, Debug, Default)]
pub struct Clocks {
  pub poll: Option<Duration>,
}

/// One turn handed to a surface: the run it is, the agent whose pane holds the program, what the run is
/// called and what it is, the work it belongs to, and the command its program stands on as its provider
/// composed it for this run. A turn carries no clock: it lasts until it ends or is stopped.
#[derive(Clone, Debug)]
pub struct SurfaceTurn {
  pub run: Run,
  pub given: Value,
  pub home: Home,
  pub name: String,
  pub title: String,
  pub root: Option<String>,
  pub parent: Option<String>,
  pub command: ResidentCommand,
  pub clocks: Clocks,
}

impl SurfaceTurn {
  /// Who the turn's row says it is: the agent whose program it goes into, and the run's own model, effort
  /// and tier, under the work it belongs to.
  pub fn identity(&self) -> Identity {
    Identity {
      agent: Some(self.home.agent.clone()),
      root: self.root.clone(),
      parent: self.parent.clone(),
      role: None,
      model: Some(self.run.model.clone()),
      effort: (!self.run.effort.is_empty()).then(|| self.run.effort.clone()),
    }
  }
}

/// How the turn came out: an answer, or the reason there is none in the words a failed step of the
/// fallback walk is carried in, or the program's own error a failed request ended the turn with, beside its
/// kind where the program names one, the pane leaving the turn's words unsubmitted in a dialog somebody
/// opened, with why, or the turn stopped from outside.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceEnding {
  Reply(Box<Reply>),
  Error(String),
  // Written out as an `error`, every reader of the ending seeing the words alone.
  #[serde(rename = "error", serialize_with = "words_alone")]
  Errored {
    error: String,
    kind: Option<String>,
  },
  Unsubmitted(String),
  Stopped(String),
}

// The program's own error, written out as its words alone.
fn words_alone<S: serde::Serializer>(
  error: &str,
  _kind: &Option<String>,
  serializer: S,
) -> Result<S::Ok, S::Error> {
  serializer.serialize_str(error)
}

/// The keys a provider's standing program is cut short by, as it declares them.
pub fn interrupt_of(resident: &Resident) -> Interrupt {
  Interrupt::Keys(resident.interrupt.clone())
}

/// One turn of the provider's surface, through the door the provider declares. An error is a turn
/// thrown out of — a pane or program gone under it, a herdr silent past its patience, a guard's
/// refusal — rather than one that came to an ending.
pub async fn turn<W: Watch>(
  ground: &Ground,
  provider: &str,
  turn: &SurfaceTurn,
  watch: &mut W,
) -> Result<SurfaceEnding, String> {
  let resident = harness_providers::provider(provider)
    .and_then(|declared| declared.declared().resident.as_ref())
    .ok_or_else(|| format!("{provider} stands no resident program"))?;

  if let Some(why) = socket_refusal(turn.home.herdr.socket(), &ground.machine()) {
    return Err(why);
  }

  match resident.turns {
    Turns::Transcript => Err(format!(
      "{provider}'s lines put their own turns through its transcript; its surface carries none"
    )),
    Turns::Endpoint => endpoint::turn(ground, provider, resident, turn, watch).await,
    Turns::Rollout => rollout::turn(ground, provider, resident, turn, watch).await,
    Turns::Inbox => inbox::turn(ground, provider, resident, turn, watch).await,
  }
}

/// A carried turn's execution row while it runs: begun before anything is stood, closed with how the
/// turn ended — the envelope first, so a collector killed before the archive's write still leaves the
/// cause behind.
pub struct Execution {
  pub row: Row,
  /// How the turn has ended so far: a launch error until it gets as far as a pane.
  pub ending: &'static str,
}

impl Execution {
  pub async fn begin(
    ground: &Ground,
    provider: &str,
    turn: &SurfaceTurn,
    session: Option<&str>,
    command: &[String],
  ) -> Result<Self, String> {
    let row =
      Row::begin(&ground.db, provider, &turn.run.line, &turn.name, session, command, &turn.identity())
        .await
        .map_err(|why| why.detail)?;

    publish_envelope(&ground.roots.data, row.id(), None);

    Ok(Self {
      row,
      ending: "launch-error",
    })
  }

  pub async fn cancelled(&self, ground: &Ground, turn: &SurfaceTurn) -> bool {
    let names = [
      turn.name.as_str(),
      self.row.id(),
      self.row.root(),
      self.row.text("parent").unwrap_or(""),
    ];

    cancelled(&ground.db, &names).await.unwrap_or(false)
  }

  pub fn set(&mut self, key: &str, value: Value) {
    self.row.set(key, value);
  }

  pub async fn save(&self, ground: &Ground, turn: &SurfaceTurn) {
    if let Err(why) = self.row.save(&ground.db).await {
      eprintln!("{}: the execution row ({}): {why}", turn.name, self.row.text("state").unwrap_or(""));
    }
  }

  /// The row closed with how the turn ended; a turn thrown out of once it had a pane was cancelled.
  pub async fn close(&mut self, ground: &Ground, turn: &SurfaceTurn, thrown: bool) {
    if thrown && self.ending == "launch-error" && !self.row.body.get("paneId").is_none_or(Value::is_null) {
      self.ending = "cancelled";
    }

    publish_envelope(&ground.roots.data, self.row.id(), Some(self.ending));
    self.row.set("state", json!(self.ending));
    self.row.set("endedAt", json!(now_ms()));
    self.save(ground, turn).await;
  }
}
