//! What the owner and a room read of the health of the chat agent's own chat session (`lang/<language>/chat-health.yaml`):
//! why a run died and where its record is, the bot's own line where the chat agent cannot answer an order, the outage's
//! rest, and the fold of a full session.

use serde::Deserialize;

/// Why a run died, by what was read where it ended.
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

/// The bot's own line where the chat agent cannot answer an order.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LastResort {
  pub journal: String,
  pub repeated: String,
  pub speaking: String,
}

/// What the owner is told as an outage's rest begins.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rest {
  pub outage: String,
}

/// A full session folded.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fold {
  pub going: String,
  pub failed: String,
  pub failed_owner: String,
  pub woken: String,
  pub deferred: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatHealthWords {
  pub causes: Causes,
  pub account: Account,
  pub last_resort: LastResort,
  pub rest: Rest,
  pub fold: Fold,
}

crate::part!(pub CHAT_HEALTH_WORDS: ChatHealthWords = "chat-health");
