//! What the service asks of the receiver (docs/boundary.md, 「Calls the server makes」): the sender of a
//! message standing in a room, which a message check and a grant are made on. The receiver keeps the rooms
//! and their messages; the service reads none of them itself.

use axum::http::StatusCode;
use serde::Deserialize;

use crate::Refusal;

/// The receiver, as the service asks it.
#[async_trait::async_trait]
pub trait Receiver: Send + Sync {
  /// The sender of a message standing in a room: none where the receiver knows no sender of it, and a
  /// refusal with 404 where no such message stands there.
  async fn sender_of(&self, room: i64, message: i64) -> Result<Option<i64>, Refusal>;
}

/// The refusal of a message the receiver does not hold in a room.
pub fn no_such_message(room: i64, message: i64) -> Refusal {
  Refusal::absent(format!("no message {message} stands in room {room}"))
}

/// The receiver over HTTP, at its base address with its token.
pub struct ReceiverClient {
  base: String,
  token: String,
  http: reqwest::Client,
}

impl ReceiverClient {
  /// A client of the receiver at a plain `http:` base address; any other address is refused.
  pub fn new(base: &str, token: &str) -> Result<Self, String> {
    if !base.starts_with("http://") {
      return Err(format!("the receiver's address {base} is not an http: address"));
    }

    Ok(Self {
      base: base.trim_end_matches('/').to_owned(),
      token: token.to_owned(),
      http: reqwest::Client::new(),
    })
  }
}

#[derive(Deserialize)]
struct Message {
  sender: Option<i64>,
}

fn unreached(why: impl std::fmt::Display) -> Refusal {
  Refusal::new(
    StatusCode::BAD_GATEWAY,
    format!("the receiver did not answer a message's sender: {why}"),
  )
}

#[async_trait::async_trait]
impl Receiver for ReceiverClient {
  async fn sender_of(&self, room: i64, message: i64) -> Result<Option<i64>, Refusal> {
    let answer = self
      .http
      .get(format!("{}/rooms/{room}/messages/{message}", self.base))
      .bearer_auth(&self.token)
      .send()
      .await
      .map_err(unreached)?;

    match answer.status() {
      reqwest::StatusCode::NOT_FOUND => Err(no_such_message(room, message)),
      status if status.is_success() => Ok(answer.json::<Message>().await.map_err(unreached)?.sender),
      status => Err(unreached(format!("it answered {status}"))),
    }
  }
}

/// The receiver where the settings name none: every ask is refused, saying so.
pub struct NoReceiver;

#[async_trait::async_trait]
impl Receiver for NoReceiver {
  async fn sender_of(&self, _room: i64, _message: i64) -> Result<Option<i64>, Refusal> {
    Err(Refusal::new(
      StatusCode::SERVICE_UNAVAILABLE,
      "no receiver is named in the settings, so no message's sender can be read",
    ))
  }
}
