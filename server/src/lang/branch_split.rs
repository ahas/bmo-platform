//! What went on inside a Claude Workflow's branches, as `bmo job branches` reads it out (docs/jobs.md),
//! each line with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BranchSplitWords {
  pub clock: Clock,
  pub none_left: String,
  pub not_found: String,
  pub branches: String,
  pub session: String,
  pub records_gone: String,
  pub branch: String,
  pub tokens: String,
  pub no_type: String,
  pub no_split: String,
  pub unnamed_tool: String,
  pub split: String,
  pub open: String,
  pub tool: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clock {
  pub seconds: String,
  pub minutes: String,
  pub hours: String,
}

crate::part!(pub BRANCH_SPLIT: BranchSplitWords = "branch-split");
