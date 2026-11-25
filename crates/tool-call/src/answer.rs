//! What a call is answered with: the protocol's tool result, its text pieces and whether it is a refusal.

use serde::{Deserialize, Serialize};

/// One piece of an answer: agency's tools answer text alone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Content {
  Text { text: String },
}

/// A tool's answer. A refusal is an answer marked as an error, which the model reads as it reads any other.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
  pub content: Vec<Content>,
  #[serde(rename = "isError", default)]
  pub is_error: bool,
}

impl Answer {
  pub fn text(text: impl Into<String>) -> Self {
    Self {
      content: vec![Content::Text { text: text.into() }],
      is_error: false,
    }
  }

  /// A tool that could not answer: the reason, marked as an error.
  pub fn failure(reason: impl std::fmt::Display) -> Self {
    Self {
      content: vec![Content::Text {
        text: format!("ERROR: {reason}"),
      }],
      is_error: true,
    }
  }

  /// The text of every piece, joined, as a reader of text alone reads the answer.
  pub fn plain(&self) -> String {
    self
      .content
      .iter()
      .map(|Content::Text { text }| text.as_str())
      .collect::<Vec<_>>()
      .join("\n")
  }
}
