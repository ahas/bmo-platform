//! This machine's room for the launcher (docs/agent-control.md, 「Room on the machine」): the node service
//! reports the machine's standing, agency judges the machine's five limits over it and its own two over the
//! jobs at work, keeps the launcher's notice of a held launch and says when the room has returned.

use agent_session::JobMark;
use policy::fill_marks;
use policy::machine_room::{CountLimit, MACHINE_ROOM, MachineLimit};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use store::all;

use crate::Refusal;
use crate::events::{Bus, Event};
use crate::peers::node::{NodeService, Standing};
use crate::queue::settings;

const LIMIT_PREFIX: &str = "machine_limit:";
const LAUNCH_HELD: &str = "machine_launch_held";
const WORKER_JOB: &str = "worker_job:";

/// A worker at work on this machine, which is also the name of its place, with the job its turn named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtWork {
  pub worker: String,
  pub job: Option<JobMark>,
}

/// This machine's room as the launcher reads it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Room {
  pub held: Vec<String>,
  pub jobs: u64,
  pub browser_jobs: u64,
  pub browsers: u64,
}

/// The launcher's notice of launches held: when, and on what.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchHeld {
  pub at: i64,
  pub held: Vec<String>,
}

/// The room returned under a held launch's notice, which is let go (docs/api.md, 「Events」).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineRoomReturned {
  pub at: i64,
  pub held: Vec<String>,
}

impl Event for MachineRoomReturned {
  const NAME: &'static str = "launch-room.changed";
}

#[derive(Serialize, Deserialize)]
struct JobKept {
  job: i64,
  item: Option<i64>,
}

/// The job a worker's turn named kept beside the worker while the turn goes, or let go where none is
/// named. A store that cannot keep it is said on the error stream; the turn goes on all the same.
pub async fn keep_worker_job(db: &DatabaseConnection, worker: &str, job: Option<JobMark>) {
  let key = format!("{WORKER_JOB}{worker}");
  let kept = match job {
    Some(job) => {
      let value = serde_json::json!(JobKept {
        job: job.job,
        item: job.item
      });

      settings::set(db, &key, &value.to_string()).await
    }
    None => settings::forget(db, &key).await,
  };

  if let Err(refusal) = kept {
    eprintln!("agency: the job of {worker}'s turn could not be kept: {}", refusal.detail);
  }
}

/// Every worker standing on this machine, its row naming no work node, with a run open in the executions,
/// by name, each with the job its turn named.
pub async fn at_work(db: &DatabaseConnection) -> Result<Vec<AtWork>, Refusal> {
  let rows = all(
    db,
    r#"SELECT DISTINCT (e."body"::json ->> 'agent') AS "name", s."value" AS "job" FROM "agent_executions" e
       LEFT JOIN "settings" s ON s."key" = $1 || (e."body"::json ->> 'agent')
       WHERE (e."body"::json ->> 'endedAt') IS NULL AND (e."body"::json ->> 'agent') IS NOT NULL
         AND NOT EXISTS (SELECT 1 FROM "supervisors" WHERE "name" = (e."body"::json ->> 'agent'))
       ORDER BY "name""#,
    vec![WORKER_JOB.into()],
  )
  .await?;
  // A checkout standing on a work node is at work there, not on this machine.
  let elsewhere: Vec<String> = crate::queue::workers::workers(db, std::path::Path::new(""))
    .await?
    .into_iter()
    .filter(|worker| worker.node.is_some())
    .map(|worker| worker.name)
    .collect();

  rows
    .iter()
    .filter(|row| row.try_get::<String>("", "name").is_ok_and(|name| !elsewhere.contains(&name)))
    .map(|row| {
      let job = row
        .try_get::<Option<String>>("", "job")?
        .and_then(|job| serde_json::from_str::<JobKept>(&job).ok())
        .map(|kept| JobMark {
          job: kept.job,
          item: kept.item,
        });

      Ok(AtWork {
        worker: row.try_get("", "name")?,
        job,
      })
    })
    .collect()
}

// A job-count limit as the settings table holds it, its default where the row is missing or no positive
// number, and none where the policy sets no default either.
async fn count_limit(db: &DatabaseConnection, name: &str, of: &CountLimit) -> Result<Option<u64>, Refusal> {
  let held = settings::get(db, &format!("{LIMIT_PREFIX}{name}")).await?;

  Ok(
    held
      .and_then(|value| value.trim().parse::<u64>().ok())
      .filter(|value| *value > 0)
      .or(of.default),
  )
}

// A machine limit as the settings table holds it, its default where the row is missing or no positive
// number.
async fn machine_limit(db: &DatabaseConnection, name: &str, of: &MachineLimit) -> Result<f64, Refusal> {
  let held = settings::get(db, &format!("{LIMIT_PREFIX}{name}")).await?;

  Ok(
    held
      .and_then(|value| value.trim().parse::<f64>().ok())
      .filter(|value| value.is_finite() && *value > 0.0)
      .unwrap_or(of.default),
  )
}

// A figure to a tenth, as the machine's own reasons write it.
fn tenth(value: f64) -> String {
  format!("{}", (value * 10.0).round() / 10.0)
}

// A limit as written in a reason, without a point where it is whole.
fn shown(value: f64) -> String {
  format!("{value}")
}

/// The jobs at work, and of them those whose place runs a browser in the standing.
pub fn counted(at_work: &[AtWork], standing: &Standing) -> (u64, u64) {
  let browsing = at_work
    .iter()
    .filter(|worker| standing.scopes.iter().any(|place| place.scope == worker.worker && place.browsers > 0))
    .count();

  (at_work.len() as u64, browsing as u64)
}

/// What the jobs at work still lack of the share each is reckoned to grow to, in gigabytes: the share less
/// what the job's place holds in the standing, none where it holds more.
pub fn still_to_grow(at_work: &[AtWork], standing: &Standing, share_gb: f64) -> f64 {
  at_work
    .iter()
    .map(|worker| {
      let held_mb = standing
        .scopes
        .iter()
        .filter(|place| place.scope == worker.worker)
        .map(|place| place.megabytes)
        .sum::<i64>();

      (share_gb - held_mb as f64 / 1024.0).max(0.0)
    })
    .sum()
}

/// This machine's room: the machine's five limits over the standing the node service reports, then
/// agency's own job-count limits met. A standing that cannot be read holds nothing of the machine's part
/// and counts no browser.
pub async fn room(db: &DatabaseConnection, nodes: &dyn NodeService) -> Result<Room, Refusal> {
  let standing = nodes.machine_standing().await.unwrap_or_else(|fault| {
    eprintln!("agency: the machine's standing: {fault}");
    Standing::unread()
  });
  let working = at_work(db).await?;
  let mut held = vec![];

  if let Some(reading) = &standing.reading {
    let full = reading.memory.as_ref().and_then(|memory| memory.full.as_ref()).map(|full| full.avg60);

    if let Some(full) = full {
      let limit = machine_limit(db, "launch_memory_full_pct", &MACHINE_ROOM.launch_memory_full_pct).await?;

      if full >= limit {
        held.push(fill_marks(
          &MACHINE_ROOM.launch_memory_full_pct.held,
          &[("value", &tenth(full)), ("limit", &shown(limit))],
        ));
      }
    }

    let limit =
      machine_limit(db, "launch_memory_available_gb", &MACHINE_ROOM.launch_memory_available_gb).await?;

    if reading.mem_available_gb < limit {
      held.push(fill_marks(
        &MACHINE_ROOM.launch_memory_available_gb.held,
        &[("value", &tenth(reading.mem_available_gb)), ("limit", &shown(limit))],
      ));
    } else {
      let share = machine_limit(db, "launch_job_memory_gb", &MACHINE_ROOM.launch_job_memory_gb).await?;
      let growth = still_to_grow(&working, &standing, share);

      if reading.mem_available_gb - growth - share < limit {
        held.push(fill_marks(
          &MACHINE_ROOM.launch_job_memory_gb.held,
          &[
            ("value", &tenth(reading.mem_available_gb)),
            ("growth", &tenth(growth)),
            ("jobs", &working.len().to_string()),
            ("share", &shown(share)),
            ("limit", &shown(limit)),
          ],
        ));
      }
    }

    let limit = machine_limit(db, "launch_load_per_thread", &MACHINE_ROOM.launch_load_per_thread).await?;

    if reading.threads > 0 && reading.load1 / reading.threads as f64 >= limit {
      held.push(fill_marks(
        &MACHINE_ROOM.launch_load_per_thread.held,
        &[
          ("value", &tenth(reading.load1)),
          ("threads", &reading.threads.to_string()),
          ("limit", &shown(limit)),
        ],
      ));
    }

    let limit = machine_limit(db, "launch_disk_free_gb", &MACHINE_ROOM.launch_disk_free_gb).await?;

    if let Some(free) = reading.disk_free_gb
      && free < limit
    {
      held.push(fill_marks(
        &MACHINE_ROOM.launch_disk_free_gb.held,
        &[("value", &tenth(free)), ("limit", &shown(limit))],
      ));
    }
  }

  let (jobs, browser_jobs) = counted(&working, &standing);

  for (name, of, count) in [
    ("launch_jobs", &MACHINE_ROOM.launch_jobs, jobs),
    ("launch_browser_jobs", &MACHINE_ROOM.launch_browser_jobs, browser_jobs),
  ] {
    let Some(limit) = count_limit(db, name, of).await? else { continue };

    if count >= limit {
      held.push(fill_marks(&of.held, &[("jobs", &count.to_string()), ("limit", &limit.to_string())]));
    }
  }

  Ok(Room {
    held,
    jobs,
    browser_jobs,
    browsers: u64::try_from(standing.browsers).unwrap_or(0),
  })
}

/// This machine's room as the launch-room resource answers it: whether a launch may start, every reason
/// it may not, and the jobs and browser jobs running.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRoom {
  pub held: bool,
  pub reasons: Vec<String>,
  pub job_count: u64,
  pub browser_job_count: u64,
}

/// The room the launcher reads: the machine's own reasons, and the reasons the launcher itself left.
pub async fn launch_room(db: &DatabaseConnection, nodes: &dyn NodeService) -> Result<LaunchRoom, Refusal> {
  let room = room(db, nodes).await?;
  let mut reasons = room.held;

  if let Some(notice) = launch_held(db).await? {
    reasons.extend(notice.held);
  }

  Ok(LaunchRoom {
    held: !reasons.is_empty(),
    reasons,
    job_count: room.jobs,
    browser_job_count: room.browser_jobs,
  })
}

/// The launcher's notice kept, or withdrawn where it holds nothing.
pub async fn tell_launch_held(db: &DatabaseConnection, notice: &LaunchHeld) -> Result<(), Refusal> {
  if notice.held.is_empty() {
    return settings::forget(db, LAUNCH_HELD).await;
  }

  settings::set(db, LAUNCH_HELD, &serde_json::json!(notice).to_string()).await
}

/// The launcher's notice of launches held, where one stands.
pub async fn launch_held(db: &DatabaseConnection) -> Result<Option<LaunchHeld>, Refusal> {
  Ok(
    settings::get(db, LAUNCH_HELD)
      .await?
      .and_then(|value| serde_json::from_str(&value).ok()),
  )
}

/// One pass of the held launch's watch: where a notice stands and nothing holds a launch any more, the
/// notice is let go and the room's return published. A notice the launcher replaced meanwhile stands.
/// Answers whether the room returned.
pub async fn watch_held(
  db: &DatabaseConnection,
  nodes: &dyn NodeService,
  events: &Bus,
) -> Result<bool, Refusal> {
  let Some(kept) = settings::get(db, LAUNCH_HELD).await? else {
    return Ok(false);
  };

  if !room(db, nodes).await?.held.is_empty() {
    return Ok(false);
  }

  let gone = store::run(
    db,
    r#"DELETE FROM "settings" WHERE "key" = $1 AND "value" = $2"#,
    vec![LAUNCH_HELD.into(), kept.clone().into()],
  )
  .await?;

  if gone == 0 {
    return Ok(false);
  }

  if let Ok(notice) = serde_json::from_str::<LaunchHeld>(&kept) {
    events.publish(MachineRoomReturned {
      at: notice.at,
      held: notice.held,
    });
  }

  Ok(true)
}
