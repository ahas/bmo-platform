//! The model catalogue as `bmo model` tells it (docs/model-catalogue.md), each line with the `{name}` marks
//! its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelWords {
  pub usage: String,
  pub not_held: String,
  pub already_held: String,
  pub placed: String,
  pub node_unreadable: String,
  pub removed: String,
  pub choosing: String,
  pub listing: Listing,
  pub run: Run,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listing {
  #[serde(rename = "use")]
  pub use_line: String,
  pub serving: String,
  pub file: String,
  #[serde(rename = "where")]
  pub where_line: String,
  pub defaults: String,
  pub thinking_on: String,
  pub thinking_off: String,
  pub note: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
  pub installing: String,
  pub not_running: String,
  pub failed: String,
}

crate::part!(pub MODEL_WORDS: ModelWords = "models");
