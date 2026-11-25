//! What each built-in agent is called and the roster the owner's own chat is given
//! (`lang/<language>/agent-catalogue.yaml`), each line with the `{name}` marks its file names.

use std::collections::BTreeMap;

use indexmap::IndexMap;
use serde::Deserialize;

/// The agents' names and the roster's lines.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCatalogue {
  pub roster: Roster,
  pub agents: BTreeMap<String, AgentNames>,
}

/// The roster's lines: the one shared chat agent with the rooms it is connected to (`{rooms}`), the owner's own
/// chat as it names it, and what each kind of agent is, in the order the roster names the kinds.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Roster {
  pub chat: String,
  pub owner_room: String,
  pub kinds: IndexMap<String, String>,
}

/// What the owner calls one built-in agent, and the other names they call it by.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentNames {
  pub label: String,
  pub aliases: Vec<String>,
}

crate::part!(pub AGENT_CATALOGUE: AgentCatalogue = "agent-catalogue");
