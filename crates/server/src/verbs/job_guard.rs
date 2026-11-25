//! What the job guard stands on in the shell hook's process (docs/live-claude-guard.md): the testbed's
//! `classify` verb for a heavy command, agency's store for the accounts' record directories, the job's
//! live-Claude opening, its own checkout and the ones it borrowed, the machine's own configuration directory,
//! and the job service for the job's item. The store is opened once, and only where a reading needs it; one
//! that cannot be reached answers that nothing is known.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use harness_providers::Machine;
use sea_orm::DatabaseConnection;
use tokio::sync::OnceCell;

use super::live_claude;
use crate::command_reading::job_guard::JobGround;
use crate::command_reading::testbed::Classified;
use crate::command_reading::writes::{Checkouts, ForeignCheckout, STRUCTURE_DOCUMENT, SealedTree};
use crate::control::claude_accounts::{accounts, machine_config_dir};
use crate::control::item_filings::{ASKED_BY, OWNER};
use crate::queue::holds::JOB_HOLDER;
use crate::queue::orderer::JobService;
use crate::queue::paths::resolve;
use crate::queue::workers::{Worker, workers};
use store::{all, one, run};

/// The testbed's binary, found on the path as every caller finds it.
const TESTBED: &str = "bmo-testbed";

/// The job guard's ground for one call of the hook.
pub struct StoreGround {
  data_root: PathBuf,
  machine: Machine,
  run: Option<String>,
  store: OnceCell<Option<DatabaseConnection>>,
  job: OnceCell<Option<Job>>,
}

// The job working in this program's run, as agency's store holds it.
#[derive(Clone, Debug)]
struct Job {
  id: i64,
  item_id: i64,
  worker: String,
}

impl StoreGround {
  /// The ground under a data root, for the job this process runs for where it is a job's.
  pub fn new(data_root: &Path, machine: Machine) -> Self {
    Self {
      data_root: data_root.to_path_buf(),
      machine,
      run: live_claude::job_run(),
      store: OnceCell::new(),
      job: OnceCell::new(),
    }
  }

  // The latest job working in the run, whatever its state: the program asking is that job's own.
  async fn job(&self) -> Option<&Job> {
    self
      .job
      .get_or_init(|| async {
        let row = one(
          self.db().await?,
          r#"SELECT "id", "item_id", "worker_name" FROM "jobs" WHERE "run_name" = $1 ORDER BY "id" DESC LIMIT 1"#,
          vec![self.run.as_deref()?.into()],
        )
        .await
        .inspect_err(|why| eprintln!("job-guard: {why}"))
        .ok()??;

        Some(Job {
          id: row.try_get("", "id").ok()?,
          item_id: row.try_get("", "item_id").ok()?,
          worker: row.try_get("", "worker_name").ok()?,
        })
      })
      .await
      .as_ref()
  }

  async fn db(&self) -> Option<&DatabaseConnection> {
    self
      .store
      .get_or_init(|| async {
        super::store(&self.data_root)
          .await
          .inspect_err(|why| eprintln!("job-guard: {why}"))
          .ok()
      })
      .await
      .as_ref()
  }
}

impl JobGround for StoreGround {
  async fn classify(&self, line: &str) -> Option<Classified> {
    let done = tokio::process::Command::new(TESTBED)
      .args(["classify", "--json", line])
      .stdin(Stdio::null())
      .stderr(Stdio::null())
      .kill_on_drop(true)
      .output()
      .await
      .inspect_err(|why| eprintln!("job-guard: {TESTBED}: {why}"))
      .ok()?;

    // Light is 0 and heavy 1; anything else is words it could not read.
    match done.status.code() {
      Some(0 | 1) => serde_json::from_slice(&done.stdout).ok(),
      _ => None,
    }
  }

  async fn config_dirs(&self) -> Vec<String> {
    let mut dirs = vec![machine_config_dir(&self.machine)];

    if let Some(db) = self.db().await {
      match accounts(db).await {
        Ok(held) => dirs.extend(held.iter().map(|account| account.records_dir(&self.machine))),
        Err(why) => eprintln!("job-guard: {}", why.error),
      }
    }

    dirs.iter().map(|dir| dir.to_string_lossy().into_owned()).collect()
  }

  async fn opening(&self) -> Option<i64> {
    let run = self.run.as_deref()?;
    let row = one(
      self.db().await?,
      r#"SELECT "id" FROM "live_claude_openings" WHERE "run" = $1 ORDER BY "id" DESC LIMIT 1"#,
      vec![run.into()],
    )
    .await
    .ok()??;

    row.try_get::<i64>("", "id").ok()
  }

  async fn record(&self, opening: i64, what: &str) {
    let Some(db) = self.db().await else { return };

    if let Err(why) = run(
      db,
      r#"INSERT INTO "live_claude_runs" ("opening_id", "what", "ran_at") VALUES ($1, $2, $3)"#,
      vec![opening.into(), what.into(), crate::now_ms().into()],
    )
    .await
    {
      eprintln!("job-guard: {why}");
    }
  }

  async fn checkouts(&self) -> Checkouts {
    let (Some(job), Some(db)) = (self.job().await, self.db().await) else {
      return Checkouts::default();
    };
    let known = match workers(db, Path::new("")).await {
      Ok(known) => known,
      Err(why) => {
        eprintln!("job-guard: {}", why.detail);
        return Checkouts::default();
      }
    };
    let Some(own) = known.iter().find(|checkout| checkout.name == job.worker) else {
      return Checkouts::default();
    };
    let borrowed: Vec<String> = match all(
      db,
      r#"SELECT "worker_name" FROM "assignments" WHERE "holder_name" = $1 AND "state" IN ('held', 'running')"#,
      vec![format!("{JOB_HOLDER}{}", job.id).into()],
    )
    .await
    {
      Ok(rows) => rows.iter().filter_map(|row| row.try_get("", "worker_name").ok()).collect(),
      Err(why) => {
        eprintln!("job-guard: {why}");
        return Checkouts::default();
      }
    };
    let path = |checkout: &Worker| resolve(&checkout.cwd).to_string_lossy().into_owned();
    let here: Vec<&Worker> = known
      .iter()
      .filter(|checkout| !checkout.builtin && !checkout.cwd.is_empty() && checkout.node == own.node)
      .collect();
    let (writes_in, others): (Vec<&Worker>, Vec<&Worker>) = here.into_iter().partition(|checkout| {
      checkout.name == own.name || borrowed.contains(&checkout.name) || path(checkout) == path(own)
    });

    // An umbrella guards the files its folder tracks, for its own job and every other: it is no other worker's
    // checkout beyond them, so a place below its folder that no checkout names stays unguarded.
    let sealed = writes_in
      .iter()
      .chain(&others)
      .filter(|checkout| checkout.umbrella)
      .map(|checkout| SealedTree {
        repository: checkout.repository().to_owned(),
        cwd: path(checkout),
        tracked: tracked_files(&path(checkout)),
      })
      .collect();

    Checkouts {
      sealed,
      own: writes_in.into_iter().map(path).collect(),
      foreign: others
        .into_iter()
        .filter(|checkout| !checkout.umbrella)
        .map(|checkout| ForeignCheckout {
          worker: checkout.name.clone(),
          repository: checkout.repository().to_owned(),
          cwd: path(checkout),
        })
        .collect(),
    }
  }

  async fn opens_structure(&self) -> bool {
    let Some(job) = self.job().await else { return false };
    let vars: BTreeMap<String, String> =
      self.machine.vars().map(|(name, value)| (name.to_owned(), value.to_owned())).collect();
    let Some(service) = JobService::of(&vars) else { return false };
    let item = match service.item(job.item_id).await {
      Ok(Some(item)) => item,
      Ok(None) => return false,
      Err(why) => {
        eprintln!("job-guard: {why}");
        return false;
      }
    };
    let names = ["title", "brief"]
      .iter()
      .any(|part| item[part].as_str().is_some_and(|words| words.contains(STRUCTURE_DOCUMENT)));

    item["labels"][ASKED_BY].as_str() == Some(OWNER) && names
  }
}

// The files an umbrella's git tracks, relative to its folder, `STRUCTURE.md` left to its own rule; none where git
// cannot read them.
fn tracked_files(root: &str) -> Vec<String> {
  let listed = std::process::Command::new("git")
    .args(["-C", root, "ls-files", "-z"])
    .stdin(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .output();

  match listed {
    Ok(listed) if listed.status.success() => listed
      .stdout
      .split(|byte| *byte == 0)
      .filter(|file| !file.is_empty())
      .map(|file| String::from_utf8_lossy(file).into_owned())
      .filter(|file| file.rsplit('/').next() != Some(STRUCTURE_DOCUMENT))
      .collect(),
    _ => {
      eprintln!("job-guard: the files the umbrella at {root} tracks could not be read");
      vec![]
    }
  }
}
