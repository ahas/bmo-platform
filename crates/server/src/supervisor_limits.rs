//! What a supervisor's settings close of its sessions (docs/supervisors.md, 「Its settings」): its brain
//! switch, which takes away every capability through which a session reaches a brain, its reference switch,
//! which takes away those through which it reaches the gate's reading of the brain, and its tool limit,
//! which keeps only the capabilities it names, leaves out the servers and the program's own tools the chat line
//! holds under a capability it does not name, and starts the program with no built-in tool but those it keeps. A
//! supervisor whose settings hold neither is handed everything, as every supervisor is by default. Where its
//! program stands on a node, the servers that run with this machine's addresses and secrets are handed as the tool
//! host's entries that carry their work instead (docs/person-nodes.md, 「A supervisor's tools on a node」).

use policy::person_nodes::PERSON_NODES;
use policy::roles::ROLES;
use policy::run_lines::{Hosted, RUN_LINES};
use sea_orm::ConnectionTrait;

use crate::Refusal;
use crate::control::supervisor_settings::Settings;
use crate::lines::fault;

/// The line a supervisor's turns go out on, whose `held` names what a limit closes outside the tool host.
const CHAT_LINE: &str = "chat";

/// A supervisor's brain switch, reference switch and tool limit, as its settings hold them, and whether its program
/// stands on a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
  brain_on: bool,
  reference_on: bool,
  tools_allowed: Option<Vec<String>>,
  on_node: bool,
}

impl Limits {
  /// What a session is handed where no supervisor's settings close anything.
  pub fn open() -> Self {
    Self {
      brain_on: true,
      reference_on: true,
      tools_allowed: None,
      on_node: false,
    }
  }

  pub fn new(brain_on: bool, reference_on: bool, tools_allowed: Option<Vec<String>>) -> Self {
    Self {
      brain_on,
      reference_on,
      tools_allowed,
      on_node: false,
    }
  }

  /// The same limits for a supervisor whose program stands on a node, or does not.
  pub fn standing_on_node(mut self, on_node: bool) -> Self {
    self.on_node = on_node;
    self
  }

  /// The supervisor's limits, read afresh from its settings, and whether its program stands on a node: the one its
  /// account's runs stand on (docs/person-nodes.md, 「Which node is a person's」).
  pub async fn of<C: ConnectionTrait>(db: &C, supervisor: &str) -> Result<Self, Refusal> {
    let settings = Settings::of(db, supervisor)
      .await
      .map_err(|refused| fault(format!("{supervisor}'s settings could not be read: {}", refused.error)))?;
    let on_node = crate::person_nodes::supervisor_node(db, supervisor).await?.is_some();

    Ok(
      Self::new(settings.brain_on, settings.reference_on, settings.tools_allowed).standing_on_node(on_node),
    )
  }

  /// Whether the supervisor's program stands on a node.
  pub fn on_node(&self) -> bool {
    self.on_node
  }

  pub fn brain_on(&self) -> bool {
    self.brain_on
  }

  /// The capabilities a record keeps of those it would hold: none the brain is reached through where the
  /// brain is off, none the gate's reading of it is reached through where the reference is off, and only
  /// those the limit names where it names any. It never widens them.
  pub fn narrowed(&self, capabilities: Vec<String>) -> Vec<String> {
    capabilities
      .into_iter()
      .filter(|capability| self.brain_on || !ROLES.brain.contains(capability))
      .filter(|capability| self.reference_on || !ROLES.reference.contains(capability))
      .filter(|capability| self.allows(capability))
      .collect()
  }

  /// Whether the chat line's configuration keeps the server of that name: every server no `held` entry
  /// names, and each one whose capability the limit allows, unless the program stands on a node and the tool
  /// host's entries stand in for it there.
  pub fn keeps_server(&self, server: &str) -> bool {
    RUN_LINES
      .held(CHAT_LINE)
      .iter()
      .filter(|held| held.servers.iter().any(|name| name == server))
      .all(|held| self.allows(&held.capability) && !(self.on_node && held.hosted.is_some()))
  }

  /// What stands in on the tool host for the servers of `line` the configuration leaves out because the program
  /// stands on a node: each capability's hosted entries, where the limit allows the capability; none where the
  /// program stands on no node.
  pub fn hosted(&self, line: &str) -> Vec<&'static Hosted> {
    if !self.on_node {
      return vec![];
    }

    RUN_LINES
      .held(line)
      .iter()
      .filter(|held| self.allows(&held.capability))
      .filter_map(|held| held.hosted.as_ref())
      .collect()
  }

  /// The tool host's namespaces a configuration and a record of `line` name: the line's, then those the hosted
  /// entries add, each once.
  pub fn namespaces(&self, line: &str) -> Vec<String> {
    once_each(
      RUN_LINES
        .namespaces(line)
        .iter()
        .chain(self.hosted(line).into_iter().flat_map(|hosted| &hosted.namespaces)),
    )
  }

  /// The entries a record of `line` shows by name: the line's, then those the hosted entries show, each once.
  pub fn shown(&self, line: &str) -> Vec<String> {
    once_each(
      RUN_LINES
        .shown(line)
        .iter()
        .chain(self.hosted(line).into_iter().flat_map(|hosted| &hosted.shown)),
    )
  }

  /// The chat line's denied list, `denied` as the line declares it, with every program tool whose
  /// capability the limit leaves out after it.
  pub fn denied(&self, denied: &str) -> String {
    let closed: Vec<&str> = RUN_LINES
      .held(CHAT_LINE)
      .iter()
      .filter(|held| !self.allows(&held.capability))
      .flat_map(|held| held.tools.iter().map(String::as_str))
      .filter(|tool| !denied.split(',').any(|named| named.trim() == *tool))
      .collect();

    std::iter::once(denied)
      .filter(|denied| !denied.is_empty())
      .chain(closed)
      .collect::<Vec<_>>()
      .join(",")
  }

  /// The built-in tools the chat line's program is started with and no other, where a limit is held: those no
  /// capability gives that a limited program keeps, then those under each capability the limit allows. None
  /// where no limit is held, so the program keeps every tool of its own but the denied list.
  pub fn kept(&self) -> Option<String> {
    self.tools_allowed.as_ref()?;

    let held = RUN_LINES
      .held(CHAT_LINE)
      .iter()
      .filter(|held| self.allows(&held.capability))
      .flat_map(|held| held.tools.iter());
    let mut kept: Vec<&str> = vec![];

    for tool in RUN_LINES.limited_keeps(CHAT_LINE).iter().chain(held) {
      if !kept.contains(&tool.as_str()) {
        kept.push(tool);
      }
    }

    Some(kept.join(","))
  }

  fn allows(&self, capability: &str) -> bool {
    self
      .tools_allowed
      .as_ref()
      .is_none_or(|allowed| allowed.iter().any(|named| named == capability))
  }
}

/// Every capability a supervisor's tool limit may name: what the chat line gives, every bundle the policy
/// declares, what the chat line holds outside the tool host, and the capabilities that operate the platform, which
/// an administrator's supervisor holds.
pub fn offered() -> Vec<String> {
  once_each(
    RUN_LINES
      .capabilities(CHAT_LINE)
      .iter()
      .chain(ROLES.bundles.iter().map(|bundle| &bundle.name))
      .chain(RUN_LINES.held(CHAT_LINE).iter().map(|held| &held.capability))
      .chain(&PERSON_NODES.administrator_capabilities),
  )
}

// Each name once, in the order first met.
fn once_each<'a>(names: impl Iterator<Item = &'a String>) -> Vec<String> {
  let mut held: Vec<String> = vec![];

  for name in names {
    if !held.contains(name) {
      held.push(name.clone());
    }
  }

  held
}
