//! What a job's mark on a long wait says where it is asked for wrongly from a shell
//! (docs/agent-control.md, 「A job's own mark on a long wait」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobWait {
  pub usage: String,
  pub what: String,
  pub target: String,
}

crate::part!(pub JOB_WAIT: JobWait = "job-wait");
