//! A word said to a worker: the clock its own turn carries, what that turn is called, what one word may
//! carry, and the lines written under it.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerWords {
  pub answer_minutes: u64,
  /// The most UTF-16 units one word may hold.
  pub most_words: usize,
  pub most_files: usize,
  pub most_file_bytes: usize,
  /// With `{worker}`.
  pub title: String,
  pub held: Held,
  pub ended: Ended,
  pub job: JobWords,
}

/// The line written under a word for what held it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Held {
  pub person: String,
  pub untrusted: String,
  pub unsubmitted: String,
  pub stopped: String,
  pub disabled: String,
  pub lost: String,
  pub resting: String,
  /// With `{provider}`.
  pub pinned: String,
}

/// The line written under the last word a turn took where it wrote no answer.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ended {
  pub silent: String,
}

/// The words for a running job: the paragraphs they go in behind and the lines written of them.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobWords {
  /// With `{item}`, `{actor}` and `{note}`.
  pub note: String,
  pub waited: String,
  pub kept: String,
  /// With `{count}`.
  pub carried: String,
}

crate::part!(pub WORKER_WORDS: WorkerWords = "worker-words.toml");
