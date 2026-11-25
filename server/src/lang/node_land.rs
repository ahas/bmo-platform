//! A commit a node's landing wrote down as waiting to go live, as `bmo node land` tells it (docs/deploy.md),
//! each line with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeLandWords {
  pub pending: String,
  pub item: String,
  pub no_item: String,
  pub urgent: String,
}

crate::part!(pub NODE_LAND: NodeLandWords = "node-land");
