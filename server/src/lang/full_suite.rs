//! The account a daily whole run that did not pass hands the chat agent (docs/test-suite.md, 「The daily whole run」),
//! with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSuite {
  pub account: String,
  pub at_head: String,
  pub no_head: String,
  pub engine: String,
  pub files: String,
  pub file: String,
  pub joiner: String,
  pub outside: String,
  pub outside_listed: String,
}

crate::part!(pub FULL_SUITE: FullSuite = "full-suite");
