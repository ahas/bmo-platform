//! What the memory verbs say of a row and refuse in its words, as `bmo memory` and the chat agent's own tools say it
//! (docs/memories.md, 「The tools and the script」), each line with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryVerbWords {
  pub global_scope: String,
  pub from_item: String,
  pub why_missing: String,
  pub action_missing: String,
  pub state_missing: String,
  pub situation_unpaired: String,
  pub situation_unfinished: String,
  pub event_unknown: String,
  pub event_empty: String,
  pub event_kind_item: String,
  pub event_takes_no_item: String,
  pub event_bad_item: String,
  pub usage: String,
}

crate::part!(pub MEMORY_VERBS: MemoryVerbWords = "memory-verbs");
