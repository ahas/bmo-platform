//! A line a person typed straight into an agent's pane (docs/api.md, 「Hooks」, the submit hook): the
//! program's submit hook asks agency, and agency asks the consumer that owns the agent to compose what the
//! line goes on with. The default composes nothing, and the line goes on as typed.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use serde::Serialize;
use serde_json::{Value, json};

use super::{Call, Refused, Route, answer};
use crate::hooks::HookPoint;

/// The line typed, and the session of the program it was typed into.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Typed {
  pub text: String,
  pub session_id: String,
}

/// The point a typed line's composition is asked at: the context the line goes on with, or none.
pub struct TypedLine;

impl HookPoint for TypedLine {
  type Ask = Typed;
  type Answer = Option<String>;
  const NAME: &'static str = "typed-line";
  /// The consumer that owns the agent registers here once, at its own start, so the registration stands
  /// again as agency starts: an unregistered point leaves every typed line uncomposed.
  const KEPT: bool = true;

  fn default_answer(_typed: Typed) -> Option<String> {
    None
  }
}

crate::wired!(TypedLine);

/// The composition of a typed line, as the consumer answers it.
pub struct TypedLineRoute;

impl Route for TypedLineRoute {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/turn-contexts";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let field = |name: &str| body.get(name).and_then(Value::as_str).map(str::to_owned);
    let (Some(text), Some(session_id)) = (field("text"), field("sessionId")) else {
      return Err(Refused::new(StatusCode::BAD_REQUEST, "text and sessionId (strings) are required"));
    };

    match call.ground().hooks.ask::<TypedLine>(Typed { text, session_id }).await {
      Some(context) => Ok(answer(StatusCode::OK, json!({ "context": context }))),
      None => Err(Refused::new(StatusCode::CONFLICT, "no consumer composes a typed line")),
    }
  }
}

crate::moved!(TypedLineRoute);
