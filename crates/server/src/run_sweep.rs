//! The ledger's runs whose turn ended without closing them (docs/run-records.md, 「The run ledger」): a job's run
//! whose job has ended with no turn of it going, and the run of a turn agency carries, a chat turn's or a role
//! session's, that no turn kept in flight carries, each closed as cut with a line in its log saying why.

use std::collections::HashSet;

use store::all;

use crate::places::Ground;
use crate::run_ledger::{self, Closing, NotClosed};
use crate::run_log::lines::Line;
use crate::run_log::{RunLog, Target};
use crate::{Refusal, now_ms};

/// How long the run of a turn agency carries stands before a pass may read it as left behind: past the moment
/// between its opening and its turn being kept in flight.
pub const TURN_GRACE_MS: i64 = 2 * 60 * 1000;

/// The outcome a run left behind is closed with.
pub const CUT: &str = "cut";

/// Why a job's run is closed by the pass.
pub const JOB_ENDED: &str =
  "Closed as cut: its job had ended and no turn of it was going, so nothing else would close this run.";

/// Why the run of a turn agency carries is closed by the pass.
pub const TURN_LOST: &str =
  "Closed as cut: no turn was going under this run any more, so nothing else would close it.";

/// One pass over the open runs: each left behind closed as cut, its log told why. Answers how many it closed.
pub async fn pass(ground: &Ground) -> Result<usize, Refusal> {
  let now = now_ms();
  let open = all(
    &ground.db,
    r#"SELECT "id", "agent", "unit", "chat_id", "started_at", "carried" FROM "runs"
       WHERE "finished_at" IS NULL AND ("unit" LIKE 'job-%' OR "carried")
       ORDER BY "id""#,
    vec![],
  )
  .await?;
  // The runs the turns kept in flight carry, read once for the pass; where they cannot be read, no carried run is
  // read as left behind.
  let kept: Option<HashSet<i64>> = crate::turn_pickup::in_flight(&ground.db)
    .await
    .ok()
    .map(|turns| turns.iter().filter_map(|turn| turn.ledger).collect());
  let mut closed = 0;

  for row in open {
    let (Ok(id), Ok(agent), Ok(unit)) = (
      row.try_get::<i64>("", "id"),
      row.try_get::<String>("", "agent"),
      row.try_get::<String>("", "unit"),
    ) else {
      continue;
    };
    let chat_id = row.try_get::<Option<i64>>("", "chat_id").ok().flatten();
    let started_at = row.try_get::<i64>("", "started_at").unwrap_or(now);
    let carried = row.try_get::<bool>("", "carried").unwrap_or_default();
    let job = unit.strip_prefix("job-").and_then(|id| id.parse::<i64>().ok());
    let why = match job {
      Some(job) if job_left(ground, job, &unit).await? => JOB_ENDED,
      None
        if carried
          && now - started_at > TURN_GRACE_MS
          && kept.as_ref().is_some_and(|kept| !kept.contains(&id)) =>
      {
        TURN_LOST
      }
      _ => continue,
    };
    let closing = Closing {
      finished_at: now,
      outcome: CUT.to_owned(),
      context_tokens: None,
      spent: None,
    };

    match run_ledger::close(&ground.db, id, &closing).await {
      Ok(()) => {}
      Err(NotClosed::Store(why)) => return Err(why),
      Err(_) => continue,
    }

    RunLog::new(Target {
      agent,
      run: unit,
      chat_id,
      job_id: job,
    })
    .write(&ground.db, Line::note(why))
    .await;
    closed += 1;
  }

  Ok(closed)
}

// Whether a job's run is left behind: the job has ended, this process carries no turn of it, and no turn of it is
// kept in flight for a server's pickup.
async fn job_left(ground: &Ground, job: i64, unit: &str) -> Result<bool, Refusal> {
  let ended = store::one(&ground.db, r#"SELECT "state" FROM "jobs" WHERE "id" = $1"#, vec![job.into()])
    .await?
    .and_then(|row| row.try_get::<String>("", "state").ok())
    .is_some_and(|state| !matches!(state.as_str(), "starting" | "running" | "reviewing"));

  Ok(ended && !crate::control::work::is_carrying(job) && !crate::turn_pickup::kept(&ground.db, unit).await)
}
