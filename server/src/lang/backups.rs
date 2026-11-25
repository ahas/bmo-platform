//! What the archive's hourly snapshot and its restore drill say (docs/archive-backup.md), with the `{name}`
//! marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Backups {
  pub snapshot_usage: String,
  pub drill_usage: Vec<String>,
  pub drill_failed: String,
  pub drill_differ: String,
  pub drill_more: String,
  pub upload_usage: Vec<String>,
  pub upload_failed_note: String,
}

crate::part!(pub BACKUPS: Backups = "backups");
