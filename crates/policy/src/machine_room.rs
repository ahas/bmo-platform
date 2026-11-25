//! This machine's room (docs/agent-control.md, 「Room on the machine」): the machine's five limits over
//! the standing the node service reports, and agency's two job-count limits.

use serde::Deserialize;

/// One limit over the machine's own reading: its default, and the reason a launch it holds is written as.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineLimit {
  /// The limit where the settings table holds no positive number for it.
  pub default: f64,
  /// The reason, with `{value}` and `{limit}` marked, and `{threads}` too where the reading is per thread.
  pub held: String,
}

/// One job-count limit: its default and the reason a launch it holds is written as.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountLimit {
  /// The limit where the settings table holds no positive number for it; none where only the owner's own
  /// setting sets one.
  #[serde(default)]
  pub default: Option<u64>,
  /// The reason, with `{jobs}` and `{limit}` marked.
  pub held: String,
}

/// The clocks of the held launch's watch and of the pressure stream.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineClocks {
  pub held_watch_seconds: u64,
  pub pressure_reopen_seconds: u64,
  pub item_sweep_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineRoom {
  /// Memory `full` over the last minute, in percent.
  pub launch_memory_full_pct: MachineLimit,
  /// `MemAvailable`, in gigabytes.
  pub launch_memory_available_gb: MachineLimit,
  /// The one-minute load divided by the thread count.
  pub launch_load_per_thread: MachineLimit,
  /// The free space of the data root's disk, in gigabytes.
  pub launch_disk_free_gb: MachineLimit,
  /// What one job is reckoned to grow to, in gigabytes, taken ahead from `MemAvailable`.
  pub launch_job_memory_gb: MachineLimit,
  /// The jobs at work on this machine.
  pub launch_jobs: CountLimit,
  /// The jobs at work whose place runs a browser.
  pub launch_browser_jobs: CountLimit,
  pub clocks: MachineClocks,
}

crate::part!(pub MACHINE_ROOM: MachineRoom = "machine-room.toml");
