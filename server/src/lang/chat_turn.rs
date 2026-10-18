//! A turn of the chat agent's chat as a person sees it go (`lang/<language>/chat-turn.yaml`).

use serde::Deserialize;

/// The title a round's turn stands under, and a fold's.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatTurnWords {
  pub title: String,
  pub fold: String,
}

crate::part!(pub CHAT_TURN_WORDS: ChatTurnWords = "chat-turn");
