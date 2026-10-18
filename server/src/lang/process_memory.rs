//! What `bmo machine memory` prints (docs/agent-control.md, 「What each program holds」), each line with the
//! `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessMemoryWords {
  pub machine: String,
  pub counted: String,
  pub runtimes: String,
  pub programs: String,
  pub none: String,
}

crate::part!(pub PROCESS_MEMORY: ProcessMemoryWords = "process-memory");
