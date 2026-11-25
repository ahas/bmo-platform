//! The node list as `bmo node list` prints it and a node's pane shows it (docs/nodes.md), each line with the
//! `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeWords {
  pub clock: String,
  pub morning: String,
  pub afternoon: String,
  pub pane: Pane,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pane {
  pub kind: String,
  pub address: String,
  pub credential: String,
  pub state: String,
  pub detail: String,
  pub checked: String,
  pub connected: String,
  pub never: String,
  pub own_ssh_setup: String,
  pub hub_not_listed: String,
  pub ssh_enter: String,
  pub jake_only: String,
  pub gone: String,
}

crate::part!(pub NODE_WORDS: NodeWords = "nodes");
