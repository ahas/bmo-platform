//! The run log held to its bounds (docs/run-records.md, 「Agent log」): an agent's newest lines, the text
//! they may hold together, and the age past which no line stays. The writer prunes its own agent's lines as
//! it goes; agency's own clock sweeps the whole table as the server starts and once a day after, which is
//! what reaches an agent whose writer is long gone.
//!
//! The sweep reads one agent's lines newest first only as far as its bounds reach, then deletes the lines
//! past them in short statements, each transaction bounded by a statement timeout: a pass never holds one
//! long transaction over the whole table.

use std::collections::HashSet;
use std::time::Duration;

use sea_orm::{DatabaseConnection, TransactionTrait};
use store::{all, one, run};

use crate::clock::every;
use crate::{Refusal, now_ms};

/// The run log's bounds: the newest lines an agent keeps, the text they may hold together, and the age
/// past which no line stays.
pub const LOG_LINES_PER_AGENT: i64 = 4000;
pub const LOG_BYTES_PER_AGENT: i64 = 64 * 1024 * 1024;
pub const LOG_MOST_AGE_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// How often the whole table is swept.
pub const PRUNE_PASS: Duration = Duration::from_secs(24 * 60 * 60);

// The lines one delete statement takes, and how long it may run before the transaction is ended.
const BATCH: i64 = 500;
const PAGE: i64 = 1000;
const STATEMENT_TIMEOUT: &str = "30s";

/// One agent's own run log held to the bounds, as its writer prunes it every so many lines: its lines past
/// its newest or past its share of text, and every line anywhere past the age but that agent's newest
/// system prompt. How many lines went.
pub async fn agent_run_log(db: &DatabaseConnection, agent: &str, now: i64) -> Result<u64, Refusal> {
  prune_agent(db, agent, now).await
}

/// The run log held to its bounds over every agent at `now`, in epoch milliseconds. How many lines went.
pub async fn run_log(db: &DatabaseConnection, now: i64) -> Result<u64, Refusal> {
  let agents = all(db, r#"SELECT DISTINCT "agent" FROM "agent_log" ORDER BY "agent""#, vec![]).await?;
  let mut gone = 0;

  for row in agents {
    let agent: String = row.try_get("", "agent")?;
    gone += prune_agent(db, &agent, now).await?;
  }

  Ok(gone)
}

// One agent's lines read newest first until its count or text bound is passed, then the lines past that
// cutoff and the lines past the age deleted in batches. An open run and the newest system prompt are never
// taken by the count or text bound; the age takes open lines too, but keeps the prompt.
async fn prune_agent(db: &DatabaseConnection, agent: &str, now: i64) -> Result<u64, Refusal> {
  let kept = one(
    db,
    r#"SELECT MAX("id") AS "id" FROM "agent_log"
       WHERE "agent" = $1 AND "job_id" IS NULL AND "kind" = 'prompt' AND "name" = 'system'"#,
    vec![agent.into()],
  )
  .await?
  .and_then(|row| row.try_get::<Option<i64>>("", "id").ok().flatten())
  .unwrap_or(0);
  let open_rows = all(
    db,
    r#"SELECT "run" FROM "agent_log" WHERE "agent" = $1 AND "job_id" IS NULL
       GROUP BY "run" HAVING SUM(CASE WHEN "kind" = 'end' THEN 1 ELSE 0 END) = 0"#,
    vec![agent.into()],
  )
  .await?;
  let open: HashSet<String> =
    open_rows.iter().filter_map(|row| row.try_get::<String>("", "run").ok()).collect();
  let mut cutoff: Option<i64> = None;
  let mut count = 0;
  let mut bytes = 0;
  let mut before = i64::MAX;

  loop {
    let rows = all(
      db,
      r#"SELECT "id", (length("text") + COALESCE(length("detail"), 0))::bigint AS "bytes", "run"
         FROM "agent_log"
         WHERE "agent" = $1 AND "job_id" IS NULL AND "id" < $2
         ORDER BY "id" DESC LIMIT $3"#,
      vec![agent.into(), before.into(), PAGE.into()],
    )
    .await?;

    if rows.is_empty() {
      break;
    }

    for row in &rows {
      let id: i64 = row.try_get("", "id")?;
      let run: String = row.try_get("", "run")?;

      if id == kept || open.contains(&run) {
        continue;
      }

      count += 1;
      bytes += row.try_get::<i64>("", "bytes")?;

      if count > LOG_LINES_PER_AGENT || bytes > LOG_BYTES_PER_AGENT {
        cutoff = Some(id);
        break;
      }
    }

    if cutoff.is_some() {
      break;
    }

    before = rows.last().map(|row| row.try_get::<i64>("", "id")).transpose()?.unwrap_or(0);

    if rows.len() < PAGE as usize {
      break;
    }
  }

  let mut gone = 0;

  if let Some(cutoff) = cutoff {
    let open: Vec<String> = open.into_iter().collect();
    let open = serde_json::to_string(&open).unwrap_or_else(|_| "[]".to_owned());

    gone += delete_batches(
      db,
      r#"DELETE FROM "agent_log" AS "a" WHERE "a"."id" IN (
           SELECT "a"."id" FROM "agent_log" AS "a"
           WHERE "a"."agent" = $1 AND "a"."job_id" IS NULL AND "a"."id" <= $2 AND "a"."id" <> $3
             AND "a"."run" NOT IN (SELECT jsonb_array_elements_text($4::jsonb))
           ORDER BY "a"."id" LIMIT $5
         )"#,
      vec![agent.into(), cutoff.into(), kept.into(), open.into(), BATCH.into()],
    )
    .await?;
  }

  gone += delete_batches(
    db,
    r#"DELETE FROM "agent_log" AS "a" WHERE "a"."id" IN (
         SELECT "a"."id" FROM "agent_log" AS "a"
         WHERE "a"."agent" = $1 AND "a"."job_id" IS NULL AND "a"."at" < $2 AND "a"."id" <> $3
         ORDER BY "a"."id" LIMIT $4
       )"#,
    vec![agent.into(), (now - LOG_MOST_AGE_MS).into(), kept.into(), BATCH.into()],
  )
  .await?;

  Ok(gone)
}

// One bounded delete statement, its transaction given a statement timeout: the pass never holds one open
// transaction over the whole table, and a statement that overruns it is ended rather than waited on.
async fn delete_batches(
  db: &DatabaseConnection,
  sql: &str,
  values: Vec<sea_orm::Value>,
) -> Result<u64, Refusal> {
  let mut gone = 0;

  loop {
    let txn = db.begin().await?;

    run(&txn, &format!("SET LOCAL statement_timeout = '{STATEMENT_TIMEOUT}'"), vec![]).await?;

    let taken = run(&txn, sql, values.clone()).await?;

    txn.commit().await?;
    gone += taken;

    if taken == 0 {
      break;
    }
  }

  Ok(gone)
}

/// The whole run log swept on agency's own clock as the server starts and once a day after, for as long as
/// the process stands; what each pass took said on its output.
pub async fn keep(db: DatabaseConnection) {
  every("run log prune", Duration::ZERO, PRUNE_PASS, move || {
    let db = db.clone();

    async move {
      let gone = run_log(&db, now_ms()).await.map_err(|refusal| refusal.detail)?;

      if gone > 0 {
        println!("run log prune: {gone} line(s) past the log's bounds taken out");
      }

      Ok(())
    }
  })
  .await;
}
