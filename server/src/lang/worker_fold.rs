//! What folding the job workers' sessions from a shell says (docs/deploy.md), each line with the `{name}`
//! marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerFold {
  pub line: String,
  pub untouched: Untouched,
  pub due: String,
  pub outcomes: Outcomes,
  pub tally: Tally,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Untouched {
  pub no_record: String,
  pub same: String,
}

/// One word or line for each way a fold can end.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcomes {
  pub compacted: String,
  pub too_short: String,
  pub busy: String,
  pub failed: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tally {
  pub none: String,
  pub sessions: String,
  pub part: String,
  pub left: String,
  pub due: String,
  pub words: Outcomes,
}

crate::part!(pub WORKER_FOLD: WorkerFold = "worker-fold");
