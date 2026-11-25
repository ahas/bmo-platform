//! What a run leaves running, taken down when the run ends (docs/run-groups.md): the record a grouped run
//! writes, the marks a run's processes are found by, the take-down every one of the run's reaper, the
//! door and the sweep runs alike, and the sweep and the listing over the records.

mod launcher;
mod processes;

use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use agent_host::identity::ProcessIdentity;
use policy::run_groups::RUN_GROUPS;
use serde::{Deserialize, Serialize};

pub use launcher::{LAUNCH_VERB, REAP_VERB, launch, launch_line, reap};
use processes::Members;
pub use processes::{Seen, ticks_now};

/// The variable a run is told its own name in.
pub const RUN_NAME: &str = "BMO_RUN_NAME";
/// The variable a standing program's place is named in.
pub const PLACE_VARIABLE: &str = "BMO_AGENT_PLACE";

/// Where the records stand under the data root.
pub fn records_dir(data: &Path) -> PathBuf {
  data.join("run-groups")
}

/// The record of the run of one execution.
pub fn record_path(data: &Path, execution: &str) -> PathBuf {
  records_dir(data).join(format!("{execution}.json"))
}

/// The log every run taken down is written to, one line a run.
pub fn log_path(data: &Path) -> PathBuf {
  data.join("run-groups.log")
}

/// What a grouped run wrote of itself as it began.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
  pub execution: String,
  /// The run's name, as its environment carries it.
  pub run: String,
  pub cwd: String,
  /// The run's control group, as a whole path, where one could be made.
  pub group: Option<PathBuf>,
  pub program: ProcessIdentity,
  pub reaper: Option<ProcessIdentity>,
  pub started_at: i64,
}

impl Record {
  pub fn read(path: &Path) -> Option<Self> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
  }

  /// Written whole or not at all, so a reader never meets half of it.
  pub fn write(&self, path: &Path) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
      std::fs::create_dir_all(dir)?;
    }

    let partial = path.with_extension("partial");

    std::fs::write(&partial, serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?)?;
    std::fs::rename(&partial, path)
  }

  fn birth(&self) -> u64 {
    self.program.birth.parse().unwrap_or(0)
  }

  /// What the run's processes are found by: its group, and its name over the window that runs from its
  /// program's start to the start of the next record of the same name.
  pub fn mark(&self, others: &[Record]) -> Mark {
    let from = self.birth();
    let until = others
      .iter()
      .filter(|other| other.execution != self.execution && other.run == self.run)
      .map(Record::birth)
      .filter(|birth| *birth > from)
      .min();

    Mark::Run {
      name: self.run.clone(),
      from,
      until,
      group: self.group.clone(),
    }
  }

  /// Whether the run's program is still the process it was.
  pub fn program_runs(&self) -> bool {
    self.program.alive()
  }
}

/// Every record under the data root, with the path each stands in.
pub fn records(data: &Path) -> Vec<(PathBuf, Record)> {
  let Ok(entries) = std::fs::read_dir(records_dir(data)) else {
    return vec![];
  };
  let mut records: Vec<(PathBuf, Record)> = entries
    .flatten()
    .map(|entry| entry.path())
    .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
    .filter_map(|path| Record::read(&path).map(|record| (path, record)))
    .collect();

  records.sort_by_key(|(_, record)| record.started_at);
  records
}

/// What a run's processes are found by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mark {
  /// A grouped run: every process in its group, and every process whose environment names the run and
  /// which began inside its window, clock ticks since boot.
  Run {
    name: String,
    from: u64,
    until: Option<u64>,
    group: Option<PathBuf>,
  },
  /// A run of a standing program: every process whose environment names the place, which began since the
  /// run did and stands outside the program's own session.
  Place {
    place: String,
    from: u64,
    program_session: i32,
  },
}

/// How a take-down waits.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
  pub grace: Duration,
  pub kill_wait: Duration,
  pub look: Duration,
}

impl Timing {
  /// The policy's.
  pub fn policy() -> Self {
    Self {
      grace: Duration::from_millis(RUN_GROUPS.grace_ms),
      kill_wait: Duration::from_millis(RUN_GROUPS.kill_wait_ms),
      look: Duration::from_millis(RUN_GROUPS.look_ms),
    }
  }
}

/// What a take-down came to: every process it found first, and whether anything still stood at its end.
#[derive(Clone, Debug, Default)]
pub struct TakenDown {
  pub found: Vec<Seen>,
  pub left: bool,
}

impl TakenDown {
  pub fn kilobytes(&self) -> u64 {
    self.found.iter().map(|seen| seen.kilobytes).sum()
  }
}

fn signal(pid: i32, signal: i32) {
  // SAFETY: a signal to a process by its number touches nothing of this process's memory.
  unsafe {
    libc::kill(pid, signal);
  }
}

// Looks until a look reads every process or the bound has passed, so what is found first is the whole of
// the mark wherever the machine lets it be read whole.
fn first_look(mark: &Mark, own: i32, bound: Duration, look: Duration) -> Members {
  let deadline = Instant::now() + bound;

  loop {
    let first = processes::members(mark, own);

    if first.whole || Instant::now() >= deadline {
      return first;
    }

    std::thread::sleep(look);
  }
}

// Waits until nothing of the mark is left or the bound has passed, sending each process found the
// signal, so a process the run started during the wait is asked too; answers what is still there.
fn wait_out(mark: &Mark, own: i32, bound: Duration, look: Duration, sent: i32) -> Members {
  let deadline = Instant::now() + bound;

  loop {
    let standing = processes::members(mark, own);

    if standing.none_left() || Instant::now() >= deadline {
      return standing;
    }

    for seen in &standing.seen {
      signal(seen.pid, sent);
    }

    std::thread::sleep(look);
  }
}

/// Every process of the mark taken down: asked to end with TERM, given the grace, and what still stands
/// killed with its group, then the group removed. This process is never among them. A look that could not
/// read every process never counts as finding nothing left.
pub fn take_down(mark: &Mark, timing: Timing) -> TakenDown {
  let own = std::process::id() as i32;
  let first = first_look(mark, own, timing.grace, timing.look);
  let group = match mark {
    Mark::Run { group, .. } => group.clone(),
    Mark::Place { .. } => None,
  };

  if !first.none_left() {
    for seen in &first.seen {
      signal(seen.pid, libc::SIGTERM);
    }

    let standing = wait_out(mark, own, timing.grace, timing.look, libc::SIGTERM);

    if !standing.none_left() {
      if let Some(group) = &group {
        let _ = std::fs::write(group.join("cgroup.kill"), "1");
      }

      wait_out(mark, own, timing.kill_wait, timing.look, libc::SIGKILL);
    }
  }

  let left = !processes::members(mark, own).none_left();

  if let Some(group) = &group
    && !left
  {
    processes::remove_group(group);
  }

  TakenDown {
    found: first.seen,
    left,
  }
}

/// A grouped run taken down, its record removed once nothing of it is left, and a line written for it
/// where it held anything; answers the line.
pub fn end_record(data: &Path, path: &Path, record: &Record, timing: Timing) -> Option<String> {
  let others: Vec<Record> = records(data).into_iter().map(|(_, other)| other).collect();
  let taken = take_down(&record.mark(&others), timing);

  if !taken.left {
    let _ = std::fs::remove_file(path);
  }

  (!taken.found.is_empty()).then(|| {
    let line = line(&record.run, &record.cwd, &taken);

    log(data, &line);
    line
  })
}

/// The line a run taken down is written as.
pub fn line(run: &str, cwd: &str, taken: &TakenDown) -> String {
  let commands: BTreeSet<&str> = taken.found.iter().map(|seen| seen.command.as_str()).collect();

  format!(
    "run {run} in {cwd}: {} processes ended, {} MB{}: {}",
    taken.found.len(),
    (taken.kilobytes() as f64 / 1024.0).round(),
    if taken.left { ", some still standing" } else { "" },
    commands.into_iter().collect::<Vec<_>>().join(", "),
  )
}

fn log(data: &Path, line: &str) {
  let opened = std::fs::OpenOptions::new().create(true).append(true).open(log_path(data));

  if let Ok(mut file) = opened {
    let _ = writeln!(file, "{} {line}", chrono::Utc::now().to_rfc3339());
  }
}

/// The run of one execution taken down at its end: its program waited on to be gone, up to the grace, then
/// whatever it left taken down. Nothing where the run wrote no record.
pub async fn end_of_run(data: &Path, execution: &str) -> Option<String> {
  let data = data.to_path_buf();
  let path = record_path(&data, execution);
  let timing = Timing::policy();

  tokio::task::spawn_blocking(move || {
    let record = Record::read(&path)?;
    let deadline = Instant::now() + timing.grace;

    while record.program_runs() && Instant::now() < deadline {
      std::thread::sleep(timing.look);
    }

    end_record(&data, &path, &record, timing)
  })
  .await
  .ok()
  .flatten()
}

/// What a run of a standing program left taken down at its end: the place's processes that began since
/// `from` outside the program's session.
pub async fn end_of_standing_run(
  data: &Path,
  run: &str,
  place: &Path,
  from: u64,
  program: Option<i32>,
) -> Option<String> {
  let data = data.to_path_buf();
  let run = run.to_owned();
  let place = place.to_string_lossy().trim_end_matches('/').to_owned();

  tokio::task::spawn_blocking(move || {
    let program_session = program.and_then(processes::session_of).unwrap_or(0);
    let mark = Mark::Place {
      place: place.clone(),
      from,
      program_session,
    };
    let taken = take_down(&mark, Timing::policy());

    (!taken.found.is_empty()).then(|| {
      let line = line(&run, &place, &taken);

      log(&data, &line);
      line
    })
  })
  .await
  .ok()
  .flatten()
}

/// Every record whose program is gone taken down, of the runs that worked in `within` where it is named;
/// answers one line for each run that still held anything.
pub async fn sweep(data: &Path, within: Option<&Path>) -> Vec<String> {
  let data = data.to_path_buf();
  let within = within.map(Path::to_path_buf);

  tokio::task::spawn_blocking(move || {
    records(&data)
      .into_iter()
      .filter(|(_, record)| within.as_ref().is_none_or(|within| Path::new(&record.cwd).starts_with(within)))
      .filter(|(_, record)| !record.program_runs())
      .filter_map(|(path, record)| end_record(&data, &path, &record, Timing::policy()))
      .collect()
  })
  .await
  .unwrap_or_default()
}

/// One record as a listing reads it.
#[derive(Clone, Debug)]
pub struct Standing {
  pub record: Record,
  pub program_runs: bool,
  pub holds: Vec<Seen>,
}

/// Every record with what its run holds now.
pub async fn standing(data: &Path) -> Vec<Standing> {
  let data = data.to_path_buf();

  tokio::task::spawn_blocking(move || {
    let all: Vec<Record> = records(&data).into_iter().map(|(_, record)| record).collect();
    let own = std::process::id() as i32;

    all
      .iter()
      .map(|record| Standing {
        record: record.clone(),
        program_runs: record.program_runs(),
        holds: processes::members(&record.mark(&all), own).seen,
      })
      .collect()
  })
  .await
  .unwrap_or_default()
}
