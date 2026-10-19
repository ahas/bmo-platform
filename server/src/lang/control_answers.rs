//! What the control port's routes answer the console with where they refuse a change, and the words the
//! console shows of Laya (docs/control-port.md, 「How a request is answered」), each line with the
//! `{name}` marks its file names.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlAnswers {
  pub request: Request,
  pub fallback: Fallback,
  pub accounts: Accounts,
  pub gates: Gates,
  pub laya: Laya,
  pub workers: Workers,
}

/// A worker placed on a node or brought back, and a worker's own sampling.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workers {
  pub back_here_path: String,
  pub sampling: Sampling,
}

/// What a save of a worker's own sampling is refused for.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sampling {
  pub no_agent: String,
  pub not_worker: String,
  pub follows_base: String,
  pub shape: String,
  pub unknown: String,
}

/// A request no route can read.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
  pub not_json: String,
  pub action: String,
}

/// The fallback roads and the run modes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fallback {
  pub no_laya: String,
  pub no_provider: String,
  pub models_unread: String,
  pub unknown_key: String,
  pub models_retired: String,
  pub order: String,
  pub provider_switch: String,
  pub run_mode: String,
  pub tier_provider: String,
}

/// The Claude accounts.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Accounts {
  pub switch: String,
  pub run_on: String,
  pub credential_kind: String,
  pub credential_path: String,
  pub name_needed: String,
  pub which_to_change: String,
  pub which_to_remove: String,
  pub choice: String,
  pub unknown: String,
  pub off: String,
  pub no_id: String,
  pub id_shape: String,
  pub id_taken: String,
  pub name_empty: String,
  pub name_long: String,
  pub name_taken: String,
  pub path_empty: String,
  pub path_relative: String,
  pub path_missing: String,
  pub path_not_directory: String,
  pub path_not_file: String,
  pub order: String,
  pub last_off: String,
  pub last_gone: String,
  pub chosen_off: String,
}

/// A gate's options.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gates {
  pub not_gate: String,
  pub own_llm: String,
  pub nothing: String,
  pub fallback: String,
  pub switch: String,
  pub always_on: String,
  pub timeout: String,
  pub mode: String,
  pub tau: String,
  pub no_question: String,
  pub no_safety: String,
  pub no_checkpoint: String,
  pub needs_tau: String,
}

/// The words the console and a sentence use of Laya: each mode, and each way its server last answered.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Laya {
  pub modes: BTreeMap<String, String>,
  pub states: BTreeMap<String, String>,
}

impl Laya {
  /// A mode as a sentence names it, or the mode itself where the file has no word for it.
  pub fn mode_word<'a>(&'a self, mode: &'a str) -> &'a str {
    self.modes.get(mode).map_or(mode, String::as_str)
  }
}

crate::part!(pub CONTROL_ANSWERS: ControlAnswers = "control-answers");
