//! What words meant for the owner are refused with where they hand them a page hosted outside the chat agent's own
//! archive (docs/archive.md), with the `{owner}` mark its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutsideHosting {
  pub refusal: String,
}

crate::part!(pub OUTSIDE_HOSTING: OutsideHosting = "outside-hosting");
