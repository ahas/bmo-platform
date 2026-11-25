//! Codex's subscription windows (docs/usage-windows.md, 「The Codex reader」). Codex's turns carry the token
//! split but not the windows, which live on the app-server's account API; the newest rollout carries the
//! windows the last request saw, which is read instead where a turn wrote it lately. Only the public
//! meter fields are kept: no account id, credit balance or plan detail, and no Codex process outlives a
//! reading.
//!
//! The app-server is spoken to in JSON-RPC lines over its standard input and output. Each line is made and
//! read with serde_json; the exchange around them — three requests written, answers read until the one
//! asked for — is written here, since no maintained crate speaks the app-server's line protocol over a
//! child's pipes.

use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use policy::providers::codex::CODEX;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use crate::reader::{Passing, Reader, Said};
use crate::windows::{Keeping, Slot, Window};

/// The provider the windows are kept under; Codex holds no logins of agency's.
pub const SLOT: Slot<'static> = Slot {
  mode: "codex",
  account: "",
};

// The id of the request the windows are asked with.
const RATE_LIMITS_REQUEST: i64 = 2;

fn text(value: Option<&Value>) -> Option<&str> {
  value.and_then(Value::as_str).filter(|text| !text.trim().is_empty())
}

// A whole number of a field that may be written as a fraction.
fn whole(value: Option<&Value>) -> Option<i64> {
  value.and_then(Value::as_f64).map(|number| number.floor() as i64)
}

fn window(id: &str, name: Option<&str>, slot: &str, held: Option<&Value>) -> Option<Window> {
  let held = held.filter(|held| held.is_object())?;
  let used = held.get("usedPercent").and_then(Value::as_f64).filter(|used| *used >= 0.0)?;

  Some(Window {
    name: format!("codex:{id}:{slot}"),
    utilization: used / 100.0,
    resets_at: whole(held.get("resetsAt")),
    duration_minutes: whole(held.get("windowDurationMins")).filter(|minutes| *minutes > 0),
    limit_name: name.map(str::to_owned),
  })
}

/// The app-server's answer as windows. Its windows by limit win where they are there; `rateLimits` is
/// their mirror for an older server.
pub fn windows_of_answer(answer: &Value) -> Result<Vec<Window>, String> {
  if !answer.is_object() {
    return Err("codex rate limits: app-server returned no result".to_owned());
  }

  let by_id: Vec<(String, &Value)> = answer
    .get("rateLimitsByLimitId")
    .and_then(Value::as_object)
    .map(|by_id| {
      by_id
        .iter()
        .filter(|(_, held)| held.is_object())
        .map(|(id, held)| (id.clone(), held))
        .collect()
    })
    .unwrap_or_default();
  let snapshots = if by_id.is_empty() {
    answer
      .get("rateLimits")
      .filter(|held| held.is_object())
      .map(|held| vec![("default".to_owned(), held)])
      .unwrap_or_default()
  } else {
    by_id
  };
  let windows: Vec<Window> = snapshots
    .iter()
    .flat_map(|(fallback, snapshot)| {
      let id = text(snapshot.get("limitId")).unwrap_or(fallback);
      let name = text(snapshot.get("limitName"));

      [
        window(id, name, "primary", snapshot.get("primary")),
        window(id, name, "secondary", snapshot.get("secondary")),
      ]
    })
    .flatten()
    .collect();

  if windows.is_empty() {
    return Err("codex rate limits: app-server returned no usable windows".to_owned());
  }

  Ok(windows)
}

/// The windows a rollout's token count carries, in the rollout's own field names.
pub fn windows_of_rollout(limits: &Value) -> Vec<Window> {
  if !limits.is_object() {
    return vec![];
  }

  let id = text(limits.get("limit_id")).unwrap_or("default");
  let name = text(limits.get("limit_name"));

  ["primary", "secondary"]
    .into_iter()
    .filter_map(|slot| {
      let held = limits.get(slot)?;
      let used = held.get("used_percent").and_then(Value::as_f64).filter(|used| *used >= 0.0)?;

      Some(Window {
        name: format!("codex:{id}:{slot}"),
        utilization: used / 100.0,
        resets_at: whole(held.get("resets_at")),
        duration_minutes: whole(held.get("window_minutes")).filter(|minutes| *minutes > 0),
        limit_name: name.map(str::to_owned),
      })
    })
    .collect()
}

fn newest_first(path: &Path) -> Vec<String> {
  let mut names: Vec<String> = std::fs::read_dir(path)
    .map(|entries| {
      entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
    })
    .unwrap_or_default();

  names.sort();
  names.reverse();
  names
}

fn modified_ms(path: &Path) -> Option<i64> {
  let modified = std::fs::metadata(path).ok()?.modified().ok()?;

  i64::try_from(modified.duration_since(UNIX_EPOCH).ok()?.as_millis()).ok()
}

// The newest rollout file under the newest few days the rollouts are filed under, by year, month and day.
fn newest_rollout(root: &Path) -> Option<PathBuf> {
  let mut newest: Option<(PathBuf, i64)> = None;
  let mut seen = 0;

  for year in newest_first(root) {
    for month in newest_first(&root.join(&year)) {
      for day in newest_first(&root.join(&year).join(&month)) {
        if seen >= CODEX.usage.rollout_days {
          return newest.map(|(path, _)| path);
        }

        seen += 1;

        let dir = root.join(&year).join(&month).join(&day);

        for name in newest_first(&dir) {
          if !name.starts_with("rollout-") || !name.ends_with(".jsonl") {
            continue;
          }

          let path = dir.join(&name);
          let Some(at) = modified_ms(&path) else { continue };

          if newest.as_ref().is_none_or(|(_, newest)| at > *newest) {
            newest = Some((path, at));
          }
        }
      }
    }
  }

  newest.map(|(path, _)| path)
}

/// The windows the last token count of the newest rollout carried, with when it was written in epoch
/// milliseconds; none where no rollout carries one.
pub fn rollout_reading(root: &Path) -> Option<(Vec<Window>, i64)> {
  let path = newest_rollout(root)?;
  let text = std::fs::read_to_string(&path).ok()?;

  text.lines().rev().find_map(|line| {
    let event: Value = serde_json::from_str(line).ok()?;
    let payload = event.get("payload")?;

    if event.get("type").and_then(Value::as_str) != Some("event_msg")
      || payload.get("type").and_then(Value::as_str) != Some("token_count")
    {
      return None;
    }

    let windows = windows_of_rollout(payload.get("rate_limits")?);

    if windows.is_empty() {
      return None;
    }

    let at = event
      .get("timestamp")
      .and_then(Value::as_str)
      .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
      .map(|at| at.timestamp_millis())
      .or_else(|| modified_ms(&path))?;

    Some((windows, at))
  })
}

// The windows asked of the app-server, the process ended with the reading.
async fn ask_app_server(program: &Path) -> Result<Vec<Window>, String> {
  let usage = &CODEX.usage;
  let mut child = tokio::process::Command::new(program)
    .args(&usage.app_server_args)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true)
    .spawn()
    .map_err(|why| format!("codex app-server could not be started: {why}"))?;
  let requests = [
    json!({
      "id": 1,
      "method": "initialize",
      "params": {
        "clientInfo": { "name": "agency", "title": "Agency", "version": crate::VERSION },
        "capabilities": null,
      },
    }),
    json!({ "method": "initialized" }),
    json!({ "id": RATE_LIMITS_REQUEST, "method": "account/rateLimits/read" }),
  ];
  let lines: String = requests.iter().map(|request| format!("{request}\n")).collect();
  let mut stdin = child.stdin.take().ok_or("codex app-server took no input")?;
  let stdout = child.stdout.take().ok_or("codex app-server gave no output")?;
  // What it says on its error stream is read as it goes, so a full pipe never holds it up, and what it has
  // said by its end is carried beside a failure: once it has ended, what it wrote stands in the pipe already
  // and is read without waiting, since a process it started may hold the stream open past its own end and
  // the stream's close is never waited for.
  let said = Arc::new(Mutex::new(Vec::new()));
  let (ended, ending) = tokio::sync::oneshot::channel::<()>();
  let reading = child.stderr.take().map(|mut stderr| {
    let said = said.clone();
    let keep = move |bytes: &[u8]| {
      said.lock().unwrap_or_else(std::sync::PoisonError::into_inner).extend_from_slice(bytes);
    };

    tokio::spawn(async move {
      let mut chunk = [0u8; 4096];
      let mut ending = std::pin::pin!(ending);

      loop {
        tokio::select! {
          read = stderr.read(&mut chunk) => match read {
            Ok(read) if read > 0 => keep(&chunk[..read]),
            _ => return,
          },
          _ = &mut ending => break,
        }
      }

      // The rest is read from the pipe itself rather than through the runtime, which may not yet have seen
      // the pipe ready when the program's end was met on a write rather than a read; the pipe is
      // non-blocking, so a read that finds it empty ends at once.
      let Ok(pipe) = stderr.as_fd().try_clone_to_owned() else { return };
      let mut pipe = std::fs::File::from(pipe);

      while let Ok(read @ 1..) = std::io::Read::read(&mut pipe, &mut chunk) {
        keep(&chunk[..read]);
      }
    })
  });
  // A program that ended before it read the requests closed before answering them, as one that ends after.
  let closed = || "app-server closed before returning rate limits".to_owned();
  let unwritten = |why: std::io::Error| {
    if why.kind() == std::io::ErrorKind::BrokenPipe { closed() } else { why.to_string() }
  };
  let answered = async {
    stdin.write_all(lines.as_bytes()).await.map_err(unwritten)?;
    stdin.flush().await.map_err(unwritten)?;

    let mut reading = BufReader::new(stdout).lines();

    while let Some(line) = reading.next_line().await.map_err(|why| why.to_string())? {
      let Ok(response) = serde_json::from_str::<Value>(line.trim()) else { continue };

      if response.get("id").and_then(Value::as_i64) != Some(RATE_LIMITS_REQUEST) {
        continue;
      }

      if let Some(error) = response.get("error") {
        return Err(
          error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("app-server request failed")
            .to_owned(),
        );
      }

      return windows_of_answer(response.get("result").unwrap_or(&Value::Null));
    }

    Err(closed())
  };
  // The read waits for the answer or for the process to close its output, however long either takes.
  let answered = answered.await;

  drop(stdin);
  let _ = child.kill().await;
  let _ = ended.send(());

  if let Some(reading) = reading {
    let _ = reading.await;
  }

  let Err(why) = answered else {
    return answered;
  };
  let said =
    String::from_utf8_lossy(&said.lock().unwrap_or_else(std::sync::PoisonError::into_inner)).into_owned();

  Err(if said.trim().is_empty() { why } else { format!("{why}: {}", said.trim()) })
}

/// The reader on the clock: where Codex keeps its rollouts and its program, and the failure last said.
pub struct CodexReader {
  sessions: PathBuf,
  program: PathBuf,
  said: Said,
}

impl CodexReader {
  /// The reader of the rollouts under `sessions` and of the program at `program`.
  pub fn new(sessions: PathBuf, program: PathBuf) -> Self {
    Self {
      sessions,
      program,
      said: Said::default(),
    }
  }

  /// The windows as they stand at `now_ms`: the rollout's where a turn wrote them lately, which costs no
  /// process, and the account's otherwise.
  pub async fn current(&self, now_ms: i64) -> Result<Vec<Window>, String> {
    match rollout_reading(&self.sessions) {
      Some((windows, at)) if now_ms - at <= CODEX.usage.rollout_fresh_seconds * 1000 => Ok(windows),
      _ => ask_app_server(&self.program).await,
    }
  }
}

impl Reader for CodexReader {
  fn mode(&self) -> &str {
    SLOT.mode
  }

  fn cadence(&self) -> Duration {
    Duration::from_secs(CODEX.usage.pass_seconds)
  }

  fn fresh(&self) -> Duration {
    self.cadence() * CODEX.usage.fresh_passes
  }

  fn pass<'a>(&'a self, keeping: &'a Keeping, now_ms: i64) -> Passing<'a> {
    Box::pin(async move {
      let outcome = match self.current(now_ms).await {
        Ok(windows) if windows.is_empty() => Err("codex rate limits: no usable windows".to_owned()),
        Ok(windows) => keeping
          .replace(SLOT, &windows, now_ms.div_euclid(1000))
          .await
          .map_err(|fault| fault.detail),
        Err(why) => Err(why),
      };

      self.said.outcome("codex rate limits", &outcome);
    })
  }
}
