//! The executions across the boundary (docs/api.md, 「Workers」): every run of an agent's process agency
//! recorded, listed or read by its id, each with whether a standing cancellation covers it, and the
//! cancellations a stop of a name stood taken back on their own.

use std::collections::BTreeSet;

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use bmo_chat::{ErrorCode, WireError};
use serde_json::{Map, Value, json};

use super::{Call, Refused, Route, answer};
use crate::queue::cancels;

// The fields an execution answers by name; every other field it recorded stands in its details.
const NAMED: [&str; 7] = ["id", "parent", "root", "line", "sharedPane", "startedAt", "endedAt"];

// The id the path names.
fn id_of(call: &Call) -> Result<&str, Refused> {
  call
    .param("executionId")
    .filter(|id| !id.is_empty())
    .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "an execution's id is required"))
}

fn ended(body: &Value) -> bool {
  body.get("endedAt").is_some_and(|ended| !ended.is_null())
}

// One execution as it is answered: the named fields, its outcome where it recorded an object, whether a
// standing cancellation covers it, and every other field as it stands.
fn shown(body: &Value, cancelled: &BTreeSet<String>) -> Value {
  let field = |key: &str| body.get(key).cloned().unwrap_or(Value::Null);
  let outcome = body.get("outcome").filter(|outcome| outcome.is_object()).cloned();
  let details: Map<String, Value> = body
    .as_object()
    .into_iter()
    .flatten()
    .filter(|(key, _)| !NAMED.contains(&key.as_str()) && (key.as_str() != "outcome" || outcome.is_none()))
    .map(|(key, value)| (key.clone(), value.clone()))
    .collect();
  let id = body.get("id").and_then(Value::as_str).unwrap_or("");

  json!({
    "id": id,
    "parent": field("parent"),
    "root": field("root"),
    "line": field("line"),
    "sharedPane": body.get("sharedPane").and_then(Value::as_bool).unwrap_or(false),
    "startedAt": field("startedAt"),
    "endedAt": field("endedAt"),
    "outcome": outcome,
    "cancelled": cancelled.contains(id),
    "details": details,
  })
}

/// The executions, the latest first; only those not yet ended where `open` is true.
pub struct ListExecutions;

impl Route for ListExecutions {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/executions";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let open = call.query("open").is_some_and(|open| open == "true");
    let rows = cancels::executions(call.store()).await?;
    let cancelled = cancels::standing(call.store(), &rows).await?;
    let listed: Vec<Value> = rows
      .iter()
      .filter(|body| !open || !ended(body))
      .map(|body| shown(body, &cancelled))
      .collect();

    Ok(answer(StatusCode::OK, json!({ "items": listed, "more": false })))
  }
}

crate::moved!(ListExecutions);

/// One execution by its id.
pub struct ReadExecution;

impl Route for ReadExecution {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/executions/{executionId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let id = id_of(&call)?;
    let rows = cancels::executions(call.store()).await?;
    let Some(body) = rows.iter().find(|body| body.get("id").and_then(Value::as_str) == Some(id)) else {
      return Ok(
        WireError::new(ErrorCode::NotFound, format!("no execution {id} is recorded")).into_response(),
      );
    };
    let cancelled = cancels::standing(call.store(), &rows).await?;

    Ok(answer(StatusCode::OK, shown(body, &cancelled)))
  }
}

crate::moved!(ReadExecution);

/// The cancellations a stop of a name stood over it and the executions it owns, taken back.
pub struct DropCancels;

impl Route for DropCancels {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/executions/{executionId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let id = id_of(&call)?;
    let dropped = cancels::take_back(call.store(), id).await?;

    Ok(answer(
      StatusCode::OK,
      json!({ "id": id, "runId": Value::Null, "command": "", "state": "cancelled", "cancelled": false, "dropped": dropped }),
    ))
  }
}

crate::moved!(DropCancels);
