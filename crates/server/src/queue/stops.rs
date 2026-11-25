//! A run asked to stop from outside (docs/agent-control.md, 「Stopping」; docs/api.md, 「Workers」, stopping
//! runs), on agency's server: the cancellation of its execution and every execution it owns, a run in a
//! pane asked to end and waited on until it has, a unit stopped by systemd, and the runs a change to a work
//! order ended, confirmed gone before their pool places are given back; and a job's stop mark, kept for
//! whoever closes the job's record.
//!
//! The asking that reaches the panes goes on by itself: whoever asked for the stop has already decided
//! what to record, and a run that reaches its own ending writes that ending over whatever was written
//! for it.

use std::collections::HashSet;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use herdr::Herdr;
use policy::fill_marks;
use policy::queue_words::QUEUE_WORDS;
use policy::run_stop::RUN_STOP;
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};
use store::one;

use super::running::{EXECUTION_CANCEL_PREFIX, Running};
use super::{cancels, settings};
use crate::places::Ground;
use crate::{Refusal, now_ms};

/// One run a change to a work order ended, as the consumer that owns the order names it: the unit it ran
/// under, and the pool place it held.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoppedRun {
  pub unit: Option<String>,
  pub pool_request: Option<String>,
}

// The settings key a job's stop mark stands under.
fn job_stop_key(job_id: i64) -> String {
  format!("job_stop:{job_id}")
}

/// A job marked stopped with the reason, before its unit is asked: whoever closes its record closes it as
/// stopped, the sweep after a restart included (docs/api.md, 「Workers」, a job's stop mark).
pub async fn mark_job_stopped(db: &DatabaseConnection, job_id: i64, why: &str) -> Result<(), Refusal> {
  println!("job {job_id}: stop queued: {}", why.trim());

  settings::set(db, &job_stop_key(job_id), why).await
}

/// The reason a job was marked stopped with, where it was.
pub async fn job_stop_reason(db: &DatabaseConnection, job_id: i64) -> Result<Option<String>, Refusal> {
  settings::get(db, &job_stop_key(job_id)).await
}

/// A job's stop mark taken away with the record it shaped.
pub async fn clear_job_stop(db: &DatabaseConnection, job_id: i64) -> Result<(), Refusal> {
  settings::forget(db, &job_stop_key(job_id)).await
}

// The file a pane turn's collector writes in its execution's directory, naming who collects it.
const PANE_ENVELOPE: &str = "collector.json";

// How often a waiting stop looks again.
const LOOK_EVERY: Duration = Duration::from_secs(2);

/// How a run in a pane was ended: asked and finished itself, its pane closed at once because it could
/// not be asked, or a name herdr is not holding at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunStop {
  Ended,
  Closed,
  Absent,
}

/// What a stop stands on: the store, this machine's herdr and the picture of it, the executions'
/// directories, and how what runs is read.
#[derive(Clone)]
pub struct Stops {
  db: DatabaseConnection,
  events: crate::events::Bus,
  herdr: Option<Herdr>,
  held: Option<herdr::picture::Watch>,
  executions: PathBuf,
  running: Running,
  systemctl: PathBuf,
}

fn text<'a>(row: &'a Value, key: &str) -> Option<&'a str> {
  row.get(key).and_then(Value::as_str)
}

// Whether a recorded process is provably gone: its pid now names another process or none at all. A
// record that names no pid or birth proves nothing.
fn gone(identity: &Value) -> bool {
  let (Some(pid), Some(birth)) = (identity.get("pid").and_then(Value::as_i64), text(identity, "birth"))
  else {
    return false;
  };
  let Ok(pid) = i32::try_from(pid) else { return false };

  pid > 1 && agent_host::identity::process_birth(pid).as_deref() != Some(birth)
}

fn read_json(path: &Path) -> Option<Value> {
  serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

impl Stops {
  /// The stops of the server standing on the ground, with the picture of this machine's herdr it watches.
  pub fn of(ground: &Ground, held: Option<herdr::picture::Watch>) -> Self {
    let machine = ground.machine();

    Self {
      db: ground.db.clone(),
      events: ground.events.clone(),
      herdr: ground.herdr.clone(),
      held: held.clone(),
      executions: ground.roots.data.join("executions"),
      running: Running::new(ground.db.clone(), &machine, held),
      systemctl: machine.program("systemctl").unwrap_or_else(|| PathBuf::from("systemctl")),
    }
  }

  async fn execution(db: &DatabaseConnection, id: &str) -> Option<Value> {
    let row = one(db, r#"SELECT "body" FROM "agent_executions" WHERE "id" = $1"#, vec![id.into()])
      .await
      .ok()??;

    serde_json::from_str(&row.try_get::<String>("", "body").ok()?).ok()
  }

  /// The pane herdr holds a run in, by the run's token; none where herdr holds none or cannot say.
  async fn pane_of(herdr: &Herdr, name: &str) -> Option<String> {
    herdr
      .snapshot()
      .await
      .ok()?
      .panes
      .into_iter()
      .find(|pane| pane.run() == Some(name))
      .and_then(|pane| pane.pane_id)
  }

  /// A run in a pane ended by asking: the stop's words put in and the run waited on until it has reached
  /// its own ending, however long that takes. A run whose pane will not take the words — one already
  /// waiting on a person — is closed at once, since nothing said to it would arrive; one whose program's
  /// provider is off is refused, since no words go into a pane then.
  pub async fn stop_run(db: &DatabaseConnection, herdr: &Herdr, name: &str) -> Result<RunStop, String> {
    let Some(pane) = Self::pane_of(herdr, name).await else {
      return Ok(RunStop::Absent);
    };
    let claude_on = crate::control::harness::enabled_providers(db)
      .await
      .map_err(|refused| refused.error)?
      .iter()
      .any(|provider| provider == "claude-code");

    if !claude_on {
      return Err("the claude-code provider is turned off, so no words go into a pane".into());
    }

    let asked = herdr
      .call("agent.prompt", json!({ "target": pane, "text": RUN_STOP.instruction }), None)
      .await
      .is_ok();

    if asked {
      Self::wait_for_run(herdr, name).await;

      return Ok(RunStop::Ended);
    }

    let _ = herdr.call("pane.close", json!({ "pane_id": pane }), None).await;

    Ok(RunStop::Closed)
  }

  // A run in a pane waited on until herdr no longer holds it: a herdr that cannot be asked holds nothing.
  async fn wait_for_run(herdr: &Herdr, name: &str) {
    loop {
      let holds = match herdr.snapshot().await {
        Ok(snapshot) => snapshot.panes.iter().any(|pane| pane.run() == Some(name)),
        Err(_) => false,
      };

      if !holds {
        return;
      }

      tokio::time::sleep(LOOK_EVERY).await;
    }
  }

  // Whether the row belongs to a turn that is its own pane's process rather than a child under a
  // supervisor envelope; a row that does not say is read by whether its envelope directory exists.
  fn pane_path(row: &Value, directory: &Path) -> bool {
    if row.get("sharedPane") == Some(&Value::Bool(true)) {
      return false;
    }

    row.get("panePath").and_then(Value::as_bool).unwrap_or_else(|| !directory.exists())
  }

  /// The stop's own reach into a resident turn. Behind the turn's collector it waits while the collector
  /// lives and the row is open, and reaches the program only where the collector is gone with the row
  /// still open; reached `now`, it goes at once. A chat turn's program is interrupted by its provider's
  /// interrupt, since a round has no work of its own to wind down. A job's turn is asked, never cut: from
  /// behind a collector the stop words are already in, and reached at once they are put in here.
  /// Answers whether anything was typed into the pane.
  pub async fn interrupt_resident(
    db: &DatabaseConnection,
    herdr: &Herdr,
    row: &Value,
    directory: &Path,
    now: bool,
  ) -> bool {
    let (Some(id), Some(pane), Some(provider)) =
      (text(row, "id"), text(row, "paneId"), text(row, "provider"))
    else {
      return false;
    };

    if row.get("resident").is_none_or(Value::is_null) {
      return false;
    }

    if !now {
      loop {
        let open = Self::execution(db, id)
          .await
          .is_some_and(|now| now.get("endedAt").is_none_or(Value::is_null));

        if !open {
          return false;
        }

        let collector =
          read_json(&directory.join(PANE_ENVELOPE)).and_then(|envelope| envelope.get("owner").cloned());

        if collector.as_ref().is_none_or(gone) {
          break;
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
      }
    }

    if text(row, "line") != Some("chat") {
      return now
        && herdr
          .call("agent.prompt", json!({ "target": pane, "text": RUN_STOP.instruction }), None)
          .await
          .is_ok();
    }

    let Some(keys) = harness_providers::provider(provider)
      .and_then(|one| one.declared().resident.as_ref())
      .map(|resident| resident.interrupt.clone())
    else {
      return false;
    };

    herdr
      .call("pane.send_keys", json!({ "pane_id": pane, "keys": keys }), None)
      .await
      .is_ok()
  }

  /// A tree of runs ended from outside: the markers raised for the run and everything it owns, and each
  /// run still going ended by what can reach it — a supervised child by its cancel file, a turn that is
  /// its own pane's process by being asked, a resident turn by its collector with the interrupt behind
  /// it. Answers whether anything was signalled and whether a resident turn was among it.
  pub async fn cancel(&self, parent: &str) -> Result<(bool, bool), Refusal> {
    let rows = cancels::executions(&self.db).await?;
    let owned = cancels::covered(&rows, parent);

    for name in &owned {
      settings::set(&self.db, &format!("{EXECUTION_CANCEL_PREFIX}{name}"), &now_ms().to_string()).await?;
    }

    let (mut signalled, mut resident) = (false, false);

    for row in &rows {
      let Some(id) = text(row, "id") else { continue };

      if !owned.contains(id) || !row.get("endedAt").is_none_or(Value::is_null) {
        continue;
      }

      signalled = true;

      // A child sharing its parent's pane runs inside its parent's program and ends with it.
      if row.get("sharedPane") == Some(&Value::Bool(true)) || text(row, "line") == Some("native-child") {
        continue;
      }

      let directory = self.executions.join(id);

      if !row.get("resident").is_none_or(Value::is_null) {
        resident = true;

        if let Some(herdr) = self.herdr.clone() {
          let (db, row) = (self.db.clone(), row.clone());

          tokio::spawn(async move { Self::interrupt_resident(&db, &herdr, &row, &directory, false).await });
        }

        continue;
      }

      let in_pane = text(row, "paneId").is_some();

      if in_pane && Self::pane_path(row, &directory) {
        self.ask_in_background(id);
      } else if directory.exists() {
        let marker = std::fs::OpenOptions::new()
          .write(true)
          .create(true)
          .truncate(true)
          .mode(0o600)
          .open(directory.join("cancel"));

        if let Err(why) = marker {
          eprintln!("agency: execution cancel marker: {why}");
        }
      } else if in_pane {
        self.ask_in_background(id);
      }
    }

    Ok((signalled, resident))
  }

  fn ask_in_background(&self, name: &str) {
    let Some(herdr) = self.herdr.clone() else { return };
    let (db, name) = (self.db.clone(), name.to_owned());

    tokio::spawn(async move {
      if let Err(why) = Self::stop_run(&db, &herdr, &name).await {
        eprintln!("agency: execution stop: {why}");
      }
    });
  }

  // The supervised children a stop of `parent` signalled, waited on until each has published its outcome
  // or its supervisor is gone: a row still open is not evidence of life, a supervisor that acknowledged
  // the child's start and is still alive is.
  async fn owned_closed(&self, parent: &str) {
    loop {
      let Ok(rows) = cancels::executions(&self.db).await else { return };
      let owned = cancels::covered(&rows, parent);
      let live = owned.iter().any(|id| {
        let directory = self.executions.join(id);

        !directory.join("outcome.json").exists()
          && read_json(&directory.join("started.json"))
            .and_then(|started| started.get("supervisor").cloned())
            .is_some_and(|supervisor| !gone(&supervisor))
      });

      if !live {
        return;
      }

      tokio::time::sleep(LOOK_EVERY).await;
    }
  }

  // Whether systemd holds the unit, asked of it alone.
  async fn unit_running(&self, unit: &str) -> bool {
    self.running.systemd(&[unit.to_owned()]).await.is_some_and(|alive| alive.contains(unit))
  }

  /// One run asked to stop by its name, a unit's or an execution's: its execution cancelled, a resident
  /// turn left to its collector, a run in a pane asked and waited on until it has ended, and a unit still
  /// standing once its runs have ended stopped by systemd.
  pub async fn stop(&self, unit: &str) {
    let mut signalled = false;

    match self.cancel(unit).await {
      Ok((asked, resident)) => {
        signalled = asked;

        // A resident turn is wound down by its own collector inside its coordinator, which ends once
        // the turn has; stopping the coordinator now would take the collector with it.
        if resident {
          return;
        }

        // The runs signalled end on their own, and their collectors observe their outcomes before their
        // coordinator is stopped.
        if asked {
          self.owned_closed(unit).await;
        }
      }
      Err(refusal) => eprintln!("agency: execution cancellation: {}", refusal.detail),
    }

    if let (Some(herdr), Some(held)) = (&self.herdr, &self.held)
      && held.names().contains(unit)
    {
      match Self::stop_run(&self.db, herdr, unit).await {
        Ok(RunStop::Ended | RunStop::Closed) => return,
        Ok(RunStop::Absent) => {}
        Err(why) => eprintln!("agency: failed to stop {unit}: {why}"),
      }
    }

    // A run in a host was reached by its markers alone, and systemd never held it.
    if signalled && !self.unit_running(unit).await {
      return;
    }

    let stopped = tokio::process::Command::new(&self.systemctl)
      .args(["--user", "stop", &format!("{unit}.service")])
      .stdin(std::process::Stdio::null())
      .status()
      .await;

    match stopped {
      Ok(status) if status.success() => {}
      Ok(status) => eprintln!(
        "agency: failed to stop {unit}: systemctl exited with code {}",
        status.code().unwrap_or(-1)
      ),
      Err(why) => eprintln!("agency: failed to stop {unit}: {why}"),
    }
  }

  /// The runs a change to a work order ended: every unit asked and confirmed gone first, so a slow stop
  /// cannot overlap the work replacing it; then each place in the pool given back and each cancellation
  /// taken back. Refused, writing nothing, where a unit still stands. Closing the jobs those runs were is
  /// the consumer's, which owns their records, once this answers.
  pub async fn stop_runs(&self, runs: &[StoppedRun], why: &str) -> Result<(), String> {
    let units: Vec<String> = runs.iter().filter_map(|run| run.unit.clone()).collect();

    for unit in &units {
      self.stop(unit).await;
    }

    let alive: HashSet<String> = self.running.active(&units).await.map_err(|refusal| refusal.detail)?;

    if !alive.is_empty() {
      let mut named: Vec<&str> =
        units.iter().filter(|unit| alive.contains(*unit)).map(String::as_str).collect();

      named.dedup();

      return Err(fill_marks(&QUEUE_WORDS.stops.not_ended, &[("units", &named.join(", "))]));
    }

    for run in runs {
      if let Some(request) = &run.pool_request {
        crate::order_run::dispatch::finish(&self.db, &self.events, request, why, true).await?;
      }
    }

    // The stop is over: every unit is gone and every place it held given back, so the cancellation has
    // nothing left to refuse.
    for unit in &units {
      cancels::take_back(&self.db, unit).await.map_err(|refusal| refusal.detail)?;
    }

    Ok(())
  }
}
