//! A worker's dev server as `bmo dev-server`, `bmo dev-server list` and the dev-server pane tell it
//! (docs/worker-bench.md, 「Dev server」), each line with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevServerWords {
  pub server: Server,
  pub listing: Listing,
  pub pane: Pane,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
  pub no_worker: String,
  pub not_up: String,
  pub not_down: String,
  pub down: String,
  pub no_command: String,
  pub no_job: String,
  pub already_up: String,
  pub not_up_in_time: String,
  pub up: String,
  pub started: String,
  pub restarted: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listing {
  pub head: String,
  pub worker: String,
  pub job_running: String,
  pub no_job: String,
  pub expired: String,
  pub left_head: String,
  pub none: String,
  pub total: String,
  pub nothing_to_end: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pane {
  pub head: String,
  pub leftover: String,
  pub start: String,
  pub ended: String,
  pub quick: String,
  pub job_over: String,
  pub not_asked: String,
  pub no_command: String,
  pub closing: String,
  pub stopping: String,
  pub expired: String,
  pub idle: String,
  pub short: String,
  pub restarting: String,
  pub moved: String,
  pub starting_now: String,
  pub pane_closed: String,
}

crate::part!(pub DEV_SERVER_WORDS: DevServerWords = "dev-servers");
