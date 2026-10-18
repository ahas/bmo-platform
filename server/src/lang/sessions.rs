//! Why a session the chat agent holds is replaced (`lang/<language>/sessions.yaml`), as the console's events and the
//! session's note name it.

use serde::Deserialize;

/// The reasons and the line an event adds where the summary did not come back.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionWords {
  pub reasons: Reasons,
  pub summary_failed: String,
  pub transcript: Transcript,
}

/// The lines of the chat's own tail of a road that named no session to resume.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transcript {
  pub room: String,
  pub answer: String,
}

/// Each reason a held session may not be resumed, and why a room's session was replaced.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reasons {
  pub route: String,
  pub account: String,
  pub placement: String,
  pub prefix: String,
  pub expired: String,
  pub no_prefix: String,
  pub downgrade: String,
  pub downgrade_forced: String,
}

crate::part!(pub SESSION_WORDS: SessionWords = "sessions");
