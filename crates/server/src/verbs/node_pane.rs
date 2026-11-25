//! What a node's workspace in herdr runs (docs/work-nodes.md, 「In herdr」): the node as the node service
//! lists it, kept current, and on a node reached over SSH the login the service names to it on Enter. It
//! ends with the head's stop once the node is gone, which ends the loop its pane runs it under.

use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Duration;

use policy::fill_marks;
use policy::verb_words::VERB_WORDS;

use super::{Answer, VerbError, service};
use crate::peers::node::{NodeRow, NodeService};
use crate::places::home::HEAD_STOPPED;

// How often the pane draws the node again while nobody is logged in from it.
const REDRAW: Duration = Duration::from_secs(2);

/// What a node's pane shows: the node as the service lists it, how it is reached, and what Enter does
/// there.
pub fn node_pane_text(node: &NodeRow) -> String {
  let words = &VERB_WORDS.node_pane;
  let detail = node
    .detail
    .as_deref()
    .map(|detail| fill_marks(&words.detail, &[("detail", detail)]))
    .unwrap_or_default();

  [
    format!("{} ({})", node.label.as_deref().unwrap_or(&node.node), node.node),
    String::new(),
    fill_marks(&words.kind, &[("kind", node.kind.word())]),
    fill_marks(&words.state, &[("state", &node.state), ("detail", &detail)]),
    String::new(),
    if node.kind.over_ssh() { words.ssh_enter.clone() } else { words.no_login.clone() },
  ]
  .join("\n")
}

// The node as it stands now, or none where it left the list.
async fn current(nodes: &dyn NodeService, name: &str) -> Result<Option<NodeRow>, VerbError> {
  let rows = nodes.node_rows().await.map_err(|fault| VerbError::Refused(fault.to_string()))?;

  Ok(rows.into_iter().find(|row| row.node == name))
}

// The login the node service names to a node reached over SSH, or none.
async fn login(nodes: &dyn NodeService, node: &NodeRow) -> Option<Vec<String>> {
  if !node.kind.over_ssh() {
    return None;
  }

  nodes.ssh_login(&node.node).await.ok().flatten()
}

/// Whether a binary run from one release tree is retired by the deploy link's binary: the link names
/// another release beside it, of the same deploy tree. A binary that runs from no release — a test, a
/// build of a checkout — is retired by nothing.
pub fn retired_by(own: &Path, live: &Path) -> bool {
  let tree = |binary: &Path| binary.parent().and_then(Path::parent).map(Path::to_path_buf);
  let (Some(own), Some(live)) = (tree(own), tree(live)) else { return false };

  own != live && own.parent().is_some() && own.parent() == live.parent()
}

// Whether a deployment retired this verb: the deploy link now names another release than the one it runs
// from, so the loop starts the link's and no node's pane keeps an old release from the deployment's prune.
fn retired() -> bool {
  let live = super::deploy_link().join("bin").join(crate::places::VERBS_BINARY);
  let (Ok(own), Ok(live)) = (std::env::current_exe(), std::fs::canonicalize(live)) else {
    return false;
  };

  retired_by(&own, &live)
}

fn draw(node: &NodeRow) {
  println!("\x1b[2J\x1b[H{}", node_pane_text(node));
  let _ = std::io::stdout().flush();
}

/// The pane run until its node leaves the list.
pub async fn node_pane(data_root: &Path, name: &str) -> Result<Answer, VerbError> {
  // A pane named no node service would read an empty list and close as if its node were gone.
  if std::env::var(crate::peers::node::ADDRESS.url)
    .ok()
    .filter(|url| !url.is_empty())
    .is_none()
  {
    return Err(VerbError::Refused(format!(
      "no node service is named: set {} for the pane",
      crate::peers::node::ADDRESS.url
    )));
  }

  let service = service(data_root).await?;
  let nodes = service.peers.node.clone();
  let gone = || Answer {
    lines: vec![fill_marks(&VERB_WORDS.node_pane.gone, &[("name", name)])],
    code: u8::try_from(HEAD_STOPPED).unwrap_or(1),
  };

  // Ctrl-C in this pane is for the login it starts: the login stops on it, and this screen stays. A
  // handler rather than an ignore, so what the pane starts still stops on one.
  if let Ok(mut interrupts) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()) {
    tokio::spawn(async move { while interrupts.recv().await.is_some() {} });
  }

  // Enter, a line at a time, read only while no login holds the terminal: the reader waits for its turn
  // after each line, so a login started on one is the only reader of what is typed next.
  let (lines, entered) = tokio::sync::mpsc::unbounded_channel::<()>();
  let mut entered = Some(entered);
  let (turn, turns) = std::sync::mpsc::channel::<()>();

  std::thread::spawn(move || {
    let mut input = std::io::stdin().lock();
    let mut line = String::new();

    loop {
      line.clear();

      if input.read_line(&mut line).unwrap_or(0) == 0 || lines.send(()).is_err() || turns.recv().is_err() {
        return;
      }
    }
  });

  let Some(mut node) = current(nodes.as_ref(), name).await? else { return Ok(gone()) };

  draw(&node);

  let mut redraw = tokio::time::interval(REDRAW);

  loop {
    tokio::select! {
      _ = redraw.tick() => {
        // Only between logins: a login holds this arm back until it ends.
        if retired() {
          return Ok(Answer { lines: vec![], code: 0 });
        }

        let Some(now) = current(nodes.as_ref(), name).await? else { return Ok(gone()) };

        node = now;
        draw(&node);
      }
      typed = async {
        match entered.as_mut() {
          Some(entered) => entered.recv().await,
          None => std::future::pending().await,
        }
      } => {
        if typed.is_none() {
          // The standing input ended; the pane keeps drawing.
          entered = None;
          continue;
        }

        if let Some(args) = login(nodes.as_ref(), &node).await {
          print!("\x1b[2J\x1b[H");
          let _ = std::io::stdout().flush();
          let _ = tokio::process::Command::new("ssh").args(args).status().await;

          redraw.reset();
        }

        let Some(now) = current(nodes.as_ref(), name).await? else { return Ok(gone()) };

        node = now;
        draw(&node);
        let _ = turn.send(());
      }
    }
  }
}
