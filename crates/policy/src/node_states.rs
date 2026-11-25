//! What a node's last check can come to, and the word and tone every list says each in
//! (docs/work-nodes.md).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One state a node's check comes to, named as the store's rows and every answer name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeState {
  Connected,
  Disconnected,
  Auth,
  Hostkey,
  Unreachable,
  Error,
  Unknown,
}

impl NodeState {
  /// Every state, in the order the lists name them.
  pub const ALL: [Self; 7] = [
    Self::Connected,
    Self::Disconnected,
    Self::Auth,
    Self::Hostkey,
    Self::Unreachable,
    Self::Error,
    Self::Unknown,
  ];

  /// A row's state as its column holds it, `unknown` where the column holds none of them.
  pub fn of_column(column: &str) -> Self {
    Self::ALL.into_iter().find(|state| state.name() == column).unwrap_or(Self::Unknown)
  }

  /// The name the column and every answer hold.
  pub fn name(self) -> &'static str {
    match self {
      Self::Connected => "connected",
      Self::Disconnected => "disconnected",
      Self::Auth => "auth",
      Self::Hostkey => "hostkey",
      Self::Unreachable => "unreachable",
      Self::Error => "error",
      Self::Unknown => "unknown",
    }
  }

  /// How the state is said.
  pub fn said(self) -> &'static Said {
    &NODE_STATES[&self]
  }
}

/// How a state reads at a glance: reached, not reached, or not yet asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
  Good,
  Bad,
  Unknown,
}

/// One state's word and tone.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Said {
  pub word: String,
  pub tone: Tone,
}

crate::part!(pub NODE_STATES: BTreeMap<NodeState, Said> = "node-states.toml");
