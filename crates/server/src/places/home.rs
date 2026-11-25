//! An agent's home in herdr (docs/agent-control.md, 「One standing pane per agent」, 「What goes where」):
//! its workspace under the label the owner has herdr show it by, whose head pane keeps the workspace
//! standing and stands the agent's program or follows its view. This is how a home is found, made,
//! renamed, tidied and closed, the twin of the TypeScript side's own (`packages/agent`'s herdr run).
//!
//! A home is found by the token it was reported under first, and by its label after: the token is live
//! state a herdr that comes back does not rebuild, while the label is. Of two homes for one agent the
//! lowest-numbered comes first, which is the order every reader of the same picture agrees on.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use herdr::{Fault, Herdr, Pane, Snapshot};
use policy::place_words::PLACE_WORDS;
use serde_json::{Value, json};

/// The token a standing agent's workspace is reported under, and the one naming the data root of a
/// workspace that is not the machine's own.
pub const AGENT_TOKEN: &str = "agent";
pub const DATA_TOKEN: &str = "data";

/// The source every token this system reports is reported under.
pub const METADATA_SOURCE: &str = "bmo";

/// The mark a head pane's loop runs under, `bmo-head-<agent>`, whichever head it starts.
pub const HEAD_MARK: &str = "bmo-head-";

/// The head program a line on the hosts runs, as the deploy tree's `bin/` holds it.
pub const PANE_HEAD: &str = "/bin/pane-head";

/// The one exit a head makes that its pane is not to start it again on.
pub const HEAD_STOPPED: i32 = 3;

/// The fault a home's workspace found gone before its head was looked for answers.
const WORKSPACE_GONE: &str = "workspace_not_found";

/// The verb a worker's dev-server pane runs, a word of its command line after the verbs' binary.
pub const DEV_VERB: &str = "dev-pane";

/// The tab a worker's dev server stands in.
pub fn dev_tab_label() -> &'static str {
  &PLACE_WORDS.dev_tab
}

/// The mark a standing pane carries on its command line, as the name its shell runs under.
pub const STANDING_MARK: &str = "bmo-standing-";

/// A command line run as a standing pane, under the mark and a name of its owner's choosing.
pub fn standing_command(name: &str, command: &[String]) -> Vec<String> {
  let mut line: Vec<String> = ["/bin/sh", "-c", r#"exec "$@""#].map(str::to_owned).into();

  line.push(format!("{STANDING_MARK}{name}"));
  line.extend(command.iter().cloned());
  line
}

/// Whether a pane's command line is an agent's head loop.
pub fn is_head(command: &[String]) -> bool {
  command.iter().any(|part| part.starts_with(HEAD_MARK))
}

/// The agent a head loop stands for, read off its mark.
pub fn head_agent(command: &[String]) -> Option<&str> {
  command.iter().find_map(|part| part.strip_prefix(HEAD_MARK))
}

/// Whether a head loop starts `pane-head` rather than another head.
pub fn runs_pane_head(command: &[String]) -> bool {
  command.iter().any(|part| part.ends_with(PANE_HEAD))
}

/// Whose dev server a command line is: the argument after the verb.
pub fn dev_worker(command: &[String]) -> Option<&str> {
  let at = command.iter().position(|part| part == DEV_VERB)?;

  command.get(at + 1).map(String::as_str)
}

/// Whether a pane's command line is a standing one of any kind: a head, a dev server, a standing pane.
pub fn keeps_standing(command: &[String]) -> bool {
  is_head(command) || command.iter().any(|part| part == DEV_VERB || part.starts_with(STANDING_MARK))
}

/// A title kept to what a sidebar and a pane border can show: its first line, flattened.
pub fn short_title(text: &str, most: usize) -> String {
  let line = text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or_default();
  let flat = line.split_whitespace().collect::<Vec<_>>().join(" ");

  if flat.chars().count() <= most {
    return flat;
  }

  format!("{}…", flat.chars().take(most - 1).collect::<String>().trim_end())
}

/// What a pane is started with.
#[derive(Clone, Debug, PartialEq)]
pub struct Launch {
  pub command: Vec<String>,
  pub cwd: String,
  pub env: BTreeMap<String, String>,
}

impl Launch {
  fn node(&self, label: &str) -> Value {
    json!({ "type": "pane", "label": label, "command": self.command, "cwd": self.cwd, "env": self.env })
  }

  /// Whether a pane stands in the folder this launch starts it in, as it is written or by its real path;
  /// a pane herdr reads no folder of stands where it stands.
  pub fn stands_in(&self, pane: &Pane) -> bool {
    let Some(cwd) = pane.cwd.as_deref() else { return true };
    let real = std::fs::canonicalize(&self.cwd).ok();

    cwd == self.cwd || real.is_some_and(|real| Path::new(cwd) == real)
  }
}

/// An agent's home: which agent, the label its workspace shows, the label it goes back to once the run
/// its label names ends, the data root it is of where that is not the machine's own, its head, and the
/// herdr it stands in.
#[derive(Clone, Debug)]
pub struct Home {
  pub agent: String,
  pub label: String,
  pub resting: Option<String>,
  pub data: Option<String>,
  pub head: Launch,
  pub herdr: Herdr,
}

/// A standing home: its workspace and its head pane.
#[derive(Clone, Debug, PartialEq)]
pub struct Standing {
  pub workspace: String,
  pub head_pane: String,
}

fn unreadable(message: &str) -> Fault {
  Fault {
    code: "unreadable".into(),
    message: message.into(),
  }
}

fn text_at<'a>(answer: &'a Value, pointer: &str) -> Option<&'a str> {
  answer.pointer(pointer).and_then(Value::as_str)
}

/// The agents herdr's workspaces stand for, read off each workspace's agent token, and, for a workspace
/// a restarted herdr revived without its tokens, off the marks of the head loops in it.
pub async fn workspace_agents(herdr: &Herdr) -> Result<Vec<String>, Fault> {
  let seen = herdr.snapshot().await?;
  let mut names = vec![];

  for workspace in &seen.workspaces {
    if let Some(token) = workspace.token(AGENT_TOKEN) {
      names.push(token.to_owned());
      continue;
    }

    for pane in seen.panes.iter().filter(|pane| pane.workspace_id == workspace.workspace_id) {
      let Some(pane_id) = pane.pane_id.as_deref().filter(|_| pane.run().is_none()) else {
        continue;
      };

      if let Some(agent) = head_agent(&herdr.command_of(pane_id).await) {
        names.push(agent.to_owned());
      }
    }
  }

  let mut seen_names = HashSet::new();

  names.retain(|name| seen_names.insert(name.clone()));

  Ok(names)
}

impl Home {
  fn tokens(&self) -> Value {
    match &self.data {
      None => json!({ AGENT_TOKEN: self.agent }),
      Some(data) => json!({ AGENT_TOKEN: self.agent, DATA_TOKEN: data }),
    }
  }

  fn marks(&self, workspace: &herdr::Workspace) -> bool {
    workspace.token(AGENT_TOKEN) == Some(self.agent.as_str())
      && workspace.token(DATA_TOKEN) == self.data.as_deref()
  }

  /// The workspaces that are this agent's, the one to use first: one carrying its token, or one
  /// carrying no agent's token under its label or the label it rests on.
  pub fn candidates(&self, snapshot: &Snapshot) -> Vec<String> {
    let mut found: Vec<&herdr::Workspace> = snapshot
      .workspaces
      .iter()
      .filter(|workspace| {
        self.marks(workspace)
          || (workspace.token(AGENT_TOKEN).is_none()
            && (workspace.label.as_deref() == Some(self.label.as_str())
              || (self.resting.is_some() && workspace.label == self.resting)))
      })
      .collect();

    found.sort_by_key(|workspace| workspace.number.unwrap_or(0));
    found.into_iter().filter_map(|workspace| workspace.workspace_id.clone()).collect()
  }

  async fn call(&self, method: &str, params: Value) -> Result<Value, Fault> {
    self.herdr.call(method, params, None).await
  }

  async fn close_panes(&self, panes: &[String]) {
    for pane in panes {
      let _ = self.call("pane.close", json!({ "pane_id": pane })).await;
    }
  }

  /// The workspace given back the label it rests on, with the head pane's tab and the pane itself, once
  /// the run its label named has ended; nothing where it rests on the label it shows.
  pub async fn rest(&self, head_pane: &str) {
    let Some(resting) = self.resting.as_deref().filter(|resting| *resting != self.label) else {
      return;
    };
    let Ok(seen) = self.herdr.snapshot().await else { return };
    let Some(workspace_id) = self.candidates(&seen).into_iter().next() else { return };
    let Some(workspace) =
      seen.workspaces.iter().find(|one| one.workspace_id.as_deref() == Some(&workspace_id))
    else {
      return;
    };

    if workspace.label.as_deref() == Some(resting) {
      return;
    }

    let _ = self
      .call("workspace.rename", json!({ "workspace_id": workspace_id, "label": resting }))
      .await;

    let head = seen.panes.iter().find(|pane| {
      pane.pane_id.as_deref() == Some(head_pane) && pane.workspace_id.as_deref() == Some(&workspace_id)
    });

    if let Some(tab) = head.and_then(|pane| pane.tab_id.as_deref()) {
      let _ = self.call("tab.rename", json!({ "tab_id": tab, "label": resting })).await;
    }

    if head.is_some() {
      let _ = self.call("pane.rename", json!({ "pane_id": head_pane, "label": resting })).await;
    }
  }

  /// The home found or made, with its head running.
  ///
  /// Two passes can both find none and both make one. Each reads the picture again after making its
  /// own, and where the agent now has more than one each takes the lowest-numbered and closes its own
  /// if that is not it, so both arrive at the same one.
  ///
  /// A workspace found that is gone by the time its head is looked for — its one pane's loop ended
  /// between the two readings — is looked for again, and made where none stands, once.
  pub async fn stand(&self) -> Result<Standing, Fault> {
    match self.found_or_made().await {
      Err(fault) if fault.code == WORKSPACE_GONE => self.found_or_made().await,
      stood => stood,
    }
  }

  async fn found_or_made(&self) -> Result<Standing, Fault> {
    let seen = self.herdr.snapshot().await?;
    let workspace = match self.candidates(&seen).into_iter().next() {
      Some(found) => found,
      None => self.make().await?,
    };

    self.headed(&workspace).await
  }

  async fn make(&self) -> Result<String, Fault> {
    let made = self
      .call(
        "workspace.create",
        json!({ "label": self.label, "cwd": self.head.cwd, "env": self.head.env }),
      )
      .await?;
    let (Some(workspace), Some(tab)) =
      (text_at(&made, "/workspace/workspace_id"), text_at(&made, "/tab/tab_id"))
    else {
      return Err(unreadable("workspace.create named no workspace and tab"));
    };
    let workspace = workspace.to_owned();
    let applied = self
      .call(
        "layout.apply",
        json!({ "tab_id": tab, "tab_label": self.label, "root": self.head.node(&self.label) }),
      )
      .await;

    if let Err(fault) = applied {
      let _ = self.call("workspace.close", json!({ "workspace_id": workspace })).await;

      return Err(fault);
    }

    if let Err(fault) = self
      .call(
        "workspace.report_metadata",
        json!({ "workspace_id": workspace, "source": METADATA_SOURCE, "tokens": self.tokens() }),
      )
      .await
    {
      eprintln!("{}: the workspace {workspace} took no token: {fault}", self.label);
    }

    let settled = match self.herdr.snapshot().await {
      Ok(again) => self.candidates(&again).into_iter().next().unwrap_or_else(|| workspace.clone()),
      Err(_) => workspace.clone(),
    };

    if settled != workspace {
      let _ = self.call("workspace.close", json!({ "workspace_id": workspace })).await;
    }

    Ok(settled)
  }

  // The home's workspace given its label and its token where it lacks them, its head started where
  // none runs, and every pane that carries no run and runs nothing of its own closed.
  async fn headed(&self, workspace_id: &str) -> Result<Standing, Fault> {
    let seen = self.herdr.snapshot().await?;
    let Some(workspace) =
      seen.workspaces.iter().find(|one| one.workspace_id.as_deref() == Some(workspace_id))
    else {
      return Err(Fault {
        code: WORKSPACE_GONE.into(),
        message: format!("{}'s workspace went away", self.label),
      });
    };
    let renamed = workspace.label.as_deref() != Some(self.label.as_str());

    if renamed {
      let _ = self
        .call("workspace.rename", json!({ "workspace_id": workspace_id, "label": self.label }))
        .await;
    }

    if !self.marks(workspace)
      && let Err(fault) = self
        .call(
          "workspace.report_metadata",
          json!({ "workspace_id": workspace_id, "source": METADATA_SOURCE, "tokens": self.tokens() }),
        )
        .await
    {
      eprintln!("{}: the workspace {workspace_id} took no token: {fault}", self.label);
    }

    let mut head: Option<(String, String)> = None;
    let mut leftovers = vec![];

    for pane in seen.panes.iter().filter(|pane| pane.workspace_id.as_deref() == Some(workspace_id)) {
      let Some(pane_id) = pane.pane_id.as_deref().filter(|_| pane.run().is_none()) else {
        continue;
      };
      let command = self.herdr.command_of(pane_id).await;

      if is_head(&command) {
        head.get_or_insert_with(|| (pane_id.to_owned(), pane.tab_id.clone().unwrap_or_default()));
      } else if !keeps_standing(&command) {
        leftovers.push(pane_id.to_owned());
      }
    }

    let Some((head_pane, head_tab)) = head else {
      let applied = self
        .call(
          "layout.apply",
          json!({ "workspace_id": workspace_id, "tab_label": self.label, "root": self.head.node(&self.label) }),
        )
        .await?;
      let Some(head_pane) = text_at(&applied, "/layout/root/pane_id") else {
        return Err(unreadable("layout.apply named no head pane"));
      };

      self.close_panes(&leftovers).await;

      return Ok(Standing {
        workspace: workspace_id.to_owned(),
        head_pane: head_pane.to_owned(),
      });
    };

    self.close_panes(&leftovers).await;

    if renamed {
      let _ = self.call("tab.rename", json!({ "tab_id": head_tab, "label": self.label })).await;
      let _ = self.call("pane.rename", json!({ "pane_id": head_pane, "label": self.label })).await;
    }

    Ok(Standing {
      workspace: workspace_id.to_owned(),
      head_pane,
    })
  }

  /// The home closed with everything standing in it, answering how many workspaces went.
  pub async fn close(&self) -> Result<usize, Fault> {
    let seen = self.herdr.snapshot().await?;
    let mut closed = 0;

    for workspace in self.candidates(&seen) {
      if self.call("workspace.close", json!({ "workspace_id": workspace })).await.is_ok() {
        closed += 1;
      }
    }

    Ok(closed)
  }

  /// The home's head panes that run another head than its own: a head loop that starts no `pane-head`.
  pub async fn heads_of_other_kind(&self) -> Result<Vec<String>, Fault> {
    let seen = self.herdr.snapshot().await?;
    let Some(workspace) = self.candidates(&seen).into_iter().next() else {
      return Ok(vec![]);
    };
    let wanted = runs_pane_head(&self.head.command);
    let mut other = vec![];

    for pane in seen
      .panes
      .iter()
      .filter(|pane| pane.workspace_id.as_deref() == Some(workspace.as_str()))
    {
      let Some(pane_id) = pane.pane_id.as_deref().filter(|_| pane.run().is_none()) else {
        continue;
      };
      let command = self.herdr.command_of(pane_id).await;

      // A head started in another folder than its launch names — a release an earlier deployment
      // started it in — is of the other kind too, so the release goes once nothing runs on the head.
      if is_head(&command) && (runs_pane_head(&command) != wanted || !self.head.stands_in(pane)) {
        other.push(pane_id.to_owned());
      }
    }

    Ok(other)
  }

  /// The leftovers of the home closed, nothing made and nothing renamed: every pane with no run in it
  /// and no command of its own. A home that is not there has none.
  pub async fn close_leftovers(&self) -> Result<usize, Fault> {
    let seen = self.herdr.snapshot().await?;
    let Some(workspace) = self.candidates(&seen).into_iter().next() else { return Ok(0) };
    let mut leftovers = vec![];

    for pane in seen
      .panes
      .iter()
      .filter(|pane| pane.workspace_id.as_deref() == Some(workspace.as_str()))
    {
      let Some(pane_id) = pane.pane_id.as_deref().filter(|_| pane.run().is_none()) else {
        continue;
      };

      if !keeps_standing(&self.herdr.command_of(pane_id).await) {
        leftovers.push(pane_id.to_owned());
      }
    }

    self.close_panes(&leftovers).await;

    Ok(leftovers.len())
  }

  /// One owner's standing tabs in the home brought to what it wants: a pane the owner claims stays
  /// where it runs a line wanted and is closed where it does not, and a wanted line no pane runs is
  /// opened in a tab of its own. Of two panes running one line the first the snapshot lists stays.
  /// Answers how many were opened and closed. Nothing is focused.
  pub async fn stand_tabs(
    &self,
    wanted: &[(String, Launch)],
    claims: impl Fn(&[String]) -> bool,
  ) -> Result<(usize, usize), Fault> {
    let standing = self.stand().await?;
    let seen = self.herdr.snapshot().await?;
    let mut kept = HashSet::new();
    let mut closing = vec![];

    for pane in seen
      .panes
      .iter()
      .filter(|pane| pane.workspace_id.as_deref() == Some(standing.workspace.as_str()))
    {
      let Some(pane_id) = pane.pane_id.as_deref().filter(|_| pane.run().is_none()) else {
        continue;
      };
      let command = self.herdr.command_of(pane_id).await;

      if !claims(&command) {
        continue;
      }

      match wanted
        .iter()
        .enumerate()
        .position(|(at, (_, tab))| !kept.contains(&at) && tab.command == command && tab.stands_in(pane))
      {
        Some(at) => {
          kept.insert(at);
        }
        None => closing.push(pane_id.to_owned()),
      }
    }

    self.close_panes(&closing).await;

    let mut opened = 0;

    for (at, (label, tab)) in wanted.iter().enumerate() {
      if kept.contains(&at) {
        continue;
      }

      let label = short_title(label, 48);

      self
        .call(
          "layout.apply",
          json!({ "workspace_id": standing.workspace, "tab_label": label, "root": tab.node(&label) }),
        )
        .await?;
      opened += 1;
    }

    Ok((opened, closing.len()))
  }

  /// The worker's dev server brought to what is wanted: one pane running the line given, in a tab of
  /// its own in the home, or none at all. A pane running another line for this worker — a script of an
  /// older version most of all — is closed and the wanted one started; of two running it the first
  /// stays. Answers the pane and whether it was started now. The caller holds the worker's dev-pane
  /// lock, so nothing else closes panes here meanwhile.
  pub async fn stand_dev_pane(&self, dev: Option<&Launch>) -> Result<(Option<String>, bool), Fault> {
    let Some(dev) = dev else {
      let seen = self.herdr.snapshot().await?;
      let places: HashSet<String> = self.candidates(&seen).into_iter().collect();

      for pane in &seen.panes {
        let Some(pane_id) = pane.pane_id.as_deref().filter(|_| pane.run().is_none()) else {
          continue;
        };

        if places.contains(pane.workspace_id.as_deref().unwrap_or_default())
          && dev_worker(&self.herdr.command_of(pane_id).await) == Some(self.agent.as_str())
        {
          let _ = self.call("pane.close", json!({ "pane_id": pane_id })).await;
        }
      }

      return Ok((None, false));
    };

    let standing = self.stand().await?;
    let seen = self.herdr.snapshot().await?;
    let mut kept = None;

    for pane in seen
      .panes
      .iter()
      .filter(|pane| pane.workspace_id.as_deref() == Some(standing.workspace.as_str()))
    {
      let Some(pane_id) = pane.pane_id.as_deref().filter(|_| pane.run().is_none()) else {
        continue;
      };
      let command = self.herdr.command_of(pane_id).await;

      if dev_worker(&command) != Some(self.agent.as_str()) {
        continue;
      }

      if kept.is_none() && command == dev.command {
        kept = Some(pane_id.to_owned());
      } else {
        let _ = self.call("pane.close", json!({ "pane_id": pane_id })).await;
      }
    }

    if kept.is_some() {
      return Ok((kept, false));
    }

    let applied = self
      .call(
        "layout.apply",
        json!({ "workspace_id": standing.workspace, "tab_label": dev_tab_label(), "root": dev.node(dev_tab_label()) }),
      )
      .await?;

    match text_at(&applied, "/layout/root/pane_id") {
      Some(pane) => Ok((Some(pane.to_owned()), true)),
      None => Err(unreadable("layout.apply named no dev-server pane")),
    }
  }
}
