//! What the harness reads, writes and starts on this machine (docs/harness-machine.md): the registered
//! models' key files, the gateway's status, the gateway's members, an engine's health, the arbiter's master
//! a card is asked of, a model's use on its engine, and a supervisor's system prompt. Nothing here reads the archive: each side reads its own and
//! hands over what the rows hold.

use serde::Serialize;

pub mod card_master;
pub mod engine_health;
pub mod gateway;
pub mod keys;
pub mod members;
pub mod model_use;
pub mod supervisor_prompt;

/// A refusal the owner reads: the field it is about, and why, in the owner's language.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Refusal {
  pub field: String,
  pub error: String,
}

impl Refusal {
  pub fn new(field: &str, error: impl Into<String>) -> Self {
    Self {
      field: field.to_owned(),
      error: error.into(),
    }
  }
}

/// Why something asked of the machine was not done: refused as the owner reads it, or failed.
#[derive(Debug)]
pub enum Fault {
  Refused(Refusal),
  Failed(String),
}

impl From<std::io::Error> for Fault {
  fn from(error: std::io::Error) -> Self {
    Self::Failed(error.to_string())
  }
}

/// A text's length as the TypeScript side counts it, in UTF-16 units, so a limit reads alike on both sides.
pub fn length(text: &str) -> usize {
  text.encode_utf16().count()
}
