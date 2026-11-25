//! What an agent place's records are read for before a line is written (docs/agent-control.md, 「A record
//! holds no secret」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordSecrets {
  /// The words a saved value's key ends in, alone or after `api`, where the value is a secret's.
  pub secret_words: Vec<String>,
  /// The fewest characters a saved value counts at.
  pub least_characters: usize,
  /// The largest saved file read, in bytes.
  pub saved_file_most_bytes: u64,
  /// The most saved files read.
  pub saved_files_most: usize,
}

crate::part!(pub RECORD_SECRETS: RecordSecrets = "record-secrets.toml");
