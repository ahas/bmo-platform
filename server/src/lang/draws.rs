//! What a fair draw says in a room (`lang/<language>/draws.yaml`), each line with the `{name}` marks its file
//! names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draws {
  pub spec: Spec,
  pub dice_total: String,
  pub line: Line,
}

/// What was asked, by its shape.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
  pub dice: String,
  pub range: String,
  pub pick: String,
  pub shuffle: String,
}

/// The line a draw posts.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Line {
  pub head: String,
  pub labelled: String,
  pub asked: String,
  pub result: String,
  pub draw: String,
  pub method: String,
  pub first: String,
  pub answered: String,
  pub next: String,
}

crate::part!(pub DRAWS: Draws = "draws");
