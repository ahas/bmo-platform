//! What the reference benches print (docs/reference-selection.md, 「What it picks and what it costs, measured」):
//! each bench's usage, and the corpus and figures of `bmo reference compare`, each line with the `{name}` marks
//! its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceBenches {
  pub usage: Usage,
  pub compare: Compare,
}

/// Each bench's usage.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Usage {
  pub compare: String,
  pub inputs: String,
  pub narrowing: String,
  pub replay: String,
  pub roads: String,
  pub waste: String,
}

/// What the comparison of the split says of its corpus as it goes and of its figures at the end.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compare {
  pub per_cell: String,
  pub corpus: String,
  pub asked: String,
  pub written: String,
  pub sample: String,
  pub sifted: String,
  pub pieces: String,
  pub none: String,
  pub unread: String,
  pub shapes: String,
  pub wanting: String,
  pub front: String,
}

crate::part!(pub REFERENCE_BENCHES: ReferenceBenches = "reference-benches");
