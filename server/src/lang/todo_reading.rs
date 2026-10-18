//! How a TODO item reads to a person or the chat agent, in a listing and opened whole (docs/todo.md), each line
//! with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Line {
  pub merged: String,
  pub merging: String,
  pub done: String,
  pub pending: String,
  pub cowork: String,
  pub note: String,
  pub report: String,
  pub unfinished: String,
  pub unfinished_resolved: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Held {
  pub stop: String,
  pub stopping: String,
  pub pause: String,
  pub pausing: String,
  pub failure: String,
  pub plain: String,
  pub pending: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
  pub done: String,
  pub open: String,
  pub merged: String,
  pub held: String,
  pub opened: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Show {
  pub state: String,
  pub state_merged: String,
  pub state_done: String,
  pub held: String,
  pub requester: String,
  pub approved: String,
  pub priority: String,
  pub model: String,
  pub model_default: String,
  pub model_fallback: String,
  pub model_changed: String,
  pub model_requested: String,
  pub worker: String,
  pub worker_unassigned: String,
  pub cowork: String,
  pub cowork_on: String,
  pub cowork_off: String,
  pub impact: String,
  pub impact_job: String,
  pub impact_node: String,
  pub impact_inside: String,
  pub impact_new_home: String,
  pub owner_note: String,
  pub owner_note_carried: String,
  pub report: String,
  pub unfinished: String,
  pub unfinished_resolved: String,
  pub resolution: String,
  pub raised: String,
  pub notes: String,
  pub draft: String,
  pub draft_intent: String,
  pub draft_instruction: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Merge {
  pub into: String,
  pub follows: String,
  pub merged: String,
  pub work: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repair {
  pub target: String,
  pub repairs: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Questions {
  pub heading: String,
  pub waiting: String,
  pub typed: String,
  pub chosen: String,
  pub defaulted: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Time {
  pub running: String,
  pub total: String,
  pub finished: String,
  pub attempt_running: String,
  pub attempt: String,
  pub no_outcome: String,
  pub phases: String,
  pub split: String,
  pub split_waiting: String,
  pub split_tooling: String,
  pub split_generating: String,
  pub inside: String,
  pub inside_more: String,
  pub no_spans: String,
  pub span_times: String,
  pub span_tokens: String,
  pub tokens: String,
  pub tokens_missing: String,
  pub no_tokens: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TodoReadingWords {
  pub line: Line,
  pub held: Held,
  pub row: Row,
  pub show: Show,
  pub merge: Merge,
  pub repair: Repair,
  pub questions: Questions,
  pub time: Time,
}

crate::part!(pub TODO_READING: TodoReadingWords = "todo-reading");
