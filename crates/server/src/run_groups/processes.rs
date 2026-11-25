//! The machine's processes as a run's marks find them (docs/run-groups.md, 「The record」), and the run's
//! control group.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use agent_host::identity::clock_ticks;
use procfs::process::{Process, Stat};
use procfs::{Current as _, ProcError, ProcResult};

use super::{Mark, PLACE_VARIABLE, RUN_NAME};

// Where the unified control-group hierarchy is mounted.
const HIERARCHY: &str = "/sys/fs/cgroup";

/// One process of a run, as it was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
  pub pid: i32,
  pub session: i32,
  /// When it began, in clock ticks since boot.
  pub start: u64,
  /// Resident memory and swap together.
  pub kilobytes: u64,
  /// Its program's name, as the kernel keeps it.
  pub command: String,
}

/// The clock ticks since boot now, as a process's start is counted.
pub fn ticks_now() -> u64 {
  procfs::Uptime::current().map_or(0, |uptime| (uptime.uptime * clock_ticks() as f64) as u64)
}

/// The session a process stands in.
pub fn session_of(pid: i32) -> Option<i32> {
  Process::new(pid).ok()?.stat().ok().map(|stat| stat.session)
}

// One process read: its stat line, where it is not a zombie, and its environment where it can be read.
struct Row {
  stat: Stat,
  environ: Option<HashMap<OsString, OsString>>,
}

// The machine's processes as one look read them, and whether the look read every one it listed.
struct Table {
  rows: Vec<Row>,
  whole: bool,
}

// Whether a reading failed on what the process itself answers, that it is gone or is not this account's to
// read, rather than on what this process lacked to read it: a descriptor, memory.
fn answered<T>(read: &ProcResult<T>) -> bool {
  match read {
    Ok(_) => true,
    Err(fault) => matches!(fault, ProcError::NotFound(_) | ProcError::PermissionDenied(_)),
  }
}

// Each process is read and its handle let go before the next is opened, since a handle holds a descriptor:
// a look holds a few files open however many processes the machine runs.
fn table() -> Table {
  let Ok(all) = procfs::process::all_processes() else {
    return Table {
      rows: vec![],
      whole: false,
    };
  };
  let mut table = Table {
    rows: vec![],
    whole: true,
  };

  for process in all {
    let read = process.and_then(|process| Ok((process.stat()?, process.environ())));

    table.whole &= answered(&read);

    if let Ok((stat, environ)) = read {
      table.whole &= answered(&environ);

      if stat.state != 'Z' {
        table.rows.push(Row {
          stat,
          environ: environ.ok(),
        });
      }
    }
  }

  table
}

fn names(row: &Row, variable: &str, value: &str) -> bool {
  row
    .environ
    .as_ref()
    .and_then(|environ| environ.get(&OsString::from(variable)))
    .is_some_and(|named| named.to_string_lossy().trim_end_matches('/') == value)
}

// Whether a control group was read, or could not be only because it is gone.
fn read_or_gone(read: &std::io::Result<impl Sized>) -> bool {
  read.as_ref().err().is_none_or(|fault| fault.kind() == std::io::ErrorKind::NotFound)
}

/// Every process in a control group and the groups under it, and whether every group could be read; a
/// group already removed holds none.
fn in_group(group: &Path) -> (HashSet<i32>, bool) {
  let mut pids = HashSet::new();
  let mut whole = true;
  let mut dirs = vec![group.to_path_buf()];

  while let Some(dir) = dirs.pop() {
    let procs = std::fs::read_to_string(dir.join("cgroup.procs"));
    let entries = std::fs::read_dir(&dir);

    whole &= read_or_gone(&procs) && read_or_gone(&entries);

    if let Ok(procs) = procs {
      pids.extend(procs.lines().filter_map(|line| line.trim().parse::<i32>().ok()));
    }

    if let Ok(entries) = entries {
      dirs.extend(entries.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()));
    }
  }

  (pids, whole)
}

fn seen(row: &Row) -> Seen {
  let status = Process::new(row.stat.pid).and_then(|process| process.status()).ok();

  Seen {
    pid: row.stat.pid,
    session: row.stat.session,
    start: row.stat.starttime,
    kilobytes: status.map_or(0, |status| status.vmrss.unwrap_or(0) + status.vmswap.unwrap_or(0)),
    command: row.stat.comm.clone(),
  }
}

/// What one look found of a mark, and whether the look read every process the machine runs.
#[derive(Clone, Debug)]
pub struct Members {
  pub seen: Vec<Seen>,
  pub whole: bool,
}

impl Members {
  /// Whether nothing of the mark stands: a look that could not read every process cannot say so.
  pub fn none_left(&self) -> bool {
    self.whole && self.seen.is_empty()
  }
}

/// Every process the mark finds now, never `own` and never the machine's first process: the group's, the
/// ones marked inside the window, and every member of a marked process's session whose leader is marked
/// or gone.
pub fn members(mark: &Mark, own: i32) -> Members {
  let Table { rows, mut whole } = table();
  let (from, until) = match mark {
    Mark::Run { from, until, .. } => (*from, *until),
    Mark::Place { from, .. } => (*from, None),
  };
  let inside =
    |row: &Row| row.stat.starttime >= from && until.is_none_or(|until| row.stat.starttime < until);
  let grouped = match mark {
    Mark::Run {
      group: Some(group), ..
    } => {
      let (grouped, read) = in_group(group);

      whole &= read;
      grouped
    }
    _ => HashSet::new(),
  };
  let marked: HashSet<i32> = rows
    .iter()
    .filter(|row| inside(row))
    .filter(|row| match mark {
      Mark::Run { name, .. } => names(row, RUN_NAME, name),
      Mark::Place {
        place,
        program_session,
        ..
      } => names(row, PLACE_VARIABLE, place) && row.stat.session != *program_session,
    })
    .map(|row| row.stat.pid)
    .collect();
  let present: HashSet<i32> = rows.iter().map(|row| row.stat.pid).collect();
  let own_session = session_of(own).unwrap_or(0);
  let sessions: HashSet<i32> = rows
    .iter()
    .filter(|row| marked.contains(&row.stat.pid))
    .map(|row| row.stat.session)
    .filter(|session| *session > 1 && *session != own_session)
    .filter(|session| marked.contains(session) || !present.contains(session))
    .filter(|session| match mark {
      Mark::Place {
        program_session, ..
      } => session != program_session,
      Mark::Run { .. } => true,
    })
    .collect();

  let found = rows
    .iter()
    .filter(|row| row.stat.pid > 1 && row.stat.pid != own)
    .filter(|row| {
      grouped.contains(&row.stat.pid)
        || marked.contains(&row.stat.pid)
        || (sessions.contains(&row.stat.session) && inside(row))
    })
    .map(seen)
    .collect();

  Members { seen: found, whole }
}

/// The control group this process stands in, as a whole path, where the machine has the unified
/// hierarchy.
pub fn own_group() -> Option<PathBuf> {
  let groups = Process::myself().ok()?.cgroups().ok()?;
  let unified = groups
    .0
    .into_iter()
    .find(|group| group.hierarchy == 0 && group.controllers.is_empty())?;
  let path = Path::new(HIERARCHY).join(unified.pathname.trim_start_matches('/'));

  path.join("cgroup.procs").exists().then_some(path)
}

/// A control group of the run's own made under `parent`.
pub fn make_group(parent: &Path, name: &str) -> std::io::Result<PathBuf> {
  if name.is_empty() || name.contains('/') || name.starts_with('.') {
    return Err(std::io::Error::other(format!("{name} cannot name a control group")));
  }

  let group = parent.join(name);

  std::fs::create_dir(&group)?;
  Ok(group)
}

/// This process moved into a control group.
pub fn enter_group(group: &Path) -> std::io::Result<()> {
  std::fs::write(group.join("cgroup.procs"), "0")
}

/// A control group and every group under it removed, the deepest first; a group still holding a process
/// stays.
pub fn remove_group(group: &Path) {
  let mut dirs = vec![];
  let mut open = vec![group.to_path_buf()];

  while let Some(dir) = open.pop() {
    if let Ok(entries) = std::fs::read_dir(&dir) {
      open.extend(entries.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()));
    }

    dirs.push(dir);
  }

  for dir in dirs.iter().rev() {
    let _ = std::fs::remove_dir(dir);
  }
}
