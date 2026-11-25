//! The one place a Rust process speaks to herdr (docs/agent-control.md, 「herdr」), the twin of the
//! TypeScript client in the agent package. herdr is a JSON-RPC server on a Unix socket: one request is
//! one line on a connection of its own, one answer is one line back, and a connection carries exactly
//! one request. The subscription is the one connection herdr keeps open to write on (`picture`).
//!
//! What a run is called is a token on its pane: the launcher reports the run's own name on the pane it
//! opened, and liveness reads the tokens back.

pub mod picture;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// The token a pane's run name is reported under.
pub const RUN_TOKEN: &str = "run";

/// What a request too long for herdr is refused as, before it is sent.
pub const TOO_LARGE: &str = "too_large";

/// The longest request line herdr reads, its newline aside (measured on herdr 0.9.3): a longer one has its
/// connection dropped with nothing answered.
pub const LINE_CAP_BYTES: usize = 1_048_576;

/// The one error code that is not a failure: an agent waiting on a person refused the words with its
/// pane left as it was.
pub const AGENT_BLOCKED: &str = "agent_blocked";

/// The code of a call another machine's herdr left unanswered past its ceiling.
pub const SILENT: &str = "silent";

// The pauses before a request that never reached herdr is sent again, about two seconds in all.
const RESEND_PAUSES: [Duration; 3] =
  [Duration::from_millis(250), Duration::from_millis(600), Duration::from_millis(1_200)];

// How long a call goes unanswered, where its caller names no other span, before it is written down as
// one of herdr's silences. The call goes on waiting for its answer.
const SILENT_AFTER: Duration = Duration::from_secs(5);

// Past this size the record of unanswered calls starts over beside its last stretch.
const SILENCES_CAP_BYTES: u64 = 256 * 1024;

/// What herdr answered where it did not answer with a result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fault {
  pub code: String,
  pub message: String,
}

impl Fault {
  fn new(code: &str, message: impl Into<String>) -> Self {
    Self {
      code: code.to_owned(),
      message: message.into(),
    }
  }

  /// Whether the call never went out, or its connection closed before an answer: only these may be
  /// sent again.
  fn connection(&self) -> bool {
    matches!(self.code.as_str(), "unreachable" | "closed")
  }
}

impl std::fmt::Display for Fault {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(formatter, "{}: {}", self.code, self.message)
  }
}

/// Where herdr's socket is: under `XDG_CONFIG_HOME`, or the home's `.config`, the only two things that
/// move it. Nothing where neither is known, so a process that names no home reaches no herdr.
pub fn socket_path(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
  let root = match xdg_config_home.filter(|named| !named.is_empty()) {
    Some(named) => PathBuf::from(named),
    None => PathBuf::from(home.filter(|named| !named.is_empty())?).join(".config"),
  };

  Some(root.join("herdr").join("herdr.sock"))
}

/// The record every process on the machine writes the calls herdr left unanswered into, which the
/// machine watch reads: `BMO_HERDR_SILENCES`, or the shared temporary directory's. The default name is
/// the one every writer and the watch agree on, so it stays as they read it.
pub fn silences_path(named: Option<&str>) -> PathBuf {
  named
    .filter(|named| !named.is_empty())
    .map_or_else(|| std::env::temp_dir().join("bmo-herdr-silences.jsonl"), PathBuf::from)
}

/// What a pane read reads, by the words herdr's `pane.read` takes for it: what the pane shows now, its latest
/// lines as they wrapped, or its latest lines with the soft wraps joined again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadSource {
  Visible,
  Recent,
  RecentUnwrapped,
}

/// One herdr: its socket, and, for this machine's own, where an unanswered call is written down; for another
/// machine's, how long a call is waited on before it is answered as herdr silent.
#[derive(Clone, Debug)]
pub struct Herdr {
  socket: PathBuf,
  silences: Option<PathBuf>,
  ceiling: Option<Duration>,
}

impl Herdr {
  /// This machine's herdr, whose silences are the machine's to count.
  pub fn here(socket: PathBuf, silences: PathBuf) -> Self {
    Self {
      socket,
      silences: Some(silences),
      ceiling: None,
    }
  }

  /// A herdr another machine holds — a work node's, through its link — whose silences say nothing of
  /// this machine.
  pub fn there(socket: PathBuf) -> Self {
    Self {
      socket,
      silences: None,
      ceiling: None,
    }
  }

  /// The same herdr, a call to which unanswered for `ceiling` is answered `silent`: a node's herdr reached
  /// through a link that can stand with nothing coming through it.
  pub fn within(self, ceiling: Duration) -> Self {
    Self {
      ceiling: Some(ceiling),
      ..self
    }
  }

  pub fn socket(&self) -> &Path {
    &self.socket
  }

  /// A pane's text as the source reads it, its latest `lines` where it reads latest lines, or what herdr said
  /// instead.
  pub async fn read_pane(
    &self,
    pane: &str,
    source: ReadSource,
    lines: Option<u32>,
  ) -> Result<String, Fault> {
    let mut params = json!({ "pane_id": pane, "source": source });

    if let Some(lines) = lines {
      params["lines"] = json!(lines);
    }

    let answer = self.call("pane.read", params, None).await?;

    answer
      .pointer("/read/text")
      .and_then(Value::as_str)
      .map(str::to_owned)
      .ok_or_else(|| Fault::new("unreadable", "herdr answered the read with no text".to_owned()))
  }

  /// One request, answered with its result or what herdr said instead, however long this machine's herdr
  /// takes, and within its ceiling for another machine's: its end is the answer, a refused connection, one
  /// that closes, or for another machine's herdr `silent` once the ceiling passes. A request that never reached herdr is
  /// sent again a few times over about two seconds; one written whole is never sent again, since herdr
  /// may have acted on it, and nor is one to a socket that is not there at all. One longer than herdr
  /// reads is refused without being sent. A call unanswered for `silent_after` — five seconds where
  /// none is named — is written down as a silence of herdr's, and goes on waiting.
  pub async fn call(
    &self,
    method: &str,
    params: Value,
    silent_after: Option<Duration>,
  ) -> Result<Value, Fault> {
    let id = format!("agency-{}", now_ms());

    let mut pauses = RESEND_PAUSES.iter();

    loop {
      let (answer, delivered) =
        self.send_once(&id, method, &params, silent_after.unwrap_or(SILENT_AFTER)).await;

      match answer {
        Err(fault) if !delivered && fault.connection() && self.socket.exists() => match pauses.next() {
          Some(pause) => tokio::time::sleep(*pause).await,
          None => return Err(fault),
        },
        answer => return answer,
      }
    }
  }

  async fn send_once(
    &self,
    id: &str,
    method: &str,
    params: &Value,
    silent_after: Duration,
  ) -> (Result<Value, Fault>, bool) {
    let line = json!({ "id": id, "method": method, "params": params }).to_string();

    if line.len() > LINE_CAP_BYTES {
      let why =
        format!("the {method} request is {} bytes, past herdr's {LINE_CAP_BYTES}-byte line", line.len());

      return (Err(Fault::new(TOO_LARGE, why)), false);
    }

    let line = format!("{line}\n");
    let exchange = async {
      let mut stream = UnixStream::connect(&self.socket)
        .await
        .map_err(|why| Fault::new("unreachable", why.to_string()))?;

      stream
        .write_all(line.as_bytes())
        .await
        .map_err(|why| Fault::new("unreachable", why.to_string()))?;

      let mut answer = String::new();
      let read = BufReader::new(stream)
        .read_line(&mut answer)
        .await
        .map_err(|why| Fault::new("closed", why.to_string()))?;

      if read == 0 || !answer.ends_with('\n') {
        return Err(Fault::new("closed", format!("herdr closed before answering {method}")));
      }

      read_frame(answer.trim_end())
    };

    let mut exchange = std::pin::pin!(exchange);
    let answer = tokio::select! {
      answer = &mut exchange => answer,
      () = tokio::time::sleep(silent_after) => {
        self.note_silence(method);

        match self.ceiling {
          Some(ceiling) => tokio::time::timeout(ceiling.saturating_sub(silent_after), exchange)
            .await
            .unwrap_or_else(|_| {
              let seconds = ceiling.as_secs();

              Err(Fault::new(SILENT, format!("herdr did not answer {method} within {seconds} seconds")))
            }),
          None => exchange.await,
        }
      }
    };

    // The line went out whole unless the connection or the write failed, which is all `unreachable` says.
    let delivered = !matches!(&answer, Err(fault) if fault.code == "unreachable");

    (answer, delivered)
  }

  // An unanswered call written down for the machine, this machine's herdr alone. A record that cannot
  // be written costs nothing but itself.
  fn note_silence(&self, method: &str) {
    let Some(path) = &self.silences else { return };

    if std::fs::metadata(path).is_ok_and(|meta| meta.len() > SILENCES_CAP_BYTES) {
      let mut old = path.as_os_str().to_owned();

      old.push(".old");
      let _ = std::fs::rename(path, old);
    }

    let written = std::fs::OpenOptions::new()
      .create(true)
      .append(true)
      .open(path)
      .and_then(|mut file| writeln!(file, "{}", json!({ "at": now_ms(), "method": method })));

    drop(written);
  }

  /// What herdr holds now, as a snapshot.
  pub async fn snapshot(&self) -> Result<Snapshot, Fault> {
    let answer = self.call("session.snapshot", json!({}), None).await?;

    serde_json::from_value(answer.get("snapshot").cloned().unwrap_or(Value::Null))
      .map_err(|why| Fault::new("unreadable", why.to_string()))
  }

  /// What herdr started a pane with, read off its tab's arrangement; nothing for a pane holding only a
  /// shell, or one herdr could not describe.
  pub async fn command_of(&self, pane: &str) -> Vec<String> {
    match self.call("layout.export", json!({ "pane_id": pane }), None).await {
      Ok(answer) => {
        let root: Option<LayoutNode> = answer
          .pointer("/layout/root")
          .cloned()
          .and_then(|root| serde_json::from_value(root).ok());

        command_of_pane(root.as_ref(), pane)
      }
      Err(_) => vec![],
    }
  }
}

/// One line off the socket read into an answer: a line that is not JSON is herdr saying something
/// nobody here understands, which is a failure of its own.
pub fn read_frame(line: &str) -> Result<Value, Fault> {
  let frame: Value = serde_json::from_str(line)
    .map_err(|_| Fault::new("unreadable", line.chars().take(200).collect::<String>()))?;

  match frame.get("error").filter(|error| !error.is_null()) {
    Some(error) => Err(Fault::new(
      error.get("code").and_then(Value::as_str).unwrap_or("unknown"),
      error.get("message").and_then(Value::as_str).unwrap_or_default(),
    )),
    None => Ok(frame.get("result").cloned().unwrap_or(Value::Null)),
  }
}

fn now_ms() -> u128 {
  SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis())
}

/// What a snapshot says of the world, as much as anything here reads.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Snapshot {
  #[serde(default)]
  pub workspaces: Vec<Workspace>,
  #[serde(default)]
  pub panes: Vec<Pane>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Workspace {
  pub workspace_id: Option<String>,
  /// Its place in herdr's list, from one: the order every reader of one snapshot agrees on.
  pub number: Option<i64>,
  pub label: Option<String>,
  pub tokens: Option<serde_json::Map<String, Value>>,
}

impl Workspace {
  /// What a token on the workspace says, where it says anything.
  pub fn token(&self, name: &str) -> Option<&str> {
    self.tokens.as_ref()?.get(name)?.as_str()
  }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Pane {
  pub pane_id: Option<String>,
  pub workspace_id: Option<String>,
  pub tab_id: Option<String>,
  pub label: Option<String>,
  pub tokens: Option<serde_json::Map<String, Value>>,
  /// The folder the pane's process stands in, as herdr reads it off the process.
  pub cwd: Option<String>,
}

// What the kernel appends to a process's folder once that folder has been removed, which herdr hands on as
// it reads it.
const REMOVED_MARK: &str = " (deleted)";

impl Pane {
  /// The run the pane carries, or nothing for a pane that is not one.
  pub fn run(&self) -> Option<&str> {
    self.tokens.as_ref()?.get(RUN_TOKEN)?.as_str().filter(|name| !name.is_empty())
  }

  /// Whether the folder the pane's process stands in has been removed.
  pub fn cwd_gone(&self) -> bool {
    self.cwd.as_deref().is_some_and(|cwd| cwd.ends_with(REMOVED_MARK))
  }
}

/// A tab's arrangement as `layout.export` answers it.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct LayoutNode {
  pub pane_id: Option<String>,
  pub command: Option<Vec<String>>,
  pub first: Option<Box<LayoutNode>>,
  pub second: Option<Box<LayoutNode>>,
}

/// What herdr started a pane with, found in its tab's tree: the root of a split has no command.
pub fn command_of_pane(root: Option<&LayoutNode>, pane: &str) -> Vec<String> {
  let Some(root) = root else { return vec![] };

  if root.pane_id.as_deref() == Some(pane) {
    return root.command.clone().unwrap_or_default();
  }

  let first = command_of_pane(root.first.as_deref(), pane);

  if first.is_empty() { command_of_pane(root.second.as_deref(), pane) } else { first }
}
