//! The queue's minute pass (docs/agent-control.md, 「An idle agent is told to the orderers of the items it could
//! take」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueIdle {
  /// How often the queue is looked at, in seconds.
  pub check_seconds: u64,
  /// How long an agent stays idle before the orderers of the items it could take are told, in seconds.
  pub launch_seconds: i64,
  /// What an orderer is told of an idle agent: `{agent}`, `{role}`, `{provider}`, `{model}` and `{items}`.
  pub agent_idle_order: String,
  /// One item an idle agent could take, as `{items}` lists it: `{item}` and `{title}`.
  pub agent_idle_item: String,
  /// How many of a draft's brief's first lines the supervisor is told.
  pub draft_brief_lines: usize,
  /// The most characters of those lines the supervisor is told.
  pub draft_brief_chars: usize,
  /// What the supervisor is told of a draft awaiting approval: `{item}`, `{title}`, `{raised_by}`,
  /// `{repository}` and `{brief}`.
  pub draft_order: String,
}

crate::part!(pub QUEUE_IDLE: QueueIdle = "queue-idle.toml");
