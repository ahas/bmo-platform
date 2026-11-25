//! The handoff a supervisor's session going away is asked for (docs/sessions.md, 「A restart hands the session
//! over」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handoff {
  /// How long agency waits before asking again while a handoff is under way, in seconds.
  pub retry_seconds: u64,
  /// The longest one ask waits for the receiver's answer, in seconds; past it the handoff was not had.
  pub wait_seconds: u64,
  /// The least time between two asks for the same session's handoff, in seconds; an ask sooner waits for it.
  pub again_seconds: u64,
  /// How often a replaced program's tool record is looked at for asynchronous calls still running, in seconds.
  pub calls_look_seconds: u64,
  pub reasons: Reasons,
}

/// The reasons a handoff is asked with.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reasons {
  /// The kept session gone.
  pub lost: String,
  /// A fresh session replacing the one the agent had.
  pub replaced: String,
  /// The documents the session's prompt is composed from changed.
  pub moved: String,
  /// The road the session went out on changed.
  pub road: String,
  /// A reset of the session asked.
  pub reset: String,
}

crate::part!(pub HANDOFF: Handoff = "handoff.toml");
