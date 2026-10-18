//! What a job's question to the owner says to the job that asks it from a shell (docs/jobs.md, 「A job's
//! question to the owner」), each line with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobAsk {
  pub usage: String,
  pub refusals: Refusals,
  pub heard: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Refusals {
  pub room: String,
  pub item: String,
  pub question: String,
  pub options: String,
  pub option: String,
  pub recommend: String,
  pub why: String,
  pub wait: String,
  pub talk: String,
  pub no_talk: String,
  pub say: String,
  pub wording: String,
}

crate::part!(pub JOB_ASK: JobAsk = "job-ask");
