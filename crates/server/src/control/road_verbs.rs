//! The verbs a room's road performs (docs/api.md, 「Hooks」, `road-verbs`): a procedure read in a room's turn
//! is held to them (docs/tools.md, 「The procedures」), and only the consumer that holds the rooms knows
//! them, so agency asks it. The default answers none.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use serde::Serialize;
use serde_json::json;

use super::{Call, Refused, Route, answer};
use crate::hooks::HookPoint;

/// The room a turn answers, and the road it answers it on where the turn names one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Road {
  pub chat: i64,
  pub via: Option<String>,
}

/// The point a road's verbs are asked at: every verb the road performs.
pub struct RoadVerbs;

impl HookPoint for RoadVerbs {
  type Ask = Road;
  type Answer = Vec<String>;
  const NAME: &'static str = "road-verbs";

  fn default_answer(_road: Road) -> Vec<String> {
    vec![]
  }
}

crate::wired!(RoadVerbs);

/// The verbs a room's road performs, as the consumer answers them.
pub struct RoadVerbsRoute;

impl Route for RoadVerbsRoute {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/road-verbs";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let Some(chat) = call.query("chat").and_then(|chat| chat.trim().parse::<i64>().ok()) else {
      return Err(Refused::new(StatusCode::BAD_REQUEST, "chat (a whole number) is required"));
    };
    let via = call.query("via").map(|via| via.trim().to_owned()).filter(|via| !via.is_empty());
    let can = call.ground().hooks.ask::<RoadVerbs>(Road { chat, via }).await;

    Ok(answer(StatusCode::OK, json!({ "can": can })))
  }
}

crate::moved!(RoadVerbsRoute);
