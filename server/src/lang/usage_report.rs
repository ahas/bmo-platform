//! What the accounts have been spending as a shell reads it (docs/usage.md), each line with the
//! `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageReport {
  pub windows: Windows,
  pub groups: Groups,
  pub kinds: Kinds,
  pub total: String,
  pub runs: String,
  pub unsplit: String,
  pub refresh: Refresh,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Windows {
  pub five_hours: String,
  pub seven_days: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Groups {
  pub models: String,
  pub agents: String,
  pub empty: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Kinds {
  pub read: String,
  pub write: String,
  pub cache_read: String,
  pub cache_write: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Refresh {
  pub unread: String,
  pub resets: String,
  pub no_reset: String,
  pub moment: String,
  pub weekdays: [String; 7],
}

crate::part!(pub USAGE_REPORT: UsageReport = "usage-report");
