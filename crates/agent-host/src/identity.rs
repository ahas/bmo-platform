//! A process as the files name it: its pid and its start time as the kernel counts it, which tells a
//! pid reused by another process apart from the one that was recorded.

use std::fs;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessIdentity {
  pub pid: i32,
  pub birth: String,
}

// The fields after the command name, which may itself hold spaces and parentheses.
fn stat_fields(pid: i32) -> Option<Vec<String>> {
  let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
  let (_, rest) = stat.rsplit_once(") ")?;

  Some(rest.split(' ').map(str::to_owned).collect())
}

/// A process as the kernel's own stat line names it: whether it still runs, its parent, the processor time
/// it and the children it reaped have burned, and when it began, in clock ticks since boot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessStat {
  /// Its state letter, `Z` for a process that has ended and is not reaped yet.
  pub state: String,
  pub ppid: i32,
  /// The processor time it has burned itself, in clock ticks.
  pub cpu: u64,
  /// The processor time of the children it has reaped, in clock ticks.
  pub reaped_cpu: u64,
  pub birth: String,
}

/// The process `pid` as the kernel's stat line names it, or none where there is no such process. The
/// kernel's stat file is a pseudo-file of one fixed shape, and no library here reads that shape.
pub fn process_stat(pid: i32) -> Option<ProcessStat> {
  let fields = stat_fields(pid)?;

  Some(ProcessStat {
    state: fields.first()?.clone(),
    ppid: fields.get(1)?.parse().ok()?,
    cpu: fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?,
    reaped_cpu: fields.get(13)?.parse::<u64>().ok()? + fields.get(14)?.parse::<u64>().ok()?,
    birth: fields.get(19)?.clone(),
  })
}

/// When the machine began, in milliseconds since the epoch: the kernel's `btime` line of `/proc/stat`.
pub fn boot_ms() -> Option<i64> {
  fs::read_to_string("/proc/stat")
    .ok()?
    .lines()
    .find_map(|line| line.strip_prefix("btime ").and_then(|seconds| seconds.trim().parse().ok()))
    .map(|seconds: i64| seconds.saturating_mul(1_000))
}

/// The kernel's clock ticks in a second.
pub fn clock_ticks() -> i64 {
  // SAFETY: sysconf has no preconditions.
  match unsafe { libc::sysconf(libc::_SC_CLK_TCK) } {
    asked if asked > 0 => asked,
    _ => 100,
  }
}

/// Whether any member of the process group `group` still runs. A zombie counts as ended: it runs
/// nothing, and reaping it is its new parent's business.
pub fn group_running(group: i32) -> bool {
  let Ok(entries) = fs::read_dir("/proc") else {
    return false;
  };

  entries
    .flatten()
    .filter_map(|entry| entry.file_name().to_str()?.parse::<i32>().ok())
    .filter_map(stat_fields)
    .any(|fields| {
      fields.get(2).and_then(|leader| leader.parse::<i32>().ok()) == Some(group)
        && !fields.first().is_some_and(|state| state.starts_with('Z'))
    })
}

/// A process's start time, field 22 of its stat line, or none where there is no such process.
pub fn process_birth(pid: i32) -> Option<String> {
  Some(process_stat(pid)?.birth)
}

/// When a process began, in milliseconds since the epoch: the machine's boot time and the process's
/// start in clock ticks after it. The boot time is kept to the second, so the answer may fall up to a
/// second early and never late.
pub fn process_started_ms(pid: i32) -> Option<i64> {
  let ticks: i64 = process_birth(pid)?.parse().ok()?;

  Some(boot_ms()? + ticks.saturating_mul(1_000) / clock_ticks())
}

impl ProcessIdentity {
  /// This process.
  pub fn own() -> Self {
    let pid = std::process::id() as i32;

    Self {
      pid,
      birth: process_birth(pid).unwrap_or_default(),
    }
  }

  /// The process `pid` as it is now.
  pub fn of(pid: i32) -> Option<Self> {
    process_birth(pid).map(|birth| Self { pid, birth })
  }

  /// Whether the recorded process still runs: the same pid with the same start time, and not a
  /// zombie, which still has a stat line and is nobody's supervisor any more.
  pub fn alive(&self) -> bool {
    if self.pid <= 1 {
      return false;
    }

    stat_fields(self.pid).is_some_and(|fields| {
      !fields.first().is_some_and(|state| state.starts_with('Z')) && fields.get(19) == Some(&self.birth)
    })
  }
}
