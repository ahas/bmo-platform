//! `pane-head <agent>`: what a head pane's loop starts (docs/agent-control.md, 「One standing pane per
//! agent」). It asks the agent host of its data root what the pane is to become, and replaces itself
//! with that: the agent's program, or a follower of the agent's view. It installs no signal handler,
//! so whatever ends it before its `exec` is an end like any other, and the loop asks again.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::Duration;

use agent_host::ask::{Answer, Request, ask};
use agent_host::layout::Layout;
use agent_host::place::STDERR_VARIABLE;

/// The one exit the pane's loop does not start the head again on.
const HEAD_STOPPED: u8 = 3;

/// The variable a head opened for a host is started with.
const HOST_HEAD: &str = "BMO_HOST_HEAD";

// What says which terminal the pane is, laid over the program's own environment: the program draws
// on this pane's terminal, which nothing that wrote the program knows.
const TERMINAL_VARIABLES: [&str; 4] = ["TERM", "COLORTERM", "TERM_PROGRAM", "TERM_PROGRAM_VERSION"];

// The prefix of what herdr sets for its pane — the pane, tab and workspace, its socket and binary, and the
// mark of running inside it — laid over the program's own environment, so the program reports its state
// to herdr.
const HERDR_PREFIX: &str = "HERDR_";

// How much of the view a follower shows as it starts.
const VIEW_LINES: &str = "200";

fn data_root() -> Option<PathBuf> {
  std::env::var_os("BMO_DATA_DIR")
    .filter(|root| !root.is_empty())
    .map(PathBuf::from)
    .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share/bmo/data")))
}

/// The program's environment, with the pane's terminal and herdr's variables for the pane over it.
fn program_environment(mut env: BTreeMap<String, String>) -> BTreeMap<String, String> {
  for name in TERMINAL_VARIABLES {
    if let Some(value) = std::env::var(name).ok().filter(|value| !value.is_empty()) {
      env.insert(name.to_owned(), value);
    }
  }

  for (name, value) in std::env::vars() {
    if name.starts_with(HERDR_PREFIX) && !value.is_empty() {
      env.insert(name, value);
    }
  }

  env
}

/// The file a command names its program's error stream to go to, opened to append and private to its
/// owner; none where it names none or the file cannot be opened, and then the stream stays the pane's.
fn error_stream(env: &mut BTreeMap<String, String>) -> Option<File> {
  let path = env.remove(STDERR_VARIABLE).filter(|path| !path.is_empty())?;

  OpenOptions::new().create(true).append(true).mode(0o600).open(path).ok()
}

/// What a start that failed says, naming the executable and the path it was looked for on.
fn start_failure(program: &str, cwd: &Path, path: &str, why: &io::Error) -> String {
  if !cwd.is_dir() {
    return format!("{program}: the directory {} is not there", cwd.display());
  }

  match why.kind() {
    io::ErrorKind::NotFound => format!("{program}: no such executable on PATH={path}"),
    io::ErrorKind::PermissionDenied => format!("{program}: not executable (PATH={path})"),
    _ => format!("{program}: {why} (PATH={path})"),
  }
}

/// What the host answered this head, under the agent's place: a pane whose program never stands leaves its
/// answer behind rather than on a screen that is gone. A program's environment is never written.
fn note_answer(root: &Path, agent: &str, answer: &Answer) {
  let note = match answer {
    Answer::Program {
      identity,
      args,
      wait_ms,
      ..
    } => format!(
      "program {} {identity} wait_ms={wait_ms}",
      args.first().map(String::as_str).unwrap_or("")
    ),
    Answer::View { path } => format!("view {}", path.display()),
    Answer::LetGo { reason } => format!("let-go {reason}"),
    Answer::Refused { reason } => format!("refused {reason}"),
    Answer::Recorded => "recorded".to_owned(),
  };
  let path = root.join("agent-places").join(agent).join("head-answer.txt");

  let _ = std::fs::write(path, format!("{} {note}\n", std::process::id()));
}

fn main() -> ExitCode {
  let Some(agent) = std::env::args().nth(1).filter(|agent| !agent.is_empty()) else {
    eprintln!("usage: pane-head <agent>");
    return ExitCode::from(2);
  };
  let Some(root) = data_root() else {
    println!("{agent}: no data root: set BMO_DATA_DIR or HOME");
    return ExitCode::SUCCESS;
  };
  let socket = Layout::new(&root).socket();
  let pid = std::process::id() as i32;
  let request = Request::Ask {
    agent: agent.clone(),
    pane: std::env::var("HERDR_PANE_ID").ok().filter(|pane| !pane.is_empty()),
    pid,
    host_head: std::env::var(HOST_HEAD).is_ok_and(|flag| flag == "1"),
  };

  // The loop asks again a second after this ends, so a host that does not answer only costs a line.
  let answer = match ask(&socket, &request) {
    Ok(answer) => answer,
    Err(why) => {
      println!("{agent}: the agent host at {} does not answer: {why}", socket.display());
      return ExitCode::SUCCESS;
    }
  };

  note_answer(&root, &agent, &answer);

  match answer {
    Answer::Program {
      identity,
      args,
      cwd,
      env,
      wait_ms,
    } => {
      let Some((program, rest)) = args.split_first() else {
        println!("{agent}: the program written for it names nothing to run");
        return ExitCode::SUCCESS;
      };

      if wait_ms > 0 {
        println!("{agent}: started often lately; starting again in {}s", wait_ms.div_ceil(1000));
        thread::sleep(Duration::from_millis(wait_ms));
      }

      let mut env = program_environment(env);
      let errors = error_stream(&mut env);
      let path = env.get("PATH").cloned().unwrap_or_default();
      let mut command = Command::new(program);

      command.args(rest).current_dir(&cwd).env_clear().envs(&env);

      if let Some(errors) = errors {
        command.stderr(errors);
      }

      let why = command.exec();
      let reason = start_failure(program, &cwd, &path, &why);
      let report = Request::Failed {
        agent: agent.clone(),
        pid,
        identity,
        reason: reason.clone(),
      };

      if let Err(why) = ask(&socket, &report) {
        println!("{agent}: the failed start could not be reported: {why}");
      }

      println!("{agent}: the program could not be started — {reason}");
      ExitCode::SUCCESS
    }
    Answer::View { path } => {
      let why = Command::new("tail").args(["-n", VIEW_LINES, "-F"]).arg(&path).exec();

      println!("{agent}: its view could not be followed: {why}");
      ExitCode::SUCCESS
    }
    Answer::LetGo { reason } => {
      println!("{agent}: letting go of this pane: {reason}");
      ExitCode::from(HEAD_STOPPED)
    }
    Answer::Recorded => ExitCode::SUCCESS,
    Answer::Refused { reason } => {
      println!("{agent}: the agent host refused: {reason}");
      ExitCode::SUCCESS
    }
  }
}
