//! The handoff a job's session leaves as the job's work moves to another session (docs/sessions.md, 「The
//! handoff a job's session leaves」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobHandoff {
  /// The turn a session going away is asked.
  pub instruction: String,
  /// The handoff ahead of the next session's prompt, with `{handoff}` and `{prompt}`.
  pub opened: String,
  /// The handoff written from the run log, with `{why}` and `{lines}`.
  pub from_log: String,
  /// How many of the job's last run log lines stand in for a session that cannot be asked.
  pub log_lines: u64,
  /// How many characters of each line.
  pub line_chars: usize,
  /// The note's writer.
  pub actor: String,
  /// The `kind` label value that marks a handoff note.
  pub kind: String,
  /// Why a session could not be asked.
  pub why: Why,
}

/// Why a session could not be asked, as its handoff note says it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Why {
  pub road_stopped: String,
  pub no_answer: String,
  pub stopped_idle: String,
  pub failed: String,
  pub record_gone: String,
}

crate::part!(pub JOB_HANDOFF: JobHandoff = "job-handoff.toml");
