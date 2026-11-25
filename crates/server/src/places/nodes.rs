//! The nodes on herdr's left (docs/work-nodes.md, 「In herdr」): one workspace per node, after every other
//! workspace, in the node list's order, labelled with the node's name and state, stood after the node
//! service's list as it now reads; and `nodes-moved` published where the list's shape moved under them
//! (docs/api.md, 「Events」).

use std::sync::{Mutex, PoisonError};

use herdr::{Fault, Herdr, Workspace};
use policy::node_states::NodeState;
use serde_json::json;

use super::Ground;
use super::home::{HEAD_STOPPED, Launch, METADATA_SOURCE, short_title};
use crate::events::Event;
use crate::peers::node::NodeRow;

/// The token a node's workspace is reported under, and the verb its pane runs.
const NODE_TOKEN: &str = "node";
const NODE_PANE_VERB: &str = "node-pane";

/// The node list's shape moved under the places — a node added, removed, renamed, relabelled, reordered
/// or changing state — so a consumer showing the nodes reads them again. It carries nothing.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct NodesMoved;

impl Event for NodesMoved {
  const NAME: &'static str = "nodes-moved";
}

/// What the node workspaces keep across their passes for as long as the process stands: the list's shape
/// as the last pass saw it, and the turn that keeps two passes from running at once.
#[derive(Default)]
pub struct NodeList {
  shape: Mutex<Option<String>>,
  turn: tokio::sync::Mutex<()>,
}

impl NodeList {
  /// The shape the last pass saw swapped for this one's, answering whether it moved. The first pass a
  /// process makes has nothing to have moved from.
  pub fn moved(&self, nodes: &[NodeRow]) -> bool {
    let shape =
      json!(nodes.iter().map(|node| json!([node.node, node.label, node.state])).collect::<Vec<_>>())
        .to_string();
    let mut kept = self.shape.lock().unwrap_or_else(PoisonError::into_inner);
    let moved = kept.as_ref().is_some_and(|before| *before != shape);

    *kept = Some(shape);
    moved
  }
}

/// The name a node is shown under: its label, else its name.
fn shown(node: &NodeRow) -> &str {
  node.label.as_deref().unwrap_or(&node.node)
}

/// What a node's workspace is labelled.
pub fn label(node: &NodeRow) -> String {
  short_title(&format!("{} · {}", shown(node), NodeState::of_column(&node.state).said().word), 48)
}

/// The pane a node's workspace runs: the node's pane verb under a loop that starts it again when a
/// deployment retires it, as an agent's head stands. The verb exits with the head's stop once its
/// node is gone, which ends the loop. Ctrl-C in the pane is meant for a login, so the loop takes it with
/// a handler of its own, which lets what it starts still stop on one.
fn pane(ground: &Ground, node: &NodeRow) -> Launch {
  let root = ground.code_root();
  let mark = format!("bmo-node-{}", node.node);
  let mut env = ground.run_env(&mark);

  // The verb reads the node list itself, so it is handed the node service as the server reaches it.
  for name in [crate::peers::node::ADDRESS.url, crate::peers::node::ADDRESS.token] {
    if let Some(value) = ground.vars().get(name) {
      env.insert(name.to_owned(), value.clone());
    }
  }

  Launch {
    command: vec![
      "/bin/sh".into(),
      "-c".into(),
      format!(
        r#"trap : INT; while :; do "$1" "$2" "$3"; case $? in {HEAD_STOPPED}) exit 0;; esac; sleep 1; done"#
      ),
      mark.clone(),
      root.join("bin").join(super::VERBS_BINARY).to_string_lossy().into_owned(),
      NODE_PANE_VERB.to_owned(),
      node.node.clone(),
    ],
    cwd: ground.pane_dir(),
    env,
  }
}

// Which node a workspace stands for, and whether it is one herdr rebuilt: a rebuilt workspace has lost
// its token but kept its label, and its pane is a fresh shell, so it is found by its label to be
// replaced rather than left beside a new one.
fn node_of(workspace: &Workspace, nodes: &[NodeRow]) -> Option<(String, bool)> {
  if let Some(token) = workspace.token(NODE_TOKEN) {
    return Some((token.to_owned(), false));
  }

  let label = workspace.label.as_deref().unwrap_or_default();

  nodes
    .iter()
    .find(|node| label.starts_with(&format!("{} · ", shown(node))))
    .map(|node| (node.node.clone(), true))
}

async fn ordered(herdr: &Herdr) -> Result<Vec<Workspace>, Fault> {
  let mut workspaces = herdr.snapshot().await?.workspaces;

  workspaces.sort_by_key(|workspace| workspace.number.unwrap_or(0));

  Ok(workspaces)
}

async fn make(herdr: &Herdr, ground: &Ground, node: &NodeRow) -> bool {
  let pane = pane(ground, node);
  let Ok(made) = herdr
    .call(
      "workspace.create",
      json!({ "label": label(node), "cwd": pane.cwd, "env": pane.env }),
      None,
    )
    .await
  else {
    return false;
  };
  let (Some(workspace), Some(tab)) = (
    made.pointer("/workspace/workspace_id").and_then(|id| id.as_str()),
    made.pointer("/tab/tab_id").and_then(|id| id.as_str()),
  ) else {
    return false;
  };
  let root = json!({ "type": "pane", "command": pane.command, "cwd": pane.cwd, "env": pane.env });

  if herdr
    .call("layout.apply", json!({ "tab_id": tab, "tab_label": shown(node), "root": root }), None)
    .await
    .is_err()
  {
    let _ = herdr.call("workspace.close", json!({ "workspace_id": workspace }), None).await;

    return false;
  }

  if let Err(fault) = herdr
    .call(
      "workspace.report_metadata",
      json!({ "workspace_id": workspace, "source": METADATA_SOURCE, "tokens": { NODE_TOKEN: node.node } }),
      None,
    )
    .await
  {
    eprintln!("node workspaces: {}'s workspace {workspace} took no token: {fault}", node.node);
  }

  true
}

/// What standing the workspaces did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tally {
  pub made: usize,
  pub renamed: usize,
  pub closed: usize,
  pub moved: usize,
}

/// Every node's workspace made, named and put in its place, and every workspace of a node that is gone
/// closed. A herdr that cannot be asked changes nothing.
pub async fn stand_workspaces(herdr: &Herdr, ground: &Ground, nodes: &[NodeRow]) -> Result<Tally, Fault> {
  let mut tally = Tally::default();
  let mut standing: Vec<(String, Workspace)> = vec![];

  for workspace in ordered(herdr).await? {
    let (Some(id), Some((name, rebuilt))) = (workspace.workspace_id.clone(), node_of(&workspace, nodes))
    else {
      continue;
    };
    let known = nodes.iter().any(|node| node.node == name);

    // A node that is gone, a rebuilt shell, and a second workspace for the same node all close.
    if !known || rebuilt || standing.iter().any(|(held, _)| *held == name) {
      if herdr.call("workspace.close", json!({ "workspace_id": id }), None).await.is_ok() {
        tally.closed += 1;
      }

      continue;
    }

    standing.push((name, workspace));
  }

  for node in nodes {
    let Some((_, workspace)) = standing.iter().find(|(name, _)| *name == node.node) else {
      if make(herdr, ground, node).await {
        tally.made += 1;
      }

      continue;
    };
    let wanted = label(node);

    if workspace.label.as_deref() != Some(wanted.as_str())
      && herdr
        .call(
          "workspace.rename",
          json!({ "workspace_id": workspace.workspace_id, "label": wanted }),
          None,
        )
        .await
        .is_ok()
    {
      tally.renamed += 1;
    }
  }

  // The node block goes last, in the list's order. Where it does not already stand so, each node's
  // workspace is sent to the end in turn, which leaves them there in that order whatever stood where.
  let order = ordered(herdr).await?;
  let block: Vec<String> = nodes
    .iter()
    .filter_map(|node| {
      order
        .iter()
        .find(|workspace| workspace.token(NODE_TOKEN) == Some(node.node.as_str()))
        .and_then(|workspace| workspace.workspace_id.clone())
    })
    .collect();
  let tail: Vec<String> = order[order.len() - block.len()..]
    .iter()
    .filter_map(|workspace| workspace.workspace_id.clone())
    .collect();

  if tail != block {
    for id in &block {
      if herdr
        .call("workspace.move", json!({ "workspace_id": id, "insert_index": order.len() - 1 }), None)
        .await
        .is_ok()
      {
        tally.moved += 1;
      }
    }
  }

  Ok(tally)
}

/// The workspaces stood after the node service's list as it now reads, and `nodes-moved` published where
/// the list's shape moved since the last pass, one pass at a time. A node service that cannot be asked
/// changes nothing.
pub async fn settle(ground: &Ground, list: &NodeList) {
  let _turn = list.turn.lock().await;
  let nodes = match ground.peers.node.node_rows().await {
    Ok(nodes) => nodes,
    Err(fault) => return eprintln!("node workspaces: {fault}"),
  };
  let moved = list.moved(&nodes);

  if let Some(herdr) = &ground.herdr
    && let Err(fault) = stand_workspaces(herdr, ground, &nodes).await
  {
    eprintln!("node workspaces could not be stood in herdr: {fault}");
  }

  if moved {
    ground.events.publish(NodesMoved);
  }
}
