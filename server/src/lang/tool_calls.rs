//! What `bmo job tool-calls` says of a job's own thread (docs/jobs.md), with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCalls {
  pub no_job: String,
  pub no_worker: String,
  pub session: String,
  pub nothing_to_read: String,
  pub job: String,
  pub last_of: String,
  pub still_running: String,
  pub record: String,
  pub lines: String,
  pub no_calls: String,
  pub no_calls_in_window: String,
  pub total: String,
  pub open: String,
  pub tool: String,
  pub tests: String,
  pub no_tests: String,
  pub no_tests_in_window: String,
  pub order: String,
  pub order_top: String,
  pub call: String,
  pub call_open: String,
  pub unnamed_tool: String,
}

crate::part!(pub TOOL_CALLS: ToolCalls = "tool-calls");
