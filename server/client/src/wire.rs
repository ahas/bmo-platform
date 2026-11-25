//! The common wire format (`base/chat/docs/conventions.md`): the body every failure carries, and the failure a
//! call came to.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The body every non-2xx answer of the service's own routes carries: a stable kebab-case code a caller decides
/// on, and one English sentence for a person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WireError {
  pub code: String,
  pub error: String,
}

/// The body sumida's own session routes refuse with: its code, the status as a number, and a reason written in
/// debug builds only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionError {
  pub code: String,
  pub status_code: u16,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reason: Option<String>,
}

/// Why a call came to no answer of the route's shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientFailure {
  /// The service answered with a failure: its status, and its error shape where it gave one.
  Answered {
    status: u16,
    answer: Option<WireError>,
  },
  /// Nothing answered, or the connection did not carry the call.
  Unreachable(String),
  /// The service answered in a shape the route does not give.
  Unreadable(String),
}

impl std::fmt::Display for ClientFailure {
  fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Answered {
        status,
        answer: Some(answer),
      } => write!(out, "the account service answered {status} {}: {}", answer.code, answer.error),
      Self::Answered {
        status,
        answer: None,
      } => write!(out, "the account service answered {status}"),
      Self::Unreachable(why) => write!(out, "the account service did not answer: {why}"),
      Self::Unreadable(why) => write!(out, "the account service's answer did not read: {why}"),
    }
  }
}

impl std::error::Error for ClientFailure {}

/// What a call came to.
pub type Answer<T> = Result<T, ClientFailure>;
