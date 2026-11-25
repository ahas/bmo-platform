//! `bmo-agency run list`: what this machine is running and what is waiting, in one call
//! (docs/agent-control.md, 「Reading everything running at once」). The reading holds what agency's store
//! keeps open — its runs, its jobs and its executions — the panes this machine's herdr holds, and the
//! machine's own load and memory. It only reads and changes nothing.
//!
//! Every row prints what a stop of it is aimed at, written `<kind>:<id>`: `unit:` for a run in a unit of
//! its own, `execution:` for a chat turn or a gate, `item:` for a work order. The same word is what
//! `bmo-agency run stop` takes, so a person reads one report and hands a row back to the other.

use std::path::Path;

use chrono::TimeZone as _;
use clap::Parser;
use herdr::Herdr;
use policy::fill_marks;
use policy::verb_words::VERB_WORDS;
use procfs::FromRead as _;
use serde_json::{Value, json};
use store::all;

use super::{Answer, VerbError};
use crate::Refusal;
use crate::now_ms;
use crate::places::Ground;
use crate::queue::cancels;

/// What `bmo-agency run list` is asked.
#[derive(Parser, Debug, Default)]
pub struct ListArgs {
  /// The same reading as structure.
  #[arg(long)]
  pub json: bool,
  /// Every row whole, where a row is cut short.
  #[arg(long)]
  pub full: bool,
}

/// One open run of the ledger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRow {
  pub id: i64,
  pub agent: String,
  pub unit: String,
  pub started_at: i64,
}

impl RunRow {
  pub fn target(&self) -> String {
    format!("unit:{}", self.unit)
  }
}

/// One open job the harness handed over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobRow {
  pub id: i64,
  pub item_id: i64,
  pub worker: String,
  pub state: String,
  pub started_at: i64,
  pub unit: Option<String>,
}

impl JobRow {
  pub fn target(&self) -> String {
    format!("item:{}", self.item_id)
  }
}

/// One execution with no end recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionRow {
  pub id: String,
  pub line: String,
  pub detail: String,
  pub started_at: Option<i64>,
}

impl ExecutionRow {
  pub fn target(&self) -> String {
    format!("execution:{}", self.id)
  }
}

/// What herdr answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Panes {
  /// No herdr is named for this machine.
  Unnamed,
  /// herdr did not answer, and why.
  Unreached(String),
  /// The panes it holds, with the agent each shows.
  Held(Vec<PaneRow>),
}

/// One pane herdr holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneRow {
  pub run: String,
  pub label: String,
  pub status: String,
}

/// The machine's own figures, in kilobytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MachineRow {
  pub load: Option<f32>,
  pub threads: usize,
  pub total_kb: Option<i64>,
  pub available_kb: Option<i64>,
  pub swap_total_kb: Option<i64>,
  pub swap_free_kb: Option<i64>,
}

/// One reading of everything running.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
  pub runs: Vec<RunRow>,
  pub jobs: Vec<JobRow>,
  pub executions: Vec<ExecutionRow>,
  pub panes: Panes,
  pub machine: MachineRow,
}

async fn open_runs(db: &sea_orm::DatabaseConnection) -> Result<Vec<RunRow>, Refusal> {
  let rows = all(
    db,
    r#"SELECT "id", "agent", "unit", "started_at" FROM "runs"
       WHERE "finished_at" IS NULL ORDER BY "started_at""#,
    vec![],
  )
  .await?;

  Ok(
    rows
      .iter()
      .map(|row| {
        Ok(RunRow {
          id: row.try_get("", "id")?,
          agent: row.try_get("", "agent")?,
          unit: row.try_get("", "unit")?,
          started_at: row.try_get("", "started_at")?,
        })
      })
      .collect::<Result<_, sea_orm::DbErr>>()?,
  )
}

async fn open_jobs(db: &sea_orm::DatabaseConnection) -> Result<Vec<JobRow>, Refusal> {
  let rows = all(
    db,
    r#"SELECT "id", "item_id", "worker_name", "state", "started_at", "run_name" FROM "jobs"
       WHERE "state" IN ('starting', 'running') ORDER BY "started_at""#,
    vec![],
  )
  .await?;

  Ok(
    rows
      .iter()
      .map(|row| {
        Ok(JobRow {
          id: row.try_get("", "id")?,
          item_id: row.try_get("", "item_id")?,
          worker: row.try_get("", "worker_name")?,
          state: row.try_get("", "state")?,
          started_at: row.try_get("", "started_at")?,
          unit: row.try_get("", "run_name")?,
        })
      })
      .collect::<Result<_, sea_orm::DbErr>>()?,
  )
}

fn field(value: &Value, key: &str) -> String {
  value.get(key).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn at(value: &Value, key: &str) -> Option<i64> {
  value.get(key).and_then(Value::as_i64)
}

async fn open_executions(db: &sea_orm::DatabaseConnection) -> Result<Vec<ExecutionRow>, Refusal> {
  let rows = cancels::executions(db).await?;

  Ok(
    rows
      .into_iter()
      .filter(|row| row.get("endedAt").is_none_or(Value::is_null))
      .filter_map(|row| {
        let id = row.get("id").and_then(Value::as_str)?.to_owned();
        let line = field(&row, "line");
        let provider = field(&row, "provider");
        let parent = field(&row, "parent");
        let detail = if !provider.is_empty() {
          provider
        } else if !parent.is_empty() {
          format!("under {parent}")
        } else if let Some(pane) = row.get("paneId").and_then(Value::as_str) {
          format!("pane {pane}")
        } else {
          "-".to_owned()
        };

        Some(ExecutionRow {
          id,
          line: if line.is_empty() { "run".to_owned() } else { line },
          detail,
          started_at: at(&row, "startedAt"),
        })
      })
      .collect(),
  )
}

// The agent each pane shows, as herdr lists them, by pane; a herdr that does not answer leaves them unknown.
async fn statuses(herdr: &Herdr) -> std::collections::HashMap<String, String> {
  herdr
    .call("agent.list", json!({}), None)
    .await
    .ok()
    .and_then(|answer| answer.get("agents").and_then(Value::as_array).cloned())
    .into_iter()
    .flatten()
    .filter_map(|agent| {
      let pane = agent.get("pane_id").and_then(Value::as_str)?.to_owned();
      let status = agent.get("agent_status").and_then(Value::as_str).unwrap_or("unknown").to_owned();

      Some((pane, status))
    })
    .collect()
}

async fn panes(ground: &Ground) -> Panes {
  let Some(herdr) = &ground.herdr else {
    return Panes::Unnamed;
  };
  let snapshot = match herdr.snapshot().await {
    Ok(snapshot) => snapshot,
    Err(fault) => return Panes::Unreached(fault.to_string()),
  };
  let status = statuses(herdr).await;

  Panes::Held(
    snapshot
      .panes
      .into_iter()
      .filter_map(|pane| {
        let name = pane.run()?.to_owned();
        let status = pane
          .pane_id
          .as_deref()
          .and_then(|pane| status.get(pane))
          .cloned()
          .unwrap_or_else(|| "unknown".to_owned());

        Some(PaneRow {
          run: name,
          label: pane.label.unwrap_or_default(),
          status,
        })
      })
      .collect(),
  )
}

fn machine() -> MachineRow {
  let proc_root =
    std::env::var_os("BMO_PROC_ROOT").map_or_else(|| Path::new("/proc").to_path_buf(), Into::into);
  let meminfo = procfs::Meminfo::from_file(proc_root.join("meminfo")).ok();
  let load = procfs::LoadAverage::from_file(proc_root.join("loadavg")).ok();

  // The meminfo reader counts bytes; the reading speaks kilobytes.
  let kilobytes = |bytes: u64| i64::try_from(bytes / 1024).unwrap_or(i64::MAX);

  MachineRow {
    load: load.map(|load| load.one),
    threads: std::thread::available_parallelism().map(std::num::NonZeroUsize::get).unwrap_or(1),
    total_kb: meminfo.as_ref().map(|memory| kilobytes(memory.mem_total)),
    available_kb: meminfo.as_ref().and_then(|memory| memory.mem_available).map(kilobytes),
    swap_total_kb: meminfo.as_ref().map(|memory| kilobytes(memory.swap_total)),
    swap_free_kb: meminfo.as_ref().map(|memory| kilobytes(memory.swap_free)),
  }
}

/// Everything running now, read off the store, herdr and the machine.
pub async fn picture(ground: &Ground) -> Result<Picture, VerbError> {
  Ok(Picture {
    runs: open_runs(&ground.db).await?,
    jobs: open_jobs(&ground.db).await?,
    executions: open_executions(&ground.db).await?,
    panes: panes(ground).await,
    machine: machine(),
  })
}

// A moment in the reading's own words: local time to the second, and how long ago it was.
pub fn started(at: Option<i64>) -> String {
  at.and_then(|at| chrono::Local.timestamp_millis_opt(at).single())
    .map(|at| at.format("%H:%M:%S").to_string())
    .unwrap_or_else(|| "-".to_owned())
}

pub fn elapsed(at: i64) -> String {
  let seconds = (now_ms() - at).max(0) / 1000;

  if seconds < 60 {
    format!("{seconds}s")
  } else if seconds < 3600 {
    format!("{}m", seconds / 60)
  } else {
    format!("{}h {}m", seconds / 3600, seconds % 3600 / 60)
  }
}

fn mb(kilobytes: Option<i64>) -> String {
  kilobytes
    .map_or_else(|| "-".to_owned(), |kb| format!("{:.1}", (kb as f64 * 10.0 / 1024.0).round() / 10.0))
}

fn machine_line(machine: &MachineRow) -> String {
  let words = &VERB_WORDS.run_list;
  let load = machine.load.map_or_else(|| "-".to_owned(), |load| format!("{load:.2}"));

  fill_marks(
    &words.machine_line,
    &[
      ("load", &load),
      ("threads", &machine.threads.to_string()),
      ("available", &mb(machine.available_kb)),
      ("total", &mb(machine.total_kb)),
      ("swap_free", &mb(machine.swap_free_kb)),
      ("swap_total", &mb(machine.swap_total_kb)),
    ],
  )
}

/// The picture as a person reads it.
pub fn text(picture: &Picture) -> Vec<String> {
  let words = &VERB_WORDS.run_list;
  let mut lines = vec![words.runs.clone()];

  if picture.runs.is_empty() {
    lines.push(words.none.clone());
  }

  lines.extend(picture.runs.iter().map(|run| {
    fill_marks(
      &words.run,
      &[
        ("target", &run.target()),
        ("agent", &run.agent),
        ("unit", &run.unit),
        ("started", &started(Some(run.started_at))),
        ("elapsed", &elapsed(run.started_at)),
      ],
    )
  }));

  lines.push(words.jobs.clone());

  if picture.jobs.is_empty() {
    lines.push(words.none.clone());
  }

  lines.extend(picture.jobs.iter().map(|job| {
    fill_marks(
      &words.job,
      &[
        ("target", &job.target()),
        ("worker", &job.worker),
        ("item", &job.item_id.to_string()),
        ("state", &job.state),
        ("started", &started(Some(job.started_at))),
        ("elapsed", &elapsed(job.started_at)),
      ],
    )
  }));

  lines.push(words.executions.clone());

  if picture.executions.is_empty() {
    lines.push(words.none.clone());
  }

  lines.extend(picture.executions.iter().map(|execution| {
    fill_marks(
      &words.execution,
      &[
        ("target", &execution.target()),
        ("line", &execution.line),
        ("detail", &execution.detail),
        ("started", &started(execution.started_at)),
        ("elapsed", &execution.started_at.map_or_else(|| "-".to_owned(), elapsed)),
      ],
    )
  }));

  lines.push(words.panes.clone());

  match &picture.panes {
    Panes::Unnamed => lines.push(words.panes_unnamed.clone()),
    Panes::Unreached(why) => lines.push(fill_marks(&words.panes_unreached, &[("why", why)])),
    Panes::Held(held) => {
      if held.is_empty() {
        lines.push(words.none.clone());
      }

      lines.extend(held.iter().map(|pane| {
        fill_marks(
          &words.pane,
          &[
            ("run", &pane.run),
            ("label", if pane.label.is_empty() { &pane.run } else { &pane.label }),
            ("status", &pane.status),
          ],
        )
      }));
    }
  }

  lines.push(words.machine.clone());
  lines.push(machine_line(&picture.machine));
  lines
}

/// The picture as structure.
pub fn value(picture: &Picture) -> Value {
  json!({
    "runs": picture.runs.iter().map(|run| json!({
      "id": run.id,
      "agent": run.agent,
      "unit": run.unit,
      "startedAt": run.started_at,
      "stop": { "kind": "unit", "id": run.unit },
    })).collect::<Vec<_>>(),
    "jobs": picture.jobs.iter().map(|job| json!({
      "id": job.id,
      "itemId": job.item_id,
      "worker": job.worker,
      "state": job.state,
      "startedAt": job.started_at,
      "unit": job.unit,
      "stop": { "kind": "item", "id": job.item_id.to_string() },
    })).collect::<Vec<_>>(),
    "executions": picture.executions.iter().map(|execution| json!({
      "id": execution.id,
      "line": execution.line,
      "detail": execution.detail,
      "startedAt": execution.started_at,
      "stop": { "kind": "execution", "id": execution.id },
    })).collect::<Vec<_>>(),
    "herdr": match &picture.panes {
      Panes::Unnamed => json!({ "asked": false, "reason": VERB_WORDS.run_list.panes_unnamed }),
      Panes::Unreached(why) => json!({ "asked": true, "answered": false, "reason": why }),
      Panes::Held(held) => json!({
        "asked": true,
        "answered": true,
        "panes": held.iter().map(|pane| json!({
          "run": pane.run, "label": pane.label, "status": pane.status,
        })).collect::<Vec<_>>(),
      }),
    },
    "machine": {
      "load": picture.machine.load,
      "threads": picture.machine.threads,
      "totalKb": picture.machine.total_kb,
      "availableKb": picture.machine.available_kb,
      "swapTotalKb": picture.machine.swap_total_kb,
      "swapFreeKb": picture.machine.swap_free_kb,
    },
  })
}

/// `bmo-agency run list` over the service the data root names.
pub async fn run_list(data_root: &Path, asked: &ListArgs) -> Result<Answer, VerbError> {
  let ground = super::ground(data_root).await?;
  let picture = picture(&ground).await?;
  let lines = if asked.json {
    vec![serde_json::to_string_pretty(&value(&picture)).unwrap_or_default()]
  } else {
    text(&picture)
  };

  Ok(Answer { lines, code: 0 })
}
