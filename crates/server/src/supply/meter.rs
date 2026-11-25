//! The meter on a person's reserved run (docs/supply.md, 「Reservations」 and 「Running out mid-turn」): on each
//! beat the run's running cost is read and written on its reservation; at the policy's share of the amount, or
//! as its expiry nears, the reservation is extended; an extension refused hands the run to the next platform
//! source that accepts a reservation, the session going on since the credential does not change; and where none
//! accepts, the run goes on for one more reservation unit before the meter says it ran out, for its carrier to
//! interrupt it.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use policy::supply::SUPPLY;
use store::one;
use tokio::sync::Mutex;

use super::reservations::{self, Held, cover, extend, of_source, settle};
use super::{Chosen, Closed, next_source, platform_after, told};
use crate::Refusal;
use crate::places::Ground;

/// Where a run's running cost is read: a gateway road's request records of the run's program, by the run name
/// its launch sends, since the run began; a Claude Code program's status line in its place; or, for a job, in
/// the place of the session its turn goes on, a thrown-away session's own, found by the job's row as it reads.
#[derive(Clone, Debug)]
pub enum Reading {
  Gateway { run: String, since: i64 },
  StatusLine { place: PathBuf, since: i64 },
  JobStatusLine { job: i64, worker: String, since: i64 },
}

/// The place a job's turn goes on: its session's thrown-away place where one stands, else its checkout's own.
pub async fn job_place(ground: &Ground, job: i64, worker: &str) -> Result<PathBuf, Refusal> {
  let session = one(&ground.db, r#"SELECT "session_id" FROM "jobs" WHERE "id" = $1"#, vec![job.into()])
    .await?
    .and_then(|row| row.try_get::<Option<String>>("", "session_id").ok().flatten())
    .filter(|session| !session.is_empty());

  if let Some(session) = session {
    let apart = ground.roots.place(&crate::apart_places::place_name(&agent_session::SessionId(session)));

    if apart.exists() {
      return Ok(apart);
    }
  }

  Ok(ground.roots.place(worker))
}

// What a Claude Code program's status line says it spent since `since` in its place, in millionths of a dollar.
fn status_cost(place: &std::path::Path, since: i64) -> i64 {
  let readings = crate::door::place_run::read_status(place, since);
  let total = |reading: Option<&crate::door::place_run::StatusRecord>| {
    reading
      .and_then(|reading| reading.payload.pointer("/cost/total_cost_usd"))
      .and_then(|cost| cost.as_f64())
  };

  match (total(readings.first()), total(readings.last())) {
    (Some(first), Some(last)) => reservations::micros((last - first).max(0.0)),
    _ => 0,
  }
}

impl Reading {
  /// What the run has cost so far, in millionths of a dollar.
  pub async fn cost(&self, ground: &Ground) -> Result<i64, Refusal> {
    match self {
      Self::Gateway { run, since } => {
        let row = one(
          &ground.db,
          r#"SELECT COALESCE(SUM("cost_micros"), 0)::bigint AS "cost" FROM "gateway_requests"
             WHERE "run" = $1 AND "at" >= $2"#,
          vec![run.clone().into(), (*since).into()],
        )
        .await?;

        Ok(row.map(|row| row.try_get::<i64>("", "cost")).transpose()?.unwrap_or_default())
      }
      Self::StatusLine { place, since } => Ok(status_cost(place, *since)),
      Self::JobStatusLine { job, worker, since } => {
        Ok(status_cost(&job_place(ground, *job, worker).await?, *since))
      }
    }
  }
}

/// A run's supply while it goes, shared between its carrier and its meter.
pub type Shared = Arc<Mutex<Metered>>;

/// What the meter keeps of a run: its supply, where its cost is read, and the cost at which the source it goes
/// out on now began paying.
#[derive(Clone, Debug)]
pub struct Metered {
  pub chosen: Chosen,
  pub reading: Reading,
  pub offset_micros: i64,
  /// Where the run goes on for one more unit before it is stopped: the cost it is stopped at and why.
  pub grace: Option<(i64, Closed)>,
  /// Where the run is told in its facts: the agent, the job and the room.
  pub agent: String,
  pub job: Option<i64>,
  pub room: Option<i64>,
}

/// The meter on a run until it runs out: answers why, where no source is left and its unit of grace is spent.
/// A run reserved nowhere is not metered and never runs out.
pub async fn watch(ground: &Ground, shared: Shared) -> Closed {
  let beat = Duration::from_secs(SUPPLY.meter_seconds.max(1));

  loop {
    tokio::time::sleep(beat).await;

    match look(ground, &shared).await {
      Ok(Some(closed)) => return closed,
      Ok(None) => {}
      Err(why) => eprintln!("agency: the supply meter: {why}"),
    }
  }
}

/// One look of the meter: the cost written, and the reservation extended, handed over or given its unit of
/// grace; the reason it ran out where the grace is spent.
pub async fn look(ground: &Ground, shared: &Shared) -> Result<Option<Closed>, Refusal> {
  let mut metered = shared.lock().await;
  let Some(held) = of_source(ground, &metered.chosen.source).await? else {
    return Ok(None);
  };
  let cost = metered.reading.cost(ground).await?;
  let covered = (cost - metered.offset_micros).max(0);

  cover(&ground.db, held.id, covered).await?;

  if let Some((limit, closed)) = &metered.grace {
    return Ok((covered >= *limit).then(|| closed.clone()));
  }

  let near = (held.amount_micros as f64 * SUPPLY.extend_at) as i64;
  let beat_ms = (SUPPLY.meter_seconds as i64).max(1) * 1000;
  let expiring = held.expires_at.is_some_and(|at| at - crate::now_ms() <= 2 * beat_ms);

  if covered < near && !expiring {
    return Ok(None);
  }

  if extend(ground, &mut metered.chosen, &held, covered).await? {
    return Ok(None);
  }

  hand_over(ground, &mut metered, &held, cost, covered).await
}

// An extension refused: the run handed to the next platform source of its order that accepts a reservation,
// what the source it leaves covered captured there; or, with none left, its unit of grace begun.
async fn hand_over(
  ground: &Ground,
  metered: &mut Metered,
  held: &Held,
  cost: i64,
  covered: i64,
) -> Result<Option<Closed>, Refusal> {
  let left = metered.chosen.source.word().to_owned();
  let mut next = metered.chosen.clone();

  match next_source(ground, &mut next, &platform_after(&metered.chosen)).await? {
    Ok(()) => {
      settle(ground, &mut next, held, covered).await?;
      metered.offset_micros = cost;

      let why = policy::fill_marks(&SUPPLY.words.ran_out, &[("source", &left)]);

      told::moved(ground, metered, &left, next.source.word(), &why).await;
      metered.chosen = next;
      super::remember(&ground.roots.data, &metered.agent, &metered.chosen);
      Ok(None)
    }
    Err(_) => {
      metered.chosen.step = next.step;

      let reason = policy::fill_marks(&SUPPLY.words.ran_out, &[("source", &left)]);
      let closed = Closed::of(super::Shortfall::RanOut, &left, reason, None);

      metered.grace = Some((held.amount_micros + held.unit_micros, closed));
      Ok(None)
    }
  }
}

/// A run's reservation settled as it ends: captured with what the source it ends on covered, the rest released.
pub async fn finish(ground: &Ground, shared: &Shared) -> Result<(), Refusal> {
  let mut metered = shared.lock().await;
  let Some(held) = of_source(ground, &metered.chosen.source).await? else {
    return Ok(());
  };
  let cost = metered.reading.cost(ground).await?;
  let covered = (cost - metered.offset_micros).max(0).max(held.covered_micros);

  settle(ground, &mut metered.chosen, &held, covered).await
}

/// A run's supply to meter: none where it went out on no reservation.
pub fn metered(
  chosen: &Chosen,
  reading: Reading,
  agent: &str,
  job: Option<i64>,
  room: Option<i64>,
) -> Option<Shared> {
  chosen.source.reservation().is_some().then(|| {
    Arc::new(Mutex::new(Metered {
      chosen: chosen.clone(),
      reading,
      offset_micros: 0,
      grace: None,
      agent: agent.to_owned(),
      job,
      room,
    }))
  })
}
