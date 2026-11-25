//! The agents that stood under another name (docs/policy.md).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRenames {
  pub renames: Vec<AgentRename>,
}

/// One agent's earlier name and the one it has now, with what moved beside the name.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRename {
  pub from: String,
  pub to: String,
  pub label: Moved,
  pub aliases: Option<Moved>,
  pub scope: Option<Changed>,
  /// The checkout's directory before the rename and after it.
  pub cwd: Option<Changed>,
  pub herdr_label: Option<String>,
}

/// A text as the row was seeded and as it reads now.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Moved {
  pub seeded: String,
  pub now: String,
}

/// A value before the rename and after it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Changed {
  pub from: String,
  pub to: String,
}

crate::part!(pub AGENT_RENAMES: AgentRenames = "agent-renames.toml");
