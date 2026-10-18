//! What the gates' evaluation set says from a shell (docs/gate-eval.md), each line with the `{name}` marks its
//! file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateEval {
  pub usage: String,
  pub refusals: Refusals,
  pub started: String,
  pub status: Status,
  pub report: Report,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Refusals {
  pub command: String,
  pub unknown_command: String,
  pub count: String,
  pub gate: String,
  pub road: String,
  pub roads_joined: String,
  pub running: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
  pub set: String,
  pub head: String,
  pub sanitizer: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
  pub none: String,
  pub head: String,
}

crate::part!(pub GATE_EVAL: GateEval = "gate-eval");
