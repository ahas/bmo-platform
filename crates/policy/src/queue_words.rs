//! What the queue's mechanics say where a person reads it (docs/worker-slots.md, docs/agent-control.md,
//! 「Stopping」): why a worker cannot be added, changed or removed, why a pin or a job's model names nothing
//! that could run, why a stop is not over, and what a job's end left in herdr.

use serde::Deserialize;

/// Why a worker cannot be added, changed or removed as asked.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerWords {
  pub name_shape: String,
  pub name_taken: String,
  pub label_empty: String,
  pub label_long: String,
  pub path_empty: String,
  pub path_relative: String,
  pub path_missing: String,
  pub path_not_directory: String,
  pub made_blocked: String,
  pub preamble_long: String,
  pub dev_one_line: String,
  pub dev_long: String,
  pub support_change: String,
  pub builtin_move: String,
  pub missing: String,
  pub support_remove: String,
  pub builtin_remove: String,
  pub support_settings: String,
  pub support_count: String,
  pub keepalive_count: String,
  pub keepalive_over: String,
  pub support_name_long: String,
}

/// Why a worker's pin names nothing that could run.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinWords {
  pub unknown: String,
  pub no_line: String,
  pub no_model: String,
  pub unlisted: String,
}

/// Why a job's model names nothing that could run.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteWords {
  pub model_wrong: String,
  pub no_model: String,
}

/// Why a run's provider cannot be had, and why a standing branch checkout could not be had.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolWords {
  pub off: String,
  pub no_place: String,
  pub head_unread: String,
  pub change_unread: String,
  pub new_files_unread: String,
}

/// Why a stop of several runs is not over.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopWords {
  pub not_ended: String,
}

/// What a job's end left open in herdr.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeftoverWords {
  pub workspace: String,
  pub unmarked: String,
  pub account: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueWords {
  pub workers: WorkerWords,
  pub pins: PinWords,
  pub routes: RouteWords,
  pub pool: PoolWords,
  pub stops: StopWords,
  pub leftovers: LeftoverWords,
}

crate::part!(pub QUEUE_WORDS: QueueWords = "queue-words.toml");
