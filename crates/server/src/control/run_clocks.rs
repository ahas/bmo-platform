//! A run's clock across the boundary (docs/api.md, 「Workers」): the clock of the runs standing under a name,
//! the asks open on runs past their clock and a verdict on one.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use serde_json::{Value, json};

use super::{Call, Refused, Route, answer};
use crate::run_clock::{self, NotSettled, Verdict};

fn bad(error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, error)
}

fn named(call: &Call, field: &str) -> Result<String, Refused> {
  call
    .query(field)
    .map(|value| value.trim().to_owned())
    .filter(|value| !value.is_empty())
    .ok_or_else(|| bad(format!("{field} is required")))
}

/// The clock of the runs standing under a name.
pub struct RunClock;

impl Route for RunClock {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/runs/clock";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let name = named(&call, "run")?;
    let clock = run_clock::clock_of(call.store(), &name).await?;

    Ok(answer(StatusCode::OK, json!(clock)))
  }
}

/// The asks open on runs past their clock, and the supervisor's verdict on one.
pub struct RunOvertime;

impl Route for RunOvertime {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/overtime-requests";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let asks = run_clock::open_asks(call.store()).await?;

    Ok(answer(StatusCode::OK, json!({ "items": asks, "more": false })))
  }
}

crate::moved!(RunOvertime);

/// A verdict on an ask.
pub struct RunOvertimeVerdict;

/// A verdict as a body names it, or why it does not read as one.
pub fn verdict_read(body: &Value) -> Result<(i64, Verdict, String), Refused> {
  let ask = body.get("ask").and_then(Value::as_i64).ok_or_else(|| bad("ask (number) is required"))?;
  let verdict = match body.get("verdict").and_then(Value::as_str) {
    Some("keep") => Verdict::Keep(match body.get("minutes") {
      None | Some(Value::Null) => None,
      Some(minutes) => Some(
        minutes
          .as_i64()
          .filter(|minutes| *minutes > 0)
          .ok_or_else(|| bad("minutes is a whole number above nothing"))?,
      ),
    }),
    Some("cut") => Verdict::Cut,
    _ => return Err(bad("verdict is keep or cut")),
  };
  let why = body.get("why").and_then(Value::as_str).unwrap_or_default().to_owned();

  Ok((ask, verdict, why))
}

impl Route for RunOvertimeVerdict {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/overtime-requests/{requestId}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let ask: i64 = call
      .param("requestId")
      .and_then(|id| id.parse().ok())
      .or_else(|| body.get("ask").and_then(Value::as_i64))
      .ok_or_else(|| bad("requestId is a number"))?;
    let (_, verdict, why) = verdict_read(&json!({
      "ask": ask,
      "verdict": body.get("verdict"),
      "minutes": body.get("minutes"),
      "why": body.get("why"),
    }))?;

    match run_clock::settle(call.store(), call.events(), ask, &verdict, &why).await {
      Ok(settled) => Ok(answer(StatusCode::OK, json!(settled))),
      Err(NotSettled::Unknown) => {
        Err(Refused::new(StatusCode::NOT_FOUND, format!("no ask is numbered {ask}")))
      }
      Err(NotSettled::Closed) => Ok(answer(
        StatusCode::CONFLICT,
        json!({ "code": "ask-closed", "error": format!("ask {ask} is closed: its run was settled or ended") }),
      )),
      Err(NotSettled::Store(why)) => Err(why.into()),
    }
  }
}

crate::moved!(RunOvertimeVerdict);
