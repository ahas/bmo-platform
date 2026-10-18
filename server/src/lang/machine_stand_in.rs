//! What `bmo machine stand-in` says (docs/rust-migration.md, 「The target」), each line with the `{name}` marks
//! its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineStandInWords {
  pub usage: Vec<String>,
  pub copying_file: String,
  pub copying_database: String,
  pub swept: String,
  pub building: String,
  pub placing: String,
  pub standing: String,
  pub loading: String,
  pub unanswered: String,
  pub answered: String,
  pub supervised: String,
  pub memory: String,
  pub weighed: String,
  pub agent_host: String,
  pub tool_host: String,
  pub followers: String,
  pub total: String,
  pub taking_down: String,
  pub home_stands: String,
}

crate::part!(pub MACHINE_STAND_IN: MachineStandInWords = "machine-stand-in");
