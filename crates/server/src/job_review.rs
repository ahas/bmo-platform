//! A job's review (docs/job-review.md): every answered turn of a job is a report its orderer examines, one check
//! at a time, before the job goes on to its next stage or ends; every move written onto the item's review in the
//! job service.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::http::{Method, StatusCode};
use axum::response::Response;
use bmo_agency_client::{JobReviewAsk, JobReviewed, ReviewPosition};
use policy::fill_marks;
use policy::job_notices::JOB_NOTICES;
use policy::job_review::JOB_REVIEW;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use store::{one, run};

use crate::control::work::{SPENT_ADDED, carry_review_turn, job, publish, written_end};
use crate::control::{Call, Refused, Route, answer};
use crate::job_notices::instruction;
use crate::outbox::{self, Keep, Notice, Receiver};
use crate::places::Ground;
use crate::queue::orderer::JobService;
use crate::road::Road;
use crate::run_ledger::Spent;

/// The kinds of turn a review runs: the work of a stage or a rework, a question, and the final report.
const WORK: &str = "work";
const QUESTION: &str = "question";
const FINAL: &str = "final";

/// The session a job's last turn answered on, the road it stands on, the effort it answered at and the key of
/// the command it went out under, where its line read one.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Resumed {
  pub provider: String,
  pub model: String,
  pub session: String,
  #[serde(default)]
  pub effort: Option<String>,
  #[serde(default)]
  pub prefix: Option<String>,
}

impl Resumed {
  /// Whether a turn going out on `taken` at `effort` can continue it: the same provider and model, and the same
  /// effort where both name one. A moved prefix is told by the turn's own line, which puts the turn back.
  pub fn on(&self, taken: &Road, effort: Option<&str>) -> bool {
    let same_effort = match (self.effort.as_deref(), effort) {
      (Some(held), Some(asked)) => held == asked,
      _ => true,
    };

    self.provider == taken.provider && self.model == taken.model && same_effort
  }
}

/// A turn the review asks for: its words, and the session it continues.
pub struct Turn {
  pub words: String,
  pub resume: Option<Resumed>,
}

/// Where a job's review stands, as its row keeps it.
#[derive(Clone, Debug, Default)]
struct Standing {
  item: i64,
  worker: String,
  state: String,
  stages: Vec<String>,
  stage: Option<i64>,
  round: i64,
  check: Option<String>,
  rejections: i64,
  question: Option<i64>,
  parked: bool,
  words: Option<String>,
  session: Option<Resumed>,
  turn: Option<String>,
  assignment: i64,
}

const STANDING: &str = r#"SELECT "item_id", "worker_name", "state", "review_stages", "review_stage", "review_round",
  "review_check", "rejections", "review_question", "review_parked", "review_words", "review_session",
  "review_turn", "assignment_id" FROM "jobs" WHERE "id" = $1"#;

async fn standing(db: &DatabaseConnection, id: i64) -> Result<Standing, Refused> {
  let Some(row) = one(db, STANDING, vec![id.into()]).await? else {
    return Err(Refused::new(StatusCode::NOT_FOUND, format!("no job {id} stands")).with_code("no-job"));
  };
  let text = |column: &str| row.try_get::<Option<String>>("", column).ok().flatten();

  Ok(Standing {
    item: row.try_get("", "item_id")?,
    worker: row.try_get("", "worker_name")?,
    state: row.try_get("", "state")?,
    stages: text("review_stages")
      .and_then(|text| serde_json::from_str(&text).ok())
      .unwrap_or_default(),
    stage: row.try_get("", "review_stage")?,
    round: row.try_get("", "review_round")?,
    check: text("review_check"),
    rejections: row.try_get("", "rejections")?,
    question: row.try_get("", "review_question")?,
    parked: row.try_get("", "review_parked")?,
    words: text("review_words"),
    session: text("review_session").and_then(|text| serde_json::from_str(&text).ok()),
    turn: text("review_turn"),
    assignment: row.try_get::<Option<i64>>("", "assignment_id")?.unwrap_or_default(),
  })
}

/// Where a job's review stands, as every read of the job answers it (docs/api.md, 「assignments and jobs」).
pub fn value(row: &sea_orm::QueryResult) -> Result<Value, sea_orm::DbErr> {
  let stages: Vec<String> = row
    .try_get::<Option<String>>("", "review_stages")?
    .and_then(|text| serde_json::from_str(&text).ok())
    .unwrap_or_default();

  Ok(json!({
    "stages": stages,
    "stage": row.try_get::<Option<i64>>("", "review_stage")?,
    "round": row.try_get::<i64>("", "review_round")?,
    "check": row.try_get::<Option<String>>("", "review_check")?,
    "rejections": row.try_get::<i64>("", "rejections")?,
    "question": row.try_get::<Option<i64>>("", "review_question")?,
    "parked": row.try_get::<bool>("", "review_parked")?,
  }))
}

/// The columns `value` reads, for a job's read to name.
pub const COLUMNS: &str = r#""review_stages", "review_stage", "review_round", "review_check", "rejections",
  "review_question", "review_parked""#;

// The reference every entry of a job's review is written under.
fn reference(id: i64) -> String {
  format!("job:{id}")
}

// The stage as an order and the owner's question name it.
fn stage_words(standing: &Standing) -> String {
  match (standing.stages.len(), standing.stage) {
    (0 | 1, _) => JOB_REVIEW.stage_one.clone(),
    (stages, Some(stage)) => fill_marks(
      &JOB_REVIEW.stage_of,
      &[
        ("stage", &stage.to_string()),
        ("stages", &stages.to_string()),
        ("step", step_of(standing, stage)),
      ],
    ),
    (_, None) => JOB_REVIEW.stage_final.clone(),
  }
}

fn step_of(standing: &Standing, stage: i64) -> &str {
  usize::try_from(stage - 1)
    .ok()
    .and_then(|at| standing.stages.get(at))
    .map_or("", String::as_str)
}

fn plan_of(stages: &[String]) -> String {
  stages
    .iter()
    .enumerate()
    .map(|(at, step)| format!("{}. {step}", at + 1))
    .collect::<Vec<_>>()
    .join("\n")
}

// The job service a review writes to, where the deployment names one.
fn service_of(vars: &BTreeMap<String, String>) -> Option<JobService> {
  JobService::of(vars)
}

pub(crate) async fn call(
  service: &JobService,
  method: reqwest::Method,
  path: &str,
  body: Option<&Value>,
) -> Result<Value, String> {
  let mut request = reqwest::Client::new()
    .request(method, format!("{}{path}", service.url))
    .bearer_auth(&service.token);

  if let Some(body) = body {
    request = request.json(body);
  }

  let answer = request.send().await.map_err(|why| why.to_string())?;
  let status = answer.status();

  if status == reqwest::StatusCode::NOT_FOUND {
    return Ok(Value::Null);
  }

  if !status.is_success() {
    let said = answer.text().await.unwrap_or_default();

    return Err(format!("the job service answered {} to {path}: {said}", status.as_u16()));
  }

  Ok(answer.json().await.unwrap_or(Value::Null))
}

// One entry of the item's review, written as the move is made; one that could not be written is said and
// stops nothing.
#[allow(clippy::too_many_arguments)]
async fn entry(
  vars: &BTreeMap<String, String>,
  id: i64,
  standing: &Standing,
  kind: &str,
  body: &str,
  actor: &str,
  labels: Value,
) {
  let Some(service) = service_of(vars) else {
    return;
  };
  let body = json!({
    "kind": kind,
    "body": if body.trim().is_empty() { "-" } else { body },
    "actorName": actor,
    "ref": reference(id),
    "stage": standing.stage,
    "round": standing.round.max(1),
    "labels": labels,
  });

  if let Err(why) = call(
    &service,
    reqwest::Method::POST,
    &format!("/items/{}/reviews", standing.item),
    Some(&body),
  )
  .await
  {
    eprintln!("job {id}: its review's {kind} was not written onto item #{}: {why}", standing.item);
  }
}

// The work order's task of a stage moved to `state`, where the item's work order holds one.
async fn task_moved(vars: &BTreeMap<String, String>, item: i64, stage: i64, state: &str) {
  let Some(service) = service_of(vars) else {
    return;
  };
  let path = format!("/items/{item}/work-order/tasks");
  let tasks = match call(&service, reqwest::Method::GET, &path, None).await {
    Ok(tasks) => tasks,
    Err(why) => {
      eprintln!("item #{item}: its work order's tasks were not read: {why}");
      return;
    }
  };
  let Some(task) = tasks["items"]
    .as_array()
    .into_iter()
    .flatten()
    .find(|task| task["stage"].as_i64() == Some(stage))
    .and_then(|task| task["id"].as_i64())
  else {
    return;
  };

  if let Err(why) = call(
    &service,
    reqwest::Method::PATCH,
    &format!("{path}/{task}"),
    Some(&json!({ "state": state })),
  )
  .await
  {
    eprintln!("item #{item}: the task of stage {stage} was not moved to {state}: {why}");
  }
}

/// The review opened as a job is launched: its stages read from the item's work order and kept on the job, the
/// first stage's task made current, the `opened` entry written, and the words the first prompt closes with. A
/// review already opened, by the launch of a process before this one, is opened no second time: the words are
/// those of the stages it kept. None where the job was stopped while its work order could not be read.
pub async fn opened(ground: &Ground, id: i64) -> Option<String> {
  let Ok(standing) = standing(&ground.db, id).await else {
    return Some(JOB_REVIEW.one_stage.clone());
  };

  if standing.stage.is_some() || standing.turn.is_some() {
    return Some(opening(&standing));
  }

  let stages = match service_of(ground.vars()) {
    Some(service) => work_order(ground, &service, id, standing.item).await?,
    None => vec![],
  };

  if let Err(why) = run(
    &ground.db,
    r#"UPDATE "jobs" SET "review_stages" = $1, "review_stage" = 1, "review_round" = 1, "review_turn" = $2
       WHERE "id" = $3"#,
    vec![serde_json::to_string(&stages).unwrap_or_default().into(), WORK.into(), id.into()],
  )
  .await
  {
    eprintln!("job {id}: where its review stands was not written: {}", why.detail);
  }

  let standing = Standing {
    stages: stages.clone(),
    stage: Some(1),
    round: 1,
    ..standing
  };
  let shown = if stages.is_empty() {
    plan_of(std::slice::from_ref(&JOB_REVIEW.stage_one))
  } else {
    plan_of(&stages)
  };

  entry(
    ground.vars(),
    id,
    &standing,
    "opened",
    &fill_marks(&JOB_REVIEW.opened, &[("stages", &stages.len().max(1).to_string()), ("plan", &shown)]),
    &JOB_REVIEW.actor,
    json!({}),
  )
  .await;

  if stages.len() > 1 {
    task_moved(ground.vars(), standing.item, 1, "current").await;
  }

  Some(opening(&standing))
}

// The words a job's first prompt closes with, from the stages its review keeps.
fn opening(standing: &Standing) -> String {
  if standing.stages.len() < 2 {
    return JOB_REVIEW.one_stage.clone();
  }

  fill_marks(
    &JOB_REVIEW.first_stage,
    &[
      ("stages", &standing.stages.len().to_string()),
      ("plan", &plan_of(&standing.stages)),
      ("step", step_of(standing, 1)),
    ],
  )
}

/// The variable a deployment names the first pause in milliseconds before a work order is asked again in, in
/// place of the policy's.
pub const WORK_ORDER_PAUSE: &str = "BMO_WORK_ORDER_PAUSE_MS";

// The steps of an item's work order, read from the job service (docs/job-review.md, 「Stages」): none where it holds
// no work order, and asked again after a doubling pause, however long it takes, where it could not be read, the
// job held with the wait meanwhile. None where the job was stopped while it waited.
async fn work_order(ground: &Ground, service: &JobService, id: i64, item: i64) -> Option<Vec<String>> {
  let path = format!("/items/{item}/work-order");
  let first = ground
    .vars()
    .get(WORK_ORDER_PAUSE)
    .and_then(|ms| ms.trim().parse().ok())
    .unwrap_or(JOB_REVIEW.work_order_first_ms);
  let mut pause = std::time::Duration::from_millis(first);
  let longest = std::time::Duration::from_millis(JOB_REVIEW.work_order_longest_ms.max(first));
  let mut held = false;

  loop {
    match call(service, reqwest::Method::GET, &path, None).await {
      Ok(order) => {
        if held {
          crate::control::work::hold_job(ground, id, None, false).await;
        }

        return Some(steps_of(&order));
      }
      Err(why) => {
        eprintln!("job {id}: its item's work order was not read, so it is asked again: {why}");

        if !held {
          let wait = fill_marks(&JOB_REVIEW.work_order_wait, &[("item", &item.to_string()), ("why", &why)]);

          crate::control::work::hold_job(ground, id, Some(&wait), true).await;
          held = true;
        }
      }
    }

    if crate::control::work::called_off(ground, id).await {
      crate::control::work::hold_job(ground, id, None, false).await;

      return None;
    }

    tokio::time::sleep(pause).await;
    pause = (pause * 2).min(longest);
  }
}

// The steps a work order holds, in order, each trimmed and none empty; none of a work order that is not there.
fn steps_of(order: &Value) -> Vec<String> {
  order["steps"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .map(str::trim)
    .filter(|step| !step.is_empty())
    .map(str::to_owned)
    .collect()
}

/// The session a job's turn answered on, kept so the review's next turn continues it.
pub async fn session_left(db: &DatabaseConnection, id: i64, taken: &Road, end: &agent_session::TurnEnd) {
  let Some(session) = end.reply.as_ref().and_then(|reply| reply.session_id.clone()) else {
    return;
  };
  let resumed = Resumed {
    provider: taken.provider.clone(),
    model: taken.model.clone(),
    session,
    effort: end.went_out.as_ref().and_then(|went| went.effort.clone()),
    prefix: end.prefix.clone(),
  };

  if let Err(why) = run(
    db,
    r#"UPDATE "jobs" SET "review_session" = $1 WHERE "id" = $2"#,
    vec![serde_json::to_string(&resumed).unwrap_or_default().into(), id.into()],
  )
  .await
  {
    eprintln!("job {id}: the session its turn answered on was not kept: {}", why.detail);
  }
}

/// The prompt a turn of the review goes out with where the job's session cannot be continued: the job's own
/// prompt, its review so far read from the job service, and the turn's words.
pub async fn carried(ground: &Ground, member: &Value, prompt: &str, words: &str) -> String {
  let id = member["id"].as_i64().unwrap_or_default();
  let item = member["itemId"].as_i64().unwrap_or_default();
  let record = match service_of(ground.vars()) {
    Some(service) => call(
      &service,
      reqwest::Method::GET,
      &format!("/items/{item}/reviews?ref={}&limit=500", reference(id)),
      None,
    )
    .await
    .map(|page| {
      page["items"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| {
          format!(
            "### {} (round {}, by {})\n\n{}",
            entry["kind"].as_str().unwrap_or_default(),
            entry["round"],
            entry["actorName"].as_str().unwrap_or_default(),
            entry["body"].as_str().unwrap_or_default()
          )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
    })
    .unwrap_or_default(),
    None => String::new(),
  };

  format!(
    "{}\n\n{words}",
    fill_marks(&JOB_REVIEW.carried, &[("prompt", prompt), ("record", &record)])
  )
}

// The orderer of the job's item, as every notice of the job finds it.
async fn orderer_of(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  id: i64,
) -> Option<(Value, crate::queue::orderer::Orderer)> {
  let (member, _) = job(db, id).await.ok()?;
  let orderer = crate::job_end::orderer(vars, &member).await;

  Some((member, orderer))
}

/// The language a job's run writes its results for a person in, as the order to its orderer names it, read off
/// the road its last turn answered on (docs/job-review.md, 「The checks」): on a registration that names the
/// languages it writes, the brief's where it is one of them and English otherwise; on every other road, or where
/// the registry cannot be read, the brief's.
pub async fn result_language(db: &DatabaseConnection, road: Option<&Resumed>) -> String {
  let held = match road {
    Some(road) if road.provider == crate::control::registry::PROVIDER => {
      crate::control::registry::registration(db, &road.model).await.unwrap_or_else(|why| {
        eprintln!("the registration {} was not read for its languages: {}", road.model, why.error);

        None
      })
    }
    _ => None,
  };

  match held.filter(|held| !held.languages.is_empty()) {
    Some(held) => fill_marks(
      &JOB_REVIEW.result_language_listed,
      &[("model", &held.name), ("languages", &crate::text::listed(&held.languages))],
    ),
    None => JOB_REVIEW.result_language_brief.clone(),
  }
}

/// The prefix of the key a job's check is owed under, its item's number after it.
pub const CHECK_OWED: &str = "job-review:";

/// What the words a check is told with are: the report, the worker's answer to the orderer's question, the
/// owner's word, or the report a check before it in the same round was told with, which a later check names
/// rather than carries (docs/job-review.md, 「The checks」).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Heard {
  Report,
  Answer,
  Ruling,
  Unchanged,
}

impl Heard {
  fn heading(self) -> &'static str {
    match self {
      Self::Report | Self::Unchanged => &JOB_REVIEW.report_heading,
      Self::Answer => &JOB_REVIEW.answer_heading,
      Self::Ruling => &JOB_REVIEW.ruling_heading,
    }
  }

  // The words the check is told with: those it was handed, or the line naming the report told before.
  fn words(self, item: i64, words: &str) -> String {
    match self {
      Self::Unchanged => fill_marks(&JOB_REVIEW.report_unchanged, &[("item", &item.to_string())]),
      _ => words.to_owned(),
    }
  }
}

/// The words a due check is told with, kept on the job as the check comes due, so the sender owes the check where
/// a stop cut its row off (docs/job-review.md, 「The checks」).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Told {
  heard: Heard,
  words: String,
  /// When the check came due, in epoch milliseconds.
  at: i64,
}

impl Told {
  fn now(heard: Heard, words: &str) -> String {
    let told = Self {
      heard,
      words: words.to_owned(),
      at: crate::now_ms(),
    };

    serde_json::to_string(&told).unwrap_or_default()
  }
}

// The check due now owed to the orderer, with the words it examines under their heading: a standing notice, told
// again until the orderer moves (docs/job-review.md, 「The checks」).
async fn tell_check(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  id: i64,
  standing: &Standing,
  heard: Heard,
  words: &str,
) {
  let heading = heard.heading();
  let words = heard.words(standing.item, words);

  if Receiver::of(vars).is_none() {
    eprintln!("job {id}: its review was not told: no receiver is named");
    return;
  }
  let Some((member, orderer)) = orderer_of(db, vars, id).await else {
    return;
  };
  let check = standing.check.clone().unwrap_or_default();
  let requirement = JOB_REVIEW.check(&check).map_or("", |check| check.requirement.as_str());
  let moves = if standing.rejections >= JOB_REVIEW.escalate_after {
    &JOB_REVIEW.moves_open
  } else {
    &JOB_REVIEW.moves_early
  };
  let language = result_language(db, standing.session.as_ref()).await;
  let order = fill_marks(
    &JOB_REVIEW.review_order,
    &[
      ("job", &id.to_string()),
      ("item", &standing.item.to_string()),
      ("worker", member["workerName"].as_str().unwrap_or_default()),
      ("stage", &stage_words(standing)),
      ("round", &standing.round.to_string()),
      ("rejections", &standing.rejections.to_string()),
      ("language", &language),
      ("check", &check),
      ("requirement", requirement),
      ("heading", heading),
      ("words", &words),
      ("moves", moves),
    ],
  );
  let body = instruction(
    "job-review",
    standing.item,
    &orderer,
    format!("The job of item #{} awaits its review's check {check}", standing.item),
    order,
  );

  if let Err(why) = outbox::owe(db, vars, Notice::instruction(body, Keep::Standing)).await {
    eprintln!("job {id}: its review's check was not owed: {why}");
  }
}

/// The checks owed to an orderer let go where no job of their item stands under review with a check due, no turn
/// going out and no owner's question out, however the job came to move on: what the sender's clock runs before
/// it sends, so no check is told again once it is no longer due.
pub async fn checks_owed(db: &DatabaseConnection, vars: &BTreeMap<String, String>) -> Result<(), String> {
  let owed = outbox::owed(db, CHECK_OWED).await?;

  if owed.is_empty() {
    return Ok(());
  }

  let due: std::collections::BTreeSet<String> = store::all(
    db,
    r#"SELECT DISTINCT "item_id" FROM "jobs" WHERE "state" = 'reviewing' AND "review_check" IS NOT NULL
         AND "review_turn" IS NULL AND "review_question" IS NULL"#,
    vec![],
  )
  .await
  .map_err(|why| why.detail)?
  .iter()
  .filter_map(|row| row.try_get::<i64>("", "item_id").ok())
  .map(|item| item.to_string())
  .collect();

  for (item, row) in owed.iter().filter(|(item, _)| !due.contains(*item)) {
    outbox::let_go_as_read(db, vars, &format!("{CHECK_OWED}{item}"), row).await?;
    println!("job review: the check owed for item #{item} let go, no longer due");
  }

  Ok(())
}

/// The checks due that no row is owed for, owed: each job standing under review with a check due, no turn going
/// out and no owner's question out, whose check came due at least `owe_grace_seconds` ago and whose item has no
/// check owed — a stop of agency's cut its row off, or the store refused it. Answers how many were owed.
pub async fn unowed_checks(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
) -> Result<usize, String> {
  if Receiver::of(vars).is_none() {
    return Ok(0);
  }

  let grace = i64::try_from(JOB_NOTICES.owe_grace_seconds * 1000).unwrap_or(i64::MAX / 2);
  let rows = store::all(
    db,
    &format!(
      r#"SELECT "id", "review_told" FROM "jobs" WHERE "state" = 'reviewing' AND "review_check" IS NOT NULL
           AND "review_turn" IS NULL AND "review_question" IS NULL AND "review_told" IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM "notices" WHERE "route" <> 'withdrawal'
             AND "key" = '{CHECK_OWED}' || "jobs"."item_id"::text)
         ORDER BY "id""#
    ),
    vec![],
  )
  .await
  .map_err(|why| why.detail)?;
  let before = crate::now_ms() - grace;
  let mut owed = 0;

  for row in rows {
    let id: i64 = row.try_get("", "id").map_err(|why| why.to_string())?;
    let told: String = row.try_get("", "review_told").map_err(|why| why.to_string())?;
    let Ok(told) = serde_json::from_str::<Told>(&told) else {
      eprintln!("job {id}: the words its check is told with were not read");
      continue;
    };

    if told.at > before {
      continue;
    }

    let standing = standing(db, id).await.map_err(|why| why.error)?;

    tell_check(db, vars, id, &standing, told.heard, &told.words).await;
    println!(
      "job {id}: its review's check {} owed, no row standing for it",
      standing.check.unwrap_or_default()
    );
    owed += 1;
  }

  Ok(owed)
}

// The check owed for a job's item as it stood before its orderer's move.
async fn check_owed(db: &DatabaseConnection, item: i64) -> Option<outbox::Owed> {
  outbox::owed(db, &format!("{CHECK_OWED}{item}")).await.ok()?.remove("")
}

// The check owed for a job's item let go once its orderer's move is made, as it stood before the move: a move that
// owed a check afresh leaves that one standing.
async fn check_moved(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  item: i64,
  owed: Option<outbox::Owed>,
) {
  let Some(owed) = owed else { return };

  if let Err(why) = outbox::let_go_as_read(db, vars, &format!("{CHECK_OWED}{item}"), &owed).await {
    eprintln!("job review: the check owed for item #{item} was not let go: {why}");
  }
}

/// A job's answered turn written into its review (docs/job-review.md, 「The checks」): the report or the answer
/// written as its entry, the job standing `reviewing` with what the turn spent added, its slot given back, and its
/// orderer told the check due now.
pub async fn reported(ground: &Ground, id: i64, words: &str, spent: Option<Spent>) {
  let db = &ground.db;
  let Ok(mut standing) = standing(db, id).await else {
    return;
  };
  let asked = standing.turn.as_deref() == Some(QUESTION);
  let heard = if asked { Heard::Answer } else { Heard::Report };
  let first = JOB_REVIEW.checks.first().map(|check| check.name.clone());

  if !asked {
    standing.check = first;
    standing.words = Some(words.to_owned());
  }

  let written = run(
    db,
    &format!(
      r#"UPDATE "jobs" SET "state" = 'reviewing', "review_turn" = NULL, "review_check" = $1,
           "review_words" = $2, "report" = $3, "review_told" = $11, {SPENT_ADDED}
         WHERE "id" = $6 AND "state" IN ('starting', 'running')"#
    ),
    vec![
      standing.check.clone().into(),
      standing.words.clone().into(),
      standing.words.clone().into(),
      spent.as_ref().map_or(0, Spent::total).into(),
      Option::<i64>::None.into(),
      id.into(),
      spent.map(|spent| spent.input_tokens).into(),
      spent.map(|spent| spent.output_tokens).into(),
      spent.map(|spent| spent.cache_read_tokens).into(),
      spent.map(|spent| spent.cache_write_tokens).into(),
      Told::now(heard, words).into(),
    ],
  )
  .await;

  match written {
    Ok(0) => return,
    Ok(_) => crate::control::work::add_job_cost(db, id, spent, None).await,
    Err(why) => {
      eprintln!("job {id}: its report was not written into its review: {}", why.detail);
      return;
    }
  }

  println!(
    "job {id}: reviewing, {} of round {}",
    if asked { "an answer" } else { "a report" },
    standing.round
  );
  entry(
    ground.vars(),
    id,
    &standing,
    if asked { "answer" } else { "report" },
    words,
    &standing.worker,
    json!({}),
  )
  .await;

  tell_check(&ground.db, ground.vars(), id, &standing, heard, words).await;

  if let Ok((member, _)) = job(db, id).await {
    publish(ground, "job.changed", member);
  }
}

// What the orderer may do with a report under review.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
  Ask,
  Reject,
  Pass,
  Escalate,
}

impl Action {
  fn read(word: &str) -> Option<Self> {
    match word {
      "ask" => Some(Self::Ask),
      "reject" => Some(Self::Reject),
      "pass" => Some(Self::Pass),
      "escalate" => Some(Self::Escalate),
      _ => None,
    }
  }
}

fn refused(status: StatusCode, code: &str, sentence: String) -> Refused {
  Refused::new(status, sentence).with_code(code)
}

/// A job's report examined by its orderer (docs/job-review.md, 「What the orderer may do」).
pub struct ReviewJob;

impl Route for ReviewJob {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/job-reviews";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let bad = |why: String| refused(StatusCode::BAD_REQUEST, "bad-request", why);
    let ask: JobReviewAsk =
      serde_json::from_value(call.body().await?).map_err(|why| bad(format!("a job's review: {why}")))?;
    let Some(action) = Action::read(ask.action.trim()) else {
      return Err(bad(format!("a job's review takes ask, reject, pass or escalate, not {}", ask.action)));
    };
    let text = ask.text.as_deref().map(str::trim).unwrap_or_default().to_owned();

    if action != Action::Pass && text.is_empty() {
      return Err(bad(format!("a job's review: {} names its words in text", ask.action.trim())));
    }

    if crate::turn_marks::of(&ask.agent_name).is_none() {
      return Err(refused(
        StatusCode::CONFLICT,
        "no-turn",
        format!("{} is carrying no turn to review a job for", ask.agent_name),
      ));
    }

    let ground = call.shared_ground();
    let sessions = call.sessions();
    if ask.stage.is_some() && action != Action::Reject {
      return Err(bad(format!("a job's review: only reject names a stage, not {}", ask.action.trim())));
    }

    let member = acted(ground, sessions, &ask.agent_name, ask.job_id, action, &text, ask.stage).await?;

    Ok(answer(StatusCode::OK, json!(reviewed(&member)?)))
  }
}

/// Where a job stands after its orderer's move, as the move answers it (docs/job-review.md, 「What the orderer
/// may do」): its id, item and state and its review's position, with nothing of its report, its steps or its
/// spend, which the review's instruction told or a read of the job answers.
fn reviewed(member: &Value) -> Result<JobReviewed, Refused> {
  let review = &member["review"];
  let read = |why: serde_json::Error| Refused::failed(format!("the job as it stands was not read: {why}"));

  Ok(JobReviewed {
    id: member["id"].as_i64().unwrap_or_default(),
    item_id: member["itemId"].as_i64().unwrap_or_default(),
    state: serde_json::from_value(member["state"].clone()).map_err(read)?,
    review: ReviewPosition {
      stage: review["stage"].as_i64(),
      stages: review["stages"].as_array().map_or(0, Vec::len).max(1) as i64,
      round: review["round"].as_i64().unwrap_or_default(),
      check: review["check"].as_str().map(str::to_owned),
      rejections: review["rejections"].as_i64().unwrap_or_default(),
      question: review["question"].as_i64(),
    },
  })
}

crate::moved!(ReviewJob);

// The move made: refused where it does not fit the job, else written, and the turn it asks for started apart
// from the answer.
async fn acted(
  ground: Arc<Ground>,
  sessions: Option<crate::herdr_backend::Shared>,
  agent: &str,
  id: i64,
  action: Action,
  text: &str,
  back_to: Option<i64>,
) -> Result<Value, Refused> {
  let db = &ground.db;
  let mut standing = standing(db, id).await?;

  if standing.state != "reviewing" {
    return Err(refused(
      StatusCode::CONFLICT,
      "not-reviewing",
      format!("job {id} is {}, not under review", standing.state),
    ));
  }

  // A turn the review asked for is going out, its checkout being taken again first: nothing is examined until
  // its words come back.
  if standing.turn.is_some() {
    return Err(refused(
      StatusCode::CONFLICT,
      "not-reviewing",
      format!("job {id}'s review has a turn going out, and its words are not back yet"),
    ));
  }

  if let Some(question) = standing.question {
    return Err(refused(
      StatusCode::CONFLICT,
      "awaits-owner",
      format!("job {id} waits on the owner's question #{question}"),
    ));
  }

  let check = standing.check.clone().unwrap_or_default();
  let owed = check_owed(db, standing.item).await;
  let item = standing.item;

  match action {
    Action::Ask => {
      entry(ground.vars(), id, &standing, "question", text, agent, json!({ "check": check })).await;
      let words = fill_marks(&JOB_REVIEW.question_turn, &[("check", &check), ("words", text)]);

      turn_out(ground.clone(), sessions, id, &standing, QUESTION, words).await?;
    }
    Action::Reject => {
      let back = sent_back_to(id, &standing, back_to)?;

      if let Some(back) = back {
        stage_reopened(&ground, &mut standing, back).await;
      }

      entry(ground.vars(), id, &standing, "rejection", text, agent, json!({ "check": check })).await;
      standing.rejections += 1;
      standing.round += 1;
      run(
        db,
        r#"UPDATE "jobs" SET "rejections" = $1, "review_round" = $2, "review_stage" = $3 WHERE "id" = $4"#,
        vec![standing.rejections.into(), standing.round.into(), standing.stage.into(), id.into()],
      )
      .await?;
      let words = match back {
        Some(back) => fill_marks(
          &JOB_REVIEW.rejection_back_turn,
          &[
            ("words", text),
            ("stage", &back.to_string()),
            ("stages", &standing.stages.len().to_string()),
            ("step", step_of(&standing, back)),
          ],
        ),
        None => fill_marks(&JOB_REVIEW.rejection_turn, &[("words", text)]),
      };

      turn_out(ground.clone(), sessions, id, &standing, WORK, words).await?;
    }
    Action::Pass => {
      entry(ground.vars(), id, &standing, "pass", text, agent, json!({ "check": check })).await;
      passed(ground.clone(), sessions, agent, id, standing, &check).await?;
    }
    Action::Escalate => {
      if standing.rejections < JOB_REVIEW.escalate_after {
        return Err(refused(
          StatusCode::CONFLICT,
          "escalation-early",
          format!(
            "job {id}'s work was sent back {} times; it is put to the owner only once it was sent back {} times",
            standing.rejections, JOB_REVIEW.escalate_after
          ),
        ));
      }

      escalated(&ground, agent, id, &standing, text).await?;
    }
  }

  check_moved(db, ground.vars(), item, owed).await;

  let (member, _) = job(db, id).await?;

  publish(&ground, "job.changed", member.clone());

  Ok(member)
}

// The stage a rejection sends the work back to, where it names one other than the stage under way: one the
// job has reached, of a job of several stages (docs/job-review.md, 「Stages」). The final report stands past
// every stage, so each of them is one it may name.
fn sent_back_to(id: i64, standing: &Standing, named: Option<i64>) -> Result<Option<i64>, Refused> {
  let Some(named) = named else { return Ok(None) };
  let stages = standing.stages.len() as i64;
  let reached = standing.stage.unwrap_or(stages);

  if stages < 2 || named < 1 || named > reached {
    return Err(refused(
      StatusCode::BAD_REQUEST,
      "bad-request",
      format!("job {id}'s work goes back only to a stage it has reached, 1 to {reached}, not {named}"),
    ));
  }

  Ok((Some(named) != standing.stage).then_some(named))
}

// The work sent back to a stage already passed: that stage under way again and every stage after it reached so
// far waiting again, on the job service's tasks as on the job.
async fn stage_reopened(ground: &Ground, standing: &mut Standing, back: i64) {
  let reached = standing.stage.unwrap_or(standing.stages.len() as i64);

  for later in (back + 1)..=reached {
    task_moved(ground.vars(), standing.item, later, "waiting").await;
  }

  task_moved(ground.vars(), standing.item, back, "current").await;
  standing.stage = Some(back);
}

// A check passed: the next check told, or the stage passed and the next stage, the final report or the job's end
// following.
async fn passed(
  ground: Arc<Ground>,
  sessions: Option<crate::herdr_backend::Shared>,
  agent: &str,
  id: i64,
  mut standing: Standing,
  check: &str,
) -> Result<(), Refused> {
  let db = &ground.db;

  // The next check of the round examines the report its first check was told with, so it names that report
  // rather than carrying its words again (docs/job-review.md, 「The checks」).
  if let Some(next) = JOB_REVIEW.after(check) {
    standing.check = Some(next.name.clone());

    run(
      db,
      r#"UPDATE "jobs" SET "review_check" = $1, "review_told" = $3 WHERE "id" = $2"#,
      vec![next.name.clone().into(), id.into(), Told::now(Heard::Unchanged, "").into()],
    )
    .await?;

    tell_check(&ground.db, ground.vars(), id, &standing, Heard::Unchanged, "").await;

    return Ok(());
  }

  let stages = standing.stages.len() as i64;

  match standing.stage {
    Some(stage) if stages > 1 => {
      task_moved(ground.vars(), standing.item, stage, "done").await;

      if stage < stages {
        let next = stage + 1;
        standing.stage = Some(next);
        standing.round = 1;
        run(
          db,
          r#"UPDATE "jobs" SET "review_stage" = $1, "review_round" = 1 WHERE "id" = $2"#,
          vec![next.into(), id.into()],
        )
        .await?;
        task_moved(ground.vars(), standing.item, next, "current").await;
        let last = if next == stages { JOB_REVIEW.last_stage.as_str() } else { "" };
        let words = fill_marks(
          &JOB_REVIEW.next_stage,
          &[
            ("passed", &stage.to_string()),
            ("stage", &next.to_string()),
            ("stages", &stages.to_string()),
            ("last", last),
            ("step", step_of(&standing, next)),
          ],
        );

        return turn_out(ground.clone(), sessions, id, &standing, WORK, words).await;
      }

      standing.stage = None;
      standing.round = 1;
      run(
        db,
        r#"UPDATE "jobs" SET "review_stage" = NULL, "review_round" = 1 WHERE "id" = $1"#,
        vec![id.into()],
      )
      .await?;

      turn_out(ground.clone(), sessions, id, &standing, FINAL, JOB_REVIEW.final_turn.clone()).await
    }
    _ => {
      let words = standing.words.clone().unwrap_or_default();

      entry(ground.vars(), id, &standing, "accepted", &words, agent, json!({})).await;
      println!("job {id}: its review took the work after {} rejections", standing.rejections);
      written_end(&ground, id, "ended", None, Some(words), None).await;

      Ok(())
    }
  }
}

// A turn the review asks for, started apart from the answer: the job written running with the kind of turn it
// is and the turn's words, which it keeps until the turn comes back from its session, its checkout taken again
// first where it gave it up.
async fn turn_out(
  ground: Arc<Ground>,
  sessions: Option<crate::herdr_backend::Shared>,
  id: i64,
  standing: &Standing,
  kind: &str,
  words: String,
) -> Result<(), Refused> {
  run(
    &ground.db,
    r#"UPDATE "jobs" SET "review_turn" = $1, "review_check" = CASE WHEN $1 = 'question' THEN "review_check" END,
         "turn_words" = $3
       WHERE "id" = $2"#,
    vec![kind.into(), id.into(), words.clone().into()],
  )
  .await?;
  let turn = Turn {
    words,
    resume: standing.session.clone(),
  };
  let standing = standing.clone();

  tokio::spawn(async move { gone_out(ground, sessions, id, &standing, turn).await });

  Ok(())
}

// A turn the review asked for carried: the job's checkout taken again first where it gave it up, the job written
// running, and the turn carried as the job's first turn is.
async fn gone_out(
  ground: Arc<Ground>,
  sessions: Option<crate::herdr_backend::Shared>,
  id: i64,
  standing: &Standing,
  turn: Turn,
) {
  if standing.parked && !reclaimed(&ground, id, standing.assignment, &standing.worker).await {
    return;
  }

  if let Err(why) = run(
    &ground.db,
    r#"UPDATE "jobs" SET "state" = 'running' WHERE "id" = $1 AND "state" = 'reviewing'"#,
    vec![id.into()],
  )
  .await
  {
    eprintln!("job {id}: its review's turn was not started: {}", why.detail);
    return;
  }

  carry_review_turn(ground, sessions, id, turn).await;
}

/// A turn the review asked for that had not gone out when agency restarted, carried again from the words the job
/// keeps (docs/api.md, 「assignments and jobs」): its checkout taken again where the job had still to take it,
/// and the turn carried on the session the job's last turn left, as it would have gone out.
pub async fn resumed_turn(ground: Arc<Ground>, sessions: Option<crate::herdr_backend::Shared>, id: i64) {
  let words = store::one(&ground.db, r#"SELECT "turn_words" FROM "jobs" WHERE "id" = $1"#, vec![id.into()])
    .await
    .ok()
    .flatten()
    .and_then(|row| row.try_get::<Option<String>>("", "turn_words").ok().flatten());
  let (Ok(standing), Some(words)) = (standing(&ground.db, id).await, words) else {
    eprintln!("job {id}: the turn its review asked for could not be read again");
    return;
  };
  let turn = Turn {
    words,
    resume: standing.session.clone(),
  };

  gone_out(ground, sessions, id, &standing, turn).await;
}

// How often a job that gave its checkout up looks again whether the checkout is free.
const RECLAIM_LOOK: std::time::Duration = std::time::Duration::from_secs(5);

// The checkout a parked job gave up taken again for it, once no other hold or job stands on it, however long
// that takes (docs/job-review.md, 「The owner's word」). False where the job was stopped while it waited.
async fn reclaimed(ground: &Ground, id: i64, assignment: i64, worker: &str) -> bool {
  let db = &ground.db;

  loop {
    if crate::control::work::called_off(ground, id).await {
      return false;
    }

    let taken = run(
      db,
      r#"UPDATE "assignments" SET "state" = 'running', "changed_at" = $1
         WHERE "id" = $2
           AND NOT EXISTS (SELECT 1 FROM "assignments" a
                 WHERE a."worker_name" = $3 AND a."id" <> $2 AND a."state" IN ('held', 'running'))
           AND NOT EXISTS (SELECT 1 FROM "jobs" j
                 WHERE j."worker_name" = $3 AND j."id" <> $4
                   AND (j."state" IN ('starting', 'running') OR (j."state" = 'reviewing' AND NOT j."review_parked")))"#,
      vec![crate::now_ms().into(), assignment.into(), worker.to_owned().into(), id.into()],
    )
    .await;

    match taken {
      Ok(1..) => break,
      Ok(0) => {}
      Err(why) => eprintln!("job {id}: its checkout {worker} could not be taken again yet: {}", why.detail),
    }

    tokio::time::sleep(RECLAIM_LOOK).await;
  }

  if let Err(why) =
    run(db, r#"UPDATE "jobs" SET "review_parked" = false WHERE "id" = $1"#, vec![id.into()]).await
  {
    eprintln!("job {id}: its checkout was taken again but not marked: {}", why.detail);
  }

  println!("job {id}: took its checkout {worker} again");

  true
}

// The work put to the owner: the escalation written and the owner's question asked on the item.
async fn escalated(
  ground: &Ground,
  agent: &str,
  id: i64,
  standing: &Standing,
  text: &str,
) -> Result<(), Refused> {
  let check = standing.check.clone().unwrap_or_default();
  // The escalation is the supervisor's own move, so it asks the owner under the supervisors' token, the one the job
  // service lets ask a person.
  let Some(service) = JobService::of_supervisor(ground.vars()) else {
    return Err(refused(
      StatusCode::BAD_GATEWAY,
      "upstream-failed",
      "no job service is named to put the work to the owner".to_owned(),
    ));
  };
  let question = json!({
    "askerName": JOB_REVIEW.actor,
    "itemId": standing.item,
    "addressee": "person",
    "text": fill_marks(
      &JOB_REVIEW.question_text,
      &[
        ("words", text),
        ("job", &id.to_string()),
        ("item", &standing.item.to_string()),
        ("stage", &stage_words(standing)),
        ("rejections", &standing.rejections.to_string()),
      ],
    ),
    "choices": [JOB_REVIEW.send_back, JOB_REVIEW.take_it],
    "waitMs": JOB_REVIEW.owner_wait_seconds * 1000,
  });
  let upstream = |why| refused(StatusCode::BAD_GATEWAY, "upstream-failed", why);
  let number = match waiting_question(&service, &question).await.map_err(upstream)? {
    Some(number) => number,
    None => call(&service, reqwest::Method::POST, "/questions", Some(&question))
      .await
      .map_err(upstream)?["id"]
      .as_i64()
      .ok_or_else(|| upstream("the job service answered the owner's question with no number".to_owned()))?,
  };

  entry(
    ground.vars(),
    id,
    standing,
    "escalation",
    text,
    agent,
    json!({ "check": check, "question": number.to_string() }),
  )
  .await;
  run(
    &ground.db,
    r#"UPDATE "jobs" SET "review_question" = $1 WHERE "id" = $2"#,
    vec![number.into(), id.into()],
  )
  .await?;
  println!("job {id}: its work was put to the owner as question #{number}");

  Ok(())
}

// The question on the item still waiting for an answer that asks what `question` asks — the same asker, words
// and choices — where one stands: an escalation made again after its question landed but its answer did not
// takes that one, so the owner is asked once and told once.
async fn waiting_question(service: &JobService, question: &Value) -> Result<Option<i64>, String> {
  let Some(item) = question["itemId"].as_i64() else {
    return Ok(None);
  };
  let page =
    call(service, reqwest::Method::GET, &format!("/questions?itemId={item}&limit=500"), None).await?;
  let asks_the_same = |standing: &&Value| {
    matches!(standing["state"].as_str(), Some("open" | "paused"))
      && ["askerName", "text", "choices"].iter().all(|field| standing[field] == question[field])
  };

  Ok(
    page["items"]
      .as_array()
      .into_iter()
      .flatten()
      .rev()
      .find(asks_the_same)
      .and_then(|one| one["id"].as_i64()),
  )
}

// The job whose work waits on the owner's question, where one does.
async fn waiting_on(db: &DatabaseConnection, question: i64) -> Option<i64> {
  one(
    db,
    r#"SELECT "id" FROM "jobs" WHERE "review_question" = $1 AND "state" = 'reviewing' ORDER BY "id" DESC LIMIT 1"#,
    vec![question.into()],
  )
  .await
  .ok()
  .flatten()
  .and_then(|row| row.try_get::<i64>("", "id").ok())
}

/// The owner's question past its due time (docs/job-review.md, 「The owner's word」): the job whose work waits on
/// it gives its checkout up, and its orderer is told to launch the work that can go now. The same event taken
/// again moves nothing more and owes the notice again, so a stop between the two leaves it owed. Fails only where
/// the notice could not be owed, so the event is taken again.
pub async fn question_due(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  question: i64,
) -> Result<(), String> {
  let Some(id) = waiting_on(db, question).await else {
    return Ok(());
  };
  let Ok(standing) = standing(db, id).await else {
    return Ok(());
  };

  if !standing.parked {
    if let Err(why) = run(
      db,
      r#"UPDATE "assignments" SET "state" = 'released', "changed_at" = $1 WHERE "id" = $2 AND "state" IN ('held', 'running')"#,
      vec![crate::now_ms().into(), standing.assignment.into()],
    )
    .await
    {
      eprintln!("job {id}: its checkout was not given up: {}", why.detail);
      return Ok(());
    }

    let _ = run(db, r#"UPDATE "jobs" SET "review_parked" = true WHERE "id" = $1"#, vec![id.into()]).await;
    println!(
      "job {id}: gave its checkout {} up while the owner's question #{question} waits",
      standing.worker
    );
  }

  if Receiver::of(vars).is_none() {
    return Ok(());
  }

  let (member, _) = job(db, id).await.map_err(|why| why.error)?;
  let orderer = crate::job_end::orderer(vars, &member).await;
  let order = fill_marks(
    &JOB_REVIEW.parked_order,
    &[
      ("job", &id.to_string()),
      ("item", &standing.item.to_string()),
      ("question", &question.to_string()),
      ("worker", &standing.worker),
    ],
  );
  let body = instruction(
    "job-review-parked",
    standing.item,
    &orderer,
    format!("The job of item #{} gave its checkout up", standing.item),
    order,
  );

  outbox::owe(db, vars, Notice::instruction(body, Keep::Once))
    .await
    .map_err(|why| format!("job {id}: its checkout given up was not owed: {why}"))
}

/// The owner's settling answer to a job's question (docs/job-review.md, 「The owner's word」): written as the
/// round's ruling, and told to the orderer as the check due, with the owner's words.
pub async fn ruled(db: &DatabaseConnection, vars: &BTreeMap<String, String>, reply: &Value) {
  let Some(question) = reply["questionId"].as_i64() else {
    return;
  };
  let Some(id) = waiting_on(db, question).await else {
    return;
  };
  let Ok(mut standing) = standing(db, id).await else {
    return;
  };
  let words = [reply["choice"].as_str(), reply["text"].as_str()]
    .into_iter()
    .flatten()
    .map(str::trim)
    .filter(|said| !said.is_empty())
    .collect::<Vec<_>>()
    .join("\n\n");
  let actor = reply["actorName"].as_str().unwrap_or("owner");

  if let Err(why) = run(
    db,
    r#"UPDATE "jobs" SET "review_question" = NULL, "review_told" = $2 WHERE "id" = $1"#,
    vec![id.into(), Told::now(Heard::Ruling, &words).into()],
  )
  .await
  {
    eprintln!("job {id}: the owner's answer was not taken: {}", why.detail);
    return;
  }

  standing.question = None;
  entry(
    vars,
    id,
    &standing,
    "ruling",
    &words,
    actor,
    json!({ "question": question.to_string() }),
  )
  .await;
  tell_check(db, vars, id, &standing, Heard::Ruling, &words).await;
}
