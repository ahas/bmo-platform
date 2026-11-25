#![allow(
  dead_code,
  unused_imports,
  unused_variables,
  clippy::needless_lifetimes,
  clippy::let_and_return,
  clippy::unnecessary_to_owned
)]
//! The run log across the boundary (docs/api.md, 「log-lines」): a run's lines after the last one a reader
//! has, each with the ledger's run it was written under, a job's lines and its last one, and a note written
//! by whoever carries the run.

use std::collections::HashMap;

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use bmo_chat::{ErrorCode, WireError};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use store::{all, one};

use super::{Call, Refused, Route, answer};
use crate::door::tool_clock::ClockNote;
use crate::run_ledger;
use crate::run_log::lines::{LINE_STATES, Line};
use crate::run_log::{MAX_TOOL_CHARS, RunLog, Target, preview};

// The most lines one read answers.
const PAGE: i64 = 500;

// A tool call's arguments as the log keeps them: whole in its detail where the line was cut.
const CALLED: &str = r#"COALESCE("detail", "text")"#;

// One line as every log route reads it.
const LINE_COLUMNS: &str = r#""id", "agent", "run", "chat_id", "at", "kind", "name", "parent", "ref",
  "text", "job_id", "state", "truncated", "detail", "metadata""#;

/// One line of the log in the shape every read answers: its own fields, and the ledger's run it was
/// written under where the ledger holds one.
async fn line_of(
  db: &sea_orm::DatabaseConnection,
  row: sea_orm::QueryResult,
  held: &mut HashMap<(String, String), Option<run_ledger::Run>>,
) -> Result<Value, Refused> {
  let text = |name: &str| row.try_get::<Option<String>>("", name);
  let number = |name: &str| row.try_get::<Option<i64>>("", name);

  let run: String = row.try_get("", "run")?;
  let agent: String = row.try_get("", "agent")?;
  let ledger = match held.get(&(agent.clone(), run.clone())) {
    Some(ledger) => ledger,
    None => {
      let read = run_ledger::of_unit(db, &agent, &run).await?;

      held.entry((agent.clone(), run.clone())).or_insert(read)
    }
  };

  let kind: String = row.try_get("", "kind")?;
  let words: String = row.try_get("", "text")?;
  let metadata = text("metadata")?.and_then(|held| serde_json::from_str::<Value>(&held).ok());
  // A clock note filed before its fields were kept is answered with them read back out of its sentence
  // (docs/agent-control.md, 「A run's tool calls are held to a clock」).
  let metadata = match metadata {
    None if kind == "note" => ClockNote::read(&words).map(|note| note.fields()),
    held => held,
  };

  Ok(json!({
    "id": row.try_get::<i64>("", "id")?,
    "runId": ledger.as_ref().map_or_else(|| run.parse::<i64>().unwrap_or(0), |ledger| ledger.id),
    "roomId": ledger.as_ref().and_then(|ledger| ledger.chat_id),
    "round": ledger.as_ref().and_then(|ledger| ledger.round.clone()),
    "messageId": ledger.as_ref().and_then(|ledger| ledger.message_id),
    "jobId": number("job_id")?,
    "agentName": row.try_get::<String>("", "agent")?,
    "kind": kind,
    "text": words,
    "at": row.try_get::<i64>("", "at")?,
    "fields": {
      "run": run,
      "chatId": number("chat_id")?,
      "name": text("name")?,
      "parent": text("parent")?,
      "reference": text("ref")?,
      "state": text("state")?,
      "truncated": row.try_get::<i64>("", "truncated")? != 0,
      "detail": text("detail")?,
    },
    "metadata": metadata.unwrap_or(Value::Null),
  }))
}

fn bad(error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, error)
}

// A query parameter read as a whole number, none where it is left out or reads as none.
fn query_number(call: &Call, name: &str) -> Option<i64> {
  call.query(name).and_then(|value| value.parse::<i64>().ok())
}

/// Log lines across runs (docs/api.md, 「log-lines」): a run's, a job's or an agent's, of the kinds named, the
/// page after an id or the page before one.
pub struct RunLogLines;

impl Route for RunLogLines {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/log-lines";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let run = call.query("runId").or_else(|| call.query("run"));
    let job = query_number(&call, "jobId").or_else(|| query_number(&call, "job"));
    let agent = call
      .query("agentName")
      .map(|agent| agent.trim().to_owned())
      .filter(|agent| !agent.is_empty());
    let kinds: Vec<String> = call
      .query("kinds")
      .map(|kinds| {
        kinds
          .split(',')
          .map(str::trim)
          .filter(|kind| !kind.is_empty())
          .map(str::to_owned)
          .collect()
      })
      .unwrap_or_default();
    let after = query_number(&call, "after");
    let before = query_number(&call, "before");
    let limit = query_number(&call, "limit").unwrap_or(PAGE).clamp(1, PAGE);

    if run.is_none() && job.is_none() && agent.is_none() {
      return Err(bad("runId, jobId or agentName is required"));
    }

    // A run of the ledger named by its id reads the lines written under its unit, beside the lines written
    // under that name itself.
    let unit = match run.as_deref().map(str::parse::<i64>) {
      Some(Ok(id)) => run_ledger::read(call.store(), id).await?.map(|held| held.unit),
      _ => None,
    };
    // Before a cursor with none after it, the nearest lines are the newest of the older ones, so they are read
    // downward and turned back to stand oldest first.
    let downward = before.is_some() && after.is_none();
    let rows = all(
      call.store(),
      &format!(
        r#"SELECT {LINE_COLUMNS} FROM "agent_log"
           WHERE ($1::text IS NULL OR "run" IN ($1, $2))
             AND ($3::bigint IS NULL OR "job_id" = $3)
             AND ($4::text IS NULL OR "agent" = $4)
             AND ($5::jsonb = '[]'::jsonb OR "kind" IN (SELECT jsonb_array_elements_text($5::jsonb)))
             AND "id" > $6
             AND ($7::bigint IS NULL OR "id" < $7)
           ORDER BY "id" {} LIMIT $8"#,
        if downward { "DESC" } else { "ASC" }
      ),
      vec![
        run.clone().into(),
        unit.or(run).into(),
        job.into(),
        agent.into(),
        serde_json::to_string(&kinds).unwrap_or_else(|_| "[]".to_owned()).into(),
        after.unwrap_or(0).into(),
        before.into(),
        (limit + 1).into(),
      ],
    )
    .await?;
    let more = rows.len() as i64 > limit;
    let mut lines = Vec::with_capacity(rows.len());
    let mut held = HashMap::new();

    for row in rows.into_iter().take(limit as usize) {
      lines.push(line_of(call.store(), row, &mut held).await?);
    }

    if downward {
      lines.reverse();
    }

    Ok(answer(StatusCode::OK, json!({ "items": lines, "more": more })))
  }
}

crate::moved!(RunLogLines);

// The lines that are one agent's: those standing under its name, and, for a role, those of every agent of the role
// (docs/run-records.md, 「Agent log」). As one filter it suits a read another index already narrows, such as the
// moment's; a read by id goes through `own_lines`.
const OWN_LINES: &str =
  r#"("agent" = $1 OR "agent" IN (SELECT "name" FROM "role_agents" WHERE "role" = $1))"#;

// The agents whose lines are one agent's, the agent's name being the query's first parameter.
const OWN_AGENTS: &str =
  r#"SELECT $1::text AS "name" UNION SELECT "name" FROM "role_agents" WHERE "role" = $1"#;

// Which way a read by id goes through the log.
#[derive(Clone, Copy)]
enum Toward {
  Newer,
  Older,
}

impl Toward {
  fn order(self) -> &'static str {
    match self {
      Toward::Newer => "ASC",
      Toward::Older => "DESC",
    }
  }
}

/// A read of one agent's lines by id: the `columns` of its lines meeting `filter`, going `toward` one end of the
/// log, `limit` of them at most. `columns` holds `"id"`. One filter over the whole log walks every line other agents
/// wrote since the agent's own, so each of its agents' lines is walked apart along that agent's own index, stopped
/// at the limit, and the walks merged by id (docs/api.md, 「log-lines」).
fn own_lines(columns: &str, filter: &str, toward: Toward, limit: &str) -> String {
  let order = toward.order();
  let filter = if filter.is_empty() { String::new() } else { format!(" AND {filter}") };

  format!(
    r#"SELECT "line".* FROM ({OWN_AGENTS}) AS "own" CROSS JOIN LATERAL (
       SELECT {columns} FROM "agent_log" WHERE "agent" = "own"."name"{filter} ORDER BY "id" {order} LIMIT {limit}
     ) AS "line" ORDER BY "line"."id" {order} LIMIT {limit}"#
  )
}

// How long after the moment named a line may stand for the run a screen opens on: a gate works its start
// back from its verdict and can be a second or two out.
const ANCHOR_WINDOW_MS: i64 = 120_000;

/// One agent's log across its runs (docs/api.md, 「log-lines」): its newest page, the page before an id, the
/// pages after one, and the page standing at a moment. The same line shape as a run's log, narrowed to the
/// agent, with `hasMore` for the direction the page was read towards and `hasOlder` for the lines before a
/// page opened at a moment.
pub struct AgentLogLines;

impl Route for AgentLogLines {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/log/agents/{agentName}/lines";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let agent = call.param("agentName").unwrap_or_default().trim().to_owned();

    if agent.is_empty() {
      return Err(bad("agentName (an agent's name) is required"));
    }

    let db = call.store();
    let limit = call
      .query("limit")
      .and_then(|limit| limit.parse::<i64>().ok())
      .unwrap_or(PAGE)
      .clamp(1, PAGE);
    let window = limit + 1;
    let before = call.query("before").and_then(|before| before.parse::<i64>().ok());
    let after = call.query("after").and_then(|after| after.parse::<i64>().ok());
    let at = call.query("at").and_then(|at| at.parse::<i64>().ok());
    let chat = call.query("chat").and_then(|chat| chat.parse::<i64>().ok());
    let rows;
    let mut reversed = false;
    let mut has_older = false;

    if let Some(at) = at {
      // The first line written at or shortly after the moment, in the room named where one is, and the head
      // of the unbroken stretch of its run: a run's name is not unique for ever.
      let found = match chat {
        Some(chat) => {
          one(
            db,
            &format!(r#"SELECT "id", "run" FROM "agent_log"
               WHERE {OWN_LINES} AND "chat_id" = $4 AND "at" >= $2 AND "at" <= $3 ORDER BY "id" ASC LIMIT 1"#),
            vec![agent.clone().into(), at.into(), (at + ANCHOR_WINDOW_MS).into(), chat.into()],
          )
          .await?
        }
        None => {
          one(
            db,
            &format!(r#"SELECT "id", "run" FROM "agent_log"
               WHERE {OWN_LINES} AND "at" >= $2 AND "at" <= $3 ORDER BY "id" ASC LIMIT 1"#),
            vec![agent.clone().into(), at.into(), (at + ANCHOR_WINDOW_MS).into()],
          )
          .await?
        }
      };

      let first = match found {
        Some(found) => {
          let id: i64 = found.try_get("", "id")?;
          let run: String = found.try_get("", "run")?;
          let before_run = one(
            db,
            &own_lines(r#""id""#, r#""id" < $2 AND "run" <> $3"#, Toward::Older, "1"),
            vec![agent.clone().into(), id.into(), run.clone().into()],
          )
          .await?
          .and_then(|row| row.try_get::<i64>("", "id").ok())
          .unwrap_or(0);
          let head = one(
            db,
            &own_lines(r#""id""#, r#""run" = $2 AND "id" > $3"#, Toward::Newer, "1"),
            vec![agent.clone().into(), run.into(), before_run.into()],
          )
          .await?
          .and_then(|row| row.try_get::<i64>("", "id").ok());

          head.unwrap_or(id)
        }
        None => 0,
      };

      if first == 0 {
        // A moment already pruned out of the log, or before its first line: the newest page stands in.
        rows = all(
          db,
          &own_lines(LINE_COLUMNS, "", Toward::Older, "$2"),
          vec![agent.clone().into(), window.into()],
        )
        .await?;
        reversed = true;
      } else {
        rows = all(
          db,
          &own_lines(LINE_COLUMNS, r#""id" > $2"#, Toward::Newer, "$3"),
          vec![agent.clone().into(), (first - 1).into(), window.into()],
        )
        .await?;
        has_older = one(
          db,
          &own_lines(r#""id""#, r#""id" < $2"#, Toward::Older, "1"),
          vec![agent.clone().into(), first.into()],
        )
        .await?
        .is_some();
      }
    } else if let Some(after) = after {
      rows = all(
        db,
        &own_lines(LINE_COLUMNS, r#""id" > $2"#, Toward::Newer, "$3"),
        vec![agent.clone().into(), after.into(), window.into()],
      )
      .await?;
    } else {
      rows = match before {
        Some(before) => {
          all(
            db,
            &own_lines(LINE_COLUMNS, r#""id" < $2"#, Toward::Older, "$3"),
            vec![agent.clone().into(), before.into(), window.into()],
          )
          .await?
        }
        None => {
          all(
            db,
            &own_lines(LINE_COLUMNS, "", Toward::Older, "$2"),
            vec![agent.clone().into(), window.into()],
          )
          .await?
        }
      };
      reversed = true;
    }

    let has_more = rows.len() as i64 > limit;
    let mut rows: Vec<_> = rows.into_iter().take(limit as usize).collect();

    if reversed {
      rows.reverse();
    }
    let mut lines = Vec::with_capacity(rows.len());
    let mut held = HashMap::new();

    for row in rows {
      lines.push(line_of(db, row, &mut held).await?);
    }

    Ok(answer(
      StatusCode::OK,
      json!({ "items": lines, "hasMore": has_more, "hasOlder": has_older }),
    ))
  }
}

crate::moved!(AgentLogLines);

// The job the path names by its id.
fn job_of(call: &Call) -> Result<i64, Refused> {
  call
    .param("job")
    .and_then(|job| job.parse().ok())
    .ok_or_else(|| bad("a job's id is a whole number"))
}

async fn read_body<T: DeserializeOwned>(call: &mut Call, what: &str) -> Result<T, Refused> {
  serde_json::from_value(call.body().await?).map_err(|error| bad(format!("{what}: {error}")))
}

/// A job's last line.
pub struct JobLastLine;

impl Route for JobLastLine {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/log/jobs/{job}/last";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let job = job_of(&call)?;
    let row = one(
      call.store(),
      r#"SELECT "id", "at", "kind", "name" FROM "agent_log" WHERE "job_id" = $1 ORDER BY "id" DESC LIMIT 1"#,
      vec![job.into()],
    )
    .await?;
    let Some(row) = row else {
      return Ok(WireError::new(ErrorCode::NotFound, format!("job {job} wrote no line")).into_response());
    };

    Ok(answer(
      StatusCode::OK,
      json!({
        "id": row.try_get::<i64>("", "id")?,
        "at": row.try_get::<i64>("", "at")?,
        "kind": row.try_get::<String>("", "kind")?,
        "name": row.try_get::<Option<String>>("", "name")?,
      }),
    ))
  }
}

// The command a call's arguments name, where they read as JSON naming one.
fn command_of(arguments: &str) -> Option<String> {
  let read: Value = serde_json::from_str(arguments).ok()?;

  read.get("command").and_then(Value::as_str).map(str::to_owned)
}

/// A job's top-level tool calls and their results, with how many model requests its runs made and how
/// often its context was compacted.
pub struct JobLines;

impl Route for JobLines {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/log/jobs/{job}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let job = job_of(&call)?;
    let db = call.store();
    let called = all(
      db,
      &format!(
        r#"SELECT "id", "agent", "run", "ref", "name", "source", "at", {CALLED} AS "arguments"
           FROM "agent_log" WHERE "job_id" = $1 AND "kind" = 'tool' AND "parent" IS NULL ORDER BY "id""#
      ),
      vec![job.into()],
    )
    .await?;
    let answered = all(
      db,
      r#"SELECT "id", "agent", "run", "ref", "detail_complete", "detail_length", "at"
         FROM "agent_log" WHERE "job_id" = $1 AND "kind" = 'tool-result' AND "parent" IS NULL ORDER BY "id""#,
      vec![job.into()],
    )
    .await?;
    let counted = one(
      db,
      r#"SELECT
           (SELECT COUNT(DISTINCT "source_id") FROM "agent_log"
              WHERE "job_id" = $1 AND "parent" IS NULL AND "kind" IN ('tool', 'text', 'thinking')) AS "requests",
           (SELECT COUNT(*) FROM "agent_log"
              WHERE "job_id" = $1 AND "parent" IS NULL AND "kind" = 'compact') AS "compactions""#,
      vec![job.into()],
    )
    .await?;
    let mut calls = Vec::with_capacity(called.len());
    let mut results = Vec::with_capacity(answered.len());

    for row in called {
      let arguments = row.try_get::<String>("", "arguments")?;

      calls.push(json!({
        "id": row.try_get::<i64>("", "id")?,
        "agent": row.try_get::<String>("", "agent")?,
        "run": row.try_get::<String>("", "run")?,
        "reference": row.try_get::<Option<String>>("", "ref")?,
        "name": row.try_get::<Option<String>>("", "name")?.unwrap_or_default(),
        "source": row.try_get::<Option<String>>("", "source")?,
        "at": row.try_get::<i64>("", "at")?,
        "command": command_of(&arguments),
        "args": preview(&arguments, Some(MAX_TOOL_CHARS)).0,
      }));
    }

    for row in answered {
      results.push(json!({
        "id": row.try_get::<i64>("", "id")?,
        "agent": row.try_get::<String>("", "agent")?,
        "run": row.try_get::<String>("", "run")?,
        "reference": row.try_get::<Option<String>>("", "ref")?,
        "complete": row.try_get::<i64>("", "detail_complete")? != 0,
        "length": row.try_get::<i64>("", "detail_length")?,
        "at": row.try_get::<i64>("", "at")?,
      }));
    }

    let count = |name: &str| counted.as_ref().map_or(Ok(0), |row| row.try_get::<i64>("", name));

    Ok(answer(
      StatusCode::OK,
      json!({
        "calls": calls,
        "results": results,
        "requests": count("requests")?,
        "compactions": count("compactions")?,
      }),
    ))
  }
}

/// A note as whoever carries a run writes it.
#[derive(Deserialize)]
struct Note {
  #[serde(alias = "agentName")]
  agent: String,
  #[serde(alias = "runId")]
  run: Value,
  #[serde(alias = "chatId")]
  chat: Option<i64>,
  #[serde(alias = "jobId")]
  job: Option<i64>,
  text: String,
  #[serde(default)]
  kind: Option<String>,
  #[serde(default, alias = "at")]
  at: Option<i64>,
  source: Option<String>,
  state: Option<String>,
}

/// A note in the bot's own words written into a run's log.
pub struct WriteNote;

impl Route for WriteNote {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/log-lines";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let note: Note = read_body(&mut call, "a note").await?;
    let state = match note.state.as_deref() {
      None => None,
      Some(state) => Some(
        LINE_STATES
          .into_iter()
          .find(|known| *known == state)
          .ok_or_else(|| bad(format!("{state} is none of the states a line stands in")))?,
      ),
    };

    let run = match &note.run {
      Value::String(run) => run.clone(),
      Value::Number(run) => run.to_string(),
      _ => String::new(),
    };

    if note.agent.is_empty() || run.is_empty() {
      return Err(bad("a note names the agent and the run it is kept under"));
    }

    let agent = note.agent.clone();
    let text = note.text.clone();
    let mut log = RunLog::new(Target {
      agent: agent.clone(),
      run: run.clone(),
      chat_id: note.chat,
      job_id: note.job,
    });
    let line = Line {
      source: note.source,
      state,
      ..Line::note(text.clone())
    };

    match log.write(call.store(), line).await {
      Some(id) => Ok(answer(
        StatusCode::CREATED,
        json!({
          "id": id,
          "runId": run.parse::<i64>().unwrap_or(0),
          "jobId": note.job,
          "agentName": agent,
          "kind": note.kind.unwrap_or_else(|| "note".to_owned()),
          "text": text,
          "at": note.at.unwrap_or_else(crate::now_ms),
          "fields": {},
        }),
      )),
      None => Err(Refused::failed("the note could not be written")),
    }
  }
}

crate::moved!(WriteNote);
