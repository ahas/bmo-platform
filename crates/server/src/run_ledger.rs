//! The run ledger (docs/api.md, 「runs」, docs/run-records.md, 「The run ledger」): one row of `runs` a run
//! of an agent's unit, opened as the job unit starts the unit and closed as it ends, or as a chat turn or a role
//! session's turn starts and ends, its times in epoch milliseconds. A run keeps the first closing it was given; the message its answer
//! went out as is attached apart from the closing. A run's runner is left in its agent's host under the
//! run's unit, and a runner the host did not start closes its run as an error.

use std::collections::BTreeMap;

use sea_orm::{ConnectionTrait, QueryResult};
use serde::{Deserialize, Serialize};
use store::{all, one, run};

use crate::Refusal;
use crate::host_run::{self, Command, Left};
use crate::places::Ground;

/// The outcome a run is closed with where its runner never started.
const LAUNCH_ERROR: &str = "error";

/// The search a run carried, as a caller names it: by its number or by its text.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum SearchId {
  Number(i64),
  Text(String),
}

impl SearchId {
  pub fn text(&self) -> String {
    match self {
      Self::Number(number) => number.to_string(),
      Self::Text(text) => text.clone(),
    }
  }
}

/// What a run is opened with.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Opening {
  pub agent: String,
  #[serde(default, alias = "chat_id")]
  pub chat_id: Option<i64>,
  /// The unit of work the run is: the name its log lines and its runner stand under.
  #[serde(rename = "unit")]
  pub work_unit: String,
  pub depth: u32,
  #[serde(default, alias = "cap_ms")]
  pub cap_ms: Option<i64>,
  #[serde(alias = "started_at")]
  pub started_at: i64,
  #[serde(default, alias = "started_by")]
  pub started_by: Option<String>,
  #[serde(default, alias = "search_id")]
  pub search_id: Option<SearchId>,
  /// The round of the room the run belongs to, where the turn named one.
  #[serde(default)]
  pub round: Option<String>,
  /// The message the run is about until its answer's is attached.
  #[serde(default, alias = "message_id")]
  pub message_id: Option<i64>,
  /// What the run was asked to do, where its opener names it.
  #[serde(default)]
  pub title: Option<String>,
  /// Whether the run is the run of a turn agency carries, which the pass over the runs left behind reads; a
  /// run opened across the boundary is never one.
  #[serde(skip)]
  pub carried: bool,
}

/// What a run is closed with: when, how it ended, the tokens its context carried and what its turn spent.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Closing {
  pub finished_at: i64,
  pub outcome: String,
  #[serde(default)]
  pub context_tokens: Option<i64>,
  #[serde(default)]
  pub spent: Option<Spent>,
}

/// What a run's turn spent, each figure summed over the models that answered it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Spent {
  pub input_tokens: i64,
  pub output_tokens: i64,
  pub cache_read_tokens: i64,
  pub cache_write_tokens: i64,
  /// What it cost, in millionths of a US dollar, summed over the models that answered; nought where none said.
  #[serde(default)]
  pub cost_micros: i64,
}

impl Spent {
  /// The four figures together.
  pub fn total(&self) -> i64 {
    self.input_tokens + self.output_tokens + self.cache_read_tokens + self.cache_write_tokens
  }
}

/// One run of the ledger, as a caller reads it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Run {
  pub id: i64,
  pub agent: String,
  pub chat_id: Option<i64>,
  /// The unit of work the run is, answered to a caller under `unit`.
  #[serde(rename = "unit")]
  pub work_unit: String,
  /// The account the run's agent is filed under, none for the platform's; kept for the reach of a list, never
  /// answered whole.
  #[serde(skip)]
  pub owner_account: Option<i64>,
  /// The organisation unit the run's agent is filed under, none where it is no unit's.
  #[serde(skip)]
  pub owner_unit: Option<i64>,
  pub title: Option<String>,
  pub depth: i64,
  pub started_at: i64,
  pub finished_at: Option<i64>,
  pub outcome: Option<String>,
  pub context_tokens: Option<i64>,
  pub spent: Option<Spent>,
  pub round: Option<String>,
  pub message_id: Option<i64>,
  /// How many tool calls its log lines held as it closed; none on an open run.
  pub tools: Option<i64>,
}

impl Run {
  fn of(row: &QueryResult) -> Result<Self, Refusal> {
    Ok(Self {
      id: row.try_get("", "id")?,
      agent: row.try_get("", "agent")?,
      chat_id: row.try_get("", "chat_id")?,
      work_unit: row.try_get("", "work_unit")?,
      owner_account: row.try_get("", "owner_account")?,
      owner_unit: row.try_get("", "owner_unit")?,
      title: row.try_get("", "title")?,
      depth: row.try_get("", "depth")?,
      started_at: row.try_get("", "started_at")?,
      finished_at: row.try_get("", "finished_at")?,
      outcome: row.try_get("", "outcome")?,
      context_tokens: row.try_get("", "context_tokens")?,
      spent: Self::spent_of(row)?,
      round: row.try_get("", "round")?,
      message_id: row.try_get("", "message_id")?,
      tools: row.try_get("", "tool_count")?,
    })
  }

  // What the run's turn spent, none where its closing named nothing.
  fn spent_of(row: &QueryResult) -> Result<Option<Spent>, Refusal> {
    let figure = |column: &str| row.try_get::<Option<i64>>("", column);
    let (Some(input_tokens), Some(output_tokens), Some(cache_read_tokens), Some(cache_write_tokens)) = (
      figure("input_tokens")?,
      figure("output_tokens")?,
      figure("cache_read_tokens")?,
      figure("cache_write_tokens")?,
    ) else {
      return Ok(None);
    };

    Ok(Some(Spent {
      input_tokens,
      output_tokens,
      cache_read_tokens,
      cache_write_tokens,
      cost_micros: figure("cost_micros").ok().flatten().unwrap_or_default(),
    }))
  }
}

/// How a closing came out short of closing the run.
#[derive(Debug)]
pub enum NotClosed {
  /// The ledger holds no run of that id.
  Unknown,
  /// The run was closed before, and keeps that closing.
  Closed,
  Store(Refusal),
}

impl From<Refusal> for NotClosed {
  fn from(refusal: Refusal) -> Self {
    Self::Store(refusal)
  }
}

/// Opens a run, answering its id.
pub async fn open<C: ConnectionTrait>(db: &C, opening: &Opening) -> Result<i64, Refusal> {
  // The run stands under its agent's owners: the account or unit the agent row is filed under, none for the
  // platform's, so a head's list and a stranger's are each read by the row's own ground (docs/api.md, 「runs」).
  let owners = one(
    db,
    r#"SELECT "owner_account", "owner_unit" FROM "supervisors" WHERE "name" = $1
       UNION ALL
       SELECT "owner_account", "owner_unit" FROM "role_agents" WHERE "name" = $1 LIMIT 1"#,
    vec![opening.agent.clone().into()],
  )
  .await?;
  let (owner_account, owner_unit) = owners.map_or((None, None), |row| {
    (
      row.try_get::<Option<i64>>("", "owner_account").unwrap_or(None),
      row.try_get::<Option<i64>>("", "owner_unit").unwrap_or(None),
    )
  });

  let row = one(
    db,
    r#"INSERT INTO "runs"
         ("agent", "chat_id", "work_unit", "owner_account", "owner_unit", "depth", "cap_ms", "started_at",
          "started_by", "search_id", "round",
          "message_id",
          "title", "carried")
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) RETURNING "id""#,
    vec![
      opening.agent.clone().into(),
      opening.chat_id.into(),
      opening.work_unit.clone().into(),
      owner_account.into(),
      owner_unit.into(),
      i64::from(opening.depth).into(),
      opening.cap_ms.into(),
      opening.started_at.into(),
      opening.started_by.clone().into(),
      opening.search_id.as_ref().map(SearchId::text).into(),
      opening.round.clone().into(),
      opening.message_id.into(),
      opening.title.clone().into(),
      opening.carried.into(),
    ],
  )
  .await?
  .ok_or_else(|| Refusal::failed("the run was not written"))?;

  Ok(row.try_get("", "id")?)
}

/// Closes a run that is still open.
pub async fn close<C: ConnectionTrait>(db: &C, id: i64, closing: &Closing) -> Result<(), NotClosed> {
  let closed = run(
    db,
    r#"UPDATE "runs" SET "finished_at" = $2, "outcome" = $3, "context_tokens" = $4,
         "input_tokens" = $5, "output_tokens" = $6, "cache_read_tokens" = $7, "cache_write_tokens" = $8,
         "cost_micros" = $9,
         "tool_count" = (SELECT COUNT(*) FROM "agent_log" l
                          WHERE l."agent" = "runs"."agent" AND l."run" = "runs"."work_unit" AND l."kind" = 'tool')
       WHERE "id" = $1 AND "finished_at" IS NULL"#,
    vec![
      id.into(),
      closing.finished_at.into(),
      closing.outcome.clone().into(),
      closing.context_tokens.into(),
      closing.spent.map(|spent| spent.input_tokens).into(),
      closing.spent.map(|spent| spent.output_tokens).into(),
      closing.spent.map(|spent| spent.cache_read_tokens).into(),
      closing.spent.map(|spent| spent.cache_write_tokens).into(),
      closing.spent.map(|spent| spent.cost_micros).filter(|cost| *cost > 0).into(),
    ],
  )
  .await
  .map_err(Refusal::from)?;

  if closed > 0 {
    return Ok(());
  }

  Err(match read(db, id).await? {
    Some(_) => NotClosed::Closed,
    None => NotClosed::Unknown,
  })
}

// What a run is read with.
const RUN_FIELDS: &str = r#""id", "agent", "chat_id", "work_unit", "owner_account", "owner_unit", "title", "depth",
  "started_at", "finished_at", "outcome",
  "context_tokens", "input_tokens", "output_tokens", "cache_read_tokens", "cache_write_tokens", "cost_micros", "round",
  "message_id", "tool_count""#;

/// A run by its id, or none where the ledger holds no row of it.
pub async fn read<C: ConnectionTrait>(db: &C, id: i64) -> Result<Option<Run>, Refusal> {
  one(db, &format!(r#"SELECT {RUN_FIELDS} FROM "runs" WHERE "id" = $1"#), vec![id.into()])
    .await?
    .as_ref()
    .map(Run::of)
    .transpose()
}

/// The latest run of an agent kept under a unit, the name its log lines are written under, or none.
pub async fn of_unit<C: ConnectionTrait>(db: &C, agent: &str, unit: &str) -> Result<Option<Run>, Refusal> {
  one(
    db,
    &format!(
      r#"SELECT {RUN_FIELDS} FROM "runs" WHERE "work_unit" = $1 AND ($2 = '' OR "agent" = $2)
         ORDER BY "id" DESC LIMIT 1"#
    ),
    vec![unit.into(), agent.into()],
  )
  .await?
  .as_ref()
  .map(Run::of)
  .transpose()
}

/// What a list of runs is narrowed by and where its page stands (docs/api.md, 「runs」): before or after a
/// run's id, open or closed, an agent, a room, a round, and names each matched against a run's unit and its
/// round; none narrows nothing, and every one named holds at once.
#[derive(Clone, Debug, Default)]
pub struct Search {
  pub after: Option<i64>,
  pub before: Option<i64>,
  pub limit: i64,
  /// True the runs still open, false the closed ones.
  pub live: Option<bool>,
  pub agent: Option<String>,
  pub room: Option<i64>,
  pub round: Option<String>,
  pub names: Vec<String>,
}

/// One page of runs, newest first, and whether more lie beyond it in the direction it was asked.
#[derive(Clone, Debug, Default)]
pub struct Page {
  pub runs: Vec<Run>,
  pub more: bool,
}

/// The page of runs a search names: the newest first, before its `before`; with `after`, the ones after it
/// nearest to it.
pub async fn search<C: ConnectionTrait>(db: &C, search: &Search) -> Result<Page, Refusal> {
  let names = serde_json::to_string(&search.names).unwrap_or_else(|_| "[]".to_owned());
  let limit = search.limit.max(1);
  // After a cursor the nearest runs are the oldest of the newer ones, so they are read upward.
  let upward = search.after.is_some();
  let rows = all(
    db,
    &format!(
      r#"SELECT {RUN_FIELDS} FROM "runs"
         WHERE ($1::bigint IS NULL OR "id" > $1)
           AND ($2::bigint IS NULL OR "id" < $2)
           AND ($3::boolean IS NULL OR ("finished_at" IS NULL) = $3)
           AND ($4::text IS NULL OR "agent" = $4)
           AND ($5::bigint IS NULL OR "chat_id" = $5)
           AND ($6::text IS NULL OR "round" = $6)
           AND ($7::jsonb = '[]'::jsonb
                OR "work_unit" IN (SELECT jsonb_array_elements_text($7::jsonb))
                OR "round" IN (SELECT jsonb_array_elements_text($7::jsonb)))
         ORDER BY "id" {} LIMIT $8"#,
      if upward { "ASC" } else { "DESC" }
    ),
    vec![
      search.after.into(),
      search.before.into(),
      search.live.into(),
      search.agent.clone().into(),
      search.room.into(),
      search.round.clone().into(),
      names.into(),
      (limit + 1).into(),
    ],
  )
  .await?;
  let mut runs = rows.iter().map(Run::of).collect::<Result<Vec<_>, _>>()?;
  let more = runs.len() as i64 > limit;

  runs.truncate(limit as usize);

  if upward {
    runs.reverse();
  }

  Ok(Page { runs, more })
}

/// What the chat turns' runs started in a span spent, by the round each names: the round, how many turns, what
/// they spent and the first and last start in epoch seconds. A run that names no round, or reported no
/// figures, is no part of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundSpend {
  pub round: String,
  pub turns: i64,
  pub spent: Spent,
  pub first_at: i64,
  pub last_at: i64,
}

/// The supply source a run went out on written on it (docs/run-records.md, 「The run ledger」).
pub async fn mark_supply<C: ConnectionTrait>(db: &C, id: i64, source: &str) -> Result<(), Refusal> {
  run(
    db,
    r#"UPDATE "runs" SET "supply_source" = $2 WHERE "id" = $1"#,
    vec![id.into(), source.into()],
  )
  .await?;

  Ok(())
}

/// The spend of the rounds whose runs started between two moments in epoch seconds, the costliest first.
pub async fn spent_by_round<C: ConnectionTrait>(
  db: &C,
  after: i64,
  before: i64,
  keep: Option<&str>,
) -> Result<Vec<RoundSpend>, Refusal> {
  let kept = match keep {
    Some(predicate) => format!(r#" AND {predicate}"#),
    None => String::new(),
  };

  all(
    db,
    &format!(
      r#"SELECT "round", COUNT(*) AS "turns",
              SUM("input_tokens")::bigint AS "input_tokens", SUM("output_tokens")::bigint AS "output_tokens",
              SUM("cache_read_tokens")::bigint AS "cache_read_tokens",
              SUM("cache_write_tokens")::bigint AS "cache_write_tokens", SUM("cost_micros")::bigint AS "cost_micros",
              MIN("started_at") AS "first_at", MAX("started_at") AS "last_at"
       FROM "runs"
       WHERE "round" IS NOT NULL AND "input_tokens" IS NOT NULL AND "started_at" >= $1 AND "started_at" <= $2{kept}
       GROUP BY "round"
       ORDER BY SUM("input_tokens" + "output_tokens" + "cache_read_tokens" + "cache_write_tokens") DESC, "round""#
    ),
    vec![after.into(), before.into()],
  )
  .await?
  .iter()
  .map(|row| {
    Ok(RoundSpend {
      round: row.try_get("", "round")?,
      turns: row.try_get("", "turns")?,
      spent: Spent {
        input_tokens: row.try_get("", "input_tokens")?,
        output_tokens: row.try_get("", "output_tokens")?,
        cache_read_tokens: row.try_get("", "cache_read_tokens")?,
        cache_write_tokens: row.try_get("", "cache_write_tokens")?,
        cost_micros: row.try_get::<Option<i64>>("", "cost_micros")?.unwrap_or_default(),
      },
      first_at: row.try_get("", "first_at")?,
      last_at: row.try_get("", "last_at")?,
    })
  })
  .collect()
}

/// The message a run's answer went out as, attached on an open or a closed run alike; whether the ledger
/// holds the run.
pub async fn attach_message<C: ConnectionTrait>(db: &C, id: i64, message: i64) -> Result<bool, Refusal> {
  Ok(
    run(
      db,
      r#"UPDATE "runs" SET "message_id" = $2 WHERE "id" = $1"#,
      vec![id.into(), message.into()],
    )
    .await?
      > 0,
  )
}

/// The ids of the runs that carried a search, in id order; the search is matched as the text it is kept as.
pub async fn of_search<C: ConnectionTrait>(db: &C, search: &SearchId) -> Result<Vec<i64>, Refusal> {
  all(
    db,
    r#"SELECT "id" FROM "runs" WHERE "search_id" = $1 ORDER BY "id""#,
    vec![search.text().into()],
  )
  .await?
  .iter()
  .map(|row| Ok(row.try_get("", "id")?))
  .collect()
}

/// The program a run's runner is: its words, the folder it starts in and the variables it is handed.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Program {
  pub program: String,
  pub args: Vec<String>,
  pub cwd: String,
  pub environment: BTreeMap<String, String>,
}

impl From<Program> for Command {
  fn from(program: Program) -> Self {
    Self {
      args: std::iter::once(program.program).chain(program.args).collect(),
      cwd: program.cwd,
      env: program.environment,
      stdin: None,
    }
  }
}

/// What a run's runner is launched with: what a person watching reads it as, the agent whose host takes it,
/// and its program.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Launch {
  #[serde(default, alias = "runId")]
  pub run_id: Option<i64>,
  pub title: String,
  pub agent: String,
  pub command: Program,
}

/// How a launch came out short of starting the runner.
#[derive(Debug)]
pub enum NotLaunched {
  /// The ledger holds no run of that id.
  Unknown,
  /// The run is closed, and nothing was started for it.
  Closed,
  /// The host did not start the runner, for the reason its code names; the run is closed as an error.
  Refused(String),
  Store(Refusal),
}

impl From<Refusal> for NotLaunched {
  fn from(refusal: Refusal) -> Self {
    Self::Store(refusal)
  }
}

/// Leaves a run's runner in its agent's host, rooted on the run's unit, so the unit reads as alive while it
/// runs and a stop of it reaches the runner. The runner closes its own run; one the host did not start has
/// its run closed here as an error, finished now.
pub async fn launch(ground: &Ground, id: i64, launch: Launch) -> Result<(), NotLaunched> {
  let run = read(&ground.db, id).await?.ok_or(NotLaunched::Unknown)?;

  if run.finished_at.is_some() {
    return Err(NotLaunched::Closed);
  }

  let left = Left {
    agent: &launch.agent,
    for_agent: None,
    name: &run.work_unit,
    title: &launch.title,
    command: launch.command.into(),
  };
  let Err(code) = host_run::leave(ground, left).await else {
    return Ok(());
  };
  let closing = Closing {
    finished_at: crate::now_ms(),
    outcome: LAUNCH_ERROR.to_owned(),
    context_tokens: None,
    spent: None,
  };

  if let Err(NotClosed::Store(why)) = close(&ground.db, id, &closing).await {
    eprintln!("{}: the run's row could not be closed: {}", run.work_unit, why.detail);
  }

  Err(NotLaunched::Refused(code))
}
