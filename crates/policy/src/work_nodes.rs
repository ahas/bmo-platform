//! The workplaces on the work nodes (docs/work-nodes.md): how often one not ready is checked again.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkNodes {
  /// How long after a workplace's last setup, sync or check that left it not ready it is checked again, in
  /// seconds.
  pub check_again_seconds: i64,
}

crate::part!(pub WORK_NODES: WorkNodes = "work-nodes.toml");
