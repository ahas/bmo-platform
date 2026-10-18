//! The one line at the foot of a resident program's screen (`lang/<language>/status-line.yaml`), each line
//! with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusLineWords {
  pub resting: String,
  pub person: String,
  pub running: String,
  pub rested_after_person: String,
  pub rested_after_run: String,
}

crate::part!(pub STATUS_LINE: StatusLineWords = "status-line");
