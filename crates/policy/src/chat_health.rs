//! What a run that died is said to have come to, and the notice of one that came back (docs/policy.md):
//! the account of a dead run, its causes, where a turn was, and the line said once a run answers again.

use serde::Deserialize;

/// Where a turn was, as a line names it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Places {
  pub owner_chat: String,
  pub room: String,
}

/// The notice of a run that came back after a failure.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recovered {
  pub failed: String,
  pub repeated: String,
  pub cause: String,
  pub answering: String,
  pub intent: String,
  pub kind: String,
  pub ttl_minutes: i64,
}

/// The account of a dead run and its pieces.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
  pub line: String,
  pub broke: String,
  pub silent: String,
  pub room: String,
  pub said: String,
  pub action: String,
  pub detail: String,
  pub record: String,
  pub run_log: String,
  pub unnamed_tool: String,
}

/// Why a run died, by how it ended.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Causes {
  pub exit: String,
  pub signal: String,
  pub error_result: String,
  pub silent: String,
  pub transport: String,
  pub unknown: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatHealth {
  pub places: Places,
  pub recovered: Recovered,
  pub account: Account,
  pub causes: Causes,
}

crate::part!(pub CHAT_HEALTH: ChatHealth = "chat-health.toml");
