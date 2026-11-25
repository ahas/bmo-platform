#![allow(
  dead_code,
  unused_imports,
  unused_variables,
  clippy::needless_lifetimes,
  clippy::let_and_return,
  clippy::unnecessary_to_owned
)]
//! The workers' routes across the boundary (docs/api.md, 「Workers」): a job's stop mark, a road's block and
//! whether an item has a road open now, runs asked to stop, which names are alive, and the deployment's
//! rest. Agency keeps each mark and each block and judges nothing of what a consumer's job is.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use bmo_agency_client::{Usage, UsageGroup};
use serde_json::{Value, json};

use super::claude_accounts::Judging;
use super::{Call, Refused, Route, answer, whole};
use crate::now_ms;
use crate::places::Ground;
use crate::queue::paths::resolve;
use crate::queue::routes::{
  block_id, block_route, clear_block_by_id, kept_blocks, route_blocked, walked_keys,
};
use crate::queue::stops::{StoppedRun, clear_job_stop, job_stop_reason, mark_job_stopped};
use crate::queue::workers::Worker;
use crate::rest::{
  RestKind, clear_passed_rests, end_rest, rest_kind, rest_until, seconds_of, standing_rest, tell_silence,
  write_rest,
};

fn bad(error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, error)
}

fn ok() -> Response {
  answer(StatusCode::OK, json!({ "ok": true }))
}

fn job_of(value: Option<&Value>) -> Result<i64, Refused> {
  value.and_then(whole).ok_or_else(|| bad("job (a whole number) is required"))
}

fn text<'a>(body: &'a Value, field: &str) -> Result<&'a str, Refused> {
  body
    .get(field)
    .and_then(Value::as_str)
    .filter(|value| !value.is_empty())
    .ok_or_else(|| bad(format!("{field} (string) is required")))
}

fn names(body: &Value, field: &str) -> Vec<String> {
  body
    .get(field)
    .and_then(Value::as_array)
    .map(|names| names.iter().filter_map(Value::as_str).map(str::to_owned).collect())
    .unwrap_or_default()
}

/// A job marked stopped with its reason, before its unit is asked to stop.
pub struct JobStopMark;

impl Route for JobStopMark {
  const METHOD: Method = Method::PUT;
  const PATH: &'static str = "/stop-marks/{jobId}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let job: i64 = call
      .param("jobId")
      .and_then(|job| job.parse().ok())
      .ok_or_else(|| bad("jobId is a number"))?;
    let reason = text(&body, "reason")?;

    mark_job_stopped(call.store(), job, reason).await?;

    Ok(answer(StatusCode::CREATED, json!({ "jobId": job, "reason": reason, "at": now_ms() })))
  }
}

crate::moved!(JobStopMark);

/// A job's stop mark, where one stands.
pub struct JobStopRead;

impl Route for JobStopRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/stop-marks/{jobId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let job: i64 = call
      .param("jobId")
      .and_then(|job| job.parse().ok())
      .ok_or_else(|| bad("jobId is a number"))?;
    let reason = job_stop_reason(call.store(), job).await?;

    match reason {
      Some(reason) => Ok(answer(StatusCode::OK, json!({ "jobId": job, "reason": reason, "at": now_ms() }))),
      None => Err(Refused::new(StatusCode::NOT_FOUND, format!("job {job} carries no stop mark"))),
    }
  }
}

crate::moved!(JobStopRead);

/// A job's stop mark taken away with its record.
pub struct JobStopClear;

impl Route for JobStopClear {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/stop-marks/{jobId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let job: i64 = call
      .param("jobId")
      .and_then(|job| job.parse().ok())
      .ok_or_else(|| bad("jobId is a number"))?;

    clear_job_stop(call.store(), job).await?;

    Ok(StatusCode::NO_CONTENT.into_response())
  }
}

crate::moved!(JobStopClear);

/// A selection's road blocked until a moment, in epoch milliseconds.
pub struct RoadBlock;

impl Route for RoadBlock {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/road-blocks";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let selection = text(&body, "selection")?;
    let until = body
      .get("until")
      .and_then(whole)
      .ok_or_else(|| bad("until (a whole number of epoch milliseconds) is required"))?;
    let reason = body.get("reason").and_then(Value::as_str).unwrap_or_default();

    if !block_route(call.store(), selection, until, reason, None).await? {
      return Err(bad(format!("{selection} names no provider's model this build carries")));
    }

    let id = block_id(call.store(), selection)
      .await?
      .ok_or_else(|| Refused::failed("the block was written but not read"))?;

    Ok(answer(
      StatusCode::CREATED,
      json!({ "id": id, "selection": selection, "until": until, "reason": reason }),
    ))
  }
}

crate::moved!(RoadBlock);

/// Every road blocked now, by the model or the worker that would walk it; none means open.
pub struct RoadOpen;

impl Route for RoadOpen {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/road-blocks";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let now = now_ms();
    let (model, worker) = (
      call.query("model").filter(|model| !model.trim().is_empty()),
      call
        .query("workerName")
        .or_else(|| call.query("worker"))
        .filter(|worker| !worker.trim().is_empty()),
    );
    let asked = model.is_some() || worker.is_some();
    let walked = if asked {
      Some(walked_keys(call.store(), call.machine(), model.as_deref(), worker.as_deref()).await?)
    } else {
      None
    };
    let items: Vec<Value> = kept_blocks(call.store())
      .await?
      .into_iter()
      .filter(|block| block.until > now)
      .filter(|block| walked.as_ref().is_none_or(|walked| walked.contains(&block.key)))
      .map(|block| {
        json!({
          "id": block.id,
          "selection": block.selection,
          "until": block.until,
          "reason": block.reason,
        })
      })
      .collect();

    Ok(answer(StatusCode::OK, json!({ "items": items, "more": false })))
  }
}

crate::moved!(RoadOpen);

/// A road opened again before its block ran out.
pub struct RoadUnblock;

impl Route for RoadUnblock {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/road-blocks/{blockId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let id = call
      .param("blockId")
      .and_then(|id| id.parse::<i64>().ok())
      .ok_or_else(|| bad("blockId (a whole number) is required"))?;

    match clear_block_by_id(call.store(), id).await? {
      Some(_) => Ok(StatusCode::NO_CONTENT.into_response()),
      None => Err(Refused::new(StatusCode::NOT_FOUND, format!("no road block has the id {id}"))),
    }
  }
}

crate::moved!(RoadUnblock);

/// Runs asked to stop by their names: one at a time as `run`, or several at once as `runs`, each a unit and
/// the pool request it held, all confirmed gone first.
pub struct RunsStop;

impl Route for RunsStop {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/runs/stop";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let stops = call.stops();

    if let Some(run) = body.get("run").and_then(Value::as_str) {
      stops.stop(run).await;

      return Ok(ok());
    }

    let runs: Vec<StoppedRun> = body
      .get("runs")
      .and_then(Value::as_array)
      .ok_or_else(|| bad("run (string) or runs (array) is required"))?
      .iter()
      .map(|run| StoppedRun {
        unit: run.get("unit").and_then(Value::as_str).map(str::to_owned),
        pool_request: run.get("poolRequest").and_then(Value::as_str).map(str::to_owned),
      })
      .collect();
    let why = body.get("reason").and_then(Value::as_str).unwrap_or("stopped");

    stops
      .stop_runs(&runs, why)
      .await
      .map_err(|why| Refused::new(StatusCode::CONFLICT, why))?;

    Ok(ok())
  }
}

/// Which of the names asked are running, or that nobody could say.
pub struct RunsAlive;

impl Route for RunsAlive {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/runs/alive";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let asked = names(&body, "names");
    let alive = call.running().answered(&asked).await?;

    Ok(answer(
      StatusCode::OK,
      match alive {
        Some(alive) => {
          let mut alive: Vec<String> = alive.into_iter().collect();

          alive.sort();
          json!({ "answered": true, "alive": alive })
        }
        None => json!({ "answered": false, "alive": null }),
      },
    ))
  }
}

/// Whether the deployment rests now, until when and why.
pub struct RestRead;

impl Route for RestRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/rest";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let now = now_ms() / 1000;
    let until = rest_until(Judging::of(call.ground()), now).await?;
    let kind = match until {
      Some(_) => Some(rest_kind(call.store(), now).await?),
      None => None,
    };

    Ok(answer(
      StatusCode::OK,
      json!({ "resting": until.is_some(), "until": until, "kind": kind }),
    ))
  }
}

crate::moved!(RestRead);

/// A rest written down: an account's limit or the service's outage, until a moment in epoch seconds.
pub struct RestWrite;

impl Route for RestWrite {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/rest";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;

    if body.get("clearPassed").and_then(Value::as_bool) == Some(true) {
      let kind = clear_passed_rests(Judging::of(call.ground()), now_ms() / 1000).await?;

      return Ok(answer(
        StatusCode::OK,
        json!({ "resting": false, "until": Value::Null, "kind": kind.map(RestKind::word), "accountId": Value::Null }),
      ));
    }

    let until = body
      .get("until")
      .and_then(Value::as_f64)
      .map(seconds_of)
      .ok_or_else(|| bad("until (seconds) is required"))?;
    let kind = match body.get("kind").and_then(Value::as_str) {
      Some("limit") => RestKind::Limit,
      Some("outage") => RestKind::Outage,
      _ => return Err(bad("kind is limit or outage")),
    };
    let account = body.get("account").and_then(Value::as_str).unwrap_or_default();
    let judging = Judging::of(call.ground());
    let now = now_ms() / 1000;
    let standing = standing_rest(judging, kind, account, now).await?;
    let silenced = write_rest(judging, until, kind, account, now).await?;
    let kept = standing_rest(judging, kind, account, now).await?;

    if silenced {
      tell_silence(
        call.ground(),
        kind,
        Some(account).filter(|named| !named.is_empty()),
        kept.unwrap_or(until as i64),
      )
      .await?;
    }

    Ok(answer(
      StatusCode::OK,
      json!({
        "resting": rest_until(judging, now).await?.is_some(),
        "until": kept,
        "kind": kind.word(),
        "accountId": (kind != RestKind::Outage).then_some(account),
        "written": standing.is_none(),
        "silenced": silenced,
      }),
    ))
  }
}

crate::moved!(RestWrite);

/// The administrator's end of the rest: every written rest of the platform's own logins, the outage and the
/// models' rests cleared and the full windows set aside, answered with the rest as it stands after.
pub struct RestEnd;

impl Route for RestEnd {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/rest";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let judging = Judging::of(call.ground());
    let now = now_ms() / 1000;
    let ended = end_rest(judging, now).await?;
    let until = rest_until(judging, now).await?;
    let kind = match until {
      Some(_) => Some(rest_kind(call.store(), now).await?),
      None => None,
    };

    Ok(answer(
      StatusCode::OK,
      json!({ "resting": until.is_some(), "until": until, "kind": kind, "ended": ended }),
    ))
  }
}

crate::moved!(RestEnd);

/// Usage by model, checkout, repository, holder, room or round over a span, with session contexts.
pub struct UsageRoute;

/// What a usage query groups the jobs by (docs/api.md, 「accounts, rest and usage」).
enum Grouping {
  /// The model the job's turn went out on, matched to the roles' sessions on that model.
  Model,
  /// A column of the job no session records, so its groups carry no context.
  Column(&'static str),
  /// The checkout the job ran in, matched to the sessions opened in the directory it stands at.
  Checkout,
  /// The repository the job's checkout belongs to, matched to the sessions opened in its checkouts'
  /// directories.
  Repository,
}

impl Grouping {
  fn named(word: &str) -> Option<Self> {
    match word {
      "model" => Some(Self::Model),
      "agent" => Some(Self::Checkout),
      "user" => Some(Self::Column("holder_name")),
      "room" => Some(Self::Column("room_id")),
      "repository" => Some(Self::Repository),
      _ => None,
    }
  }

  /// The groups the jobs started in the span make, the most tokens first.
  async fn groups(&self, ground: &Ground, after: i64, before: i64) -> Result<Vec<UsageGroup>, Refused> {
    let db = &ground.db;
    let (jobs_by, contexts) = match self {
      Self::Model => ("model", session_contexts(db, "model", after, before).await?),
      Self::Column(column) => (*column, HashMap::new()),
      Self::Checkout => (
        "worker_name",
        checkout_contexts(ground, after, before, |checkout| checkout.name.clone()).await?,
      ),
      Self::Repository => (
        "worker_name",
        checkout_contexts(ground, after, before, |checkout| {
          checkout.support_of.clone().unwrap_or_else(|| checkout.name.clone())
        })
        .await?,
      ),
    };
    let mut spends = JobSpend::by_column(db, jobs_by, after, before).await?;

    if let Self::Repository = self {
      spends = by_repository(db, spends).await?;
    }

    spends.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.key.cmp(&b.key)));

    Ok(spends.into_iter().map(|spend| spend.group(&contexts)).collect())
  }
}

/// The tokens the roles' sessions answered in the span carry, by the value of one of their columns.
async fn session_contexts(
  db: &sea_orm::DatabaseConnection,
  column: &str,
  after: i64,
  before: i64,
) -> Result<HashMap<String, i64>, Refused> {
  let rows = store::all(
    db,
    &format!(
      r#"SELECT COALESCE(CAST("{column}" AS text), '') AS "key", COALESCE(SUM("context_tokens"), 0)::bigint AS "tokens"
         FROM "role_sessions" WHERE "answered_at" >= $1 AND "answered_at" <= $2
         GROUP BY 1"#
    ),
    vec![after.into(), before.into()],
  )
  .await?;

  rows
    .iter()
    .map(|row| Ok((row.try_get("", "key")?, row.try_get("", "tokens")?)))
    .collect::<Result<_, sea_orm::DbErr>>()
    .map_err(Refused::from)
}

/// The tokens the sessions answered in the span carry, summed by the key each standing checkout is gathered
/// under (docs/worker-slots.md) over the distinct directories its checkouts stand at, since a session records
/// its directory and not the machine; a key no session's directory meets is absent.
async fn checkout_contexts(
  ground: &Ground,
  after: i64,
  before: i64,
  key_of: impl Fn(&Worker) -> String,
) -> Result<HashMap<String, i64>, Refused> {
  let mut by_directory: HashMap<PathBuf, i64> = HashMap::new();

  for (path, tokens) in session_contexts(&ground.db, "path", after, before).await? {
    *by_directory.entry(resolve(&path)).or_default() += tokens;
  }

  let mut directories: HashMap<String, HashSet<PathBuf>> = HashMap::new();

  for checkout in crate::queue::workers::workers(&ground.db, &ground.source).await? {
    directories.entry(key_of(&checkout)).or_default().insert(resolve(&checkout.cwd));
  }

  Ok(
    directories
      .into_iter()
      .filter_map(|(key, gathered)| {
        let met: Vec<i64> =
          gathered.iter().filter_map(|directory| by_directory.get(directory)).copied().collect();

        (!met.is_empty()).then(|| (key, met.iter().sum()))
      })
      .collect(),
  )
}

/// The checkouts' spends folded into their repositories' (docs/worker-slots.md), a checkout no repository
/// holds standing as a group of its own.
async fn by_repository(
  db: &sea_orm::DatabaseConnection,
  checkouts: Vec<JobSpend>,
) -> Result<Vec<JobSpend>, Refused> {
  let held: HashSet<String> = store::all(db, r#"SELECT "name" FROM "repositories""#, vec![])
    .await?
    .iter()
    .map(|row| row.try_get("", "name"))
    .collect::<Result<_, _>>()?;
  let mut repositories: HashMap<String, JobSpend> = HashMap::new();

  for checkout in checkouts {
    let repository = crate::queue::workers::repository_named_by(&checkout.key, |name| held.contains(name))
      .unwrap_or(&checkout.key)
      .to_owned();
    let spend = match repositories.remove(&repository) {
      Some(gathered) => gathered.joined(checkout),
      None => JobSpend {
        key: repository.clone(),
        ..checkout
      },
    };

    repositories.insert(repository, spend);
  }

  Ok(repositories.into_values().collect())
}

/// What the jobs of one group spent: the total, the four kinds summed over the jobs that reported all four
/// (none where no job did), and the jobs that spent tokens without reporting them with what they spent.
struct JobSpend {
  key: String,
  tokens: i64,
  jobs: i64,
  first_at: i64,
  last_at: i64,
  input_tokens: Option<i64>,
  output_tokens: Option<i64>,
  cache_read_tokens: Option<i64>,
  cache_write_tokens: Option<i64>,
  unsplit_jobs: i64,
  unsplit_tokens: i64,
  cost_micros: Option<i64>,
}

// Two groups' sums of one kind as one: none only where neither group has a job that reported it.
fn kinds_added(one: Option<i64>, other: Option<i64>) -> Option<i64> {
  match (one, other) {
    (None, None) => None,
    _ => Some(one.unwrap_or(0) + other.unwrap_or(0)),
  }
}

impl JobSpend {
  /// The jobs started in the span, a spend to each value of one of their columns.
  async fn by_column(
    db: &sea_orm::DatabaseConnection,
    column: &str,
    after: i64,
    before: i64,
  ) -> Result<Vec<Self>, Refused> {
    let rows = store::all(
      db,
      &format!(
        r#"SELECT "key", SUM("token_count")::bigint AS "tokens", COUNT(*) AS "jobs",
                  MIN("started_at") AS "first", MAX(COALESCE("ended_at", "started_at")) AS "last",
                  (SUM("input_tokens") FILTER (WHERE "split"))::bigint AS "input",
                  (SUM("output_tokens") FILTER (WHERE "split"))::bigint AS "output",
                  (SUM("cache_read_tokens") FILTER (WHERE "split"))::bigint AS "cache_read",
                  (SUM("cache_write_tokens") FILTER (WHERE "split"))::bigint AS "cache_write",
                  COUNT(*) FILTER (WHERE NOT "split" AND "token_count" > 0) AS "unsplit_jobs",
                  COALESCE(SUM("token_count") FILTER (WHERE NOT "split"), 0)::bigint AS "unsplit_tokens",
                  SUM("cost_micros")::bigint AS "cost"
           FROM (
             SELECT COALESCE(CAST("{column}" AS text), '') AS "key", "token_count", "started_at", "ended_at",
                    "input_tokens", "output_tokens", "cache_read_tokens", "cache_write_tokens", "cost_micros",
                    ("input_tokens" IS NOT NULL AND "output_tokens" IS NOT NULL
                      AND "cache_read_tokens" IS NOT NULL AND "cache_write_tokens" IS NOT NULL) AS "split"
             FROM "jobs" WHERE "started_at" >= $1 AND "started_at" <= $2
           ) AS "spent"
           GROUP BY "key""#
      ),
      vec![after.into(), before.into()],
    )
    .await?;

    rows.iter().map(Self::read).collect::<Result<_, _>>().map_err(Refused::from)
  }

  fn read(row: &sea_orm::QueryResult) -> Result<Self, sea_orm::DbErr> {
    let figure = |column: &str| row.try_get::<i64>("", column);
    let kind = |column: &str| row.try_get::<Option<i64>>("", column);

    Ok(Self {
      key: row.try_get("", "key")?,
      tokens: figure("tokens")?,
      jobs: figure("jobs")?,
      first_at: figure("first")?,
      last_at: figure("last")?,
      input_tokens: kind("input")?,
      output_tokens: kind("output")?,
      cache_read_tokens: kind("cache_read")?,
      cache_write_tokens: kind("cache_write")?,
      unsplit_jobs: figure("unsplit_jobs")?,
      unsplit_tokens: figure("unsplit_tokens")?,
      cost_micros: kind("cost")?,
    })
  }

  /// Two groups' spends as one, under this one's key.
  fn joined(self, other: Self) -> Self {
    Self {
      key: self.key,
      tokens: self.tokens + other.tokens,
      jobs: self.jobs + other.jobs,
      first_at: self.first_at.min(other.first_at),
      last_at: self.last_at.max(other.last_at),
      input_tokens: kinds_added(self.input_tokens, other.input_tokens),
      output_tokens: kinds_added(self.output_tokens, other.output_tokens),
      cache_read_tokens: kinds_added(self.cache_read_tokens, other.cache_read_tokens),
      cache_write_tokens: kinds_added(self.cache_write_tokens, other.cache_write_tokens),
      unsplit_jobs: self.unsplit_jobs + other.unsplit_jobs,
      unsplit_tokens: self.unsplit_tokens + other.unsplit_tokens,
      cost_micros: kinds_added(self.cost_micros, other.cost_micros),
    }
  }

  /// The group as the route answers it, with the context the sessions matched to its key carry.
  fn group(self, contexts: &HashMap<String, i64>) -> UsageGroup {
    UsageGroup {
      context_tokens: contexts.get(&self.key).copied(),
      key: self.key,
      tokens: self.tokens,
      jobs: self.jobs,
      turns: None,
      first_at: self.first_at,
      last_at: self.last_at,
      input_tokens: self.input_tokens,
      output_tokens: self.output_tokens,
      cache_read_tokens: self.cache_read_tokens,
      cache_write_tokens: self.cache_write_tokens,
      unsplit_jobs: self.unsplit_jobs,
      unsplit_tokens: self.unsplit_tokens,
      cost_usd: self.cost_micros.map(crate::supply::reservations::usd),
    }
  }
}

impl Route for UsageRoute {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/usage";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let now = now_ms();
    let after = call
      .query("after")
      .and_then(|after| after.parse::<i64>().ok())
      .unwrap_or(now - 86_400_000);
    let before = call.query("before").and_then(|before| before.parse::<i64>().ok()).unwrap_or(now);
    let word = call.query("groupBy").unwrap_or_else(|| "model".to_owned());
    let groups = if word == "round" {
      by_round(call.store(), after, before).await?
    } else if let Some(key) = RunKey::named(&word) {
      by_runs(call.store(), key, after, before).await?
    } else {
      Grouping::named(&word)
        .ok_or_else(|| {
          bad(format!(
            "groupBy is model, agent, repository, user, room, round, account or supply, not {word}"
          ))
        })?
        .groups(call.ground(), after, before)
        .await?
    };

    Ok(answer(
      StatusCode::OK,
      json!(Usage {
        groups,
        after,
        before
      }),
    ))
  }
}

crate::moved!(UsageRoute);

// What the chat turns' runs started in a span spent, a group to a round (docs/run-records.md, 「The run
// ledger」), the span in epoch milliseconds as the route's.
async fn by_round(
  db: &sea_orm::DatabaseConnection,
  after: i64,
  before: i64,
) -> Result<Vec<UsageGroup>, Refused> {
  let rounds = crate::run_ledger::spent_by_round(db, after, before).await?;

  Ok(
    rounds
      .iter()
      .map(|round| UsageGroup {
        key: round.round.clone(),
        tokens: round.spent.total(),
        jobs: 0,
        turns: Some(round.turns),
        first_at: round.first_at,
        last_at: round.last_at,
        input_tokens: Some(round.spent.input_tokens),
        output_tokens: Some(round.spent.output_tokens),
        cache_read_tokens: Some(round.spent.cache_read_tokens),
        cache_write_tokens: Some(round.spent.cache_write_tokens),
        unsplit_jobs: 0,
        unsplit_tokens: 0,
        context_tokens: None,
        cost_usd: (round.spent.cost_micros > 0)
          .then(|| crate::supply::reservations::usd(round.spent.cost_micros)),
      })
      .collect(),
  )
}

/// What the ledger's runs are grouped by over a span (docs/supply.md, 「The ledger」): the person a run was for,
/// `none` for the deployment's own, or the supply source it went out on.
#[derive(Clone, Copy)]
enum RunKey {
  Account,
  Supply,
}

impl RunKey {
  fn named(word: &str) -> Option<Self> {
    match word {
      "account" => Some(Self::Account),
      "supply" => Some(Self::Supply),
      _ => None,
    }
  }

  // The expression a run's key is, over the ledger joined to the supervisors and the role agents it ran for.
  fn expression(self) -> &'static str {
    match self {
      Self::Account => {
        r#"COALESCE(CAST("s"."owner_account" AS text), NULLIF("a"."owner_account", ''),
             CAST("h"."owner_account" AS text), 'none')"#
      }
      Self::Supply => r#"COALESCE("r"."supply_source", 'platform')"#,
    }
  }
}

// What the runs started in a span spent and cost, a group to each value of the key.
async fn by_runs(
  db: &sea_orm::DatabaseConnection,
  key: RunKey,
  after: i64,
  before: i64,
) -> Result<Vec<UsageGroup>, Refused> {
  let rows = store::all(
    db,
    &format!(
      r#"SELECT {key} AS "key", COUNT(*) AS "turns",
                COALESCE(SUM(COALESCE("r"."input_tokens", 0) + COALESCE("r"."output_tokens", 0)
                  + COALESCE("r"."cache_read_tokens", 0) + COALESCE("r"."cache_write_tokens", 0)), 0)::bigint
                  AS "tokens",
                MIN("r"."started_at") AS "first", MAX(COALESCE("r"."finished_at", "r"."started_at")) AS "last",
                SUM("r"."input_tokens")::bigint AS "input", SUM("r"."output_tokens")::bigint AS "output",
                SUM("r"."cache_read_tokens")::bigint AS "cache_read",
                SUM("r"."cache_write_tokens")::bigint AS "cache_write",
                SUM("r"."cost_micros")::bigint AS "cost"
         FROM "runs" AS "r"
         LEFT JOIN "supervisors" AS "s" ON "s"."name" = "r"."agent"
         LEFT JOIN "role_agents" AS "a" ON "a"."name" = "r"."agent"
         LEFT JOIN "jobs" AS "j" ON "j"."run_id" = "r"."id"
         LEFT JOIN "supervisors" AS "h" ON "h"."name" = "j"."holder_name"
         WHERE "r"."started_at" >= $1 AND "r"."started_at" <= $2
         GROUP BY 1 ORDER BY "tokens" DESC, 1"#,
      key = key.expression()
    ),
    vec![after.into(), before.into()],
  )
  .await?;

  rows
    .iter()
    .map(|row| {
      let figure = |column: &str| row.try_get::<i64>("", column);
      let kind = |column: &str| row.try_get::<Option<i64>>("", column);

      Ok(UsageGroup {
        key: row.try_get("", "key")?,
        tokens: figure("tokens")?,
        jobs: 0,
        turns: Some(figure("turns")?),
        first_at: figure("first")?,
        last_at: figure("last")?,
        input_tokens: kind("input")?,
        output_tokens: kind("output")?,
        cache_read_tokens: kind("cache_read")?,
        cache_write_tokens: kind("cache_write")?,
        unsplit_jobs: 0,
        unsplit_tokens: 0,
        context_tokens: None,
        cost_usd: kind("cost")?.map(crate::supply::reservations::usd),
      })
    })
    .collect::<Result<_, sea_orm::DbErr>>()
    .map_err(Refused::from)
}
