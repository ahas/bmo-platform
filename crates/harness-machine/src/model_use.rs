//! A model's use on its engine (docs/harness-machine.md, 「A model's use on its engine」): the user
//! unit a choice runs in, the engine's installer it runs with the choice's log, and the end of that log
//! a screen is shown. Nothing here knows an engine but by its installer's name.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Serialize;
use std::io::Write;
use std::sync::Mutex;

use tokio::io::{AsyncRead, AsyncReadExt};

// The lines of a choice's log a screen is shown.
const TAIL_LINES: usize = 20;

/// The user unit a choice for one engine runs as, one at a time per engine.
pub fn unit(engine: &str) -> String {
  format!("bmo-model-use-{engine}")
}

/// What a choice's unit is started with: the choice, the archive it was made in, and the machine its
/// caller stands on.
#[derive(Clone, Debug)]
pub struct Start {
  pub engine: String,
  pub use_id: i64,
  /// The archive's place, handed to the catalogue verb as the archive to carry the choice out in.
  pub archive: String,
  /// The PATH the run is handed.
  pub path: String,
  pub source_root: PathBuf,
  /// The tree the catalogue verb is run out of: its verbs' binary.
  pub code_root: PathBuf,
}

impl Start {
  /// The command line that starts the unit.
  pub fn line(&self) -> Vec<String> {
    vec![
      "systemd-run".into(),
      "--user".into(),
      "--quiet".into(),
      "--collect".into(),
      format!("--unit={}", unit(&self.engine)),
      format!("--setenv=PATH={}", self.path),
      format!("--setenv=BMO_SOURCE_DIR={}", self.source_root.display()),
      format!("--working-directory={}", self.code_root.display()),
      self.code_root.join("bin").join("bmo-agency").display().to_string(),
      "model".into(),
      "run-use".into(),
      self.use_id.to_string(),
      "--db".into(),
      self.archive.clone(),
    ]
  }

  /// The unit started, or what systemd said where it was not.
  pub fn run(&self) -> Result<(), String> {
    let line = self.line();
    let mut command = std::process::Command::new(&line[0]);

    command.args(&line[1..]).stdin(Stdio::null());
    with_user_bus(&mut command);

    match command.output() {
      Ok(done) if done.status.success() => Ok(()),
      Ok(done) => {
        let said =
          format!("{}{}", String::from_utf8_lossy(&done.stdout), String::from_utf8_lossy(&done.stderr));

        Err(said.trim().to_owned())
      }
      Err(error) => Err(error.to_string()),
    }
  }
}

/// A spawn that asks systemd anything reaches the user's own manager: the runtime directory named
/// where it is missing, and a disabled session bus address taken off.
pub fn with_user_bus(command: &mut std::process::Command) {
  if std::env::var_os("XDG_RUNTIME_DIR").is_none_or(|dir| dir.is_empty()) {
    // SAFETY: getuid has no preconditions and cannot fail.
    command.env("XDG_RUNTIME_DIR", format!("/run/user/{}", unsafe { libc::getuid() }));
  }

  if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some_and(|address| address == "disabled:") {
    command.env_remove("DBUS_SESSION_BUS_ADDRESS");
  }
}

/// How an installer's run ended: whether well, and the last line it wrote that was not empty.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Installed {
  pub ok: bool,
  pub last: String,
}

/// What an installer is run with.
#[derive(Clone, Debug)]
pub struct Install {
  pub engine: String,
  pub entry: String,
  pub log: PathBuf,
  /// The archive the choice was made in, which the installer reads the entry out of.
  pub archive: String,
  pub source_root: PathBuf,
  pub bun: PathBuf,
}

// The last line a stretch of output carries that is not empty, where it carries one.
fn last_line(chunk: &str) -> Option<&str> {
  chunk.trim_end().split('\n').rfind(|line| !line.trim().is_empty())
}

/// How much of the bytes held decodes now: all of them, but for a character still arriving at their end.
pub fn decodable(held: &[u8]) -> usize {
  match std::str::from_utf8(held) {
    Err(fault) if fault.error_len().is_none() => fault.valid_up_to(),
    _ => held.len(),
  }
}

// One stretch of output written into the log, naming its last line.
fn take(text: &str, log: &Mutex<std::fs::File>, last: &Mutex<String>) -> std::io::Result<()> {
  let mut file = log.lock().expect("the log is never held across a panic");

  file.write_all(text.as_bytes())?;
  file.flush()?;
  drop(file);

  if let Some(line) = last_line(text) {
    *last.lock().expect("the last line is never held across a panic") = line.to_owned();
  }

  Ok(())
}

// One stream copied into the log as it comes, decoded as a stream so a character split between two
// reads is written whole.
async fn copy(
  mut stream: impl AsyncRead + Unpin,
  log: &Mutex<std::fs::File>,
  last: &Mutex<String>,
) -> std::io::Result<()> {
  let mut buffer = vec![0; 8192];
  let mut held = vec![];

  loop {
    let read = stream.read(&mut buffer).await?;

    if read == 0 {
      return if held.is_empty() { Ok(()) } else { take(&String::from_utf8_lossy(&held), log, last) };
    }

    held.extend_from_slice(&buffer[..read]);

    let whole = decodable(&held);

    if whole > 0 {
      take(&String::from_utf8_lossy(&held[..whole]), log, last)?;
      held.drain(..whole);
    }
  }
}

impl Install {
  /// The engine's installer in the gate's training tooling (`training/tooling/scripts/` under the source
  /// root), run with the entry's name, its output written into the log. An engine with no installer is a
  /// failure naming its path.
  pub async fn run(&self) -> std::io::Result<Installed> {
    let tooling = self.source_root.join("gate").join("training").join("tooling");
    let script = Path::new("scripts").join(format!("{}-install.ts", self.engine));

    if !tooling.join(&script).exists() {
      return Ok(Installed {
        ok: false,
        last: format!(
          "The {} engine has no installer at {}.",
          self.engine,
          Path::new("gate").join("training").join("tooling").join(&script).display()
        ),
      });
    }

    if let Some(folder) = self.log.parent() {
      std::fs::create_dir_all(folder)?;
    }

    let log = Mutex::new(std::fs::File::create(&self.log)?);
    let last = Mutex::new(String::new());
    let mut child = tokio::process::Command::new(&self.bun)
      .arg(&script)
      .args(["--model", &self.entry])
      .current_dir(&tooling)
      .env("BMO_DB_PATH", &self.archive)
      .stdin(Stdio::null())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .kill_on_drop(true)
      .spawn()?;
    let stdout = child.stdout.take().expect("the output was piped");
    let stderr = child.stderr.take().expect("the error was piped");
    let (out, err, status) =
      tokio::join!(copy(stdout, &log, &last), copy(stderr, &log, &last), child.wait());

    out?;
    err?;

    Ok(Installed {
      ok: status?.success(),
      last: last.into_inner().expect("the last line is never held across a panic"),
    })
  }
}

/// The end of a choice's log a screen is shown, none where it has no log.
pub fn log_tail(log: Option<&Path>) -> Vec<String> {
  let Some(text) = log.and_then(|path| std::fs::read_to_string(path).ok()) else {
    return vec![];
  };
  let lines: Vec<&str> = text.trim_end().split('\n').collect();

  lines[lines.len().saturating_sub(TAIL_LINES)..]
    .iter()
    .map(|line| (*line).to_owned())
    .collect()
}
