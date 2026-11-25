//! Who may put words into a pane's Claude Code session through this door (docs/live-claude-guard.md,
//! 「Second layer: the guard in agency's code」). The door is agency's own and runs in its service,
//! which carries neither of a job's two marks; a test reaches only a stand-in herdr under its own home
//! or the temporary directory; and a job's own process is refused, since the opening a job makes for
//! itself is read by the TypeScript guard (the agent package's `live-claude-guard.ts`) alone, but for a run
//! a procedure requires, which its verb declares first. Claude Code switched off closes every turn on a
//! Claude Code program the door would put in, and none on a program standing on a registration's road.
//!
//! The marks are read off the machine the caller hands in, its ground's: the process's own environment
//! in a service or a verb, and the one a test stands in for its ground, which names the test's home and
//! carries neither of a job's marks.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use harness_providers::Machine;
use harness_providers::registered::{registered_road, road_home};
use policy::door_words::DOOR_WORDS;
use sea_orm::DatabaseConnection;

use crate::control::harness::provider_enabled;

const TEST_HOME: &str = "BMO_TEST_HOME";
const SESSION_MARK: &str = "CLAUDECODE";
const JOB_RUNNER_MARKER: &str = "BMO_TASK_LOCKED";

// A run a procedure requires, said by the verb that makes it before it makes it.
static DECLARED: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// A run a procedure requires — the fold a prefix change asks for after a deployment — said by the verb
/// that makes it before it makes it, which lets it through from inside a job as the TypeScript guard lets a
/// declared run through. The files that may say so are held by a test (docs/live-claude-guard.md).
pub fn declare_required_run(purpose: &str) {
  let _ = DECLARED.set(purpose.to_owned());
}

fn declared() -> bool {
  DECLARED.get().is_some()
}

fn within(path: &Path, dir: &Path) -> bool {
  let absolute = |path: &Path| std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());

  absolute(path).starts_with(absolute(dir))
}

/// Whether the machine given is a test's, its environment naming a test's home: such a process reaches no user, unit
/// or firewall of this machine (docs/account-isolation.md, 「Taking it down」).
pub fn names_test_home(machine: &Machine) -> bool {
  machine.var(TEST_HOME).is_some()
}

/// Why a process on this machine may not reach the herdr at `socket`, or nothing.
pub fn socket_refusal(socket: &Path, machine: &Machine) -> Option<String> {
  let home = PathBuf::from(machine.var(TEST_HOME)?);

  (!within(socket, &home) && !within(socket, &std::env::temp_dir())).then(|| {
    format!(
      "a test was about to reach the herdr at {}; a test reaches only a stand-in under its own home or the temporary directory (docs/live-claude-guard.md)",
      socket.display()
    )
  })
}

/// Why a process on this machine may not put words into a pane's Claude Code session, or nothing.
pub fn prompt_refusal(machine: &Machine) -> Option<String> {
  if machine.var(TEST_HOME).is_some()
    || machine.var(SESSION_MARK).is_none()
    || machine.var(JOB_RUNNER_MARKER).is_none()
    || declared()
  {
    return None;
  }

  Some(
    "refused: this process runs inside a job's Claude Code session, and agency's server's resident door puts no job's words into a pane (docs/live-claude-guard.md)"
      .to_owned(),
  )
}

// The lines whose program, Claude Code, opens a session that writes a whole prefix to the cache.
const SESSION_LINES: [&str; 6] = ["worker", "chat", "search", "browse", "plain", "pane"];

// The programs a launch is judged by: Claude Code itself, and herdr, which stands one in a pane.
const GUARDED: [&str; 2] = ["claude", "herdr"];

fn basename(path: &str) -> &str {
  path.rsplit('/').next().unwrap_or(path)
}

// Where a program is found on a search path, as a shell finds it: the first executable file of that name.
fn which(program: &str, path: &str) -> Option<PathBuf> {
  use std::os::unix::fs::PermissionsExt;

  if program.contains('/') {
    return Some(PathBuf::from(program));
  }

  path
    .split(':')
    .filter(|dir| !dir.is_empty())
    .map(|dir| Path::new(dir).join(program))
    .find(|found| {
      std::fs::metadata(found).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    })
}

/// Why a process on this machine may not start this command, with the environment it is started with, on a
/// line, or nothing: a test starts only a stand-in under its own home or the temporary directory, never the
/// machine's own program, and a job's own process opens no Claude Code session on a line that keeps one,
/// since the opening a job makes for itself is read by the TypeScript guard alone.
pub fn launch_refusal(
  line: &str,
  args: &[String],
  env: &BTreeMap<String, String>,
  machine: &Machine,
) -> Option<String> {
  let program = args.iter().find(|part| GUARDED.contains(&basename(part)))?;

  if let Some(home) = machine.var(TEST_HOME) {
    let path = env.get("PATH").map(String::as_str).or_else(|| machine.var("PATH")).unwrap_or_default();
    let found = which(program, path)?;

    if within(&found, Path::new(home)) || within(&found, &std::env::temp_dir()) {
      return None;
    }

    return Some(format!(
      "a test was about to start {}, the machine's own {}; a test starts only the stand-in in its own home or a fake of its own (docs/live-claude-guard.md)",
      found.display(),
      basename(program)
    ));
  }

  if basename(program) != GUARDED[0]
    || !SESSION_LINES.contains(&line)
    || machine.var(SESSION_MARK).is_none()
    || machine.var(JOB_RUNNER_MARKER).is_none()
    || declared()
  {
    return None;
  }

  Some(format!(
    "refused: this process runs inside a job's Claude Code session, and agency's server starts no Claude Code session on the {line} line for a job (docs/live-claude-guard.md)"
  ))
}

/// Why a turn on a program of this environment may not go in now, or nothing.
pub async fn turn_refusal(
  db: &DatabaseConnection,
  machine: &Machine,
  work_key: &str,
  env: &BTreeMap<String, String>,
) -> Option<String> {
  if road_home(machine, env).is_none()
    && !registered_road(machine, env)
    && !provider_enabled(db, "claude-code").await.unwrap_or(false)
  {
    return Some(format!(
      "{work_key}: selected claude-code route did not answer: {}",
      DOOR_WORDS.provider_off
    ));
  }

  prompt_refusal(machine)
}
