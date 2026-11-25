//! How a run is stopped from outside (docs/agent-control.md, 「Stopping」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunStop {
  /// What a run in a pane is told when it is being stopped.
  pub instruction: String,
  /// How long the interrupt of a turn waits for the turn to end, in milliseconds.
  pub interrupt_wait_ms: u64,
  /// How often the interrupt looks whether the turn has ended meanwhile, in milliseconds.
  pub interrupt_look_ms: u64,
}

crate::part!(pub RUN_STOP: RunStop = "run-stop.toml");
