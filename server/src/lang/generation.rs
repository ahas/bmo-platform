//! Why a value is not one a generation setting takes (`lang/<language>/generation.yaml`), each line with the `{name}`
//! marks its file names.

use serde::Deserialize;

/// Why a value is not one a generation setting takes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationWords {
  pub refusals: Refusals,
}

/// The `refusals` lines.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Refusals {
  pub switch: String,
  pub number: String,
  pub whole: String,
  pub range: String,
  pub above: String,
  pub least: String,
  pub most: String,
}

crate::part!(pub GENERATION_WORDS: GenerationWords = "generation");
