//! A browse run as a person watching herdr reads it (docs/research-runs.md, 「The browse tool」), each line
//! with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Browse {
  pub run_title: String,
}

crate::part!(pub BROWSE: Browse = "browse");
