//! What a job's spans are called where a person reads a breakdown of them: the CLI's tools and the MCP
//! servers by the act they do, a co-worker, a length of time and the kinds a run's tokens divide into,
//! each line with the `{name}` marks its file names.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clock {
  pub seconds: String,
  pub minutes: String,
  pub hours: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Kinds {
  pub input: String,
  pub output: String,
  pub cache_read: String,
  pub cache_write: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanWords {
  pub tools: BTreeMap<String, String>,
  pub servers: BTreeMap<String, String>,
  pub coworker: String,
  pub coworker_kind: String,
  pub clock: Clock,
  pub kinds: Kinds,
  pub total: String,
}

crate::part!(pub SPAN_WORDS: SpanWords = "span-words");
