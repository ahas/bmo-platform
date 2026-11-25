//! A supervisor's background work as room state (docs/api.md, 「background-work」): what every supervisor's
//! program still runs after the turn that launched it, read from the background records as they stand.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use serde_json::json;

use super::{Call, Refused, Route, answer};
use crate::watches::background_work::running_work;

/// The background work still running, of one room where the call names one.
pub struct BackgroundWorkList;

impl Route for BackgroundWorkList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/background-work";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let room = match call.query("roomId") {
      None => None,
      Some(room) => Some(
        room
          .trim()
          .parse::<i64>()
          .map_err(|_| Refused::new(StatusCode::BAD_REQUEST, "roomId is a whole number"))?,
      ),
    };
    let work = running_work(&call.data_root(), room);

    Ok(answer(StatusCode::OK, json!({ "items": work, "more": false })))
  }
}

crate::moved!(BackgroundWorkList);
