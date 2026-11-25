#![allow(
  dead_code,
  unused_imports,
  unused_variables,
  clippy::needless_lifetimes,
  clippy::let_and_return,
  clippy::unnecessary_to_owned
)]
//! The run ledger across the boundary (docs/api.md, 「Runs and log lines」): a run opened, read by its id,
//! listed by the search it carried or by agent, room, round and names, closed or given the message its
//! answer went out as, and its runner launched. A run the ledger holds no row of is
//! answered in the base chat library's code for nothing kept at the address a path names.

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use bmo_chat::{ErrorCode, WireError};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use store::{all, one};

use super::supervisor_reset::to_its_end;
use super::{Call, Refused, Route, answer};
use crate::run_ledger::{self, Closing, Launch, NotClosed, NotLaunched, Opening, SearchId};

fn bad(error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, error)
}

/// One run as the boundary answers it.
fn member(run: &run_ledger::Run) -> Value {
  json!({
    "id": run.id,
    "name": format!("{}:{}", run.agent, run.unit),
    "agentName": run.agent.clone(),
    "unitName": run.unit.clone(),
    "title": run.title.clone(),
    "workerName": Value::Null,
    "jobId": Value::Null,
    "state": if run.finished_at.is_some() { "closed" } else { "open" },
    "startedAt": run.started_at,
    "endedAt": run.finished_at,
    "outcome": run.outcome.clone(),
    "capEndsAt": Value::Null,
    "keptUntil": Value::Null,
    "contextTokens": run.context_tokens,
    "spentTokens": run.spent.as_ref().map(run_ledger::Spent::total),
    "usage": run.spent.as_ref().map(|spent| json!({
      "inputTokens": spent.input_tokens,
      "outputTokens": spent.output_tokens,
      "cacheReadTokens": spent.cache_read_tokens,
      "cacheWriteTokens": spent.cache_write_tokens,
    })),
    "counts": run.tools.map_or_else(|| json!({}), |tools| json!({ "tools": tools })),
    "roomId": run.chat_id,
    "round": run.round.clone(),
    "messageId": run.message_id,
  })
}

// The run the path names by its id.
fn id_of(call: &Call) -> Result<i64, Refused> {
  call
    .param("runId")
    .and_then(|id| id.parse().ok())
    .ok_or_else(|| bad("the run's id is a whole number"))
}

fn nothing_held(id: i64) -> Response {
  WireError::new(ErrorCode::NotFound, format!("the ledger holds no run {id}")).into_response()
}

async fn read_body<T: DeserializeOwned>(call: &mut Call, what: &str) -> Result<T, Refused> {
  serde_json::from_value(call.body().await?).map_err(|error| bad(format!("{what}: {error}")))
}

/// A run opened.
pub struct OpenRun;

impl Route for OpenRun {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/runs";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let opening: Opening = read_body(&mut call, "a run's opening").await?;
    let id = run_ledger::open(call.store(), &opening).await?;
    let run = run_ledger::read(call.store(), id)
      .await?
      .ok_or_else(|| Refused::failed("the run was opened but not read"))?;

    Ok(answer(StatusCode::CREATED, member(&run)))
  }
}

crate::moved!(OpenRun);

/// The runs that carried a search, or the runs a list narrows to by agent, room, round and names.
pub struct SearchRuns;

impl Route for SearchRuns {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/runs";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    if let Some(name) = call.query_names().into_iter().find(|name| !FILTERS.contains(&name.as_str())) {
      return Err(bad(format!("the runs are not narrowed by {name}")));
    }

    let page = match call.query("search").filter(|search| !search.trim().is_empty()) {
      Some(search) => {
        let mut runs = Vec::new();

        for id in run_ledger::of_search(call.store(), &SearchId::Text(search)).await?.into_iter().rev() {
          runs.extend(run_ledger::read(call.store(), id).await?);
        }

        run_ledger::Page { runs, more: false }
      }
      None => {
        let search = list_search(&call)?;

        run_ledger::search(call.store(), &search).await?
      }
    };
    let items: Vec<Value> = page.runs.iter().map(member).collect();

    Ok(answer(StatusCode::OK, json!({ "items": items, "more": page.more })))
  }
}

/// Every name a list of runs is asked by (docs/api.md, 「runs」).
const FILTERS: [&str; 9] =
  ["search", "live", "agentName", "roomId", "round", "names", "after", "before", "limit"];

// What a list of runs is narrowed by, as the query names it.
fn list_search(call: &Call) -> Result<run_ledger::Search, Refused> {
  let named =
    |name: &str| call.query(name).map(|value| value.trim().to_owned()).filter(|value| !value.is_empty());
  let number = |name: &str| {
    named(name)
      .map(|value| value.parse::<i64>().map_err(|_| bad(format!("{name} is a whole number"))))
      .transpose()
  };
  let live = named("live")
    .map(|value| match value.as_str() {
      "true" => Ok(true),
      "false" => Ok(false),
      _ => Err(bad("live is true or false")),
    })
    .transpose()?;

  Ok(run_ledger::Search {
    after: number("after")?,
    before: number("before")?,
    limit: number("limit")?.unwrap_or(50).clamp(1, 500),
    live,
    agent: named("agentName"),
    room: number("roomId")?,
    round: named("round"),
    names: named("names")
      .map(|names| {
        names
          .split(',')
          .map(str::trim)
          .filter(|name| !name.is_empty())
          .map(str::to_owned)
          .collect()
      })
      .unwrap_or_default(),
  })
}

crate::moved!(SearchRuns);

/// A run read by its id.
pub struct ReadRun;

impl Route for ReadRun {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/runs/{runId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let id = id_of(&call)?;

    Ok(match run_ledger::read(call.store(), id).await? {
      Some(run) => answer(StatusCode::OK, member(&run)),
      None => nothing_held(id),
    })
  }
}

crate::moved!(ReadRun);

/// A run closed.
pub struct CloseRun;

impl Route for CloseRun {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/runs/{runId}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let id = id_of(&call)?;
    let asked = call.body().await?;
    let closes = ["finishedAt", "finished_at", "outcome", "state", "contextTokens", "context_tokens"]
      .iter()
      .any(|field| asked.get(field).is_some_and(|value| !value.is_null()));

    // The message the run's answer went out as, attached apart from the closing.
    if let Some(message) = asked.get("messageId").filter(|value| !value.is_null()) {
      let message = message.as_i64().ok_or_else(|| bad("messageId is a whole number"))?;

      if !run_ledger::attach_message(call.store(), id, message).await? {
        return Ok(nothing_held(id));
      }

      if !closes {
        return Ok(match run_ledger::read(call.store(), id).await? {
          Some(run) => answer(StatusCode::OK, member(&run)),
          None => nothing_held(id),
        });
      }
    }

    let closing = Closing {
      finished_at: asked
        .get("finishedAt")
        .or_else(|| asked.get("finished_at"))
        .and_then(Value::as_i64)
        .unwrap_or_else(crate::now_ms),
      outcome: asked
        .get("outcome")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
          asked.get("state").and_then(Value::as_str).map(|state| match state {
            "stopping" | "stopped" => "stopped".to_owned(),
            _ => "closed".to_owned(),
          })
        })
        .unwrap_or_else(|| "closed".to_owned()),
      context_tokens: asked
        .get("contextTokens")
        .or_else(|| asked.get("context_tokens"))
        .and_then(Value::as_i64),
      spent: None,
    };

    match run_ledger::close(call.store(), id, &closing).await {
      Ok(()) => Ok(match run_ledger::read(call.store(), id).await? {
        Some(run) => answer(StatusCode::OK, member(&run)),
        None => nothing_held(id),
      }),
      Err(NotClosed::Unknown) => Ok(nothing_held(id)),
      Err(NotClosed::Closed) => Err(Refused::new(
        StatusCode::CONFLICT,
        format!("run {id} is closed already and keeps its closing"),
      )),
      Err(NotClosed::Store(why)) => Err(why.into()),
    }
  }
}

crate::moved!(CloseRun);

/// A run's runner launched in its agent's host.
pub struct LaunchRun;

impl Route for LaunchRun {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/executions";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let launch: Launch = read_body(&mut call, "a run's launch").await?;
    let id = launch.run_id.ok_or_else(|| bad("runId (a number) is required"))?;

    if launch.command.program.trim().is_empty() {
      return Err(bad("a run's launch names the program its runner is"));
    }
    let program = launch.command.program.clone();

    // Carried to its end apart from the request, so a runner the host refused never leaves its run open.
    let ground = call.shared_ground();
    let launched = to_its_end(async move { Ok(run_ledger::launch(&ground, id, launch).await) }).await?;

    match launched {
      Ok(()) => Ok(answer(
        StatusCode::CREATED,
        json!({
          "id": id,
          "runId": id,
          "command": program,
          "state": "launched",
          "cancelled": false,
        }),
      )),
      Err(NotLaunched::Unknown) => Ok(nothing_held(id)),
      Err(NotLaunched::Closed) => Err(Refused::new(
        StatusCode::CONFLICT,
        format!("run {id} is closed already, so no runner is started for it"),
      )),
      Err(NotLaunched::Refused(code)) => Err(Refused::new(
        StatusCode::SERVICE_UNAVAILABLE,
        format!("run {id}'s runner did not start ({code}), and the run is closed as an error"),
      )),
      Err(NotLaunched::Store(why)) => Err(why.into()),
    }
  }
}

crate::moved!(LaunchRun);
