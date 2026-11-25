//! What herdr is holding right now, kept in memory so asking is answered without asking
//! (docs/agent-control.md, 「Asking whether it is alive」). The process attaches once, in the order that
//! loses nothing — the subscription acknowledged before the snapshot is asked for, the events that
//! arrive meanwhile held and applied after it — and then keeps a picture of which runs herdr holds by
//! applying the events as they arrive.
//!
//! A run is the token on its pane, so the picture is panes carrying a run's name, with the tab and the
//! workspace each stands in. A pane closed or ended is said about the pane; a tab closed takes its
//! panes without a word about each; a workspace closed deliberately says so. A pane moved into another
//! tab arrives after the tab it left has been announced closed, carrying itself whole.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

use crate::{Herdr, Pane, Snapshot};

/// What the picture needs told to it; the subscriptions are dotted and the events that come back
/// underscored.
pub const WATCHED_EVENTS: [&str; 7] = [
  "workspace.closed",
  "tab.closed",
  "pane.created",
  "pane.updated",
  "pane.moved",
  "pane.closed",
  "pane.exited",
];

// How long before a dropped subscription is opened again.
const RETRY_AFTER: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
struct Held {
  run: String,
  workspace: String,
  tab: String,
}

/// The runs herdr holds, by the pane each stands in.
#[derive(Debug, Default)]
pub struct Picture {
  held: HashMap<String, Held>,
  // Tabs and panes known gone: herdr does not deliver its events in the order it did what they
  // describe, so a word about a pane can arrive after it or its tab was announced gone.
  closed_tabs: HashSet<String>,
  closed_panes: HashSet<String>,
}

fn text(value: Option<&Value>) -> Option<&str> {
  value?.as_str().filter(|text| !text.is_empty())
}

impl Picture {
  /// Everything replaced with what herdr says it has.
  pub fn install(&mut self, snapshot: &Snapshot) {
    self.held.clear();
    self.closed_tabs.clear();
    self.closed_panes.clear();

    for pane in &snapshot.panes {
      self.hold_pane(pane);
    }
  }

  fn hold_pane(&mut self, pane: &Pane) {
    let (Some(pane_id), Some(workspace), Some(run)) =
      (pane.pane_id.as_deref(), pane.workspace_id.as_deref(), pane.run())
    else {
      return;
    };
    let tab = pane.tab_id.clone().unwrap_or_default();

    if self.closed_panes.contains(pane_id) || self.closed_tabs.contains(&tab) {
      return;
    }

    self.held.insert(
      pane_id.to_owned(),
      Held {
        run: run.to_owned(),
        workspace: workspace.to_owned(),
        tab,
      },
    );
  }

  fn hold(&mut self, value: Option<&Value>) {
    if let Some(pane) = value.and_then(|value| serde_json::from_value::<Pane>(value.clone()).ok()) {
      self.hold_pane(&pane);
    }
  }

  fn drop_pane(&mut self, pane: &str) {
    self.closed_panes.insert(pane.to_owned());
    self.held.remove(pane);
  }

  /// One event applied.
  pub fn apply(&mut self, event: &Value) {
    let data = event.get("data").cloned().unwrap_or(Value::Null);

    match event.get("event").and_then(Value::as_str) {
      Some("pane_created" | "pane_updated") => self.hold(data.get("pane")),
      // Moved whole, under an id that may be new: the old id let go first.
      Some("pane_moved") => {
        let previous = text(data.get("previous_pane_id"));
        let moved = text(data.pointer("/pane/pane_id"));

        if let Some(previous) = previous.filter(|previous| Some(*previous) != moved) {
          self.drop_pane(previous);
        }

        self.hold(data.get("pane"));
      }
      Some("pane_closed" | "pane_exited") => {
        if let Some(pane) = text(data.get("pane_id")) {
          self.drop_pane(pane);
        }
      }
      Some("tab_closed") => {
        if let Some(tab) = text(data.get("tab_id")) {
          self.closed_tabs.insert(tab.to_owned());
          self.held.retain(|_, held| held.tab != tab);
        }
      }
      Some("workspace_closed") => {
        if let Some(workspace) =
          text(data.get("workspace_id")).or_else(|| text(data.pointer("/workspace/workspace_id")))
        {
          self.held.retain(|_, held| held.workspace != workspace);
        }
      }
      _ => {}
    }
  }

  /// The runs held.
  pub fn names(&self) -> HashSet<String> {
    self.held.values().map(|held| held.run.clone()).collect()
  }
}

/// Which of a snapshot's panes carry work, asked of the panes themselves: a pane a revived herdr
/// rebuilt has its label back and nothing in it, so a pane is held only where herdr also says what it
/// was started with.
pub async fn carrying_work(herdr: &Herdr, snapshot: Snapshot) -> Snapshot {
  let mut working = vec![];

  for pane in snapshot.panes {
    let Some(pane_id) = pane.pane_id.as_deref().filter(|_| pane.run().is_some()) else {
      continue;
    };

    if !herdr.command_of(pane_id).await.is_empty() {
      working.push(pane);
    }
  }

  Snapshot {
    panes: working,
    ..snapshot
  }
}

#[derive(Default)]
struct State {
  picture: Picture,
  // Whether the subscription stands dropped; set until the first attachment.
  dropped: bool,
  // Whether the socket refused the last attempt to attach again: no herdr is there, and the panes it
  // held went with it.
  refused: bool,
}

/// One herdr watched for as long as the process stands.
#[derive(Clone)]
pub struct Watch {
  state: Arc<Mutex<State>>,
  // How many times the subscription has stood, counted as each attachment installs its snapshot.
  attached: Arc<tokio::sync::watch::Sender<u64>>,
}

impl Watch {
  /// The watch attached, and attached again after every drop, on a task of its own.
  pub fn start(herdr: Herdr) -> Self {
    let watch = Self {
      state: Arc::new(Mutex::new(State {
        picture: Picture::default(),
        dropped: true,
        refused: false,
      })),
      attached: Arc::new(tokio::sync::watch::channel(0).0),
    };
    let held = watch.clone();

    tokio::spawn(async move {
      let mut complained = false;

      loop {
        if let Err(why) = held.attach(&herdr).await
          && !complained
        {
          complained = true;
          eprintln!("herdr watch: {why}");
        }

        held.lost();
        tokio::time::sleep(RETRY_AFTER).await;
      }
    });

    watch
  }

  fn state(&self) -> std::sync::MutexGuard<'_, State> {
    self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
  }

  fn lost(&self) {
    self.state().dropped = true;
  }

  /// Whether the subscription stands.
  pub fn attached(&self) -> bool {
    !self.state().dropped
  }

  /// Each attachment as it comes — the first, and every one after a drop — counted from the first, so
  /// a follower acts on each once its snapshot stands. A herdr that came back holds what it held
  /// before its fall without the processes that ran in it.
  pub fn attachments(&self) -> tokio::sync::watch::Receiver<u64> {
    self.attached.subscribe()
  }

  /// The runs herdr holds: the last picture while the subscription is down but herdr's socket still
  /// takes a connection, and none once the socket refuses one, since a herdr that is not there holds
  /// nothing.
  pub fn names(&self) -> HashSet<String> {
    let state = self.state();

    if state.dropped && state.refused {
      return HashSet::new();
    }

    state.picture.names()
  }

  // One attachment, held until the stream ends: the subscription acknowledged, the snapshot installed,
  // what arrived meanwhile applied after it, and every event after that as it comes.
  async fn attach(&self, herdr: &Herdr) -> Result<(), String> {
    let connected = UnixStream::connect(herdr.socket()).await;

    self.state().refused = connected.is_err();

    let mut stream = connected.map_err(|why| why.to_string())?;
    let subscribe = json!({
      "id": "agency-watch",
      "method": "events.subscribe",
      "params": { "subscriptions": WATCHED_EVENTS.iter().map(|kind| json!({ "type": kind })).collect::<Vec<_>>() },
    });

    stream
      .write_all(format!("{subscribe}\n").as_bytes())
      .await
      .map_err(|why| why.to_string())?;

    let mut lines = BufReader::new(stream).lines();
    let ack = lines
      .next_line()
      .await
      .map_err(|why| why.to_string())?
      .ok_or("herdr closed the subscription before acknowledging it")?;

    crate::read_frame(&ack).map_err(|fault| format!("events.subscribe: {fault}"))?;

    let (events, mut held) = mpsc::unbounded_channel::<Value>();
    let reading = tokio::spawn(async move {
      while let Ok(Some(line)) = lines.next_line().await {
        if let Ok(event) = serde_json::from_str::<Value>(&line)
          && events.send(event).is_err()
        {
          return;
        }
      }
    });
    let snapshot = match herdr.snapshot().await {
      Ok(snapshot) => carrying_work(herdr, snapshot).await,
      Err(fault) => {
        reading.abort();

        return Err(format!("session.snapshot: {fault}"));
      }
    };

    {
      let mut state = self.state();

      state.picture.install(&snapshot);
      state.dropped = false;
    }

    self.attached.send_modify(|count| *count += 1);

    while let Some(event) = held.recv().await {
      self.state().picture.apply(&event);
    }

    Ok(())
  }
}
