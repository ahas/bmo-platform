//! The work a harness hands the agency (docs/api.md, 「Assignments and jobs」): a worker held for an item,
//! the item handed over with its lease and run on that worker, and what a worker's session carries of the
//! item and its documents.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use agent_session::{
  Apart, JobMark, Mapping as WorkMapping, Resume, Scope, SessionId, Sessions, Start, Turn, TurnContext,
  TurnEnd, WorkParts,
};
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use policy::job_handoff::JOB_HANDOFF;
use sea_orm::{QueryResult, TransactionTrait, Value as DbValue};
use serde_json::{Value, json};

use super::{Call, Refused, Route, answer};
use crate::events::Named;
use crate::herdr_backend::turn_spent;
use crate::lines::{Trees, worker};
use crate::now_ms;
use crate::order::Entry;
use crate::order_run::dispatch::{self, Request};
use crate::places::{AgentRow, Ground};
use crate::queue::holds::{is_free, release_ended};
use crate::queue::routes::JOB_WORK_KEY;
use crate::repositories::{AGENT_LABEL, MODEL_LABEL, Pin, ROLE_LABEL, named_by};
use crate::road::{self, Closed, Road};
use crate::run_ledger::{self, Closing, NotClosed, Opening, Spent};
use crate::run_log::lines::Line;
use crate::run_log::{RunLog, Target};
use store::{all, one, run};

fn bad(error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, error)
}

fn missing(what: impl std::fmt::Display) -> Refused {
  Refused::new(StatusCode::NOT_FOUND, format!("no {what} stands"))
}

async fn body(call: &mut Call) -> Result<Value, Refused> {
  call.body().await
}

fn text<'a>(body: &'a Value, field: &str) -> Result<&'a str, Refused> {
  body
    .get(field)
    .and_then(Value::as_str)
    .filter(|value| !value.trim().is_empty())
    .ok_or_else(|| bad(format!("{field} (a string) is required")))
}

fn number(body: &Value, field: &str) -> Option<i64> {
  body.get(field).and_then(Value::as_i64)
}

fn page(items: Vec<Value>, more: bool) -> Value {
  json!({ "items": items, "more": more })
}

fn limit(call: &Call) -> usize {
  call
    .query("limit")
    .and_then(|limit| limit.parse::<usize>().ok())
    .unwrap_or(50)
    .clamp(1, 500)
}

/// Told to the streams: one event of the agency's own, its body the member as its route answers it.
pub fn publish(ground: &Ground, kind: &str, member: Value) {
  ground.events.publish_named(Named {
    id: 0,
    at: 0,
    name: Cow::Owned(kind.to_owned()),
    body: member,
  });
}

// ---- assignments ----

/// One assignment as its routes answer it.
fn assignment_value(row: &QueryResult) -> Result<Value, sea_orm::DbErr> {
  Ok(json!({
    "id": row.try_get::<i64>("", "id")?,
    "itemId": row.try_get::<i64>("", "item_id")?,
    "holderName": row.try_get::<String>("", "holder_name")?,
    "workerName": row.try_get::<String>("", "worker_name")?,
    "roleName": row.try_get::<String>("", "role")?,
    "state": row.try_get::<String>("", "state")?,
    "createdAt": row.try_get::<i64>("", "created_at")?,
    "changedAt": row.try_get::<i64>("", "changed_at")?,
  }))
}

const ASSIGNMENT_COLUMNS: &str =
  r#""id", "item_id", "holder_name", "worker_name", "role", "state", "created_at", "changed_at""#;

async fn assignment(db: &sea_orm::DatabaseConnection, id: i64) -> Result<Value, Refused> {
  let sql = format!(r#"SELECT {ASSIGNMENT_COLUMNS} FROM "assignments" WHERE "id" = $1"#);
  let row = one(db, &sql, vec![id.into()]).await?;

  match row {
    Some(row) => {
      let mut value = assignment_value(&row)?;

      name_repositories(db, std::slice::from_mut(&mut value)).await?;
      Ok(value)
    }
    None => Err(missing(format!("assignment {id}"))),
  }
}

/// Each assignment's or job's `repositoryName` beside its `workerName`: the repository its checkout is of, or
/// null for a checkout no repository holds any longer (docs/api.md, 「assignments and jobs」).
async fn name_repositories(db: &sea_orm::DatabaseConnection, members: &mut [Value]) -> Result<(), Refused> {
  let repositories = crate::queue::workers::repositories_by_checkout(db).await?;

  for member in members {
    let repository = member["workerName"].as_str().and_then(|checkout| repositories.get(checkout)).cloned();

    member["repositoryName"] = json!(repository);
  }

  Ok(())
}

/// The checkouts of the repository a list is narrowed to, as the list's query parameter, or none where it is
/// narrowed to none.
async fn checkouts_named(
  db: &sea_orm::DatabaseConnection,
  repository: Option<&str>,
) -> Result<Option<String>, Refused> {
  let Some(repository) = repository else { return Ok(None) };
  let checkouts: Vec<String> = crate::queue::workers::repositories_by_checkout(db)
    .await?
    .into_iter()
    .filter(|(_, of)| of == repository)
    .map(|(checkout, _)| checkout)
    .collect();

  Ok(Some(serde_json::to_string(&checkouts).unwrap_or_else(|_| "[]".to_owned())))
}

/// That an assignment's job has ended: the worker it held is free, whichever state the hold stood in and
/// however the job ended (docs/api.md, 「assignments and jobs」).
async fn release(db: &sea_orm::DatabaseConnection, assignment_id: i64) -> Result<(), Refused> {
  run(
    db,
    r#"UPDATE "assignments" SET "state" = 'released', "changed_at" = $1 WHERE "id" = $2 AND "state" IN ('held', 'running')"#,
    vec![now_ms().into(), assignment_id.into()],
  )
  .await?;

  Ok(())
}

/// A session ended: every hold it took that no running job owns is released, so a worker held for a turn
/// that never started its job does not stay held once that turn's session is gone (docs/api.md,
/// 「assignments and jobs」). An assignment a job owns is left to that job's own end.
pub async fn release_held(db: &sea_orm::DatabaseConnection, holder: &str) -> Result<u64, Refused> {
  Ok(
    run(
      db,
      r#"UPDATE "assignments" a SET "state" = 'released', "changed_at" = $1
         WHERE a."state" = 'held' AND a."holder_name" = $2
           AND NOT EXISTS (SELECT 1 FROM "jobs" j
                 WHERE j."assignment_id" = a."id" AND j."state" IN ('starting', 'running', 'reviewing'))"#,
      vec![now_ms().into(), holder.into()],
    )
    .await?,
  )
}

/// The newest hold an ask already stands in: a `held` assignment for the same item and holder, as its
/// routes answer it (docs/api.md, 「assignments and jobs」).
async fn standing_hold(
  db: &sea_orm::DatabaseConnection,
  item_id: i64,
  holder: &str,
) -> Result<Option<Value>, Refused> {
  let row = one(
    db,
    r#"SELECT "id" FROM "assignments"
       WHERE "item_id" = $1 AND "holder_name" = $2 AND "state" = 'held'
       ORDER BY "id" DESC LIMIT 1"#,
    vec![item_id.into(), holder.into()],
  )
  .await?;

  match row {
    Some(row) => {
      let id: i64 = row.try_get("", "id")?;
      Ok(Some(assignment(db, id).await?))
    }
    None => Ok(None),
  }
}

// The most repository names a refusal lists before it points at `GET /repositories` instead.
const NAMES_LISTED: usize = 12;

/// The refusal of an assignee no repository holds a checkout by: the name it gave, and the repositories' names
/// where they are few enough to list.
async fn unknown_repository(db: &sea_orm::DatabaseConnection, assignee: &str) -> Result<Refused, Refused> {
  let names: Vec<String> = crate::repositories::every(db)
    .await?
    .into_iter()
    .map(|repository| repository.name)
    .collect();
  let which = if names.len() <= NAMES_LISTED {
    format!("a repository is named exactly as GET /repositories lists it: {}", names.join(", "))
  } else {
    "a repository is named exactly as GET /repositories lists it".to_owned()
  };

  Ok(
    Refused::new(
      StatusCode::UNPROCESSABLE_ENTITY,
      format!("no repository holds a checkout named {assignee}; {which}"),
    )
    .with_code("unknown-repository"),
  )
}

/// Where an assignment's item goes (docs/api.md, 「assignments and jobs」): the `assigneeName` the ask names,
/// else the one of the item it hands over, else the item's own as the job service answers it, and the checkout
/// named `worker` for an item naming no repository, so an ask never takes a checkout of another repository than
/// its item's.
async fn item_destination(ground: &Ground, asked: &Value, item_id: i64) -> Result<String, Refused> {
  let named = |value: &Value| {
    value["assigneeName"]
      .as_str()
      .map(str::trim)
      .filter(|name| !name.is_empty())
      .map(str::to_owned)
  };

  if let Some(assignee) = named(asked).or_else(|| named(&asked["item"])) {
    return Ok(assignee);
  }

  let held = match crate::queue::orderer::JobService::of(ground.vars()) {
    Some(service) => service.item(item_id).await.map_err(|why| {
      Refused::new(StatusCode::BAD_GATEWAY, format!("item #{item_id} could not be read: {why}"))
        .with_code("upstream-failed")
    })?,
    None => None,
  };

  Ok(
    held
      .as_ref()
      .and_then(named)
      .unwrap_or_else(|| crate::queue::workers::BUILTIN_WORKER.to_owned()),
  )
}

/// A worker held for an item for the harness that asked.
pub struct CreateAssignment;

impl Route for CreateAssignment {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/assignments";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let asked = body(&mut call).await?;
    let item_id = number(&asked, "itemId").ok_or_else(|| bad("itemId (a number) is required"))?;
    let holder = text(&asked, "holderName")?.trim().to_owned();
    if asked.get("item").is_none_or(Value::is_null) {
      return Err(bad("item is required"));
    }
    let assignee = item_destination(call.ground(), &asked, item_id).await?;
    let assignee = assignee.as_str();
    let db = &call.ground().db;
    // The role the item's job runs as, `worker` where it names none; any role the store holds, and an unknown
    // one is refused (docs/repositories.md, 「A job」).
    let named = asked
      .get("roleName")
      .and_then(Value::as_str)
      .map(str::trim)
      .filter(|name| !name.is_empty());

    if let Some(role) = named
      && !crate::agent_roles::holds(db, role).await?
    {
      return Err(
        Refused::new(StatusCode::UNPROCESSABLE_ENTITY, format!("no role is named {role}"))
          .with_code("unknown-role"),
      );
    }

    let role = named.unwrap_or_else(|| crate::agent_roles::job_role()).to_owned();

    release_ended(db).await?;

    // A checkout no assignment or job holds now, of the repository or the checkout the item goes to.
    let mut picked = None;
    let checkouts = crate::queue::workers::workers(db, &call.ground().source).await?;

    if !checkouts.iter().any(|checkout| checkout.name == assignee) {
      return Err(unknown_repository(db, assignee).await?);
    }

    holder_works_on(call.ground(), &holder, assignee, &[]).await?;

    crate::queue::holds::release_others(db, item_id, &holder).await?;

    // An ask for the same item by the same holder answers the hold it already stands in, so a lost answer
    // read again is no busy worker; an ask naming another worker or role replaces that hold (docs/api.md,
    // 「assignments and jobs」).
    if let Some(standing) = standing_hold(db, item_id, &holder).await? {
      let worker = standing["workerName"].as_str().unwrap_or_default();
      let repository = standing["repositoryName"].as_str().unwrap_or(worker);
      let same_worker = crate::queue::workers::answers_to(worker, repository, assignee);
      let same_role = named.is_none_or(|role| standing["roleName"].as_str() == Some(role));

      if same_worker && same_role {
        return Ok(answer(StatusCode::OK, standing));
      }

      let standing_id = standing["id"].as_i64().unwrap_or_default();

      if let Err(why) = release(db, standing_id).await {
        eprintln!("assignment {standing_id} was not released before its replacement: {}", why.error);
      }
    }

    for checkout in checkouts {
      if checkout.builtin || !checkout.answers_to(assignee) {
        continue;
      }

      if is_free(db, &checkout.name).await? {
        picked = Some(checkout);
        break;
      }
    }
    let Some(picked) = picked else {
      // A repository set to `auto` grows its next support checkout instead (docs/worker-slots.md, 「Growing a
      // checkout at a launch」); a checkout named on its own never grows.
      if let Some(repository) = crate::repositories::named(db, assignee).await? {
        let account = crate::supervisors::named(db, &holder)
          .await?
          .and_then(|supervisor| supervisor.owner_account)
          .map(|account| account.to_string());
        let seating = super::support_growth::Seating {
          item: &asked["item"],
          role: &role,
          account: account.as_deref(),
        };
        let holding = super::support_growth::Holding {
          item_id,
          holder: &holder,
          role: &role,
        };

        if let Some(grown) =
          super::support_growth::grow(call.ground(), &repository, Some(seating), holding).await?
        {
          return Ok(answer(StatusCode::CREATED, grown.assignment));
        }
      }

      return Err(
        Refused::new(StatusCode::UNPROCESSABLE_ENTITY, format!("no worker {assignee} is free"))
          .with_code("no-worker-free"),
      );
    };
    let member = hold(call.ground(), item_id, &holder, &picked.name, &role).await?;

    Ok(answer(StatusCode::CREATED, member))
  }
}

/// A checkout held for an item in a holder's name as the role given, answered as the assignment and published
/// as created.
pub(crate) async fn hold(
  ground: &Ground,
  item_id: i64,
  holder: &str,
  worker: &str,
  role: &str,
) -> Result<Value, Refused> {
  let id = write_hold(&ground.db, item_id, holder, worker, role).await?;

  held(ground, id).await
}

/// A hold written in whatever connection a caller holds, by its assignment's id, neither answered nor
/// published yet.
pub(crate) async fn write_hold<C: sea_orm::ConnectionTrait>(
  db: &C,
  item_id: i64,
  holder: &str,
  worker: &str,
  role: &str,
) -> Result<i64, Refused> {
  let now = now_ms();
  let row = one(
    db,
    r#"INSERT INTO "assignments" ("item_id", "holder_name", "worker_name", "role", "state", "created_at",
            "changed_at")
       VALUES ($1, $2, $3, $5, 'held', $4, $4) RETURNING "id""#,
    vec![item_id.into(), holder.into(), worker.into(), now.into(), role.into()],
  )
  .await?
  .ok_or_else(|| Refused::failed("the assignment was written but not answered"))?;

  Ok(row.try_get("", "id")?)
}

/// A hold written, answered as the assignment and published as created.
pub(crate) async fn held(ground: &Ground, id: i64) -> Result<Value, Refused> {
  let member = assignment(&ground.db, id).await?;

  publish(ground, "assignment.created", member.clone());

  Ok(member)
}

crate::moved!(CreateAssignment);

/// The assignments, by holder, worker or state.
pub struct ListAssignments;

impl Route for ListAssignments {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/assignments";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let db = &call.ground().db;
    release_ended(db).await?;

    let after = call.query("after").and_then(|after| after.parse::<i64>().ok()).unwrap_or(0);
    let limit = limit(&call);
    let holder = call.query("holderName").unwrap_or_default();
    let worker = call.query("workerName").unwrap_or_default();
    let state = call.query("state").unwrap_or_default();
    let repository = call
      .query("repositoryName")
      .map(|name| name.trim().to_owned())
      .filter(|name| !name.is_empty());
    let checkouts = checkouts_named(db, repository.as_deref()).await?;
    let sql = format!(
      r#"SELECT {ASSIGNMENT_COLUMNS} FROM "assignments"
         WHERE "id" > $1 AND ($2 = '' OR "holder_name" = $2) AND ($3 = '' OR "worker_name" = $3)
           AND ($4 = '' OR "state" = $4)
           AND ($6::jsonb IS NULL OR "worker_name" IN (SELECT jsonb_array_elements_text($6::jsonb)))
         ORDER BY "id" LIMIT $5"#
    );
    let rows = all(
      db,
      &sql,
      vec![
        after.into(),
        holder.into(),
        worker.into(),
        state.into(),
        ((limit + 1) as i64).into(),
        checkouts.into(),
      ],
    )
    .await?;
    let mut items: Vec<Value> = rows.iter().map(assignment_value).collect::<Result<_, _>>()?;
    let more = items.len() > limit;
    items.truncate(limit);
    name_repositories(db, &mut items).await?;

    Ok(answer(StatusCode::OK, page(items, more)))
  }
}

crate::moved!(ListAssignments);

/// One assignment.
pub struct ReadAssignment;

impl Route for ReadAssignment {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/assignments/{assignmentId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let id: i64 = call
      .param("assignmentId")
      .and_then(|id| id.parse().ok())
      .ok_or_else(|| bad("assignmentId is a number"))?;
    release_ended(&call.ground().db).await?;

    Ok(answer(StatusCode::OK, assignment(&call.ground().db, id).await?))
  }
}

crate::moved!(ReadAssignment);

/// Release an assignment.
pub struct ChangeAssignment;

impl Route for ChangeAssignment {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/assignments/{assignmentId}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let id: i64 = call
      .param("assignmentId")
      .and_then(|id| id.parse().ok())
      .ok_or_else(|| bad("assignmentId is a number"))?;
    let asked = body(&mut call).await?;
    let db = &call.ground().db;
    let now = now_ms();
    let current = assignment(db, id).await?;
    let state = current["state"].as_str().unwrap_or_default();

    if state != "held" && state != "running" {
      return Err(Refused::new(StatusCode::CONFLICT, format!("assignment {id} is {state}")));
    }

    match asked.get("state").and_then(Value::as_str) {
      Some("released") => {
        run(
          db,
          r#"UPDATE "assignments" SET "state" = 'released', "changed_at" = $1 WHERE "id" = $2"#,
          vec![now.into(), id.into()],
        )
        .await?;
      }
      Some(other) => return Err(bad(format!("an assignment is released, not {other}"))),
      None => {}
    }

    let member = assignment(db, id).await?;
    publish(call.ground(), "assignment.changed", member.clone());

    Ok(answer(StatusCode::OK, member))
  }
}

crate::moved!(ChangeAssignment);

// ---- jobs ----

const JOB_COLUMNS: &str = r#""id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "role",
  "session_id", "run_id", "run_name", "room_id", "state", "outcome", "report", "token_count",
  "input_tokens", "output_tokens", "cache_read_tokens", "cache_write_tokens", "instruction",
  "started_at", "ended_at", "prompt", "prompt_english", "roads", "held_for", "chosen_by", "review_stages",
  "review_stage", "review_round", "review_check", "rejections", "review_question", "review_parked", "item_agent""#;

/// What a job's turn spent, apart; none until the turn ends and where its road reported no figures.
fn usage_of(row: &QueryResult) -> Result<Value, sea_orm::DbErr> {
  let figure = |column: &str| row.try_get::<Option<i64>>("", column);

  Ok(
    match (
      figure("input_tokens")?,
      figure("output_tokens")?,
      figure("cache_read_tokens")?,
      figure("cache_write_tokens")?,
    ) {
      (Some(input), Some(output), Some(read), Some(write)) => json!({
        "inputTokens": input,
        "outputTokens": output,
        "cacheReadTokens": read,
        "cacheWriteTokens": write,
      }),
      _ => Value::Null,
    },
  )
}

/// One job as its routes answer it.
fn job_value(row: &QueryResult) -> Result<Value, sea_orm::DbErr> {
  Ok(json!({
    "id": row.try_get::<i64>("", "id")?,
    "itemId": row.try_get::<i64>("", "item_id")?,
    "leaseId": row.try_get::<i64>("", "lease_id")?,
    "holderName": row.try_get::<String>("", "holder_name")?,
    "assignmentId": row.try_get::<i64>("", "assignment_id")?,
    "workerName": row.try_get::<String>("", "worker_name")?,
    "roleName": row.try_get::<String>("", "role")?,
    "agentName": row.try_get::<Option<String>>("", "item_agent")?,
    "sessionId": row.try_get::<Option<String>>("", "session_id")?,
    "runId": row.try_get::<Option<i64>>("", "run_id")?,
    "runName": row.try_get::<Option<String>>("", "run_name")?,
    "roomId": row.try_get::<Option<i64>>("", "room_id")?,
    "state": row.try_get::<String>("", "state")?,
    "outcome": row.try_get::<Option<String>>("", "outcome")?,
    "report": row.try_get::<Option<String>>("", "report")?,
    "tokenCount": row.try_get::<i64>("", "token_count")?,
    "usage": usage_of(row)?,
    "instruction": row
      .try_get::<Option<String>>("", "instruction")?
      .and_then(|text| serde_json::from_str::<Value>(&text).ok()),
    "roads": row
      .try_get::<Option<String>>("", "roads")?
      .and_then(|text| serde_json::from_str::<Value>(&text).ok())
      .unwrap_or_else(|| json!([])),
    "heldFor": row.try_get::<Option<String>>("", "held_for")?,
    "chosenBy": row.try_get::<Option<String>>("", "chosen_by")?,
    "review": crate::job_review::value(row)?,
    "startedAt": row.try_get::<i64>("", "started_at")?,
    "endedAt": row.try_get::<Option<i64>>("", "ended_at")?,
  }))
}

pub(crate) async fn job(db: &sea_orm::DatabaseConnection, id: i64) -> Result<(Value, String), Refused> {
  let sql = format!(r#"SELECT {JOB_COLUMNS} FROM "jobs" WHERE "id" = $1"#);
  let row = one(db, &sql, vec![id.into()]).await?;

  match row {
    Some(row) => {
      let mut value = job_value(&row)?;

      name_repositories(db, std::slice::from_mut(&mut value)).await?;
      Ok((value, row.try_get("", "prompt")?))
    }
    None => Err(missing(format!("job {id}"))),
  }
}

// The four fields of a small-model instruction, in the order a prompt and a step say them.
const INSTRUCTION_LISTS: [&str; 3] = ["documents", "sentences", "checks"];

/// A small-model instruction as a caller names it (docs/worker-bench.md, 「A small-model instruction」):
/// its repository and its documents, sentences and checks, every one present and none empty; none where
/// it names none.
pub fn instruction_of(named: Option<&Value>) -> Result<Option<Value>, Refused> {
  let Some(named) = named.filter(|named| !named.is_null()) else {
    return Ok(None);
  };
  let repository = named
    .get("repository")
    .and_then(Value::as_str)
    .map(str::trim)
    .filter(|repository| !repository.is_empty())
    .ok_or_else(|| bad("instruction.repository (a string) is required"))?;
  let mut instruction = json!({ "repository": repository });

  for field in INSTRUCTION_LISTS {
    let entries = named
      .get(field)
      .and_then(Value::as_array)
      .filter(|entries| !entries.is_empty())
      .ok_or_else(|| bad(format!("instruction.{field} (a list that is not empty) is required")))?;
    let mut kept = Vec::with_capacity(entries.len());

    for entry in entries {
      match entry.as_str().map(str::trim).filter(|entry| !entry.is_empty()) {
        Some(entry) => kept.push(Value::from(entry)),
        None => return Err(bad(format!("instruction.{field}: every entry is a string that is not empty"))),
      }
    }

    instruction[field] = Value::Array(kept);
  }

  Ok(Some(instruction))
}

fn entries<'a>(instruction: &'a Value, field: &str) -> Vec<&'a str> {
  instruction[field]
    .as_array()
    .map(|entries| entries.iter().filter_map(Value::as_str).collect())
    .unwrap_or_default()
}

/// An instruction as a job's prompt renders it, ahead of the item's brief.
pub fn instruction_prompt(instruction: &Value) -> String {
  let mut text = format!(
    "## Instruction\n\nRepository: `{}`\n",
    instruction["repository"].as_str().unwrap_or_default()
  );
  let headings = [
    ("documents", "Read first, the canonical documents the change answers to:"),
    ("sentences", "The change must make or keep each of these true:"),
    ("checks", "It is done when each of these passes:"),
  ];

  for (field, heading) in headings {
    text.push_str(&format!("\n{heading}\n\n"));

    for entry in entries(instruction, field) {
      if field == "checks" {
        text.push_str(&format!("- `{entry}`\n"));
      } else {
        text.push_str(&format!("- {entry}\n"));
      }
    }
  }

  text
}

/// An instruction as the one line a work-order step is written as.
pub fn instruction_line(instruction: &Value) -> String {
  format!(
    "In `{}`, reading {}: {} Checked by {}.",
    instruction["repository"].as_str().unwrap_or_default(),
    entries(instruction, "documents").join(", "),
    entries(instruction, "sentences").join(" "),
    entries(instruction, "checks")
      .iter()
      .map(|check| format!("`{check}`"))
      .collect::<Vec<_>>()
      .join(", ")
  )
}

/// An instruction's lines as the gate's reference road reads them: its repository and each document,
/// sentence and check, one a line.
fn instruction_lines(instruction: &Value) -> Vec<String> {
  let mut lines = Vec::new();

  if let Some(repository) = instruction["repository"].as_str() {
    lines.push(repository.to_owned());
  }

  for field in INSTRUCTION_LISTS {
    lines.extend(entries(instruction, field).into_iter().map(str::to_owned));
  }

  lines
}

/// A prompt with the reference the gate picked attached, its entries under a heading of their own, each
/// named by its path and source; the prompt stands alone where no entry was picked.
fn with_reference(prompt: &str, entries: &[bmo_gate_client::Entry]) -> String {
  crate::peers::gate::attach(prompt, entries)
}

/// The prompt a job's worker is given, with the reference the gate picked over the job's instruction
/// attached ahead of the run, read from the brain of the job's holder: the supervisor that launched it, whose
/// account the item is filed under. A job with no instruction or no holder, or a gate that picks nothing,
/// stands as it was.
pub async fn referenced(gate: &dyn crate::peers::gate::Gate, job: &Value, prompt: &str) -> String {
  let Some(instruction) = job.get("instruction").filter(|instruction| !instruction.is_null()) else {
    return prompt.to_owned();
  };
  let Some(holder) = job["holderName"].as_str().filter(|holder| !holder.is_empty()) else {
    return prompt.to_owned();
  };
  let lines = instruction_lines(instruction);

  if lines.is_empty() {
    return prompt.to_owned();
  }

  match gate.reference(holder, lines).await {
    Ok(entries) => with_reference(prompt, &entries),
    Err(why) => {
      eprintln!("job: the gate's reference was not picked: {why}");

      prompt.to_owned()
    }
  }
}

/// The prompt a job's worker is given: the item as the harness read it, its instruction first where it
/// carries one.
fn prompt_of(item: &Value, instruction: Option<&Value>) -> String {
  let title = item.get("title").and_then(Value::as_str).unwrap_or("the item");
  let brief = item.get("brief").and_then(Value::as_str).unwrap_or_default();
  let mut prompt = format!("# {title}\n\n");

  if let Some(instruction) = instruction {
    prompt.push_str(&instruction_prompt(instruction));
    prompt.push('\n');
  }

  if !brief.trim().is_empty() {
    prompt.push_str(brief);
    prompt.push('\n');
  }

  if let Some(order) = item.get("workOrder").filter(|order| !order.is_null()) {
    prompt.push_str("\n## Work order\n\n");
    prompt.push_str(&serde_json::to_string_pretty(order).unwrap_or_default());
    prompt.push('\n');
  }

  if let Some(notes) = item.get("notes").and_then(Value::as_array).filter(|notes| !notes.is_empty()) {
    // A handoff note stands ahead of the prompt where it is the item's newest, and is no note below it.
    let texts: Vec<&str> = notes
      .iter()
      .filter(|note| !crate::job_handoff::is_handoff(note))
      .map(|note| note.get("text").and_then(Value::as_str).or_else(|| note.as_str()).unwrap_or_default())
      .collect();

    if !texts.is_empty() {
      prompt.push_str("\n## Notes\n\n");
      prompt.push_str(&notes_within(&texts, policy::job_prompt::JOB_PROMPT.notes.most_bytes));
    }
  }

  prompt
}

/// A job's prompt with its title, its first line that opens a heading of the first level, in the worker's language
/// (docs/worker-language.md, 「What agency writes for a worker」); the prompt as it stands where it has no title.
pub async fn title_in_english(gate: &dyn crate::peers::gate::Gate, prompt: &str) -> String {
  let mut at = 0;

  for line in prompt.split_inclusive('\n') {
    if let Some(title) = line.trim_end_matches('\n').strip_prefix("# ") {
      let english = crate::worker_english::english_titles(gate, vec![title.to_owned()])
        .await
        .pop()
        .unwrap_or_default();
      let rest = &prompt[at + line.len()..];
      let ending = if line.ends_with('\n') { "\n" } else { "" };

      return format!("{}# {english}{ending}{rest}", &prompt[..at]);
    }

    at += line.len();
  }

  prompt.to_owned()
}

/// A job's prompt with a section under its title, the title being its first line that opens a heading of the
/// first level; the section stands first where it has none, and the prompt stands as it is where the section is
/// empty.
pub fn under_title(prompt: &str, section: &str) -> String {
  if section.is_empty() {
    return prompt.to_owned();
  }

  let mut at = 0;

  for line in prompt.split_inclusive('\n') {
    at += line.len();

    if line.starts_with("# ") {
      let rest = &prompt[at..];
      let blank = if rest.starts_with('\n') { 1 } else { 0 };
      let (head, tail) = prompt.split_at(at + blank);

      return format!("{head}{}{section}\n{tail}", if blank == 0 { "\n" } else { "" });
    }
  }

  format!("{section}\n{prompt}")
}

/// An item's notes as a job's prompt carries them: the newest that fit `most` bytes, in their order, after a
/// line saying how many older ones were left out where any were.
pub fn notes_within(notes: &[&str], most: usize) -> String {
  let mut kept = Vec::new();
  let mut bytes = 0;

  for note in notes.iter().rev() {
    let mut line = format!("- {note}\n");

    // The newest note alone past the bytes is cut at them rather than left out whole.
    if kept.is_empty() && line.len() > most {
      let mut cut = most.saturating_sub(5);

      while !line.is_char_boundary(cut) {
        cut -= 1;
      }

      line.truncate(cut);
      line.push_str(" …\n");
    }

    if bytes + line.len() > most && !kept.is_empty() {
      break;
    }

    bytes += line.len();
    kept.push(line);
  }

  let left = notes.len() - kept.len();
  let mut text = String::new();

  if left > 0 {
    text.push_str(&policy::fill_marks(
      &policy::job_prompt::JOB_PROMPT.notes.left_out,
      &[("n", &left.to_string())],
    ));
    text.push('\n');
  }

  for line in kept.into_iter().rev() {
    text.push_str(&line);
  }

  text
}

/// Run an item on its assigned worker.
pub struct CreateJob;

impl Route for CreateJob {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/jobs";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let asked = body(&mut call).await?;
    let assignment_id =
      number(&asked, "assignmentId").ok_or_else(|| bad("assignmentId (a number) is required"))?;
    let item_id = number(&asked, "itemId").ok_or_else(|| bad("itemId (a number) is required"))?;
    let lease_id = number(&asked, "leaseId").ok_or_else(|| bad("leaseId (a number) is required"))?;
    let holder = text(&asked, "holderName")?.trim().to_owned();
    let item = asked.get("item").cloned().ok_or_else(|| bad("item is required"))?;
    let instruction = instruction_of(asked.get("instruction").or_else(|| item.get("instruction")))?;
    let db = &call.ground().db;
    let now = now_ms();

    release_ended(db).await?;
    let held = assignment(db, assignment_id).await?;
    let state = held["state"].as_str().unwrap_or_default();
    if state != "held" {
      return Err(Refused::new(StatusCode::CONFLICT, format!("assignment {assignment_id} is {state}")));
    }
    if held["itemId"].as_i64() != Some(item_id) || held["holderName"].as_str() != Some(holder.as_str()) {
      return Err(Refused::new(
        StatusCode::CONFLICT,
        "the assignment holds another item or another holder",
      ));
    }
    let worker_name: String = held["workerName"].as_str().unwrap_or_default().to_owned();
    let lent_named = to_lend(call.ground(), &item, &worker_name).await?;

    holder_works_on(call.ground(), &holder, &worker_name, &lent_named).await?;
    // Every job names its agent, or the model and the role whose free agent it takes, and that agent carries its
    // role and its road (docs/repositories.md, 「An agent an item names」).
    let asked_role = item_role(&call.shared_ground(), item_id, &item).await?.unwrap_or_else(|| {
      held["roleName"].as_str().unwrap_or_else(|| crate::agent_roles::job_role()).to_owned()
    });
    let account = crate::supervisors::named(db, &holder)
      .await?
      .and_then(|supervisor| supervisor.owner_account)
      .map(|account| account.to_string());
    let seat = item_seat(&call.shared_ground(), item_id, &item, &asked_role, account.as_deref()).await?;
    // A launch of the item opens where the last session that worked on it left off (docs/sessions.md, 「The
    // handoff a job's session leaves」).
    let prompt = match crate::job_handoff::of_item(&call.shared_ground(), item_id).await {
      Some(handoff) => crate::job_handoff::ahead(&handoff, &prompt_of(&item, instruction.as_ref())),
      None => prompt_of(&item, instruction.as_ref()),
    };
    let room_id = number(&asked, "roomId")
      .or_else(|| number(&item, "roomId"))
      .or_else(|| item.get("scope").and_then(Value::as_str).and_then(|scope| scope.trim().parse().ok()))
      .or_else(|| number(&item, "scope"));
    // Agency starts no job by itself (docs/role-agents.md, 「Which agent a run takes」): a launch onto an agent
    // that carries a run is refused now rather than queued to start once the agent frees. The agent is chosen
    // and the job written in one transaction holding every candidate's row, so two launches never take one
    // free agent.
    let written = db.begin().await.map_err(|why| Refused::failed(why.to_string()))?;
    let item_agent = seat.take(&written, item_id).await?;
    let role = item_agent.role.clone();
    let item_model = item_agent.entry().map(|entry| Pin {
      provider: entry.provider,
      model: entry.model,
    });
    // A job is never refused for quota (docs/api.md, 「assignments and jobs」): where nothing in its walk can
    // answer now, it is taken and waits `starting`, held with the reason, until a road can. The walk read
    // here only says which chose the job's road: its agent's road is always one to follow.
    let resolution = road::for_job(
      call.ground(),
      Some(&role),
      Some(&worker_name),
      item_model.as_ref(),
      &[],
      Some(holder.as_str()),
    )
    .await
    .map_err(|why| Refused::new(StatusCode::SERVICE_UNAVAILABLE, why))?;

    let row = one(
      &written,
      r#"INSERT INTO "jobs" ("item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
              "room_id", "state", "token_count", "started_at", "prompt", "instruction", "role", "item_provider",
              "item_model", "chosen_by", "item_agent", "notes_through", "named_repositories")
         VALUES ($1, $2, $3, $4, $5, $6, 'starting', 0, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16) RETURNING "id""#,
      vec![
        item_id.into(),
        lease_id.into(),
        holder.into(),
        assignment_id.into(),
        worker_name.clone().into(),
        room_id.map(Into::into).unwrap_or(DbValue::BigInt(None)),
        now.into(),
        prompt.clone().into(),
        instruction.as_ref().map(Value::to_string).into(),
        role.into(),
        item_model.as_ref().map(|named| named.provider.clone()).into(),
        item_model.as_ref().map(|named| named.model.clone()).into(),
        resolution.chosen_by.map(|chosen| chosen.word().to_owned()).into(),
        Some(item_agent.name.clone()).into(),
        crate::job_words::notes_through(&item).into(),
        Some(lent_named.join(",")).filter(|named| !named.is_empty()).into(),
      ],
    )
    .await?
    .ok_or_else(|| Refused::failed("the job was written but not answered"))?;
    let id: i64 = row.try_get("", "id")?;

    // A job that will wait is written waiting (docs/api.md, 「assignments and jobs」), so no answer or event
    // reads it as going out before its first turn has asked.
    if let Some(waits) =
      first_wait(call.ground(), id, &resolution, &item_agent.role, &item_agent.name, &worker_name).await
    {
      run(
        &written,
        r#"UPDATE "jobs" SET "held_for" = $1 WHERE "id" = $2"#,
        vec![waits.into(), id.into()],
      )
      .await?;
    }

    written.commit().await.map_err(|why| Refused::failed(why.to_string()))?;
    crate::watches::queue_idle::agent_told_let_go(db, item_id).await;
    crate::watches::queue_idle::no_candidate_told_let_go(db, item_id).await;

    run(
      db,
      r#"UPDATE "assignments" SET "state" = 'running', "changed_at" = $1 WHERE "id" = $2"#,
      vec![now.into(), assignment_id.into()],
    )
    .await?;
    let (member, _) = job(db, id).await?;
    publish(call.ground(), "job.created", member.clone());

    // What the launch read of the item stands fixed while the job runs (docs/repositories.md, 「An agent an item
    // names」).
    let fixing = call.shared_ground();
    let agent = item_agent.name.clone();
    tokio::spawn(
      async move { crate::job_leases::hold_launch(fixing.vars(), id, lease_id, Some(&agent)).await },
    );

    let ground = call.shared_ground();
    let sessions = call.sessions();
    tokio::spawn(async move { launch(ground, sessions, id).await });

    Ok(answer(StatusCode::CREATED, member))
  }
}

crate::moved!(CreateJob);

/// Whether a holder's work may stand on the repository of a checkout and on each repository lent beside it
/// (docs/api.md, 「assignments and jobs」): the holder's account, the supervisor's or the seat's it is, against each
/// repository's owner, refused 403 `foreign-repository` where they stand on different sides.
async fn holder_works_on(
  ground: &Ground,
  holder: &str,
  worker: &str,
  lent: &[String],
) -> Result<(), Refused> {
  let account = crate::supply::account_of(ground, holder).await?;
  let known = crate::queue::workers::workers(&ground.db, &ground.source).await?;
  let own = known
    .iter()
    .find(|checkout| checkout.name == worker)
    .map_or(worker, |checkout| checkout.repository());

  for name in std::iter::once(own).chain(lent.iter().map(String::as_str)) {
    crate::access::works_on(ground, account, name).await?;
  }

  Ok(())
}

/// The repositories an item's label names to be lent to its job, the job's own left out, each judged
/// (docs/api.md, 「assignments and jobs」): a repository the store holds, with a checkout on the machine the job's
/// own checkout stands on, and no more than the bound of checkouts one job holds, its own among them.
async fn to_lend(ground: &Ground, item: &Value, worker: &str) -> Result<Vec<String>, Refused> {
  let named = crate::repositories::named_repositories(&item["labels"]);

  if named.is_empty() {
    return Ok(named);
  }

  let known = crate::queue::workers::workers(&ground.db, &ground.source).await?;
  let own = known.iter().find(|checkout| checkout.name == worker);
  let own_repository = own.map_or(worker, |checkout| checkout.repository());
  let node = own.and_then(|checkout| checkout.node.as_deref());
  let lent: Vec<String> = named.into_iter().filter(|name| name != own_repository).collect();

  for name in &lent {
    if crate::repositories::named(&ground.db, name).await?.is_none() {
      return Err(
        Refused::new(StatusCode::UNPROCESSABLE_ENTITY, format!("no repository is named {name}"))
          .with_code("unknown-repository"),
      );
    }

    if super::loans::candidates(&known, name, node).is_empty() {
      return Err(
        Refused::new(
          StatusCode::UNPROCESSABLE_ENTITY,
          format!("{name} is no repository with a checkout on this job's machine"),
        )
        .with_code("no-checkout-here"),
      );
    }
  }

  if lent.len() + 1 > policy::workers::WORKERS.job_checkouts_most {
    return Err(super::loans::too_many(policy::workers::WORKERS.job_checkouts_most));
  }

  Ok(lent)
}

/// The wait a job's first turn will begin with, judged as the job is written (docs/api.md, 「assignments and
/// jobs」): the closed road's sentence where no road of its walk answers, as its turn will wait for a road, else
/// the seat it waits for where the line would not let its first ask in. None where it would go straight out, or
/// where the line could not be read, its turn then telling the wait as it begins.
async fn first_wait(
  ground: &Ground,
  id: i64,
  resolution: &road::Resolution,
  role: &str,
  agent: &str,
  worker: &str,
) -> Option<String> {
  let Some((taken, entry)) = resolution.going() else {
    return Some(resolution.no_road.clone().unwrap_or_else(|| resolution.sentence()));
  };
  // The ask its first turn makes (`OnSession::slot`), judged without being made: the same owner, road, agent
  // and checkout.
  let asked = dispatch::Asked {
    request_key: format!("job-{id}-first"),
    owner: format!("{}{id}", dispatch::JOB_OWNER),
    wanted: dispatch::Wanted {
      role,
      path: None,
      fixed: Some(crate::order::Entry {
        provider: taken.provider.clone(),
        model: taken.model.clone(),
        effort: entry.effort.clone(),
        thinking: None,
        answer_tokens: None,
        advisor: None,
      }),
      agent: Some(agent),
    },
    place_key: Some(worker),
    ..dispatch::Asked::default()
  };

  match dispatch::first_ask_waits(ground, &asked, dispatch::JOB).await {
    Ok(waits) => waits.then(|| format!("it waits for {}", seat_words(role, Some(agent)))),
    Err(why) => {
      eprintln!("job {id}: whether its first ask would wait was not judged: {why}");
      None
    }
  }
}

// The seat a job's turn asks for, as its wait and its log name it.
fn seat_words(role: &str, agent: Option<&str>) -> String {
  match agent {
    Some(agent) => format!("the agent {agent} of the {role} role"),
    None => format!("a slot of the {role} role"),
  }
}

/// What an agent carries at work now, in words a launch's refusal names it by (docs/repositories.md, 「An agent an
/// item names」): a job standing on it — starting, running, or under a review that has not paused it — else the
/// run a slot request let in on it stands for, a job standing in the machine's turn leaving it free
/// (docs/role-agents.md, 「When an agent is busy」). None where it is free.
pub(crate) async fn agent_carries<C: sea_orm::ConnectionTrait>(
  db: &C,
  agent: &str,
) -> Result<Option<String>, Refused> {
  let job = one(
    db,
    r#"SELECT "id", "item_id" FROM "jobs"
       WHERE "item_agent" = $1 AND NOT "machine_turn"
         AND ("state" IN ('starting', 'running') OR ("state" = 'reviewing' AND NOT "review_parked"))
       ORDER BY "id" LIMIT 1"#,
    vec![agent.into()],
  )
  .await?;

  if let Some(job) = job {
    let id: i64 = job.try_get("", "id")?;
    let item: i64 = job.try_get("", "item_id")?;

    return Ok(Some(format!("job {id} of item #{item}")));
  }

  let run = one(
    db,
    &format!(
      r#"SELECT "source", "title" FROM "role_slot_requests" WHERE "seat" = $1 AND "state" = 'active' AND {}
         ORDER BY "sequence" LIMIT 1"#,
      crate::watches::machine_turn::AT_WORK
    ),
    vec![agent.into()],
  )
  .await?;

  let Some(run) = run else {
    return Ok(None);
  };
  let source: String = run.try_get("", "source")?;
  let title: Option<String> = run.try_get("", "title")?;

  Ok(Some(match title.filter(|title| !title.trim().is_empty()) {
    Some(title) => format!("a {source} run, {title}"),
    None => format!("a {source} run"),
  }))
}

/// The work parts a run of `role` at a checkout is carried with on a road: its provider and model, and the
/// effort and answer ceiling the road's entry names for it.
pub(crate) async fn role_work_parts(
  ground: &Ground,
  role: &str,
  worker_name: &str,
  room: i64,
  name: &str,
  title: &str,
  entry: &Entry,
) -> Result<WorkParts, String> {
  // The entry names the provider, model, effort and answer ceiling the turn goes out with, whichever road it
  // is: an agent's own, an item's model, a repository's pin, or the road a role that names none follows.
  let provider = entry.provider.clone();
  let effort = entry.effort.clone();
  let answer_tokens = entry.answer_tokens;
  let model = entry.model.clone();
  let row = AgentRow::named(&ground.db, worker_name)
    .await
    .map_err(|why| why.detail)?
    .ok_or_else(|| format!("no worker is named {worker_name}"))?;
  let cwd = row.cwd.unwrap_or_else(|| ground.source.to_string_lossy().into_owned());
  let trees = Trees::of(ground);
  let mut line = worker::worker_run(ground, &trees, Some(worker_name), None, &model, &cwd)
    .await
    .map_err(|why| why.detail)?;
  line.effort = effort.clone().unwrap_or(line.effort);
  let mut variables = BTreeMap::new();
  variables.insert("BMO_DATA_DIR".to_owned(), ground.roots.data.to_string_lossy().into_owned());

  Ok(WorkParts {
    provider,
    model: model.clone(),
    mapping: WorkMapping {
      model,
      effort,
      tier: None,
      answer_tokens: answer_tokens.and_then(|tokens| u64::try_from(tokens).ok()),
    },
    resume: Resume::Auto,
    cwd,
    name: name.to_owned(),
    title: title.to_owned(),
    chat: room,
    unit: Some(name.to_owned()),
    user: None,
    work_key: role.to_owned(),
    hears_words: true,
    variables,
  })
}

// The jobs this process carries now, from their launch until their end is written: one waiting for a road, or
// between one road and the next, has no live execution behind it and is still no job the process lost. Each job
// counts the marks held on it, since a turn its end hands to the review may mark it again before the mark of the
// turn that ended is let go.
static CARRYING: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<i64, usize>>> =
  std::sync::LazyLock::new(Default::default);

/// Whether this process carries a turn of the job now.
pub fn is_carrying(id: i64) -> bool {
  carrying().contains_key(&id)
}

fn carrying() -> std::sync::MutexGuard<'static, std::collections::HashMap<i64, usize>> {
  CARRYING.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

// A mark that this process carries a turn of a job, let go as it drops.
struct Carrying(i64);

impl Carrying {
  fn of(id: i64) -> Self {
    *carrying().entry(id).or_default() += 1;

    Self(id)
  }
}

impl Drop for Carrying {
  fn drop(&mut self) {
    let mut held = carrying();

    if let Some(marks) = held.get_mut(&self.0) {
      *marks -= 1;

      if *marks == 0 {
        held.remove(&self.0);
      }
    }
  }
}

// The run of one job: the worker's session started, the item's turns carried across its roads, and the
// job's end written, its assignment released, and an end that is no answer noted in the job's log. A job
// ended from outside while it waited for a road keeps the end written for it. The job stays carried until its
// end is written, so no pass that ends a job left running takes it in between.
async fn launch(ground: Arc<Ground>, sessions: Option<crate::herdr_backend::Shared>, id: i64) {
  let _carried = Carrying::of(id);
  let ended = launched(&ground, sessions, id, None).await;

  finish(&ground, id, ended).await;
  crate::places::base_mains::after_turn(&ground);
}

/// A turn a job's review asks for (docs/job-review.md, 「The turns the review asks for」): carried as the job's
/// first turn is, in the job's own session where it can be resumed, and its end written as any job turn's end
/// is, back into the review where it answered.
pub async fn carry_review_turn(
  ground: Arc<Ground>,
  sessions: Option<crate::herdr_backend::Shared>,
  id: i64,
  turn: crate::job_review::Turn,
) {
  let _carried = Carrying::of(id);
  let ended = launched(&ground, sessions, id, Some(turn)).await;

  finish(&ground, id, ended).await;
  crate::places::base_mains::after_turn(&ground);
}

/// The words of the turn a job carries, kept on its row until that turn comes back from its session, or let go
/// (docs/api.md, 「assignments and jobs」): a server that starts launches a job whose turn had not gone out again
/// from them. A store that would not take them is said and stops nothing.
pub(crate) async fn keep_turn_words(db: &sea_orm::DatabaseConnection, id: i64, words: Option<&str>) {
  if let Err(why) = run(
    db,
    r#"UPDATE "jobs" SET "turn_words" = $1 WHERE "id" = $2"#,
    vec![words.map(str::to_owned).into(), id.into()],
  )
  .await
  {
    eprintln!("job {id}: the words of its turn were not kept: {why}");
  }
}

// The words of the turn a job carries, as its row keeps them.
async fn kept_turn_words(db: &sea_orm::DatabaseConnection, id: i64) -> Option<String> {
  one(db, r#"SELECT "turn_words" FROM "jobs" WHERE "id" = $1"#, vec![id.into()])
    .await
    .ok()
    .flatten()?
    .try_get::<Option<String>>("", "turn_words")
    .ok()
    .flatten()
}

/// Every job whose turn had not gone out when the process before this one went away, launched again from what
/// its row keeps (docs/api.md, 「assignments and jobs」): one still `starting`, one `running` with the words of
/// a turn that never came back from its session, and one `reviewing` whose review's turn was still taking its
/// checkout back. A job this process carries, or whose turn is kept in flight for the pickup, is left; so is a
/// `running` job whose checkout stands on another machine. Each is marked carried before this answers, so the
/// pass that ends a job left running, which runs next, leaves it. Answers how many were launched again.
pub async fn resume_launches(
  ground: &Arc<Ground>,
  sessions: Option<crate::herdr_backend::Shared>,
) -> usize {
  let rows = match all(
    &ground.db,
    r#"SELECT "id", "state", "worker_name", "run_name", "run_id", "report", "turn_words" FROM "jobs"
       WHERE "state" = 'starting'
          OR ("state" = 'running' AND "turn_words" IS NOT NULL)
          OR ("state" = 'reviewing' AND "review_turn" IS NOT NULL AND "turn_words" IS NOT NULL)
       ORDER BY "id""#,
    vec![],
  )
  .await
  {
    Ok(rows) => rows,
    Err(why) => {
      eprintln!("agency: the jobs whose turn had not gone out were not read: {why}");
      return 0;
    }
  };
  let mut resumed = 0;

  for row in rows {
    let Ok(id) = row.try_get::<i64>("", "id") else { continue };
    let state: String = row.try_get("", "state").unwrap_or_default();
    let worker: String = row.try_get("", "worker_name").unwrap_or_default();
    let run_name: Option<String> = row.try_get("", "run_name").ok().flatten();
    let run_id: Option<i64> = row.try_get("", "run_id").ok().flatten();
    let reported = row.try_get::<Option<String>>("", "report").ok().flatten().is_some();

    if is_carrying(id) {
      continue;
    }

    if let Some(run_name) = run_name.as_deref().filter(|name| !name.is_empty())
      && crate::turn_pickup::kept(&ground.db, run_name).await
    {
      continue;
    }

    if state == "running" && on_node(ground, &worker).await {
      println!("job {id}: not launched again: its worker {worker} stands on another machine");
      continue;
    }

    // A turn that went out and never came back left its run in the ledger open: it is closed as cut, and the
    // job's next turn opens its own.
    if state == "running"
      && let Some(run_id) = run_id.filter(|run| *run > 0)
      && run_ledger::read(&ground.db, run_id)
        .await
        .is_ok_and(|held| held.is_some_and(|held| held.finished_at.is_none()))
    {
      closed(ground, run_id, crate::run_sweep::CUT, None, None).await;
    }

    let carried = Carrying::of(id);

    noted_line(ground, id, RESUMED).await;
    println!("job {id}: launched again after agency restarted, {state}");

    let ground = ground.clone();
    let sessions = sessions.clone();

    // A job that never reported carries its first turn; one that did carries the turn its review asked for.
    if reported {
      tokio::spawn(async move {
        let _carried = carried;

        crate::job_review::resumed_turn(ground, sessions, id).await;
      });
    } else {
      tokio::spawn(async move {
        let _carried = carried;

        launch(ground, sessions, id).await;
      });
    }

    resumed += 1;
  }

  resumed
}

/// What a job launched again after agency's restart says in its log.
pub const RESUMED: &str =
  "agency restarted before this job's turn went out, so the job is launched again from what its row keeps.";

// Whether a job's checkout stands on another machine, whose runs are no record this machine keeps.
async fn on_node(ground: &Ground, worker: &str) -> bool {
  crate::queue::workers::worker_row(&ground.db, worker, &ground.source)
    .await
    .ok()
    .flatten()
    .and_then(|checkout| checkout.node)
    .is_some_and(|node| !node.trim().is_empty())
}

/// A job's turn that ended after the process that launched it went away, ended as the launch would have
/// ended it (docs/api.md, 「assignments and jobs」): its run closed with the turn's end, the job ended with
/// its report, its assignment released, its lease returned and its end told to the room.
pub async fn job_turn_ended(ground: &Ground, id: i64, end: agent_session::TurnEnd) {
  let ledger = match job(&ground.db, id).await {
    Ok((member, _)) => member["runId"].as_i64().unwrap_or_default(),
    Err(why) => {
      eprintln!("job {id}: its turn's end was not written: {}", why.error);
      return;
    }
  };
  let ended = ended_of(ground, ledger, end).await;

  finish(ground, id, Ok(Some(ended))).await;
}

// A job's end written whole, however its turn came to it: a job a stop marked ends stopped with the stop's
// reason, and nothing is written where the job was ended from outside, while it waited for a road or after.
async fn finish(ground: &Ground, id: i64, ended: Result<Option<Ended>, String>) {
  let stop = crate::queue::stops::job_stop_reason(&ground.db, id).await.ok().flatten();
  let _ = crate::queue::stops::clear_job_stop(&ground.db, id).await;

  if ended_outside(ground, id).await {
    return;
  }

  let (state, outcome, report, spent) = match (stop, ended) {
    (Some(why), ended) => {
      let ended = ended.ok().flatten();

      // The turn's answer to the stop words is the job's handoff (docs/sessions.md, 「The handoff a job's
      // session leaves」); a turn that ended with none leaves its run log instead.
      let answer = ended.as_ref().and_then(|(state, outcome, report, _)| {
        (state == "ended" && outcome.is_none()).then_some(report.as_deref()).flatten()
      });
      let handoff = match crate::job_handoff::answered(answer) {
        Some(handoff) => handoff,
        None => crate::job_handoff::from_log(&ground.db, id, &JOB_HANDOFF.why.no_answer).await,
      };

      job_handoff_noted(ground, id, &handoff).await;

      let spent = ended.and_then(|(_, _, _, spent)| spent);

      println!("job {id}: stop consumed: {}", why.trim());

      ("stopped".to_owned(), Some("stopped".to_owned()), Some(why), spent)
    }
    (None, Ok(Some(end))) => end,
    (None, Ok(None)) => return,
    (None, Err(why)) => ("stopped".to_owned(), Some("failed".to_owned()), Some(why), None),
  };

  // What was said into the job's turn while it ran stands in its report, read off its run's own log.
  let report = match crate::words::with_words(&ground.db, id, report.clone()).await {
    Ok(report) => report,
    Err(why) => {
      eprintln!("job {id}: the words said into its turn were not added to its report: {}", why.detail);

      report
    }
  };

  // An answered turn ends no job: its report goes to the job's review (docs/job-review.md).
  if state == "ended" && outcome.is_none() {
    crate::job_review::reported(ground, id, report.as_deref().unwrap_or_default(), spent).await;

    return;
  }

  written_end(ground, id, &state, outcome, report, spent).await;
}

// A handoff noted on the item of the job it was written for.
async fn job_handoff_noted(ground: &Ground, id: i64, handoff: &crate::job_handoff::Handoff) {
  if let Ok((member, _)) = job(&ground.db, id).await
    && let Some(item) = member["itemId"].as_i64()
  {
    crate::job_handoff::noted(ground, id, item, handoff).await;
  }
}

/// A job's end written: its state, outcome and report, what its last turn spent added to the rest, its slot
/// given back, its assignment released, and its end delivered to its item's orderer (docs/api.md, 「assignments
/// and jobs」).
pub(crate) async fn written_end(
  ground: &Ground,
  id: i64,
  state: &str,
  outcome: Option<String>,
  report: Option<String>,
  spent: Option<Spent>,
) {
  let now = now_ms();

  println!("job {id}: ended {state}, outcome {}", outcome.as_deref().unwrap_or("answered"));

  if let Some(outcome) = &outcome {
    noted(ground, id, outcome, report.as_deref()).await;
  }

  if let Err(why) = run(
    &ground.db,
    &format!(
      r#"UPDATE "jobs" SET "state" = $1, "outcome" = $2, "report" = $3, "ended_at" = $5, {SPENT_ADDED}
         WHERE "id" = $6"#
    ),
    vec![
      state.to_owned().into(),
      outcome.clone().into(),
      report.clone().into(),
      spent.as_ref().map_or(0, Spent::total).into(),
      now.into(),
      id.into(),
      spent.map(|spent| spent.input_tokens).into(),
      spent.map(|spent| spent.output_tokens).into(),
      spent.map(|spent| spent.cache_read_tokens).into(),
      spent.map(|spent| spent.cache_write_tokens).into(),
    ],
  )
  .await
  {
    eprintln!("job {id}: its end was not written: {why}");
  }

  add_job_cost(&ground.db, id, spent, None).await;
  crate::job_end::slot_given_back(&ground.db, &ground.events, id, &ending(state, outcome.as_deref())).await;

  if let Ok((member, _)) = job(&ground.db, id).await {
    let assignment_id = member["assignmentId"].as_i64().unwrap_or_default();

    if let Err(why) = release(&ground.db, assignment_id).await {
      eprintln!("job {id}: its assignment {assignment_id} was not released: {}", why.error);
    }

    crate::job_end::delivered(ground, &member).await;
    publish(ground, if state == "ended" { "job.ended" } else { "job.changed" }, member);
  }
}

/// What a job's turn spent added to what its earlier turns spent: `$4` the total, `$7` to `$10` the input,
/// output, cache-read and cache-write tokens, each figure left as it stood where the turn reported none.
pub const SPENT_ADDED: &str = r#""token_count" = COALESCE("token_count", 0) + $4,
  "input_tokens" = CASE WHEN $7::bigint IS NULL THEN "input_tokens" ELSE COALESCE("input_tokens", 0) + $7 END,
  "output_tokens" = CASE WHEN $8::bigint IS NULL THEN "output_tokens" ELSE COALESCE("output_tokens", 0) + $8 END,
  "cache_read_tokens" = CASE WHEN $9::bigint IS NULL THEN "cache_read_tokens"
    ELSE COALESCE("cache_read_tokens", 0) + $9 END,
  "cache_write_tokens" = CASE WHEN $10::bigint IS NULL THEN "cache_write_tokens"
    ELSE COALESCE("cache_write_tokens", 0) + $10 END"#;

/// What a job's turns cost added to its row, in millionths of a dollar, and the supply source its last turn took
/// (docs/run-records.md, 「The run ledger」); nothing where the turn reported no cost and took no source.
pub async fn add_job_cost(
  db: &sea_orm::DatabaseConnection,
  id: i64,
  spent: Option<Spent>,
  source: Option<&str>,
) {
  let cost = spent.map(|spent| spent.cost_micros).filter(|cost| *cost > 0);

  if cost.is_none() && source.is_none() {
    return;
  }

  if let Err(why) = run(
    db,
    r#"UPDATE "jobs" SET
         "cost_micros" = CASE WHEN $2::bigint IS NULL THEN "cost_micros" ELSE COALESCE("cost_micros", 0) + $2 END,
         "supply_source" = COALESCE($3, "supply_source")
       WHERE "id" = $1"#,
    vec![id.into(), cost.into(), source.map(str::to_owned).into()],
  )
  .await
  {
    eprintln!("job {id}: its cost was not written: {}", why.detail);
  }
}

// The reason a job's end gives its slot back with: its state and, where it names one, its outcome.
fn ending(state: &str, outcome: Option<&str>) -> String {
  match outcome.filter(|outcome| !outcome.is_empty() && *outcome != state) {
    Some(outcome) => format!("{state}, {outcome}"),
    None => state.to_owned(),
  }
}

/// How long after it began a job is old enough to be read as gone when no live execution stands behind it:
/// past any launch window, so a launch still placing its turn is never mistaken for one the process lost.
pub const REAP_GRACE_MS: i64 = 2 * 60 * 1000;

/// What a stopped job's report says where its stop named no reason.
pub const STOPPED: &str = "The job was stopped from outside before it finished.";

/// What a job with no live execution behind it is told: the process that ran it went away.
pub const REAPED: &str = "The agency process that ran this job went away before the job could write its own end (a restart), so the job is stopped.";

// A job's live execution: an open execution row of its worker's, laid on the worker line, begun at the job's
// own start. The worker runs one job at a time, so one that begins in the job's first minute is its own.
async fn live_execution(
  db: &sea_orm::DatabaseConnection,
  worker: &str,
  started_at: i64,
) -> Result<bool, Refused> {
  Ok(
    one(
      db,
      r#"SELECT "id" FROM "agent_executions"
         WHERE ("body"::jsonb)->>'agent' = $1
           AND ("body"::jsonb)->>'line' = 'worker'
           AND (("body"::jsonb)->>'endedAt') IS NULL
           AND (("body"::jsonb)->>'startedAt')::bigint >= $2
           AND (("body"::jsonb)->>'startedAt')::bigint <= $3
         LIMIT 1"#,
      vec![worker.to_owned().into(), (started_at - 1_000).into(), (started_at + 60_000).into()],
    )
    .await?
    .is_some(),
  )
}

/// A job found `starting` or `running` with no live execution behind it, ended as stopped with the reason
/// (docs/api.md, 「assignments and jobs」): the process that ran it went away, so no later end would ever be
/// written. Its assignment is released, its open run closed, its lease returned and its report told to the
/// room, exactly as any other end. A job whose worker stands on another machine is left, since its execution
/// is no record this machine keeps. Answers how many jobs ended.
pub async fn reap_stale(ground: &Ground) -> Result<usize, Refused> {
  let now = now_ms();
  let rows = all(
    &ground.db,
    r#"SELECT "id", "worker_name", "assignment_id", "run_id", "run_name", "started_at", "state"
       FROM "jobs" WHERE "state" IN ('starting', 'running') ORDER BY "id""#,
    vec![],
  )
  .await?;
  let mut reaped = 0;

  for row in rows {
    let id: i64 = row.try_get("", "id")?;
    let worker: String = row.try_get("", "worker_name")?;
    let run_id: Option<i64> = row.try_get("", "run_id")?;
    let started_at: i64 = row.try_get("", "started_at")?;
    let state: String = row.try_get("", "state")?;

    if now - started_at < REAP_GRACE_MS || is_carrying(id) {
      continue;
    }

    // A worker on a node runs elsewhere: its execution is no row this machine holds.
    let node: Option<String> = crate::queue::workers::worker_row(&ground.db, &worker, &ground.source)
      .await?
      .and_then(|checkout| checkout.node)
      .filter(|node| !node.trim().is_empty());

    if node.is_some() {
      println!("job {id}: left {state}: its worker {worker} stands on {}", node.unwrap_or_default());
      continue;
    }

    // A turn kept in flight is the pickup's: it hands the job its end once the turn's record holds one.
    let run_name: Option<String> = row.try_get("", "run_name").ok().flatten();

    if let Some(run_name) = run_name.filter(|name| !name.is_empty())
      && crate::turn_pickup::kept(&ground.db, &run_name).await
    {
      continue;
    }

    let live = live_execution(&ground.db, &worker, started_at).await?;
    let run_open = match run_id.filter(|run| *run > 0) {
      Some(run) => run_ledger::read(&ground.db, run)
        .await
        .map(|held| held.is_some_and(|held| held.finished_at.is_none()))
        .unwrap_or(false),
      None => false,
    };

    if live && (run_open || run_id.is_none()) {
      continue;
    }

    let report = format!("{REAPED} It was {state} when the process ended.");

    if run(
      &ground.db,
      r#"UPDATE "jobs" SET "state" = 'stopped', "outcome" = 'stopped', "report" = $1, "ended_at" = $2
         WHERE "id" = $3 AND "state" IN ('starting', 'running')"#,
      vec![report.clone().into(), now.into(), id.into()],
    )
    .await?
      == 0
    {
      continue;
    }

    noted(ground, id, "stopped", Some("the agency process that ran it went away (a restart)")).await;
    crate::job_end::slot_given_back(
      &ground.db,
      &ground.events,
      id,
      "stopped: the agency process that ran it went away",
    )
    .await;

    if let Some(assignment) = row.try_get::<Option<i64>>("", "assignment_id").ok().flatten()
      && assignment > 0
      && let Err(why) = release(&ground.db, assignment).await
    {
      eprintln!("job {id}: its assignment {assignment} was not released: {}", why.error);
    }

    if let Some(run_id) = run_id.filter(|run| *run > 0) {
      let closing = Closing {
        finished_at: now,
        outcome: "stopped".to_owned(),
        context_tokens: None,
        spent: None,
      };

      if let Err(why) = run_ledger::close(&ground.db, run_id, &closing).await {
        println!("job {id}: its run {run_id} was not closed: {why:?}");
      }
    }

    if let Ok((member, _)) = job(&ground.db, id).await {
      crate::job_end::delivered(ground, &member).await;
      publish(ground, "job.changed", member);
    }

    reaped += 1;
  }

  Ok(reaped)
}

// A job's end that is no answer, written into its log under its run as a note: its outcome and why.
async fn noted(ground: &Ground, id: i64, outcome: &str, why: Option<&str>) {
  let Ok((member, _)) = job(&ground.db, id).await else {
    return;
  };
  let mut log = RunLog::new(Target {
    agent: member["workerName"].as_str().unwrap_or_default().to_owned(),
    run: member["runName"].as_str().map_or_else(|| format!("job-{id}"), str::to_owned),
    chat_id: member["roomId"].as_i64(),
    job_id: Some(id),
  });
  let text = match why.map(str::trim).filter(|why| !why.is_empty()) {
    Some(why) => format!("The job ended {outcome}: {why}"),
    None => format!("The job ended {outcome}, and its turn gave no reason."),
  };

  log.write(&ground.db, Line::note(text)).await;
}

type Ended = (String, Option<String>, Option<String>, Option<Spent>);

// What a job's report says where its turn stopped on a tool call written as text.
const TEXT_TOOL_CALL_REPORT: &str = "The turn stopped on a tool call written as text rather than made. The \
  call did not run, so the job left no report. Check that the tool it called is offered to the run, then \
  run the job again.";

/// What a job's end takes from its turn's end: the outcome — none where the turn answered — and the report
/// it leaves. An answer that is a tool call written as text is no report: the call never ran and the work
/// never happened, so the job ends failed with a line saying so rather than an empty outcome holding the
/// call's text (docs/api.md, 「A job ends by itself as its turn ends」).
fn ended_by(
  outcome: agent_session::Outcome,
  answer: Option<String>,
  reason: Option<String>,
) -> (Option<String>, Option<String>) {
  if matches!(outcome, agent_session::Outcome::Answered)
    && answer.as_deref().is_some_and(crate::door::written_call::text_tool_call)
  {
    return (Some("failed".to_owned()), Some(TEXT_TOOL_CALL_REPORT.to_owned()));
  }

  let outcome = match outcome {
    agent_session::Outcome::Answered => None,
    other => Some(format!("{other:?}").to_lowercase()),
  };
  let report = match outcome {
    None => answer,
    Some(_) => answer.or(reason),
  };

  (outcome, report)
}

/// What carries one turn of a job on one road: the worker's session through the session contract, or a
/// test's stand-in for it. A turn that could not be carried at all is its error; any turn that was is its
/// end, however it ended. The entry is the road's, with the effort and answer ceiling the turn goes out on.
pub trait Carrier {
  fn carry(
    &mut self,
    taken: &Road,
    entry: &Entry,
    prompt: &str,
  ) -> impl Future<Output = Result<TurnEnd, String>>;
}

/// The job a walk of roads carries: its id, its worker, the holder that asked it, its `holderName`, and the
/// prompt it was launched with. A holder the store holds a supervisor for is the supervisor a role that names
/// no road follows.
pub struct Carried<'a> {
  pub id: i64,
  pub worker: &'a str,
  pub holder: &'a str,
  pub prompt: &'a str,
}

/// Why a turn's end says its road stopped answering rather than the work failing, where it does
/// (docs/cache-fallback.md, 「Automatic walks only on providers, in order」; docs/provider-order.md, 「A road that fails」): the provider refusing its quota, its window, its balance or its credential, its
/// last login closing under it, answering with a server error or refusing the connection, or the road
/// switched off — read off the end's own failure, its engine's error, and its words through the one failure
/// reading every run asks. A turn that answered, was stopped from outside, or whose request the provider
/// refused for its shape never says so.
pub fn road_stopped(end: &TurnEnd) -> Option<String> {
  use agent_session::{AccountFailure, Failure, Outcome};

  if matches!(end.outcome, Outcome::Answered | Outcome::Stopped | Outcome::Reprepared) {
    return None;
  }

  let words = end
    .reason
    .clone()
    .or_else(|| end.answer.clone())
    .or_else(|| end.failure.as_ref().map(Failure::words))
    .unwrap_or_default();
  let said = || {
    Some(if words.trim().is_empty() {
      "its road stopped answering".to_owned()
    } else {
      words.clone()
    })
  };

  match &end.failure {
    Some(Failure::Stopped) => return None,
    Some(
      Failure::Limit { .. }
      | Failure::Unavailable { .. }
      | Failure::Account {
        failure: AccountFailure::Limit | AccountFailure::Login | AccountFailure::Outage,
        ..
      },
    ) => return said(),
    _ => {}
  }

  use crate::job_run::claude::failure::{self as reading, AccountFailure as Read};

  let engine = end.engine_error.as_ref().and_then(|engine| engine.kind.as_deref());

  if reading::request_refused(&words, engine) {
    return None;
  }

  if matches!(
    engine,
    Some(
      "rate_limit"
        | "billing_error"
        | "authentication_error"
        | "authentication_failed"
        | "server_error"
        | "overloaded"
    )
  ) {
    return said();
  }

  match reading::of_errored(&words, JOB_WORK_KEY) {
    reading::Failure::Limit { .. }
    | reading::Failure::Account {
      failure: Read::Limit | Read::Login | Read::Outage,
      ..
    } => said(),
    _ => None,
  }
}

/// The line a job's prompt is opened with as it goes out again on the next road, ahead of its own words.
pub fn continued(from: &Road, why: &str, prompt: &str) -> String {
  let line =
    policy::fill_marks(&policy::roads::ROADS.job.continued, &[("from", &from.words()), ("why", why)]);

  format!("{line}\n\n{prompt}")
}

// One road a job's turn went out on, as the job keeps and answers it.
fn road_value(taken: &Road, reason: Option<&str>) -> Value {
  json!({
    "provider": taken.provider,
    "model": taken.model,
    "account": taken.account,
    "at": now_ms(),
    "reason": reason,
  })
}

// The line a job's report ends with where its turns went out on more than one road: each road it left, why,
// and the road it ended on.
fn moved_line(roads: &[(Road, Option<String>)]) -> Option<String> {
  if roads.len() < 2 {
    return None;
  }

  let words = &policy::roads::ROADS.job;
  let mut parts = vec![];

  for (at, (taken, _)) in roads.iter().enumerate() {
    match roads.get(at + 1).and_then(|(_, why)| why.as_deref()) {
      Some(why) => parts.push(policy::fill_marks(&words.road, &[("road", &taken.words()), ("why", why)])),
      None => parts.push(taken.words()),
    }
  }

  Some(policy::fill_marks(&words.moved, &[("roads", &parts.join(", then "))]))
}

// A job's report with the account of its roads at its end, where it moved.
fn with_roads(report: Option<String>, roads: &[(Road, Option<String>)]) -> Option<String> {
  match (report, moved_line(roads)) {
    (Some(report), Some(moved)) => Some(format!("{report}\n\n{moved}")),
    (None, Some(moved)) => Some(moved),
    (report, None) => report,
  }
}

fn summed(spent: Option<Spent>, more: Option<Spent>) -> Option<Spent> {
  match (spent, more) {
    (Some(spent), Some(more)) => Some(Spent {
      input_tokens: spent.input_tokens + more.input_tokens,
      output_tokens: spent.output_tokens + more.output_tokens,
      cache_read_tokens: spent.cache_read_tokens + more.cache_read_tokens,
      cache_write_tokens: spent.cache_write_tokens + more.cache_write_tokens,
      cost_micros: spent.cost_micros + more.cost_micros,
    }),
    (spent, more) => spent.or(more),
  }
}

// The roads a job went out on written onto its row, with the model of the last and what chose it, and the
// job told.
async fn kept_roads(
  ground: &Ground,
  id: i64,
  roads: &[(Road, Option<String>)],
  values: &[Value],
  chosen_by: Option<road::ChosenBy>,
) {
  let model = roads.last().map(|(taken, _)| taken.model.clone());

  if let Err(why) = run(
    &ground.db,
    r#"UPDATE "jobs" SET "roads" = $1, "model" = COALESCE($2, "model"), "chosen_by" = COALESCE($4, "chosen_by")
       WHERE "id" = $3"#,
    vec![
      Value::Array(values.to_vec()).to_string().into(),
      model.into(),
      id.into(),
      chosen_by.map(|chosen| chosen.word().to_owned()).into(),
    ],
  )
  .await
  {
    eprintln!("job {id}: its roads were not written: {why}");
  }
}

/// A road a job's turn moved to inside its provider — the next Claude account, the one before it passed over
/// for its own failure (docs/cache-fallback.md, 「The Claude Code road is a list of accounts」) — added to
/// the job's roads with why the one before was left.
pub async fn add_road(ground: &Ground, id: i64, taken: &Road, reason: &str) {
  let mut values = roads_kept(ground, id).await.map(|(_, values)| values).unwrap_or_default();

  values.push(road_value(taken, Some(reason)));

  if let Err(why) = run(
    &ground.db,
    r#"UPDATE "jobs" SET "roads" = $1 WHERE "id" = $2"#,
    vec![Value::Array(values).to_string().into(), id.into()],
  )
  .await
  {
    eprintln!("job {id}: the road its turn moved to was not written: {why}");
  }
}

// The roads a job's row keeps, each with why the one before it was left, as the walk and the carrying wrote
// them; none where the row keeps none it can read.
async fn roads_kept(ground: &Ground, id: i64) -> Option<(Vec<(Road, Option<String>)>, Vec<Value>)> {
  let row = one(&ground.db, r#"SELECT "roads" FROM "jobs" WHERE "id" = $1"#, vec![id.into()])
    .await
    .ok()
    .flatten()?;
  let text: String = row.try_get::<Option<String>>("", "roads").ok().flatten()?;
  let values: Vec<Value> = serde_json::from_str(&text).ok()?;
  let roads = values
    .iter()
    .map(|value| {
      let road = serde_json::from_value::<Road>(value.clone()).ok()?;
      let reason = value["reason"].as_str().map(str::to_owned);

      Some((road, reason))
    })
    .collect::<Option<Vec<_>>>()?;

  Some((roads, values))
}

// That a job waits for a road, with the reason, or no longer does; the job told either way, and its item's
// orderer as the wait begins.
async fn held_for(ground: &Ground, id: i64, why: Option<&str>) {
  hold_job(ground, id, why, why.is_some()).await;
}

/// That a job waits, with the reason — for a road, or for its turn's card (docs/own-llm.md, 「An engine the
/// arbiter stands」) — or no longer does: its `heldFor` written and the job published, and its item's orderer
/// told of the wait where `tell` says this is where the wait begins.
pub async fn hold_job(ground: &Ground, id: i64, why: Option<&str>, tell: bool) {
  // A wait told begins unowed until its notice's row is written, so the sender owes one a stop cut off.
  if let Err(why) = run(
    &ground.db,
    r#"UPDATE "jobs" SET "held_for" = $1,
         "wait_unowed_at" = CASE WHEN $1 IS NULL THEN NULL WHEN $3 THEN $4 ELSE "wait_unowed_at" END
       WHERE "id" = $2"#,
    vec![
      why.map(str::to_owned).into(),
      id.into(),
      (why.is_some() && tell).into(),
      now_ms().into(),
    ],
  )
  .await
  {
    eprintln!("job {id}: its hold was not written: {why}");
  }

  if let Ok((member, _)) = job(&ground.db, id).await {
    if let Some(why) = why.filter(|_| tell) {
      tell_waiting(ground, &member, why).await;
    }

    publish(ground, "job.changed", member);
  }
}

// A job's wait told to its item's orderer as it begins (docs/agent-control.md, 「An item's change is told to its
// orderer」); a test's ground tells no one.
async fn tell_waiting(ground: &Ground, member: &Value, why: &str) {
  if ground.testing() {
    return;
  }

  crate::job_notices::tell_waiting(&ground.db, ground.vars(), member, why).await;
}

// Whether a job was ended or stopped from outside while this process carried it.
async fn ended_outside(ground: &Ground, id: i64) -> bool {
  job(&ground.db, id)
    .await
    .is_ok_and(|(member, _)| matches!(member["state"].as_str(), Some("ended" | "stopped")))
}

// Whether a job stopped from outside, or asked to stop, is to go no further while it waits.
pub(crate) async fn called_off(ground: &Ground, id: i64) -> bool {
  ended_outside(ground, id).await
    || crate::queue::stops::job_stop_reason(&ground.db, id).await.ok().flatten().is_some()
}

// A job waiting for a road: held with the closed road's sentence and asked again as quota or a road moves, and
// on the policy's clock, until a provider of its walk can answer. Where `blocked` names the road whose provider
// answered that it cannot serve now, the wait holds while that answer's block stands, since no other reading of
// the road sees the answer. False where the job was ended from outside, or asked to stop, while it waited.
async fn wait_for_road(
  ground: &Ground,
  job: &Carried<'_>,
  resolution: &road::Resolution,
  blocked: Option<&Road>,
) -> Result<bool, String> {
  // Asked again as each usage reading, account change, rest or road fact comes in, and on the policy's clock
  // where none does.
  let mut moved = crate::order::quota_watch(ground);
  let sentence = resolution.no_road.clone().unwrap_or_else(|| resolution.sentence());

  held_for(ground, job.id, Some(&sentence)).await;
  // A role with no road to follow has no walk to report: the sentence is the whole of it.
  if resolution.no_road.is_none() {
    road::went(ground, resolution, None, &[]).await;
  }
  noted_line(ground, job.id, &sentence).await;

  let every = std::time::Duration::from_secs(policy::roads::ROADS.job.wait_seconds);

  loop {
    if let Ok(None) = tokio::time::timeout(every, moved.next()).await {
      tokio::time::sleep(every).await;
    }

    if called_off(ground, job.id).await {
      held_for(ground, job.id, None).await;

      return Ok(false);
    }

    // The road a provider answered it cannot serve now stays closed while the block that answer wrote stands:
    // the road's own availability says it answers, so only its block holds the job.
    if let Some(road) = blocked
      && road::block_until(ground, road).await.is_some()
    {
      continue;
    }

    let item = item_model_of_job(ground, job.id).await;
    let asked_by = Some(job.holder).filter(|name| !name.is_empty());

    if road::for_job(
      ground,
      Some(&role_of_job(ground, job.id).await),
      Some(job.worker),
      item.as_ref(),
      &[],
      asked_by,
    )
    .await?
    .road()
    .is_some()
    {
      held_for(ground, job.id, None).await;
      noted_line(ground, job.id, "A road is open, going out").await;

      return Ok(true);
    }
  }
}

/// A job carried across its roads (docs/cache-fallback.md, 「One road resolution」): each turn goes out on
/// the road the resolution answers for the job's worker now; a turn whose road stopped answering is carried
/// again on the next road, the same job in the same checkout, at most once on each provider, its prompt
/// opened with the line saying it continues the work; where nothing in the order can answer, the job
/// waits, held with the reason, until a provider can; and where the provider answered that it cannot serve
/// now — a 5xx, a refused connection — the job waits for that answer's block to open, its sentence in
/// `heldFor`, rather than ending as the turn did — unless its last turn ended on every Claude account
/// refusing it, which ends the job on that login failure. The end it answers carries every road's spending
/// and, in its report, where the job ran and why it moved, the Claude accounts its turns moved across among
/// them; none where the job was ended from outside.
pub async fn carry_job<C: Carrier>(
  ground: &Ground,
  job: &Carried<'_>,
  carrier: &mut C,
) -> Result<Option<Ended>, String> {
  // A support worker's worktree stands before any road takes its turn, since the turn's program is stood
  // in it and a program handed a folder that is not there never starts (docs/worker-slots.md, 「Names and
  // places」).
  crate::places::checkouts::ensure_support_checkout(ground, job.worker).await?;

  let role = role_of_job(ground, job.id).await;
  let role = Some(role.as_str());
  let asked_by = Some(job.holder).filter(|name| !name.is_empty());
  let item = item_model_of_job(ground, job.id).await;
  let item = item.as_ref();
  let mut left: Vec<Closed> = vec![];
  let mut roads: Vec<(Road, Option<String>)> = vec![];
  let mut values: Vec<Value> = vec![];
  let mut spent: Option<Spent> = None;
  let mut prompt = job.prompt.to_owned();
  let mut last: Option<Ended> = None;
  // The kind of failure the last turn left its road for, where it left one.
  let mut last_stop: Option<road::BlockKind> = None;
  // Whether the last turn ended on every Claude account of its walk refusing it.
  let mut refused_everywhere = false;
  // Whether the job waits for supply, told once for the wait (docs/supply.md, 「Running out mid-turn」).
  let mut supply_waits = false;
  // The provider and model of a model the person owns that refused the job's last turn, which marks no key of
  // theirs, so the job's next choice on it passes their personal supply over (docs/supply.md, 「Owned supply」).
  let mut owned_refused: Option<(String, String)> = None;

  keep_turn_words(&ground.db, job.id, Some(&prompt)).await;

  loop {
    let resolution = road::for_job(ground, role, Some(job.worker), item, &left, asked_by).await?;
    let Some((taken, entry)) = resolution.going().map(|(road, entry)| (road.clone(), entry.clone())) else {
      // A provider that answered it cannot serve now — a 5xx, a refused connection — is waited for rather
      // than ended as that turn's failure: the road's own availability does not read that answer, so the
      // answer's block is what holds the job (docs/cache-fallback.md, 「Who receives that failure」).
      let answered_it_cannot_serve = last_stop == Some(road::BlockKind::Outage);

      // Every road that can answer now was already tried in this job, or every account of the last refused
      // it, which only a person settles: it ends as its last turn did.
      if !answered_it_cannot_serve
        && let Some((state, outcome, report, _)) = last.take()
        && (refused_everywhere
          || road::for_job(ground, role, Some(job.worker), item, &[], asked_by)
            .await?
            .road()
            .is_some())
      {
        return Ok(Some((state, outcome, with_roads(report, &roads), spent)));
      }

      let blocked = if answered_it_cannot_serve { roads.last().map(|(road, _)| road) } else { None };

      if !wait_for_road(ground, job, &resolution, blocked).await? {
        return Ok(None);
      }

      left.clear();
      last = None;
      last_stop = None;
      continue;
    };
    // The supply the turn goes out on inside its road: a person's job waits, holding no slot, where none is
    // left (docs/supply.md, 「Choosing a source as a run starts」).
    let passed = owned_refused
      .as_ref()
      .is_some_and(|(provider, model)| *provider == taken.provider && *model == taken.model);
    let chosen = match supply_of_turn(ground, job, &taken, passed).await? {
      crate::supply::Choice::Go(chosen) => chosen,
      crate::supply::Choice::Closed(closed) => {
        if !wait_for_supply(ground, job, &taken, &closed, &mut supply_waits).await? {
          return Ok(None);
        }

        // A beat of the wait past, the model the person owns is asked again, as a refused key is once its
        // person sets it again.
        owned_refused = None;
        continue;
      }
    };

    if std::mem::take(&mut supply_waits) {
      held_for(ground, job.id, None).await;
    }

    let why = roads.last().and_then(|_| left.last().map(|left| left.reason.clone()));
    // A turn that goes out again on the road the last one left, its provider having answered it cannot serve
    // now, has not moved: the road is written once, and the job's report names no move.
    let moved = roads.last().is_none_or(|(last, _)| last != &taken);

    road::went(ground, &resolution, Some(&taken), &[]).await;

    if moved {
      values.push(road_value(&taken, why.as_deref()));
      roads.push((taken.clone(), why));
      kept_roads(ground, job.id, &roads, &values, resolution.chosen_by).await;
    }

    let carried = Box::pin(carrier.carry(&taken, &entry, &prompt));
    let (end, ran_out) = Box::pin(metered_carry(ground, job, &taken, &chosen, carried)).await?;
    let end = end?;

    // A platform source with nothing left stopped the turn: the job waits for supply and carries on in the same
    // checkout once a source takes it (docs/supply.md, 「Running out mid-turn」).
    if let Some(closed) = ran_out {
      spent = summed(spent, turn_spent(&end));
      noted_line(ground, job.id, &closed.sentence).await;
      prompt = crate::job_handoff::ahead(
        &crate::job_handoff::road_stopped(ground, job.id).await.text,
        &continued(&taken, &closed.reason, job.prompt),
      );
      keep_turn_words(&ground.db, job.id, Some(&prompt)).await;

      if !wait_for_supply(ground, job, &taken, &closed, &mut supply_waits).await? {
        return Ok(Some((
          "ended".to_owned(),
          Some("stopped".to_owned()),
          with_roads(Some(closed.sentence), &roads),
          spent,
        )));
      }

      continue;
    }

    // The walk inside the turn keeps each Claude account it moved the turn to on the job's row as it moves.
    if let Some((kept, kept_values)) = roads_kept(ground, job.id).await
      && kept_values.len() > values.len()
    {
      roads = kept;
      values = kept_values;
    }

    spent = summed(spent, turn_spent(&end));

    let (outcome, report) = ended_by(end.outcome, end.answer.clone(), end.reason.clone());

    // A job asked to stop goes out on no further road, whatever its turn came to.
    let stopped = match road_stopped(&end) {
      Some(stopped) if !called_off(ground, job.id).await => stopped,
      _ => return Ok(Some(("ended".to_owned(), outcome, with_roads(report, &roads), spent))),
    };

    // The road the turn ended on: the Claude account its walk last moved it to, where it moved.
    let ended_on = roads.last().map_or_else(|| taken.clone(), |(road, _)| road.clone());

    // A person's own credential, or their organisation's, the provider refused passes over to the next source on the
    // same road, in a new session, and blocks no road (docs/supply.md, 「Running out mid-turn」).
    if matches!(
      chosen.source,
      crate::supply::Source::Personal
        | crate::supply::Source::Owned
        | crate::supply::Source::Organisation { .. }
    ) && matches!(
      road::BlockKind::of_words(&stopped),
      Some(road::BlockKind::Access | road::BlockKind::Limit { .. })
    ) {
      personal_refused(ground, job, &chosen, &stopped).await;
      if matches!(chosen.source, crate::supply::Source::Owned) {
        owned_refused = Some((chosen.provider.clone(), chosen.model.clone()));
      }
      noted_line(
        ground,
        job.id,
        &format!("The job's own supply on {} was refused: {stopped}", ended_on.words()),
      )
      .await;

      let handoff = crate::job_handoff::road_stopped(ground, job.id).await;

      prompt = crate::job_handoff::ahead(&handoff.text, &continued(&ended_on, &stopped, job.prompt));
      keep_turn_words(&ground.db, job.id, Some(&prompt)).await;
      refused_everywhere = false;
      last_stop = road::BlockKind::of_words(&stopped);
      last = Some(("ended".to_owned(), outcome, report, None));
      continue;
    }

    noted_line(
      ground,
      job.id,
      &format!("The job's road {} stopped answering: {stopped}", ended_on.words()),
    )
    .await;
    road::leave(ground, &taken, &stopped).await;

    // A road that stopped answering cannot be asked for the session's handoff: the job's run log stands for
    // it, noted on the item and opening the turn on the next road (docs/sessions.md, 「The handoff a job's
    // session leaves」).
    let handoff = crate::job_handoff::road_stopped(ground, job.id).await;

    prompt = crate::job_handoff::ahead(&handoff.text, &continued(&ended_on, &stopped, job.prompt));
    keep_turn_words(&ground.db, job.id, Some(&prompt)).await;
    last_stop = road::BlockKind::of_words(&stopped);
    left.push(Closed::plain(&taken.provider, stopped));
    refused_everywhere = matches!(
      &end.failure,
      Some(agent_session::Failure::Account {
        failure: agent_session::AccountFailure::Login,
        ..
      })
    );
    last = Some(("ended".to_owned(), outcome, report, None));
  }
}

/// The supply a job's turn takes on the road it goes out on (docs/supply.md, 「Choosing a source as a run
/// starts」): the source chosen, kept for its launch and its walk under its checkout and its job and written on its
/// row; or, where none is left, why.
/// The supply a job's turn goes out on; where `owned_passed`, a model the person owns refused the job on this
/// road, and its personal supply is passed over to the rest of its order.
async fn supply_of_turn(
  ground: &Ground,
  job: &Carried<'_>,
  taken: &Road,
  owned_passed: bool,
) -> Result<crate::supply::Choice, String> {
  let account = crate::supply::account_of(ground, job.holder).await.map_err(|why| why.detail)?;
  let room = room_of_job(ground, job.id).await;
  let asked = crate::supply::Asked {
    kind: crate::supply::Kind::Job,
    id: job.id,
    agent: job.worker,
    account,
    provider: &taken.provider,
    model: &taken.model,
    job: Some(job.id),
    supervisor: Some(job.holder).filter(|holder| !holder.is_empty()),
    room,
  };
  let mut choice = crate::supply::choose(ground, &asked).await.map_err(|why| why.detail)?;

  if owned_passed
    && let crate::supply::Choice::Go(chosen) = &choice
    && matches!(chosen.source, crate::supply::Source::Owned)
  {
    choice = crate::supply::pass_owned(ground, chosen).await.map_err(|why| why.detail)?;
  }

  if let crate::supply::Choice::Go(chosen) = &choice {
    let told = crate::supply::told::Told {
      supervisor: Some(job.holder.to_owned()).filter(|holder| !holder.is_empty()),
      room,
    };

    crate::supply::settle_choice(ground, job.worker, chosen, Some(job.id), &told).await;
    crate::supply::remember(&ground.roots.data, &crate::supply::job_key(job.id), chosen);
    add_job_cost(&ground.db, job.id, None, Some(chosen.source.word())).await;
  }

  Ok(choice)
}

/// The room a job answers in, where its row names one.
async fn room_of_job(ground: &Ground, id: i64) -> Option<i64> {
  one(&ground.db, r#"SELECT "room_id" FROM "jobs" WHERE "id" = $1"#, vec![id.into()])
    .await
    .ok()
    .flatten()?
    .try_get::<Option<i64>>("", "room_id")
    .ok()
    .flatten()
}

/// A job with no supply left held with the sentence that says so, holding no slot, for one beat of the supply's
/// wait, told once for the wait: answers whether it goes on to ask again, false where it was called off.
async fn wait_for_supply(
  ground: &Ground,
  job: &Carried<'_>,
  taken: &Road,
  closed: &crate::supply::Closed,
  waits: &mut bool,
) -> Result<bool, String> {
  if !std::mem::replace(waits, true) {
    held_for(ground, job.id, Some(&closed.sentence)).await;
    noted_line(ground, job.id, &closed.sentence).await;

    let account = crate::supply::account_of(ground, job.holder).await.map_err(|why| why.detail)?;
    let told = crate::supply::told::Told {
      supervisor: Some(job.holder.to_owned()).filter(|holder| !holder.is_empty()),
      room: room_of_job(ground, job.id).await,
    };

    crate::supply::told::closed(
      ground,
      &crate::supply::job_key(job.id),
      account,
      Some(job.id),
      &taken.provider,
      &taken.model,
      &told,
      closed,
    )
    .await;
  }

  tokio::time::sleep(std::time::Duration::from_secs(policy::supply::SUPPLY.wait_seconds.max(1))).await;

  if called_off(ground, job.id).await {
    held_for(ground, job.id, None).await;

    return Ok(false);
  }

  Ok(true)
}

/// A job's turn carried with the meter on its reservation, where it went out on one: the turn's end, and where
/// the meter found no source left, why, the turn cancelled as its program is asked to stop. The reservation is
/// settled as the turn ends.
async fn metered_carry<F>(
  ground: &Ground,
  job: &Carried<'_>,
  taken: &Road,
  chosen: &crate::supply::Chosen,
  carry: F,
) -> Result<(Result<TurnEnd, String>, Option<crate::supply::Closed>), String>
where
  F: std::future::Future<Output = Result<TurnEnd, String>>,
{
  use crate::supply::meter::{self, Reading};

  let since = now_ms();
  let reading = if taken.provider == "claude-code" {
    Reading::JobStatusLine {
      job: job.id,
      worker: job.worker.to_owned(),
      since,
    }
  } else {
    Reading::Gateway {
      run: job.worker.to_owned(),
      since,
    }
  };
  let Some(shared) =
    meter::metered(chosen, reading, job.worker, Some(job.id), room_of_job(ground, job.id).await)
  else {
    return Ok((carry.await, None));
  };
  let watch = meter::watch(ground, shared.clone());

  tokio::pin!(carry);
  tokio::pin!(watch);

  let (end, ran_out) = tokio::select! {
    end = &mut carry => (end, None),
    closed = &mut watch => {
      let closed = Some(closed);

      if let Ok((member, _)) = crate::control::work::job(&ground.db, job.id).await
        && let Some(name) = member["runName"].as_str().filter(|name| !name.is_empty())
      {
        let _ = crate::queue::stops::Stops::of(ground, None).cancel(name).await;
      }

      (carry.await, closed)
    }
  };

  if let Err(why) = meter::finish(ground, &shared).await {
    eprintln!("job {}: its supply reservation was not settled: {why}", job.id);
  }

  Ok((end, ran_out))
}

/// A person's own credential refused under a job's turn: a gateway road's key waits for its person, told once
/// to their own supervisor room; a Claude login's walk already marked it, and a model they own marks no key,
/// since none was used (docs/supply.md, 「Owned supply」).
async fn personal_refused(ground: &Ground, job: &Carried<'_>, chosen: &crate::supply::Chosen, why: &str) {
  if chosen.account.is_none() {
    return;
  }

  if let Err(refused) = crate::supply::refuse_personal_key(ground, chosen).await {
    eprintln!("job {}: its refused key was not marked: {}", job.id, refused.detail);
  }

  let told = crate::supply::told::Told {
    supervisor: Some(job.holder.to_owned()).filter(|holder| !holder.is_empty()),
    room: room_of_job(ground, job.id).await,
  };

  crate::supply::told::refused(ground, &told, &chosen.run, &chosen.provider, why).await;
}

/// The agent a job's item names, as its row keeps it from the launch (docs/repositories.md, 「An agent an item
/// names」); none where its item named none.
pub async fn item_agent_of_job(ground: &Ground, id: i64) -> Option<String> {
  one(&ground.db, r#"SELECT "item_agent" FROM "jobs" WHERE "id" = $1"#, vec![id.into()])
    .await
    .ok()
    .flatten()?
    .try_get::<Option<String>>("", "item_agent")
    .ok()
    .flatten()
    .filter(|name| !name.is_empty())
}

/// The model a job's item names, as its row keeps it from the launch (docs/repositories.md, 「A model an item
/// names」); none where its item named none.
pub async fn item_model_of_job(ground: &Ground, id: i64) -> Option<Pin> {
  let row = one(
    &ground.db,
    r#"SELECT "item_provider", "item_model" FROM "jobs" WHERE "id" = $1"#,
    vec![id.into()],
  )
  .await
  .ok()
  .flatten()?;

  Pin::of_parts(
    row.try_get("", "item_provider").ok().flatten(),
    row.try_get("", "item_model").ok().flatten(),
  )
}

/// What the item `POST /jobs` is handed names its job's agent by (docs/repositories.md, 「An agent an item
/// names」), before the launch takes one: the agent its `agent` label names alone, or the candidates of its role
/// for the model its `model` label names, by name.
struct Seat {
  candidates: Vec<crate::role_agents::RoleAgent>,
  /// The role and the model asked, for an item naming a model.
  asked: Option<(String, String)>,
}

impl Seat {
  /// The agent the job is launched on, taken inside the transaction that writes the job: every candidate's row
  /// held in name order, so a launch at once with this one waits and then finds the agent this one took busy;
  /// then the first candidate free (docs/role-agents.md, 「When an agent is busy」). 409 `agent-busy` where none
  /// is, naming what each carries.
  async fn take<C: sea_orm::ConnectionTrait>(
    self,
    db: &C,
    item_id: i64,
  ) -> Result<crate::role_agents::RoleAgent, Refused> {
    let Seat { candidates, asked } = self;

    for agent in &candidates {
      one(
        db,
        r#"SELECT 1 FROM "role_agents" WHERE "name" = $1 FOR UPDATE"#,
        vec![agent.name.clone().into()],
      )
      .await?;
    }

    let mut carried: Vec<String> = vec![];

    for agent in candidates {
      match agent_carries(db, &agent.name).await? {
        None => return Ok(agent),
        Some(what) => carried.push(format!("{}, which carries {what}", agent.name)),
      }
    }

    let sentence = match asked {
      None => format!(
        "Item #{item_id} names the agent {}; launch it once the agent is free",
        carried.join("; ")
      ),
      Some((role, model)) => format!(
        "Item #{item_id} names {MODEL_LABEL}={model} for the {role} role, and every agent of that role on it is \
         busy: {}; launch it once one of them is free",
        carried.join("; ")
      ),
    };

    Err(Refused::new(StatusCode::CONFLICT, sentence).with_code("agent-busy"))
  }
}

/// What the item `POST /jobs` is handed names its job's agent by (docs/repositories.md, 「An agent an item
/// names」): the agent its `agent` label names, which stands over a `model` label; else the candidates of `role`
/// for the model its `model` label names, of the launching supervisor's `account`. 422 `agent-unnamed` for
/// neither, listing the agents of the role it would run as so its supervisor names one; 422 `unknown-agent` for
/// an agent the agency does not register; 422 `agent-names-no-road` for one that follows a supervisor's road,
/// since no supervisor asks a job; and 422 `no-candidate` for a model no agent of the role runs. A refusal is
/// told to the item's orderer once for that label, or for that role and model.
async fn item_seat(
  ground: &Arc<Ground>,
  item_id: i64,
  item: &Value,
  role: &str,
  account: Option<&str>,
) -> Result<Seat, Refused> {
  let db = &ground.db;
  let labels = &item["labels"];
  let label = named_by(labels, AGENT_LABEL);

  if label.is_none()
    && let Some(model) = named_by(labels, MODEL_LABEL)
  {
    let candidates = crate::role_agents::candidates(db, role, model, account).await?;

    if !candidates.is_empty() {
      return Ok(Seat {
        candidates,
        asked: Some((role.to_owned(), model.to_owned())),
      });
    }

    let agents = crate::role_agents::listing(db, role).await?;
    let sentence = policy::fill_marks(
      &policy::job_review::JOB_REVIEW.no_candidate,
      &[
        ("item", &format!("Item #{item_id}")),
        ("model", model),
        ("role", role),
        ("agents", &agents),
      ],
    );

    if !ground.testing() {
      let ground = ground.clone();
      let item = item.clone();
      let (role, model) = (role.to_owned(), model.to_owned());
      let roads = crate::role_agents::roads_held(&role, &crate::role_agents::of_role(db, &role).await?);

      tokio::spawn(async move {
        if let Err(fault) = crate::watches::queue_idle::tell_no_candidate(
          &ground.db,
          ground.vars(),
          item_id,
          &item,
          &role,
          &model,
          &roads,
        )
        .await
        {
          eprintln!("job of item #{item_id}: its model with no candidate was not told: {fault}");
        }
      });
    }

    return Err(Refused::new(StatusCode::UNPROCESSABLE_ENTITY, sentence).with_code("no-candidate"));
  }

  let (sentence, code) = match label {
    None => {
      let agents = crate::role_agents::listing(db, role).await?;
      let sentence = policy::fill_marks(
        &policy::job_review::JOB_REVIEW.agent_unnamed,
        &[("item", &format!("Item #{item_id}")), ("role", role), ("agents", &agents)],
      );

      (sentence, "agent-unnamed")
    }
    Some(label) => match crate::role_agents::named(db, label).await? {
      Some(agent) if agent.entry().is_some() => {
        return Ok(Seat {
          candidates: vec![agent],
          asked: None,
        });
      }
      Some(_) => (
        format!(
          "Item #{item_id} names {AGENT_LABEL}={label}, which follows a supervisor's road and no supervisor asks a job"
        ),
        "agent-names-no-road",
      ),
      None => (
        format!("Item #{item_id} names {AGENT_LABEL}={label}, which is no agent the agency registers"),
        "unknown-agent",
      ),
    },
  };

  if !ground.testing() {
    let ground = ground.clone();
    let item = item.clone();
    let told = label.unwrap_or(crate::repositories::AUTO).to_owned();
    let why = sentence.clone();

    tokio::spawn(async move {
      if let Err(fault) = crate::watches::queue_idle::tell_agent_refused(
        &ground.db,
        ground.vars(),
        item_id,
        &item,
        &told,
        &why,
      )
      .await
      {
        eprintln!("job of item #{item_id}: its refused agent was not told: {fault}");
      }
    });
  }

  Err(Refused::new(StatusCode::UNPROCESSABLE_ENTITY, sentence).with_code(code))
}

/// The role the item `POST /jobs` is handed names with its `role` label (docs/repositories.md, 「A role an item
/// names」): none for `auto` or no label, and 422 `unknown-role` with the label's text for one the store does not
/// hold.
async fn item_role(ground: &Arc<Ground>, item_id: i64, item: &Value) -> Result<Option<String>, Refused> {
  let db = &ground.db;
  let label = item["labels"][ROLE_LABEL].as_str().map(str::trim).unwrap_or_default();

  if label.is_empty() || label == crate::repositories::AUTO {
    return Ok(None);
  }

  match crate::agent_roles::named(db, label).await {
    Ok(Some(_)) => Ok(Some(label.to_owned())),
    Ok(None) => {
      if !ground.testing() {
        let ground = ground.clone();
        let item = item.clone();
        let label = label.to_owned();
        let roles = crate::agent_roles::names(db).await?;

        tokio::spawn(async move {
          let told = crate::watches::queue_idle::tell_role_unknown(
            &ground.db,
            ground.vars(),
            item_id,
            &item,
            &label,
            &roles,
          );

          if let Err(fault) = told.await {
            eprintln!("job of item #{item_id}: its unknown role was not told: {fault}");
          }
        });
      }

      Err(
        Refused::new(
          StatusCode::UNPROCESSABLE_ENTITY,
          format!("Item #{item_id} names {ROLE_LABEL}={label}, which is no role the store holds"),
        )
        .with_code("unknown-role"),
      )
    }
    Err(why) => Err(Refused::new(StatusCode::INTERNAL_SERVER_ERROR, why.to_string())),
  }
}

/// The role a job runs as: the one its row names, `worker` where it names none (docs/repositories.md, 「A
/// job」).
pub async fn role_of_job(ground: &Ground, id: i64) -> String {
  one(&ground.db, r#"SELECT "role" FROM "jobs" WHERE "id" = $1"#, vec![id.into()])
    .await
    .ok()
    .flatten()
    .and_then(|row| row.try_get::<String>("", "role").ok())
    .filter(|role| !role.is_empty())
    .unwrap_or_else(|| crate::agent_roles::job_role().to_owned())
}

// One line written into a job's log under its run.
pub(crate) async fn noted_line(ground: &Ground, id: i64, text: &str) {
  let Ok((member, _)) = job(&ground.db, id).await else {
    return;
  };
  let mut log = RunLog::new(Target {
    agent: member["workerName"].as_str().unwrap_or_default().to_owned(),
    run: member["runName"].as_str().map_or_else(|| format!("job-{id}"), str::to_owned),
    chat_id: member["roomId"].as_i64(),
    job_id: Some(id),
  });

  log.write(&ground.db, Line::note(text.to_owned())).await;
}

// How often a job waiting for its role's slot asks for it again: the line counts asks, never time
// (docs/role-slots.md, 「The line」).
const SLOT_ASK_EVERY: std::time::Duration = std::time::Duration::from_secs(1);

// A job's turns, each a run of its role at its checkout (docs/agents.md, 「A run」): on the work parts of its
// road, holding a slot of its role while it goes, in a session of the role's started for the turn and ended
// with it, with a run of the ledger opened for it and closed as it ends.
struct OnSession<'a, S: Sessions> {
  ground: &'a Ground,
  sessions: Arc<S>,
  member: &'a Value,
  worker: &'a str,
  role: &'a str,
  // The one agent its item named, where it named one.
  agent: Option<&'a str>,
  room: i64,
  name: &'a str,
  title: &'a str,
  roads: usize,
  // When this turn of the job began, which keeps each turn's slot requests apart from those of the job's
  // earlier turns, a review asking for several (docs/job-review.md, 「The turns the review asks for」).
  began: i64,
  // The session a turn of the job's review continues, where the job's last turn left one.
  resume: Option<crate::job_review::Resumed>,
  // The prompt a turn of its review goes out with where that session cannot be continued on the road taken.
  carried: Option<String>,
}

impl<S: Sessions + Send + Sync> OnSession<'_, S> {
  // The seat one turn of the job goes out on, on its road and keyed by its checkout so one checkout carries one
  // run at a time.
  async fn slot(&self, id: i64, taken: &Road, parts: &WorkParts) -> Result<Option<Request>, String> {
    let fixed = crate::order::Entry {
      provider: taken.provider.clone(),
      model: taken.model.clone(),
      effort: parts.mapping.effort.clone(),
      thinking: None,
      answer_tokens: parts.mapping.answer_tokens.and_then(|tokens| i64::try_from(tokens).ok()),
      advisor: None,
    };
    let asked = dispatch::Asked {
      request_key: format!("job-{id}-{}-{}", self.began, self.roads),
      owner: format!("{}{id}", dispatch::JOB_OWNER),
      wanted: dispatch::Wanted {
        role: self.role,
        path: Some(&parts.cwd),
        fixed: Some(fixed),
        agent: self.agent,
      },
      place_key: Some(self.worker),
      title: Some(self.title),
      ..dispatch::Asked::default()
    };

    job_seat(self.ground, id, &asked).await
  }
}

/// The seat a turn of a job goes out on (docs/role-slots.md, 「The line」). The seat the job already holds is
/// taken as it stands, asking nothing, so a later turn of the job — a stage handed over, a question or a rework
/// of its review — runs before any other job waiting for its agent. A job that holds none, at its first turn or
/// after its review paused it, asks for a slot of its role and asks again until the line lets it in, however
/// long that takes. None where the job was stopped while it waited, its ask then cancelled.
///
/// The lease is bound to no run: the job's turns each start and end a session of their own and its review waits
/// with none, so a lease bound to a turn's run would be given back as that turn ended. Only the job's end, or
/// its review's pause, gives it back.
pub async fn job_seat(
  ground: &Ground,
  id: i64,
  asked: &dispatch::Asked<'_>,
) -> Result<Option<Request>, String> {
  if let Some(kept) = kept_seat(ground, id).await? {
    return Ok(Some(kept));
  }

  let mut held = match waiting_seat(ground, id, asked).await? {
    Some(waiting) => waiting,
    None => {
      dispatch::enqueue(&ground.db, &ground.events, asked, dispatch::JOB, async |_, _| Ok(())).await?
    }
  };
  let mut told = false;
  // A job written waiting for its seat (docs/api.md, 「assignments and jobs」) that the line lets in at its first
  // ask after all goes out with its hold cleared.
  let written_held = job_held(ground, id).await;
  let seat = seat_words(asked.wanted.role, asked.wanted.agent);

  loop {
    paused_seats_given_back(ground).await;

    if let Some(let_in) = dispatch::acquire(ground, &held).await? {
      if told || written_held {
        hold_job(ground, id, None, false).await;
      }

      if told {
        noted_line(ground, id, &format!("Let in on {seat}, going out")).await;
      }

      return Ok(Some(let_in));
    }

    // Not let in at its first ask: the job names the wait as its hold until it is let in, the wait is told
    // to the item's orderer once, and the job's log says it.
    if !told {
      told = true;

      hold_job(ground, id, Some(&format!("it waits for {seat}")), true).await;
      noted_line(ground, id, &format!("Waiting for {seat}")).await;
    }

    if called_off(ground, id).await {
      dispatch::finish(&ground.db, &ground.events, &held.token, "cancelled: the job was stopped", true)
        .await?;

      return Ok(None);
    }

    tokio::time::sleep(SLOT_ASK_EVERY).await;
    held = dispatch::request(&ground.db, &held.token)
      .await?
      .ok_or("The job's slot request is gone")?;
  }
}

// Whether a job's row names a wait now.
async fn job_held(ground: &Ground, id: i64) -> bool {
  one(&ground.db, r#"SELECT "held_for" FROM "jobs" WHERE "id" = $1"#, vec![id.into()])
    .await
    .ok()
    .flatten()
    .and_then(|row| row.try_get::<Option<String>>("", "held_for").ok().flatten())
    .is_some()
}

/// The seat a job holds now, kept from an earlier turn: its newest slot request still let in.
async fn kept_seat(ground: &Ground, id: i64) -> Result<Option<Request>, String> {
  let row = one(
    &ground.db,
    r#"SELECT "token" FROM "role_slot_requests" WHERE "owner" = $1 AND "state" = 'active'
       ORDER BY "sequence" DESC LIMIT 1"#,
    vec![format!("{}{id}", dispatch::JOB_OWNER).into()],
  )
  .await
  .map_err(|why| why.to_string())?;
  let Some(token) = row
    .map(|row| row.try_get::<String>("", "token"))
    .transpose()
    .map_err(|why| why.to_string())?
  else {
    return Ok(None);
  };

  dispatch::request(&ground.db, &token).await
}

// The job's request still waiting in the line, left by a launch of the process before this one (docs/api.md,
// 「assignments and jobs」): taken up again where it asks for the road and the agent this turn asks for, so the
// job keeps its place, and cancelled otherwise, so the job asks again.
async fn waiting_seat(
  ground: &Ground,
  id: i64,
  asked: &dispatch::Asked<'_>,
) -> Result<Option<Request>, String> {
  let row = one(
    &ground.db,
    r#"SELECT "token" FROM "role_slot_requests" WHERE "owner" = $1 AND "state" = 'waiting'
       ORDER BY "sequence" DESC LIMIT 1"#,
    vec![format!("{}{id}", dispatch::JOB_OWNER).into()],
  )
  .await
  .map_err(|why| why.to_string())?;
  let Some(token) = row.and_then(|row| row.try_get::<String>("", "token").ok()) else {
    return Ok(None);
  };
  let Some(waiting) = dispatch::request(&ground.db, &token).await? else {
    return Ok(None);
  };
  let road = asked.wanted.fixed.as_ref().map(|fixed| (fixed.provider.as_str(), fixed.model.as_str()));
  let same = road.is_none_or(|(provider, model)| waiting.provider == provider && waiting.model == model)
    && waiting.agent_wanted.as_deref() == asked.wanted.agent
    && waiting.role == asked.wanted.role;

  if same {
    println!("job {id}: its slot request {} waiting in the line is taken up again", waiting.token);

    return Ok(Some(waiting));
  }

  dispatch::finish(
    &ground.db,
    &ground.events,
    &waiting.token,
    "cancelled: the job asks again on another road",
    true,
  )
  .await?;

  Ok(None)
}

/// The seats of jobs their review paused given back (docs/job-review.md, 「The owner's word」): a job whose owner's
/// question passed its due time gave its checkout up and answers for its agent no longer until it takes them
/// again, so a job waiting for that agent is let in on it.
pub async fn paused_seats_given_back(ground: &Ground) {
  let paused = all(
    &ground.db,
    r#"SELECT j."id" FROM "jobs" j
       WHERE j."state" = 'reviewing' AND j."review_parked"
         AND EXISTS (SELECT 1 FROM "role_slot_requests" r WHERE r."owner" = $1 || j."id" AND r."state" = 'active')"#,
    vec![dispatch::JOB_OWNER.into()],
  )
  .await;

  for row in paused.unwrap_or_default() {
    if let Ok(id) = row.try_get::<i64>("", "id") {
      crate::job_end::slot_given_back(
        &ground.db,
        &ground.events,
        id,
        "paused: its review waits on the owner",
      )
      .await;
    }
  }
}

impl<S: Sessions + Send + Sync> Carrier for OnSession<'_, S> {
  async fn carry(&mut self, taken: &Road, entry: &Entry, prompt: &str) -> Result<TurnEnd, String> {
    let ground = self.ground;
    let member = self.member;
    let id = member["id"].as_i64().unwrap_or_default();
    let parts =
      role_work_parts(ground, self.role, self.worker, self.room, self.name, self.title, entry).await?;

    self.roads += 1;

    // The seat stays the job's past this turn: its end, or its review's pause, gives it back.
    let Some(_seat) = self.slot(id, taken, &parts).await? else {
      return Err(format!("job {id} was stopped while it waited for a slot of the {} role", self.role));
    };
    // A turn of the review continues the job's own session on the road and the effort that session stands on;
    // on another, or where the turn's line puts it back for a prefix moved under the session, the session is
    // asked for its handoff first, noted on the item, and the turn opens a new one carrying the job's prompt and
    // its review so far, opened on that handoff (docs/sessions.md, 「The handoff a job's session leaves」).
    let effort = parts.mapping.effort.as_deref();
    let resume = self
      .resume
      .as_ref()
      .filter(|resumed| resumed.on(taken, effort))
      .map(|resumed| resumed.session.clone());

    if let (Some(left), None) = (&self.resume, &resume) {
      return self.opened_on_handoff(id, taken, &parts, prompt, left).await;
    }

    // A fresh session opens on the job's prompt, so it carries the checkout's bundle read now, under the title
    // and above the handoff (docs/worker-bench.md, 「Preamble」).
    let item = member["itemId"].as_i64().unwrap_or_default();
    let fresh = match (&resume, &self.carried) {
      (None, Some(carried)) => {
        Some(crate::control::preamble::bundled(ground, self.worker, item, carried).await)
      }
      _ => None,
    };
    let prompt = fresh.as_deref().unwrap_or(prompt);
    // Where the session's record is gone the turn opens a new one on what it carries, its run log standing for
    // the handoff the session can no longer write.
    let gone = match (&resume, &self.carried) {
      (Some(_), Some(_)) => {
        Some(crate::job_handoff::from_log(&ground.db, id, &JOB_HANDOFF.why.record_gone).await)
      }
      _ => None,
    };
    let instead = match (&gone, &self.carried) {
      (Some(handoff), Some(carried)) => {
        let opening = crate::job_handoff::ahead(&handoff.text, carried);

        Some(crate::control::preamble::bundled(ground, self.worker, item, &opening).await)
      }
      _ => None,
    };
    let continued = Continued {
      session: resume.clone(),
      instead,
      prefix: self.resume.as_ref().and_then(|resumed| resumed.prefix.clone()),
    };
    let carried = self.on_session(id, taken, &parts, prompt, continued).await;

    // The turn's line put it back before anything went in: the prefix the session was opened under moved.
    if let (Ok(end), Some(left)) = (&carried, &self.resume)
      && end.outcome == agent_session::Outcome::Reprepared
    {
      return self.opened_on_handoff(id, taken, &parts, prompt, left).await;
    }

    if let Ok(end) = &carried {
      crate::job_review::session_left(&ground.db, id, taken, end).await;

      // A turn that answered on another session than the one it was to continue opened on the run log.
      if let (Some(named), Some(handoff)) = (&resume, &gone)
        && end.reply.as_ref().and_then(|reply| reply.session_id.as_deref()) != Some(named.as_str())
      {
        self.noted(id, handoff).await;
      }
    }

    carried
  }
}

impl<S: Sessions + Send + Sync> OnSession<'_, S> {
  // The turn on a fresh session where the job's own cannot be continued: the session it leaves asked for its
  // handoff and the handoff noted on the item before anything else, then the turn opened on it ahead of what it
  // carries.
  async fn opened_on_handoff(
    &self,
    id: i64,
    taken: &Road,
    parts: &WorkParts,
    prompt: &str,
    left: &crate::job_review::Resumed,
  ) -> Result<TurnEnd, String> {
    let ground = self.ground;
    let handoff = asked_handoff(ground, self.sessions.as_ref(), self.member, left).await;

    self.noted(id, &handoff).await;

    let item = self.member["itemId"].as_i64().unwrap_or_default();
    let fresh = match &self.carried {
      Some(carried) => {
        let opening = crate::job_handoff::ahead(&handoff.text, carried);

        Some(crate::control::preamble::bundled(ground, self.worker, item, &opening).await)
      }
      None => None,
    };
    let carried = self
      .on_session(id, taken, parts, fresh.as_deref().unwrap_or(prompt), Continued::default())
      .await;

    if let Ok(end) = &carried {
      crate::job_review::session_left(&ground.db, id, taken, end).await;
    }

    carried
  }
}

impl<S: Sessions + Send + Sync> OnSession<'_, S> {
  // A handoff noted on the job's item.
  async fn noted(&self, id: i64, handoff: &crate::job_handoff::Handoff) {
    if let Some(item) = self.member["itemId"].as_i64() {
      crate::job_handoff::noted(self.ground, id, item, handoff).await;
    }
  }
}

// The job's session a turn continues, where it names one: the harness session, the words the turn goes out with
// instead where its record is gone, and the key of the command it was opened under.
#[derive(Default)]
struct Continued {
  session: Option<String>,
  instead: Option<String>,
  prefix: Option<String>,
}

impl<S: Sessions + Send + Sync> OnSession<'_, S> {
  // One turn of the job in a session of its role started for it at the checkout on the road, the session
  // ended with the turn.
  async fn on_session(
    &self,
    id: i64,
    taken: &Road,
    parts: &WorkParts,
    prompt: &str,
    continued: Continued,
  ) -> Result<TurnEnd, String> {
    let ground = self.ground;
    let member = self.member;
    let session = self
      .sessions
      .start(Start {
        agent: self.worker.to_owned(),
        scope: Scope::Apart(Apart {
          provider: taken.provider.clone(),
          model: taken.model.clone(),
          effort: parts.mapping.effort.clone(),
          cwd: parts.cwd.clone(),
          role: Some(self.role.to_owned()),
          resume: continued.session,
          resume_else: continued.instead,
          resume_prefix: continued.prefix,
          ..Apart::default()
        }),
      })
      .await
      .map_err(|fault| fault.message)?;
    let ledger = run_ledger::open(
      &ground.db,
      &Opening {
        agent: self.worker.to_owned(),
        chat_id: member["roomId"].as_i64(),
        unit: self.name.to_owned(),
        depth: 0,
        cap_ms: None,
        started_at: now_ms(),
        started_by: Some(member["holderName"].as_str().unwrap_or_default().to_owned()),
        search_id: None,
        round: None,
        message_id: None,
        title: None,
        carried: false,
      },
    )
    .await
    .map_err(|why| why.detail)?;

    // The supply source the turn went out on is kept on its run (docs/run-records.md, 「The run ledger」).
    if let Some(chosen) = crate::supply::chosen_for(&ground.roots.data, self.worker) {
      let _ = run_ledger::mark_supply(&ground.db, ledger, chosen.source.word()).await;
    }

    let marked = run(
      &ground.db,
      r#"UPDATE "jobs" SET "state" = 'running', "session_id" = $1, "run_name" = $2, "model" = $3, "run_id" = $5
         WHERE "id" = $4"#,
      vec![
        session.id.0.clone().into(),
        self.name.to_owned().into(),
        parts.mapping.model.clone().into(),
        id.into(),
        ledger.into(),
      ],
    )
    .await;

    // A run opened for a turn that never goes out is closed with it.
    if let Err(why) = marked {
      closed(ground, ledger, "failed", None, None).await;
      self.ended(&session.id).await;
      return Err(why.detail);
    }

    crate::job_words::session_began(&ground.db, id, &session.id.0).await;

    if let Ok((member, _)) = job(&ground.db, id).await {
      publish(ground, "job.changed", member);
    }

    // The words kept for the job while no turn of it took them go out behind this turn's own prompt
    // (docs/agent-control.md, 「Words for a running job」).
    let kept = crate::job_words::carried(&ground.db, ground.peers.gate.as_ref(), id).await;
    let sent = self
      .sessions
      .send_turn(Turn {
        session: session.id.clone(),
        prompt: kept.behind(prompt),
        context: TurnContext {
          job: Some(JobMark {
            job: id,
            item: member["itemId"].as_i64(),
          }),
          work: Some(parts.clone()),
          ..TurnContext::default()
        },
      })
      .await;

    // The turn came back from its session, answered or not: its words are no longer the job's to go out with.
    keep_turn_words(&ground.db, id, None).await;
    self.ended(&session.id).await;

    let end = match sent {
      Ok(end) => end,
      Err(fault) => {
        kept.left(&ground.db).await;
        closed(ground, ledger, "failed", None, None).await;
        return Err(fault.message);
      }
    };

    // A turn whose road stopped answering goes out again on the next, which carries the kept words again.
    if road_stopped(&end).is_some() {
      kept.left(&ground.db).await;
    } else {
      kept.taken(&ground.db, id, &parts.name).await;
    }
    let outcome = match end.outcome {
      agent_session::Outcome::Answered => "answered".to_owned(),
      other => format!("{other:?}").to_lowercase(),
    };

    closed(
      ground,
      ledger,
      &outcome,
      end.reply.as_ref().and_then(|reply| reply.context_tokens),
      turn_spent(&end),
    )
    .await;

    Ok(end)
  }

  // A turn's session ended with it, its place closed (docs/agents.md, 「A run in a pane of its own」).
  async fn ended(&self, session: &SessionId) {
    if let Err(fault) = self.sessions.end(session).await {
      eprintln!("{}: the job's session was not ended: {}", session.0, fault.message);
    }
  }
}

/// The handoff of the session a job's last turn answered on, `left`, asked one more turn on its own road and
/// under the job's run (docs/sessions.md, 「The handoff a job's session leaves」): what that session answered,
/// else the last lines the job's run logged.
pub(crate) async fn asked_handoff<S: Sessions>(
  ground: &Ground,
  sessions: &S,
  member: &Value,
  left: &crate::job_review::Resumed,
) -> crate::job_handoff::Handoff {
  let id = member["id"].as_i64().unwrap_or_default();
  let role = member["roleName"].as_str().unwrap_or_else(|| crate::agent_roles::job_role());
  let worker = member["workerName"].as_str().unwrap_or_default();
  let room = member["roomId"].as_i64().unwrap_or_default();
  let name = format!("job-{id}");
  let title = member["prompt"]
    .as_str()
    .and_then(|prompt| prompt.lines().next())
    .map_or_else(|| name.clone(), |line| line.trim_start_matches("# ").to_owned());
  let entry = Entry {
    provider: left.provider.clone(),
    model: left.model.clone(),
    effort: None,
    thinking: None,
    answer_tokens: None,
    advisor: None,
  };
  let parts = match role_work_parts(ground, role, worker, room, &name, &title, &entry).await {
    Ok(parts) => parts,
    Err(why) => {
      eprintln!("job {id}: the session it leaves could not be asked for its handoff: {why}");
      return crate::job_handoff::from_log(&ground.db, id, &JOB_HANDOFF.why.failed).await;
    }
  };
  let session = match sessions
    .start(Start {
      agent: worker.to_owned(),
      scope: Scope::Apart(Apart {
        provider: left.provider.clone(),
        model: left.model.clone(),
        // The session is asked at the effort it answered on, where its last turn named one.
        effort: left.effort.clone().or(parts.mapping.effort.clone()),
        cwd: parts.cwd.clone(),
        role: Some(role.to_owned()),
        resume: Some(left.session.clone()),
        ..Apart::default()
      }),
    })
    .await
  {
    Ok(session) => session,
    Err(fault) => {
      eprintln!("job {id}: the session it leaves could not be asked for its handoff: {}", fault.message);
      return crate::job_handoff::from_log(&ground.db, id, &JOB_HANDOFF.why.failed).await;
    }
  };
  let sent = sessions
    .send_turn(Turn {
      session: session.id.clone(),
      prompt: JOB_HANDOFF.instruction.clone(),
      context: TurnContext {
        job: Some(JobMark {
          job: id,
          item: member["itemId"].as_i64(),
        }),
        work: Some(parts),
        ..TurnContext::default()
      },
    })
    .await;

  if let Err(fault) = sessions.end(&session.id).await {
    eprintln!("{}: the session asked for its handoff was not ended: {}", session.id.0, fault.message);
  }

  match sent {
    // Only the session asked writes its handoff: an answer from one opened in its place is no handoff of it.
    Ok(end)
      if end.reply.as_ref().and_then(|reply| reply.session_id.as_deref())
        == Some(left.session.as_str()) =>
    {
      match crate::job_handoff::answered(end.answer.as_deref()) {
        Some(handoff) => handoff,
        None => crate::job_handoff::from_log(&ground.db, id, &JOB_HANDOFF.why.no_answer).await,
      }
    }
    Ok(_) => crate::job_handoff::from_log(&ground.db, id, &JOB_HANDOFF.why.record_gone).await,
    Err(fault) => {
      eprintln!("job {id}: the session it leaves did not answer for its handoff: {}", fault.message);
      crate::job_handoff::from_log(&ground.db, id, &JOB_HANDOFF.why.failed).await
    }
  }
}

/// The handoff of a session handed over as its job ends, asked before the session is closed and noted on the
/// job's item, where a job's turn last answered on it (docs/sessions.md, 「The handoff a job's session leaves」).
pub async fn handed_over<S: Sessions>(ground: &Ground, sessions: &S, session: &str) {
  // A deployment that names no job service has nowhere to note it, so nothing is asked.
  if crate::queue::orderer::JobService::of(ground.vars()).is_none() {
    return;
  }

  let Ok(Some(row)) = one(
    &ground.db,
    r#"SELECT "id", "review_session" FROM "jobs" WHERE "session_id" = $1 ORDER BY "id" DESC LIMIT 1"#,
    vec![session.to_owned().into()],
  )
  .await
  else {
    return;
  };
  let id: i64 = row.try_get("", "id").unwrap_or_default();
  let left: Option<crate::job_review::Resumed> = row
    .try_get::<Option<String>>("", "review_session")
    .ok()
    .flatten()
    .and_then(|kept| serde_json::from_str(&kept).ok());
  let Ok((member, _)) = job(&ground.db, id).await else {
    return;
  };
  let Some(item) = member["itemId"].as_i64() else {
    return;
  };
  let handoff = match &left {
    Some(left) => asked_handoff(ground, sessions, &member, left).await,
    None => crate::job_handoff::from_log(&ground.db, id, &JOB_HANDOFF.why.record_gone).await,
  };

  crate::job_handoff::noted(ground, id, item, &handoff).await;
}

/// One turn of a job carried on the sessions given, to the end it comes to: its first, launched with its kept
/// words or its composed prompt, or one its review asks for (docs/job-review.md, 「The turns the review asks
/// for」).
pub async fn launched<S: Sessions + Send + Sync>(
  ground: &Ground,
  sessions: Option<Arc<S>>,
  id: i64,
  turn: Option<crate::job_review::Turn>,
) -> Result<Option<Ended>, String> {
  let (member, prompt) = job(&ground.db, id).await.map_err(|why| why.error)?;
  let worker_name = member["workerName"].as_str().unwrap_or_default().to_owned();
  let holder = member["holderName"].as_str().unwrap_or_default().to_owned();
  let item = member["itemId"].as_i64().unwrap_or_default();

  println!("job {id}: starting on worker {worker_name} for item {item}");

  let role = role_of_job(ground, id).await;
  let room = member["roomId"].as_i64().unwrap_or(0);
  let title = prompt.lines().next().unwrap_or_default().trim_start_matches("# ").to_owned();
  let name = format!("job-{id}");
  let Some(sessions) = sessions else {
    return Err("agency stands no places to carry sessions on".to_owned());
  };
  // Every checkout the job is to be lent is lent before its turn goes out, the job waiting while one is busy
  // (docs/worker-slots.md, 「A checkout lent to a job」); a job stopped meanwhile goes no further.
  let Some(lent) = crate::control::loans::lend_job(ground, id).await? else {
    return Ok(None);
  };
  let lent_section =
    crate::control::loans::lent_words(&lent, &crate::places::loan_branch::job_branch(item));

  // The job's own support worktree is made before its prompt is composed, since the prompt's header reads the
  // checkout's directory (docs/worker-slots.md, 「Names and places」).
  crate::places::checkouts::ensure_support_checkout(ground, &worker_name).await?;

  // The gate picks the reference a job's instruction calls for, and the run carries it in its first
  // prompt; a job with no instruction stands as it was. The first prompt closes with what the review opens
  // with; a turn the review asks for goes out with its own words instead.
  let (prompt, resume, carried) = match turn {
    // A first turn launched again after agency's restart goes out with the words it kept, its reference and its
    // review's opening already in them.
    None => match kept_turn_words(&ground.db, id).await {
      Some(words) => (words, None, None),
      None => {
        let prompt = under_title(&prompt, &lent_section);

        if !lent_section.is_empty() {
          kept_prompt(&ground.db, id, &prompt).await;
        }

        // The checkout's bundle is read now and stands above the lent checkouts, never kept on the job's row
        // (docs/worker-bench.md, 「Preamble」).
        let prompt = crate::control::preamble::bundled(ground, &worker_name, item, &prompt).await;
        let prompt = referenced(ground.peers.gate.as_ref(), &member, &prompt).await;
        let Some(opening) = crate::job_review::opened(ground, id).await else {
          return Ok(None);
        };

        (format!("{prompt}\n\n{opening}"), None, None)
      }
    },
    Some(turn) => {
      // A turn that took loans again after its review paused says where each now stands, ahead of its words.
      let words = match lent.iter().any(|lent| lent.fresh) {
        true => format!("{lent_section}\n{}", turn.words),
        false => turn.words,
      };
      let carried = crate::job_review::carried(ground, &member, &prompt, &words).await;

      (words, turn.resume, Some(carried))
    }
  };
  let item_agent = item_agent_of_job(ground, id).await;
  let mut carrier = OnSession {
    ground,
    sessions,
    member: &member,
    worker: &worker_name,
    role: &role,
    agent: item_agent.as_deref(),
    room,
    name: &name,
    title: &title,
    roads: 0,
    began: now_ms(),
    resume,
    carried,
  };

  carry_job(
    ground,
    &Carried {
      id,
      worker: &worker_name,
      holder: &holder,
      prompt: &prompt,
    },
    &mut carrier,
  )
  .await
}

// The prompt a job was launched with, written again once it names the checkouts lent to it.
async fn kept_prompt(db: &sea_orm::DatabaseConnection, id: i64, prompt: &str) {
  if let Err(why) =
    run(db, r#"UPDATE "jobs" SET "prompt" = $1 WHERE "id" = $2"#, vec![prompt.into(), id.into()]).await
  {
    eprintln!("job {id}: the prompt naming its loans was not kept: {why}");
  }
}

// What a job's turn's end makes of the job, its run closed with it.
async fn ended_of(ground: &Ground, ledger: i64, end: agent_session::TurnEnd) -> Ended {
  let spent = turn_spent(&end);
  let context = end.reply.as_ref().and_then(|reply| reply.context_tokens);
  let (outcome, report) = ended_by(end.outcome, end.answer, end.reason);

  closed(ground, ledger, outcome.as_deref().unwrap_or("answered"), context, spent).await;

  ("ended".to_owned(), outcome, report, spent)
}

// A job turn's run closed in the ledger as the turn ended, with what it spent.
async fn closed(
  ground: &Ground,
  ledger: i64,
  outcome: &str,
  context_tokens: Option<i64>,
  spent: Option<Spent>,
) {
  let closing = Closing {
    finished_at: now_ms(),
    outcome: outcome.to_owned(),
    context_tokens,
    spent,
  };

  if let Err(NotClosed::Store(why)) = run_ledger::close(&ground.db, ledger, &closing).await {
    eprintln!("run {ledger}: the job's run could not be closed in the ledger: {}", why.detail);
  }
}

/// The jobs: recent, by room, by worker, by holder, running.
pub struct ListJobs;

impl Route for ListJobs {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/jobs";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let db = &call.ground().db;
    let asked = job_search(&call)?;
    // After a cursor the nearest jobs are the oldest of the newer ones, so they are read upward.
    let upward = asked.after.is_some();
    let sql = format!(
      r#"SELECT {JOB_COLUMNS} FROM "jobs"
         WHERE ($1::bigint IS NULL OR "id" > $1) AND ($2::bigint IS NULL OR "id" < $2)
           AND ($3::text IS NULL OR "worker_name" = $3) AND ($4::text IS NULL OR "holder_name" = $4)
           AND ($5::jsonb = '[]'::jsonb OR "state" IN (SELECT jsonb_array_elements_text($5::jsonb)))
           AND ($6::boolean IS NULL OR ("state" IN ('starting', 'running', 'reviewing')) = $6)
           AND ($7::bigint IS NULL OR "room_id" = $7) AND ($8::bigint IS NULL OR "item_id" = $8)
           AND ($10::jsonb IS NULL OR "worker_name" IN (SELECT jsonb_array_elements_text($10::jsonb)))
           AND ($11::text IS NULL OR "role" = $11)
         ORDER BY "id" {} LIMIT $9"#,
      if upward { "ASC" } else { "DESC" }
    );
    let states = serde_json::to_string(&asked.states).unwrap_or_else(|_| "[]".to_owned());
    let rows = all(
      db,
      &sql,
      vec![
        asked.after.into(),
        asked.before.into(),
        asked.worker.into(),
        asked.holder.into(),
        states.into(),
        asked.running.into(),
        asked.room.into(),
        asked.item.into(),
        ((asked.limit + 1) as i64).into(),
        checkouts_named(db, asked.repository.as_deref()).await?.into(),
        asked.role.into(),
      ],
    )
    .await?;
    let mut items: Vec<Value> = rows.iter().map(job_value).collect::<Result<_, _>>()?;
    let more = items.len() > asked.limit;

    name_repositories(db, &mut items).await?;

    items.truncate(asked.limit);

    if upward {
      items.reverse();
    }

    Ok(answer(StatusCode::OK, page(items, more)))
  }
}

/// Every name the jobs' list is asked by (docs/api.md, 「assignments and jobs」).
const JOB_FILTERS: [&str; 11] = [
  "running",
  "itemId",
  "roomId",
  "repositoryName",
  "roleName",
  "workerName",
  "holderName",
  "state",
  "after",
  "before",
  "limit",
];

// What the jobs' list is narrowed by and where its page stands, as the query names it.
struct JobSearch {
  after: Option<i64>,
  before: Option<i64>,
  limit: usize,
  running: Option<bool>,
  states: Vec<String>,
  item: Option<i64>,
  room: Option<i64>,
  repository: Option<String>,
  worker: Option<String>,
  role: Option<String>,
  holder: Option<String>,
}

fn job_search(call: &Call) -> Result<JobSearch, Refused> {
  if let Some(name) = call.query_names().into_iter().find(|name| !JOB_FILTERS.contains(&name.as_str())) {
    return Err(bad(format!("the jobs are not narrowed by {name}")));
  }

  let named =
    |name: &str| call.query(name).map(|value| value.trim().to_owned()).filter(|value| !value.is_empty());
  let number = |name: &str| {
    named(name)
      .map(|value| value.parse::<i64>().map_err(|_| bad(format!("{name} is a whole number"))))
      .transpose()
  };
  let running = named("running")
    .map(|value| match value.as_str() {
      "true" => Ok(true),
      "false" => Ok(false),
      _ => Err(bad("running is true or false")),
    })
    .transpose()?;
  let limit = match number("limit")? {
    Some(limit) if limit < 1 => return Err(bad("limit is a whole number of at least 1")),
    Some(limit) => usize::try_from(limit).unwrap_or(500).min(500),
    None => 50,
  };

  Ok(JobSearch {
    after: number("after")?,
    before: number("before")?,
    limit,
    running,
    states: named("state")
      .map(|states| {
        states
          .split(',')
          .map(str::trim)
          .filter(|state| !state.is_empty())
          .map(str::to_owned)
          .collect()
      })
      .unwrap_or_default(),
    item: number("itemId")?,
    room: number("roomId")?,
    repository: named("repositoryName"),
    worker: named("workerName"),
    role: named("roleName"),
    holder: named("holderName"),
  })
}

crate::moved!(ListJobs);

/// One job and the tokens its model has spent.
pub struct ReadJob;

impl Route for ReadJob {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/jobs/{jobId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let id: i64 = call
      .param("jobId")
      .and_then(|id| id.parse().ok())
      .ok_or_else(|| bad("jobId is a number"))?;

    Ok(answer(StatusCode::OK, job(&call.ground().db, id).await?.0))
  }
}

crate::moved!(ReadJob);

// How often a stop looks again whether the job's turn has ended.
const STOP_LOOK: std::time::Duration = std::time::Duration::from_millis(250);

/// A job stopped by asking its turn to end (docs/api.md, 「assignments and jobs」): the reason kept as its stop
/// mark and its run cancelled, then waited on, however long the turn's own ending takes, until the turn's
/// end has written the job stopped with that reason. A job no turn carries is written stopped at once.
pub async fn stop_job(
  ground: &Ground,
  stops: &crate::queue::stops::Stops,
  id: i64,
  reason: &str,
) -> Result<Value, Refused> {
  let db = &ground.db;
  let (member, _) = job(db, id).await?;
  let state = member["state"].as_str().unwrap_or_default();

  if !matches!(state, "starting" | "running" | "reviewing") {
    return Err(Refused::new(StatusCode::CONFLICT, format!("job {id} is {state}")));
  }

  let run_name = member["runName"].as_str().filter(|name| !name.is_empty()).map(str::to_owned);
  let kept = match &run_name {
    Some(name) => crate::turn_pickup::kept(db, name).await,
    None => false,
  };

  crate::queue::stops::mark_job_stopped(db, id, reason).await?;

  if let Some(name) = &run_name {
    stops.cancel(name).await?;
  }

  if is_carrying(id) || kept {
    // The turn has ended once the job's end is written and nothing carries or keeps the turn any more, so
    // the worker reads idle by the time the stop answers.
    loop {
      let (member, _) = job(db, id).await?;
      let held = match &run_name {
        Some(name) => is_carrying(id) || crate::turn_pickup::kept(db, name).await,
        None => is_carrying(id),
      };

      if !held && !matches!(member["state"].as_str(), Some("starting" | "running" | "reviewing")) {
        if let Some(name) = &run_name {
          let _ = crate::queue::cancels::take_back(db, name).await;
        }

        return Ok(member);
      }

      tokio::time::sleep(STOP_LOOK).await;
    }
  }

  // No turn carries it: the stop is written here, as the turn's end would have written it, its run log
  // standing for the handoff of a session no turn holds.
  println!("job {id}: stop consumed: no turn carried it: {}", reason.trim());
  job_handoff_noted(ground, id, &crate::job_handoff::from_log(db, id, &JOB_HANDOFF.why.stopped_idle).await)
    .await;
  let _ = crate::queue::stops::clear_job_stop(db, id).await;
  let now = now_ms();

  run(
    db,
    r#"UPDATE "jobs" SET "state" = 'stopped', "outcome" = 'stopped', "report" = $1, "ended_at" = $2
       WHERE "id" = $3 AND "state" IN ('starting', 'running', 'reviewing')"#,
    vec![reason.into(), now.into(), id.into()],
  )
  .await?;
  crate::job_end::slot_given_back(db, &ground.events, id, &format!("stopped: {reason}")).await;
  release(db, member["assignmentId"].as_i64().unwrap_or_default()).await?;

  if let Some(name) = &run_name {
    let _ = crate::queue::cancels::take_back(db, name).await;
  }

  let (member, _) = job(db, id).await?;

  crate::job_end::delivered(ground, &member).await;
  publish(ground, "job.changed", member.clone());

  Ok(member)
}

/// A head pane that died: every job its worker is running ends failed with the pane's exit code and last
/// stderr line as its report; its lease is given back by the ordinary end.
pub async fn pane_died(ground: &Ground, worker: &str, code: i32, last: &str) -> Result<Vec<i64>, String> {
  let rows = all(
    &ground.db,
    r#"SELECT "id" FROM "jobs" WHERE "worker_name" = $1 AND "state" IN ('starting', 'running') ORDER BY "id""#,
    vec![worker.into()],
  )
  .await
  .map_err(|why| why.to_string())?;
  let mut ended = vec![];
  let reason = format!(
    "The pane for {worker} died with exit code {code}. Its last stderr line: {}",
    if last.trim().is_empty() { "none" } else { last.trim() }
  );

  for row in rows {
    let Ok(id) = row.try_get::<i64>("", "id") else { continue };

    job_turn_ended(ground, id, agent_session::TurnEnd::failed("pane-death", &reason, None)).await;
    ended.push(id);
  }

  Ok(ended)
}

/// A pane's death told to agency: the worker, its exit code and the last line it wrote on stderr.
pub struct PaneDeath;

impl Route for PaneDeath {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/pane-deaths";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let worker = call.query("agent").unwrap_or_default();
    let code: i32 = call
      .query("code")
      .unwrap_or_default()
      .parse()
      .map_err(|_| bad("code is a whole number"))?;
    let last = call.query("last").unwrap_or_default();

    if worker.trim().is_empty() {
      return Err(bad("agent is required"));
    }

    match pane_died(call.ground(), &worker, code, &last).await {
      Ok(jobs) => Ok(answer(StatusCode::ACCEPTED, json!({ "jobs": jobs }))),
      Err(why) => Err(Refused::failed(why)),
    }
  }
}

crate::moved!(PaneDeath);

/// End a job with its outcome, or stop it, its turn asked to end first.
pub struct ChangeJob;

impl Route for ChangeJob {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/jobs/{jobId}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let id: i64 = call
      .param("jobId")
      .and_then(|id| id.parse().ok())
      .ok_or_else(|| bad("jobId is a number"))?;
    let asked = body(&mut call).await?;
    let db = &call.ground().db;
    let now = now_ms();
    let (member, _) = job(db, id).await?;
    let state = member["state"].as_str().unwrap_or_default();
    if state == "ended" || state == "stopped" {
      return Err(Refused::new(StatusCode::CONFLICT, format!("job {id} is {state}")));
    }

    let wanted = asked
      .get("state")
      .and_then(Value::as_str)
      .map(|state| match state {
        "ended" => Ok("ended"),
        "stopped" => Ok("stopped"),
        other => Err(bad(format!("a job ends or stops, not {other}"))),
      })
      .transpose()?
      .unwrap_or("ended");
    let outcome = asked.get("outcome").and_then(Value::as_str).map(str::to_owned);
    let report = asked.get("report").and_then(Value::as_str).map(str::to_owned);

    if wanted == "stopped" {
      let reason = report.unwrap_or_else(|| STOPPED.to_owned());

      return Ok(answer(StatusCode::OK, stop_job(call.ground(), &call.stops(), id, &reason).await?));
    }

    run(
      db,
      r#"UPDATE "jobs" SET "state" = $1, "outcome" = COALESCE($2, "outcome"),
              "report" = COALESCE($3, "report"), "ended_at" = $4 WHERE "id" = $5"#,
      vec![wanted.into(), outcome.clone().into(), report.into(), now.into(), id.into()],
    )
    .await?;
    crate::job_end::slot_given_back(db, call.events(), id, &ending(wanted, outcome.as_deref())).await;
    let assignment_id = member["assignmentId"].as_i64().unwrap_or_default();

    if let Err(why) = release(db, assignment_id).await {
      eprintln!("job {id}: its assignment {assignment_id} was not released: {}", why.error);
    }

    let (member, _) = job(db, id).await?;

    crate::job_end::delivered(call.ground(), &member).await;
    publish(
      call.ground(),
      if wanted == "ended" { "job.ended" } else { "job.changed" },
      member.clone(),
    );

    Ok(answer(StatusCode::OK, member))
  }
}

crate::moved!(ChangeJob);

/// The line a work-order step is written as, composed from an instruction.
pub struct InstructionText;

impl Route for InstructionText {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/instruction-texts";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let asked = body(&mut call).await?;
    let instruction = instruction_of(Some(&asked))?.ok_or_else(|| bad("an instruction is required"))?;

    Ok(answer(StatusCode::OK, json!({ "text": instruction_line(&instruction) })))
  }
}

crate::moved!(InstructionText);

/// The prompt a job was launched with.
pub struct ReadJobPrompt;

impl Route for ReadJobPrompt {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/jobs/{jobId}/prompt";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let id: i64 = call
      .param("jobId")
      .and_then(|id| id.parse().ok())
      .ok_or_else(|| bad("jobId is a number"))?;
    let row = one(
      &call.ground().db,
      r#"SELECT "prompt", "prompt_english" FROM "jobs" WHERE "id" = $1"#,
      vec![id.into()],
    )
    .await?
    .ok_or_else(|| missing(format!("job {id}")))?;

    Ok(answer(
      StatusCode::OK,
      json!({
        "jobId": id,
        "text": row.try_get::<String>("", "prompt")?,
        "english": row.try_get::<Option<String>>("", "prompt_english")?,
      }),
    ))
  }
}

crate::moved!(ReadJobPrompt);

// ---- session work and loads ----

fn results_of(row: Option<&QueryResult>) -> Result<Vec<Value>, Refused> {
  let Some(row) = row else { return Ok(Vec::new()) };
  let text: String = row.try_get("", "results")?;

  Ok(serde_json::from_str(&text).unwrap_or_default())
}

fn session_work_value(session: &str, row: Option<&QueryResult>) -> Result<Value, Refused> {
  let item = row.and_then(|row| row.try_get::<Option<i64>>("", "item_id").ok().flatten());

  Ok(json!({ "sessionId": session, "itemId": item, "results": results_of(row)? }))
}

async fn session_work_row(
  db: &sea_orm::DatabaseConnection,
  session: &str,
) -> Result<Option<QueryResult>, Refused> {
  Ok(
    one(
      db,
      r#"SELECT "item_id", "results" FROM "session_work" WHERE "session_id" = $1"#,
      vec![session.into()],
    )
    .await?,
  )
}

/// The item and results a worker session holds.
pub struct ReadSessionWork;

impl Route for ReadSessionWork {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/session-work/{sessionId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let session = call.param("sessionId").unwrap_or_default();
    let row = session_work_row(&call.ground().db, session).await?;

    Ok(answer(StatusCode::OK, session_work_value(session, row.as_ref())?))
  }
}

crate::moved!(ReadSessionWork);

/// Record the item a session serves.
pub struct ChangeSessionWork;

impl Route for ChangeSessionWork {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/session-work/{sessionId}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let session = call.param("sessionId").unwrap_or_default().to_owned();
    let asked = body(&mut call).await?;
    let item = number(&asked, "itemId");
    let db = &call.ground().db;
    let now = now_ms();

    run(
      db,
      r#"INSERT INTO "session_work" ("session_id", "item_id", "results", "changed_at")
         VALUES ($1, $2, '[]', $3)
         ON CONFLICT ("session_id") DO UPDATE SET "item_id" = EXCLUDED."item_id", "changed_at" = EXCLUDED."changed_at""#,
      vec![session.clone().into(), item.map(Into::into).unwrap_or(DbValue::BigInt(None)), now.into()],
    )
    .await?;
    let row = session_work_row(db, &session).await?;

    Ok(answer(StatusCode::OK, session_work_value(&session, row.as_ref())?))
  }
}

crate::moved!(ChangeSessionWork);

/// Record a result sent into a session.
pub struct PutSessionResult;

impl Route for PutSessionResult {
  const METHOD: Method = Method::PUT;
  const PATH: &'static str = "/session-work/{sessionId}/results/{resultKey}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let session = call.param("sessionId").unwrap_or_default().to_owned();
    let key = call.param("resultKey").unwrap_or_default().to_owned();
    let result = body(&mut call).await?;
    let db = &call.ground().db;
    let row = session_work_row(db, &session).await?;
    let mut results = results_of(row.as_ref())?;
    results.retain(|held| held.get("key").and_then(Value::as_str) != Some(key.as_str()));
    results.push(json!({ "key": key, "result": result }));
    let text = serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_owned());

    run(
      db,
      r#"INSERT INTO "session_work" ("session_id", "results", "changed_at") VALUES ($1, $2, $3)
         ON CONFLICT ("session_id") DO UPDATE SET "results" = EXCLUDED."results", "changed_at" = EXCLUDED."changed_at""#,
      vec![session.clone().into(), text.into(), now_ms().into()],
    )
    .await?;
    let row = session_work_row(db, &session).await?;

    Ok(answer(StatusCode::OK, session_work_value(&session, row.as_ref())?))
  }
}

crate::moved!(PutSessionResult);

/// Forget a session's results as it folds.
pub struct DeleteSessionResults;

impl Route for DeleteSessionResults {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/session-work/{sessionId}/results";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let session = call.param("sessionId").unwrap_or_default();
    run(
      &call.ground().db,
      r#"UPDATE "session_work" SET "results" = '[]', "changed_at" = $1 WHERE "session_id" = $2"#,
      vec![now_ms().into(), session.into()],
    )
    .await?;

    Ok(StatusCode::NO_CONTENT.into_response())
  }
}

crate::moved!(DeleteSessionResults);

async fn loads_value(db: &sea_orm::DatabaseConnection, session: &str) -> Result<Value, Refused> {
  let row = one(
    db,
    r#"SELECT "documents" FROM "session_loads" WHERE "session_id" = $1"#,
    vec![session.into()],
  )
  .await?;
  let documents: Vec<Value> = row
    .as_ref()
    .and_then(|row| row.try_get::<String>("", "documents").ok())
    .and_then(|text| serde_json::from_str(&text).ok())
    .unwrap_or_default();

  Ok(json!({ "sessionId": session, "documents": documents }))
}

/// The documents loaded into a session.
pub struct ReadSessionLoads;

impl Route for ReadSessionLoads {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/session-loads/{sessionId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let session = call.param("sessionId").unwrap_or_default();

    Ok(answer(StatusCode::OK, loads_value(&call.ground().db, session).await?))
  }
}

crate::moved!(ReadSessionLoads);

/// Keep a session's loads.
pub struct PutSessionLoads;

impl Route for PutSessionLoads {
  const METHOD: Method = Method::PUT;
  const PATH: &'static str = "/session-loads/{sessionId}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let session = call.param("sessionId").unwrap_or_default().to_owned();
    let asked = body(&mut call).await?;
    let documents = asked.get("documents").cloned().filter(Value::is_array).unwrap_or_else(|| json!([]));
    let db = &call.ground().db;
    let text = serde_json::to_string(&documents).unwrap_or_else(|_| "[]".to_owned());

    run(
      db,
      r#"INSERT INTO "session_loads" ("session_id", "documents", "changed_at") VALUES ($1, $2, $3)
         ON CONFLICT ("session_id") DO UPDATE SET "documents" = EXCLUDED."documents", "changed_at" = EXCLUDED."changed_at""#,
      vec![session.clone().into(), text.into(), now_ms().into()],
    )
    .await?;

    Ok(answer(StatusCode::OK, loads_value(db, &session).await?))
  }
}

crate::moved!(PutSessionLoads);

/// Forget a session's loads.
pub struct DeleteSessionLoads;

impl Route for DeleteSessionLoads {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/session-loads/{sessionId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let session = call.param("sessionId").unwrap_or_default();
    run(
      &call.ground().db,
      r#"DELETE FROM "session_loads" WHERE "session_id" = $1"#,
      vec![session.into()],
    )
    .await?;

    Ok(StatusCode::NO_CONTENT.into_response())
  }
}

crate::moved!(DeleteSessionLoads);
