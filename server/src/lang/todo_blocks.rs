//! The words a TODO item's instruction is composed in beyond its own parts and the copied conversation, each
//! line with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TodoBlockWords {
  pub note: String,
  pub note_source: String,
  pub judgement: String,
  pub source: String,
  pub source_line: String,
  pub raised_by_item: String,
  pub filed_by: String,
  pub fast_track: String,
  pub message: String,
}

crate::part!(pub TODO_BLOCKS: TodoBlockWords = "todo-blocks");
