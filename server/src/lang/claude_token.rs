//! One Claude account's token for the work nodes, as `bmo node claude-token` says it makes and keeps it
//! (docs/work-nodes.md, 「The logins on a node」), each line with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaudeTokenWords {
  pub unknown_account: String,
  pub making: String,
  pub approve: String,
  pub approve_as: String,
  pub sign_in_failed: String,
  pub paste: String,
  pub stopped: String,
  pub refused: String,
  pub not_a_token: String,
  pub cut: String,
  pub kept: String,
  pub from_next_run: String,
}

crate::part!(pub CLAUDE_TOKEN: ClaudeTokenWords = "claude-token");
