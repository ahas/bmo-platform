//! An agent's place, as the launcher writes it and the agent host answers from it
//! (docs/agent-control.md, 「One standing pane per agent」): the program to stand, the program standing,
//! and a start that failed. The field names are the TypeScript launcher's, which reads them back.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::disk::{publish, read_json};
use crate::identity::ProcessIdentity;
use crate::layout::name_component;

/// The variable a command names the file its program's error stream is appended to in: the head points
/// the program's error stream there as it starts it and hands the program its environment without it.
pub const STDERR_VARIABLE: &str = "BMO_PANE_STDERR";

/// A command as the launcher hands it to a pane.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResidentCommand {
  pub args: Vec<String>,
  pub cwd: PathBuf,
  #[serde(default)]
  pub env: BTreeMap<String, String>,
  /// The configuration file the program's login keeps, whose first-run screens are marked as passed and in which
  /// its folder is trusted by the agent host that starts it, on whichever machine that is (docs/work-nodes.md,
  /// 「The logins on a node」).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub login_file: Option<PathBuf>,
}

/// The program the launcher wrote into the place.
#[derive(Clone, Debug, Deserialize)]
pub struct ResidentProgram {
  pub identity: String,
  pub command: ResidentCommand,
  /// The same program started again, resuming the session the first command opened.
  #[serde(default)]
  pub restart: Option<ResidentCommand>,
  /// The record whose existence says that session is open.
  #[serde(default)]
  pub opened: Option<PathBuf>,
  /// The files its commands name that it cannot start without, as the launcher found them when it wrote the
  /// program (docs/agent-control.md, 「One standing pane per supervisor」).
  #[serde(default)]
  pub needs: Vec<PathBuf>,
}

impl ResidentProgram {
  /// The files the program needs that are not there.
  pub fn missing(&self) -> Vec<&Path> {
    self.needs.iter().map(PathBuf::as_path).filter(|path| !path.is_file()).collect()
  }
}

/// The program standing in the place: which identity, its process, the head that became it, and how
/// often it has been started.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningProgram {
  pub identity: String,
  pub pid: i32,
  pub birth: Option<String>,
  #[serde(rename = "loop")]
  pub head: ProcessIdentity,
  pub starts: u64,
  pub started_at: u64,
  #[serde(default)]
  pub pane_id: Option<String>,
  #[serde(default)]
  pub recent: Vec<u64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub pending_until: Option<u64>,
  /// Since when the program has rested: it started in a loop and is started no more until the place
  /// changes (docs/agent-control.md, 「One standing pane per supervisor」).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub resting_since: Option<u64>,
}

impl RunningProgram {
  fn program_alive(&self) -> bool {
    self.birth.as_ref().is_some_and(|birth| {
      ProcessIdentity {
        pid: self.pid,
        birth: birth.clone(),
      }
      .alive()
    })
  }
}

/// A program that could not be started at all.
#[derive(Serialize)]
pub struct StartFailure<'a> {
  pub identity: &'a str,
  pub at: u64,
  pub reason: &'a str,
}

// How far back the starts of a place are remembered; the starts within a minute that go ahead at
// once; and the bounds of the wait after them.
const REMEMBERED_MS: u64 = 5 * 60_000;
const FREE_STARTS: u32 = 3;
const FIRST_WAIT_MS: u64 = 2_000;
const LONGEST_WAIT_MS: u64 = 30_000;
/// The starts within the remembered window that make a program one in a loop.
pub const LOOPING_STARTS: usize = 6;
/// How long a program written into its place waits for a file it needs before the place records that it does
/// not start: long enough for a node's link to bring the files a program names after the program itself.
pub const FILES_WAIT_MS: u64 = 30_000;

/// The starts of the program `identity` names still remembered at `now`. A start on another program than
/// the one that ran last is a command written in its place, not the same program starting again, so its
/// history begins afresh.
pub fn recent_starts(running: Option<&RunningProgram>, identity: &str, now: u64) -> Vec<u64> {
  running.filter(|running| running.identity == identity).map_or_else(Vec::new, |running| {
    running
      .recent
      .iter()
      .copied()
      .filter(|at| now.saturating_sub(*at) < REMEMBERED_MS)
      .collect()
  })
}

/// Whether starts remembered at `now` make a program one in a loop, which is started no more.
pub fn in_a_loop(recent: &[u64], now: u64) -> bool {
  recent.iter().filter(|at| now.saturating_sub(**at) < REMEMBERED_MS).count() >= LOOPING_STARTS
}

/// How long to wait before starting again: nothing for the first three starts within a minute, then
/// twice as long each time from two seconds, up to thirty.
pub fn restart_delay(recent: &[u64], now: u64) -> u64 {
  let last_minute = recent.iter().filter(|at| now.saturating_sub(**at) < 60_000).count() as u32;

  if last_minute < FREE_STARTS {
    return 0;
  }

  FIRST_WAIT_MS
    .saturating_mul(1u64 << (last_minute - FREE_STARTS).min(16))
    .min(LONGEST_WAIT_MS)
}

/// One agent's place under a data root.
pub struct Place(pub PathBuf);

impl Place {
  pub fn of(data_root: &Path, agent: &str) -> Self {
    Self::named(data_root, &name_component(agent))
  }

  /// The place whose directory is `dir`, which is its agent's name as a path component.
  pub fn named(data_root: &Path, dir: &str) -> Self {
    Self(data_root.join("agent-places").join(dir))
  }

  pub fn program(&self) -> Option<ResidentProgram> {
    read_json(&self.0.join("program.json"))
  }

  pub fn running(&self) -> Option<RunningProgram> {
    read_json(&self.0.join("running.json"))
  }

  pub fn record_running(&self, running: &RunningProgram) -> std::io::Result<()> {
    publish(&self.0.join("running.json"), running)
  }

  /// Whether the program written in the place rests: the place's record says the program of that
  /// identity started in a loop, and nothing has changed the place since.
  pub fn resting(&self) -> bool {
    match (self.program(), self.running()) {
      (Some(program), Some(running)) => {
        running.resting_since.is_some() && running.identity == program.identity
      }
      _ => false,
    }
  }

  pub fn record_failure(&self, failure: &StartFailure) -> std::io::Result<()> {
    publish(&self.0.join("start-failed.json"), failure)
  }

  /// Whether the place's start failure already says `reason` of the program `identity` names.
  pub fn failure_holds(&self, identity: &str, reason: &str) -> bool {
    read_json::<serde_json::Value>(&self.0.join("start-failed.json")).is_some_and(|failed| {
      failed["identity"].as_str() == Some(identity) && failed["reason"].as_str() == Some(reason)
    })
  }

  /// When the program file was last written, in milliseconds since the epoch.
  pub fn program_written_at(&self) -> Option<u64> {
    let modified = std::fs::metadata(self.0.join("program.json")).ok()?.modified().ok()?;

    modified
      .duration_since(std::time::UNIX_EPOCH)
      .ok()
      .map(|since| since.as_millis() as u64)
  }

  /// Whether a live head other than `me` holds the program: running it, or waiting out its bound
  /// before it does.
  pub fn held_elsewhere(&self, me: Option<&ProcessIdentity>, now: u64) -> bool {
    let Some(running) = self.running() else {
      return false;
    };

    if me == Some(&running.head) || !running.head.alive() {
      return false;
    }

    running.program_alive() || running.pending_until.is_some_and(|until| until > now)
  }
}

/// What to start, and how many times this identity has been started before.
pub struct NextStart {
  pub identity: String,
  pub command: ResidentCommand,
  pub starts: u64,
}

/// The first start of an identity takes its command and every start after it the restart, once the
/// session the first one opened exists.
pub fn next_start(program: &ResidentProgram, before: Option<&RunningProgram>) -> NextStart {
  let starts = before
    .filter(|running| running.identity == program.identity)
    .map_or(0, |running| running.starts);
  let resumable = program.opened.as_ref().is_none_or(|opened| opened.exists());
  let command = match &program.restart {
    Some(restart) if starts > 0 && resumable => restart.clone(),
    _ => program.command.clone(),
  };

  NextStart {
    identity: program.identity.clone(),
    command,
    starts,
  }
}
