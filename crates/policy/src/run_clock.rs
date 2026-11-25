//! The clock a run of work is held to and the ask put past it (docs/agent-control.md, 「A clock asks, it
//! never cuts」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunClock {
  /// How often a run waiting on a verdict looks for one.
  pub look_every_seconds: u64,
  /// The longest one keep gives.
  pub longest_keep_seconds: u64,
  /// How long a clock's rows are kept once its run has ended.
  pub kept_days: u64,
  /// The command a verdict is given with.
  pub verb: String,
  /// Why a run cut on its clock failed: the supervisor's call.
  pub cut_by_agent: String,
}

crate::part!(pub RUN_CLOCK: RunClock = "run-clock.toml");
