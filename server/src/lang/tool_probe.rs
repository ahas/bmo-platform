//! The tool probe's reading as `bmo tool probe` prints it (docs/toolbook.md, 「Measuring」), each line with
//! the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolProbe {
  pub title: String,
  pub columns: Columns,
  pub ways: Ways,
  pub unknown_line: String,
}

/// The reading's columns, in the order they stand.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Columns {
  pub line: String,
  pub tools_shown: String,
  pub first_request: String,
  pub input: String,
  pub cache_write: String,
  pub cache_read: String,
  pub tool: String,
  pub way: String,
  pub to_result: String,
  pub whole: String,
  pub exit: String,
}

/// How a session reached its tool.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ways {
  pub unreached: String,
  pub toolbook: String,
  pub direct: String,
}

crate::part!(pub TOOL_PROBE: ToolProbe = "tool-probe");
