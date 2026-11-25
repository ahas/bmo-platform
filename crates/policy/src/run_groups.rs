//! How what a run leaves running is taken down when the run ends (docs/run-groups.md).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunGroups {
  /// How long a run's processes are given to end on TERM before what still stands is killed.
  pub grace_ms: u64,
  /// How long what was killed is waited on to be gone before the record is left for the sweep.
  pub kill_wait_ms: u64,
  /// How often the run's processes are read again while they are waited on.
  pub look_ms: u64,
}

crate::part!(pub RUN_GROUPS: RunGroups = "run-groups.toml");
