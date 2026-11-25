#![allow(
  dead_code,
  unused_imports,
  unused_variables,
  clippy::needless_lifetimes,
  clippy::let_and_return,
  clippy::unnecessary_to_owned
)]
//! A room forgotten as its owner removes its registration (docs/api.md, 「Sessions」, forgetting a room): a
//! dry run answers what would go and what stays, table by table; the forgetting answers what went.

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::{Call, Refused, Route, answer, whole};

/// A room's rows forgotten, or counted.
pub struct RoomForget;

impl Route for RoomForget {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/room-holdings/{roomId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let room: i64 = call
      .param("roomId")
      .and_then(|room| room.parse().ok())
      .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "roomId (a whole number) is required"))?;
    let dry_run = call.query("dryRun").is_some_and(|dry| dry == "true");
    let forgotten = crate::rooms::forget(call.store(), room, dry_run).await?;
    let member = serde_json::to_value(&forgotten).map_err(Refused::failed)?;

    if dry_run {
      return Ok(answer(StatusCode::OK, member));
    }

    Ok(StatusCode::NO_CONTENT.into_response())
  }
}

crate::moved!(RoomForget);

/// What the agency holds of a room, counted without letting go.
pub struct RoomHolding;

impl Route for RoomHolding {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/room-holdings/{roomId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let room: i64 = call
      .param("roomId")
      .and_then(|room| room.parse().ok())
      .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "roomId (a whole number) is required"))?;
    let held = crate::rooms::forget(call.store(), room, true).await?;
    let holdings = serde_json::to_value(&held).map_err(Refused::failed)?;

    Ok(answer(StatusCode::OK, serde_json::json!({ "roomId": room, "holdings": holdings })))
  }
}

crate::moved!(RoomHolding);
