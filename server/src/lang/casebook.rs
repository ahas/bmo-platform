//! How one case of the formatting gate's casebook reads (docs/output-gate.md, 「The case collection」),
//! each line with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Casebook {
  pub room: String,
  pub no_room: String,
  pub message: String,
  pub no_message: String,
  pub code_made: String,
  pub attempt: String,
  pub reason: String,
  pub text: String,
  pub markup: String,
}

crate::part!(pub CASEBOOK: Casebook = "casebook");
