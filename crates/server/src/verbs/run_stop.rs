//! `bmo-agency run stop`: the shell's road to the one stop verb (docs/agent-control.md, 「Stopping one from
//! a shell」). Called with nothing it lists what is running with every row's target first, the same
//! `<kind>:<id>` `bmo-agency run list` prints; called with a target it asks that run to end and says
//! whether it was only asked or also confirmed gone.

use std::path::Path;
use std::time::Duration;

use clap::Parser;
use policy::fill_marks;
use policy::verb_words::VERB_WORDS;
use serde_json::Value;
use store::one;

use super::run_list::{self, JobRow, Picture};
use super::{Answer, VerbError};
use crate::now_ms;
use crate::places::Ground;
use crate::queue::cancels;
use crate::queue::running::Running;
use crate::queue::stops::Stops;

/// How often a stop looks again whether what it asked is gone.
const LOOK_EVERY: Duration = Duration::from_secs(1);

/// What `bmo-agency run stop` is asked.
#[derive(Parser, Debug, Default)]
pub struct StopArgs {
  /// `<kind>:<id>`: a unit, an execution or an item. No target lists what is running.
  pub target: Option<String>,
  /// Every target the reading names, in the order it stands them in.
  #[arg(long)]
  pub all: bool,
  /// Why a work order is stopped, kept on its item.
  #[arg(long, allow_hyphen_values = true)]
  pub why: Option<String>,
}

/// What a stop is aimed at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
  Unit(String),
  Execution(String),
  Item(i64),
}

impl Target {
  pub fn words(&self) -> String {
    match self {
      Self::Unit(id) => format!("unit:{id}"),
      Self::Execution(id) => format!("execution:{id}"),
      Self::Item(id) => format!("item:{id}"),
    }
  }
}

/// A stop target out of the words a report prints, `<kind>:<id>`; none where it is not one.
pub fn target_of(text: &str) -> Option<Target> {
  let (kind, id) = text.split_once(':')?;
  let id = id.trim();

  if id.is_empty() {
    return None;
  }

  match kind {
    "unit" => Some(Target::Unit(id.to_owned())),
    "execution" => Some(Target::Execution(id.to_owned())),
    "item" => id.parse().ok().map(Target::Item),
    _ => None,
  }
}

fn bad_target(said: &str) -> VerbError {
  VerbError::Ended {
    code: 2,
    said: fill_marks(&VERB_WORDS.run_stop.bad_target, &[("target", said)]),
  }
}

// What a row is, in the words a person reads beside its target.
fn run_what(run: &run_list::RunRow) -> String {
  format!("{} · {} · {}", run.agent, run.unit, run_list::elapsed(run.started_at))
}

fn job_what(job: &JobRow) -> String {
  format!(
    "{} · item {} · {} · {}",
    job.worker,
    job.item_id,
    job.state,
    run_list::elapsed(job.started_at)
  )
}

fn execution_what(execution: &run_list::ExecutionRow) -> String {
  format!(
    "{} · {} · {}",
    execution.line,
    execution.detail,
    execution.started_at.map_or_else(|| "-".to_owned(), run_list::elapsed)
  )
}

// Every target the reading names, in the order the report stands them — runs, then jobs, then executions,
// each target once — and how many rows have nothing to aim at.
fn targets(picture: &Picture) -> (Vec<(String, String)>, usize) {
  let mut rows: Vec<(String, String)> = Vec::new();
  let mut left_out = 0;
  let mut add = |target: String, what: String| {
    if target_of(&target).is_none() {
      left_out += 1;
    } else if !rows.iter().any(|(held, _)| *held == target) {
      rows.push((target, what));
    }
  };

  for run in &picture.runs {
    add(run.target(), run_what(run));
  }

  for job in &picture.jobs {
    add(job.target(), job_what(job));
  }

  for execution in &picture.executions {
    add(execution.target(), execution_what(execution));
  }

  (rows, left_out)
}

// The picture of herdr waited for until it has attached, or herdr refuses to be asked at all.
async fn attached(herdr: &herdr::Herdr, watch: &herdr::picture::Watch) -> bool {
  loop {
    if watch.attached() {
      return true;
    }

    if herdr.snapshot().await.is_err() {
      return false;
    }

    tokio::time::sleep(Duration::from_millis(100)).await;
  }
}

async fn pane_holds(ground: &Ground, unit: &str) -> bool {
  let Some(herdr) = &ground.herdr else { return false };
  let Ok(snapshot) = herdr.snapshot().await else { return false };

  snapshot.panes.iter().any(|pane| pane.run() == Some(unit))
}

async fn systemctl_stop(ground: &Ground, unit: &str) {
  let Some(program) = ground.machine().program("systemctl") else { return };
  let mut command = std::process::Command::new(program);

  command.args(["--user", "stop", &format!("{unit}.service")]);
  command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null());
  command.stderr(std::process::Stdio::null());
  harness_machine::model_use::with_user_bus(&mut command);

  let _ = tokio::process::Command::from(command).status().await;
}

// A unit and every execution it owns asked to end: the cancellation written, a run in a pane asked and
// waited on until it has ended, and the unit stopped by systemd where it still stands after that. A turn agency
// keeps in flight under the name is asked by the cancellation alone, which it reads wherever it stands.
async fn ask_unit(
  ground: &Ground,
  stops: &Stops,
  running: &Running,
  unit: &str,
) -> Result<bool, VerbError> {
  let (mut asked, _) = stops.cancel(unit).await?;
  let in_pane = pane_holds(ground, unit).await;

  if in_pane && let Some(herdr) = &ground.herdr {
    asked = true;
    let _ = Stops::stop_run(&ground.db, herdr, unit).await;
  }

  if running.systemd(&[unit.to_owned()]).await.is_some_and(|alive| alive.contains(unit)) {
    asked = true;
    systemctl_stop(ground, unit).await;
  }

  Ok(asked || crate::turn_pickup::kept(&ground.db, unit).await)
}

/// A turn agency kept in flight under the name waited on until it no longer keeps it, its end handed back,
/// looking again every `look`.
pub async fn turn_gone(db: &sea_orm::DatabaseConnection, unit: &str, look: Duration) -> bool {
  loop {
    let kept = crate::queue::settings::get(db, &format!("{}{unit}", crate::turn_pickup::PREFIX)).await;

    if kept.is_ok_and(|kept| kept.is_none()) {
      return true;
    }

    tokio::time::sleep(look).await;
  }
}

// A unit waited on until nothing of it still runs.
async fn unit_gone(running: &Running, unit: &str) -> bool {
  loop {
    if running.active(&[unit.to_owned()]).await.is_ok_and(|alive| alive.is_empty()) {
      return true;
    }

    tokio::time::sleep(LOOK_EVERY).await;
  }
}

async fn execution_open(db: &sea_orm::DatabaseConnection, id: &str) -> bool {
  cancels::executions(db).await.is_ok_and(|rows| {
    rows.iter().any(|row| {
      row.get("id").and_then(Value::as_str) == Some(id) && row.get("endedAt").is_none_or(Value::is_null)
    })
  })
}

// An execution waited on until its row has closed.
async fn execution_gone(db: &sea_orm::DatabaseConnection, id: &str) -> bool {
  loop {
    if !execution_open(db, id).await {
      return true;
    }

    tokio::time::sleep(LOOK_EVERY).await;
  }
}

// The open jobs of an item, as the reading stands them.
fn jobs_of(picture: &Picture, item: i64) -> Vec<JobRow> {
  picture.jobs.iter().filter(|job| job.item_id == item).cloned().collect()
}

// A job marked stopped and closed, its assignment released. The server's route is asked where it answers,
// since its consumers hear the change there; a server that does not answer leaves the rows written here,
// where the verb's own store stands.
async fn stop_job(ground: &Ground, job: &JobRow, why: &str) -> Result<bool, VerbError> {
  let body = serde_json::json!({ "state": "stopped", "outcome": "stopped", "report": why });

  if let Some(token) = super::control_token()
    && super::control_call(&token, reqwest::Method::PATCH, &format!("/jobs/{}", job.id), Some(&body))
      .await
      .is_ok_and(|(status, _)| (200..300).contains(&status))
  {
    return Ok(true);
  }

  let stopped = job.state == "stopped";

  crate::queue::stops::mark_job_stopped(&ground.db, job.id, why).await?;
  store::run(
    &ground.db,
    r#"UPDATE "jobs" SET "state" = 'stopped', "outcome" = COALESCE("outcome", 'stopped'), "ended_at" = $1
       WHERE "id" = $2 AND "state" IN ('starting', 'running')"#,
    vec![now_ms().into(), job.id.into()],
  )
  .await?;
  store::run(
    &ground.db,
    r#"UPDATE "assignments" SET "state" = 'released', "changed_at" = $1
       WHERE "id" = (SELECT "assignment_id" FROM "jobs" WHERE "id" = $2) AND "state" IN ('held', 'running')"#,
    vec![now_ms().into(), job.id.into()],
  )
  .await?;
  crate::job_end::slot_given_back(&ground.db, &ground.events, job.id, &format!("stopped: {why}")).await;

  Ok(!stopped)
}

async fn item_stopped(db: &sea_orm::DatabaseConnection, id: i64) -> bool {
  one(db, r#"SELECT "state" FROM "jobs" WHERE "id" = $1"#, vec![id.into()])
    .await
    .ok()
    .flatten()
    .is_some_and(|row| row.try_get::<String>("", "state").is_ok_and(|state| state == "stopped"))
}

/// One target asked to end, and whether anything was asked at all.
async fn ask_one(
  ground: &Ground,
  stops: &Stops,
  running: &Running,
  picture: &Picture,
  target: &Target,
  why: &str,
) -> Result<bool, VerbError> {
  match target {
    Target::Unit(unit) => ask_unit(ground, stops, running, unit).await,
    Target::Execution(id) => Ok(stops.cancel(id).await?.0),
    Target::Item(item) => {
      let mut asked = false;

      for job in jobs_of(picture, *item) {
        asked |= stop_job(ground, &job, why).await?;

        if let Some(unit) = &job.unit {
          asked |= ask_unit(ground, stops, running, unit).await?;
        }
      }

      Ok(asked)
    }
  }
}

/// Whether a target asked to end is now gone: a unit's runs ended and none of its units still stand, an
/// execution's row closed, an item's jobs closed and their units gone.
async fn gone(ground: &Ground, running: &Running, picture: &Picture, target: &Target) -> bool {
  match target {
    Target::Unit(unit) => {
      let gone = unit_gone(running, unit).await && turn_gone(&ground.db, unit, LOOK_EVERY).await;

      if gone {
        let _ = cancels::take_back(&ground.db, unit).await;
      }

      gone
    }
    Target::Execution(id) => {
      let gone = execution_gone(&ground.db, id).await;

      if gone {
        let _ = cancels::take_back(&ground.db, id).await;
      }

      gone
    }
    Target::Item(item) => {
      let mut gone = true;

      for job in jobs_of(picture, *item) {
        gone &= item_stopped(&ground.db, job.id).await;

        if let Some(unit) = &job.unit {
          let unit_gone = unit_gone(running, unit).await;

          if unit_gone {
            let _ = cancels::take_back(&ground.db, unit).await;
          }

          gone &= unit_gone;
        }
      }

      gone
    }
  }
}

/// What a stop reached: whether it was asked at all, and whether the run was then seen gone.
async fn stop_one(
  ground: &Ground,
  stops: &Stops,
  running: &Running,
  picture: &Picture,
  target: &Target,
  why: &str,
) -> Result<(bool, bool), VerbError> {
  let asked = ask_one(ground, stops, running, picture, target, why).await?;

  Ok((asked, if asked { gone(ground, running, picture, target).await } else { false }))
}

/// `bmo-agency run stop`, over the service the data root names.
pub async fn run_stop(data_root: &Path, asked: &StopArgs) -> Result<Answer, VerbError> {
  let words = &VERB_WORDS.run_stop;
  let ground = super::ground(data_root).await?;
  let picture = run_list::picture(&ground).await?;
  let (rows, left_out) = targets(&picture);

  if asked.target.is_none() && !asked.all {
    let mut lines = Vec::new();

    if rows.is_empty() {
      lines.push(words.nothing.clone());
    }

    lines.extend(
      rows
        .iter()
        .map(|(target, what)| fill_marks(&words.row, &[("target", target), ("what", what)])),
    );

    if left_out > 0 {
      lines.push(fill_marks(&words.left_out, &[("count", &left_out.to_string())]));
    }

    return Ok(Answer { lines, code: 0 });
  }

  let watch = ground.herdr.clone().map(herdr::picture::Watch::start);

  if let (Some(herdr), Some(watch)) = (&ground.herdr, &watch) {
    attached(herdr, watch).await;
  }

  let stops = Stops::of(&ground, watch.clone());
  let running = Running::new(ground.db.clone(), &ground.machine(), watch);

  if asked.all {
    let reason = asked.why.as_deref().unwrap_or("stopped from a shell");
    let mut asked_targets: Vec<(Target, bool)> = Vec::new();

    // Every target is asked first and the whole walk confirmed once, so stopping a dozen runs takes no
    // longer than stopping one.
    for (target, _) in &rows {
      let Some(target) = target_of(target) else { continue };
      let was_asked = ask_one(&ground, &stops, &running, &picture, &target, reason).await?;

      asked_targets.push((target, was_asked));
    }

    let mut count_asked = 0;
    let mut count_confirmed = 0;
    let mut lines = Vec::new();

    for (target, was_asked) in &asked_targets {
      let gone = *was_asked && gone(&ground, &running, &picture, target).await;

      count_asked += usize::from(*was_asked);
      count_confirmed += usize::from(gone);
      lines.push(if gone {
        fill_marks(&words.confirmed, &[("target", &target.words())])
      } else {
        fill_marks(&words.not_confirmed, &[("target", &target.words())])
      });
    }

    lines.push(fill_marks(
      &words.all_done,
      &[("asked", &count_asked.to_string()), ("confirmed", &count_confirmed.to_string())],
    ));

    return Ok(Answer { lines, code: 0 });
  }

  let said = asked.target.as_deref().unwrap_or_default();
  let Some(target) = target_of(said) else { return Err(bad_target(said)) };

  if let Target::Item(_) = target
    && asked.why.as_deref().map(str::trim).unwrap_or_default().is_empty()
  {
    return Err(VerbError::Ended {
      code: 2,
      said: words.no_reason.clone(),
    });
  }

  let known = match &target {
    Target::Unit(unit) => {
      picture.runs.iter().any(|run| run.unit == *unit)
        || picture.executions.iter().any(|execution| execution.id == *unit)
        || rows.iter().any(|(held, _)| *held == target.words())
    }
    Target::Execution(id) => picture.executions.iter().any(|execution| execution.id == *id),
    Target::Item(item) => !jobs_of(&picture, *item).is_empty(),
  };

  if !known {
    return Err(VerbError::Ended {
      code: 1,
      said: fill_marks(&words.unknown, &[("target", &target.words())]),
    });
  }

  let (was_asked, gone) =
    stop_one(&ground, &stops, &running, &picture, &target, asked.why.as_deref().unwrap_or_default())
      .await?;

  if !was_asked && !gone {
    return Err(VerbError::Ended {
      code: 1,
      said: fill_marks(&words.unknown, &[("target", &target.words())]),
    });
  }

  let mut lines = vec![fill_marks(&words.asked, &[("target", &target.words())])];

  lines.push(if gone {
    fill_marks(&words.confirmed, &[("target", &target.words())])
  } else {
    fill_marks(&words.not_confirmed, &[("target", &target.words())])
  });

  Ok(Answer { lines, code: 0 })
}
