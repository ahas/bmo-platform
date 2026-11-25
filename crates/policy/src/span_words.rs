//! The words a run's spend and a job's spans are said in (docs/run-records.md): how long a stretch took,
//! each of the four kinds of token a run reports, and their total.

use serde::Deserialize;

/// How long a stretch took, under a minute, under an hour, and past it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clock {
  pub seconds: String,
  pub minutes: String,
  pub hours: String,
}

/// The four kinds of token a run reports.
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
  pub total: String,
  pub clock: Clock,
  pub kinds: Kinds,
}

crate::part!(pub SPAN_WORDS: SpanWords = "span-words.toml");
