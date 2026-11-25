//! Execution rows closed on evidence where nothing else closed them (docs/agent-control.md, 「Completion,
//! cancellation and loss」), on agency's server: a run left in its host has no collector,
//! and a collector killed before it wrote its end leaves its row open, so the row would read as running for
//! ever and every wait on its name would wait with it. A row closes here only on what was written for it — an
//! outcome its host published, an ending its collector published, a parent whose pane it shared ending, a
//! supervisor and its child's group both gone, a resident turn's collector gone with its program ended or gone,
//! or nothing at all placed a day after it began, past any launch window — and never on an absence a launch
//! still under way could explain; a row that says none of these stays open.

use std::collections::HashSet;
use std::path::Path;

use agent_host::envelope::Outcome;
use agent_host::identity::{ProcessIdentity, group_running};
use sea_orm::DatabaseConnection;
use serde_json::{Map, Value, json};
use store::all;

use crate::door::execution::{ENVELOPE, Row};
use crate::door::place_run::{HookRecord, read_hooks};
use crate::{Refusal, now_ms};

/// How a row closes: its state, the exit code where one was observed, and the evidence it closed on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ending {
  pub state: String,
  pub exit_code: Option<i32>,
  pub evidence: &'static str,
}

impl Ending {
  /// An ending in its state, with the exit code observed and the evidence it closed on.
  pub fn new(state: &str, exit_code: Option<i32>, evidence: &'static str) -> Self {
    Self {
      state: state.to_owned(),
      exit_code,
      evidence,
    }
  }
}

fn read_json(path: &Path) -> Option<Value> {
  serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Whether a process of this machine is still running; another user's process answers that it may not be
/// signalled, and is alive all the same.
pub fn process_alive(pid: i64) -> bool {
  let Ok(pid) = libc::pid_t::try_from(pid) else { return false };

  // SAFETY: signal 0 asks only whether the process may be signalled.
  pid > 0
    && (unsafe { libc::kill(pid, 0) } == 0
      || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
}

// Whether a recorded process is provably gone. A record naming its pid alone is gone once the pid is.
fn gone(identity: Option<&Value>) -> bool {
  let Some(identity) = identity else { return true };
  let Some(pid) = identity.get("pid").and_then(Value::as_i64) else { return true };

  match identity.get("birth").and_then(Value::as_str) {
    Some(birth) => !ProcessIdentity {
      pid: i32::try_from(pid).unwrap_or(0),
      birth: birth.to_owned(),
    }
    .alive(),
    None => !process_alive(pid),
  }
}

// An execution id is opaque, so only a plain one is read as a directory name.
fn plain(id: &str) -> bool {
  !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// How long after it began a row with nothing placed is read as never placed: far past any launch window.
pub const NEVER_PLACED_MS: i64 = 24 * 60 * 60 * 1000;
/// The evidence a row never placed closes on.
pub const NEVER_PLACED: &str = "interrupted: no start acknowledged and no pane a day after it began";

/// What a resident turn whose collector died holding its row closes on: its program's hooks ending the turn,
/// or its program gone with its collector.
pub const ENDED_BY_ITS_HOOKS: &str = "its collector gone, the program's own hooks ended its turn";
pub const PROGRAM_GONE: &str = "interrupted: its collector and its program are both gone";

/// Whether a row's resident program is gone, by the pid and birth it recorded.
pub fn resident_gone(body: &Map<String, Value>) -> bool {
  body.get("resident").is_some_and(|resident| gone(Some(resident)))
}

// A resident turn whose collector is gone — agency's server restarting under it, among others — on what its program
// says of it, since the program outlives the turn: ended where the program's hooks wrote a turn's end in its place
// since the turn began, interrupted where the program the row names is gone. A program still standing that wrote
// no end may still be in the turn.
fn resident_ending(body: &Map<String, Value>) -> Option<Ending> {
  let resident = body.get("resident")?;
  let began = body.get("startedAt").and_then(Value::as_i64).unwrap_or(0);
  let place = resident.get("place").and_then(Value::as_str);

  if place.is_some_and(|place| read_hooks(Path::new(place), began).iter().any(HookRecord::ends_turn)) {
    return Some(Ending::new("exited", None, ENDED_BY_ITS_HOOKS));
  }

  gone(Some(resident)).then(|| Ending::new("cancelled", Some(1), PROGRAM_GONE))
}

/// How an open row ends, at `now`, on what its envelope directory and its parent say, or none where nothing says
/// it. `parent_open` is whether the row its pane's parent is still open.
pub fn ending(body: &Map<String, Value>, parent_open: bool, directory: &Path, now: i64) -> Option<Ending> {
  let text = |key: &str| body.get(key).and_then(Value::as_str);
  let resident = !body.get("resident").is_none_or(Value::is_null);
  let in_pane = !body.get("paneId").is_none_or(Value::is_null);

  // A native child runs inside its parent's program and cannot outlive it; how it went was never observed.
  if body.get("sharedPane") == Some(&Value::Bool(true)) {
    return (text("parent").is_some() && !parent_open)
      .then(|| Ending::new("unknown", None, "ended with the parent turn whose pane it shared"));
  }

  if !text("id").is_some_and(plain) {
    return None;
  }

  if let Some(outcome) = read_json(&directory.join("outcome.json"))
    .and_then(|outcome| serde_json::from_value::<Outcome>(outcome).ok())
  {
    return Some(Ending::new(
      outcome.cause.as_str(),
      Some(outcome.exit_code),
      "durable transport outcome",
    ));
  }

  // A turn in a pane, or a resident turn, keeps its collector's envelope.
  if let Some(collector) = read_json(&directory.join(ENVELOPE)) {
    if let Some(cause) = collector.get("cause").and_then(Value::as_str) {
      return Some(Ending::new(cause, None, "the turn's own collector published how it ended"));
    }

    if !gone(collector.get("owner")) {
      return None;
    }

    if resident {
      return resident_ending(body);
    }

    return (!in_pane).then(|| {
      Ending::new("cancelled", Some(1), "interrupted: the collector died before its turn had a pane")
    });
  }

  // Every launch has its start acknowledged in its envelope, its pane or its resident record within its launch
  // window, so a row with none of them a day after it began was never placed anywhere.
  let Some(started) = read_json(&directory.join("started.json")) else {
    let began = body.get("startedAt").and_then(Value::as_i64);

    return (!resident && !in_pane && began.is_some_and(|began| now - began > NEVER_PLACED_MS))
      .then(|| Ending::new("cancelled", Some(1), NEVER_PLACED));
  };
  // A supervised child: its supervisor publishes its outcome before it exits, so a supervisor gone with its
  // child's group and no outcome is a run that was cut.
  let group = started.get("pid").and_then(Value::as_i64).and_then(|pid| i32::try_from(pid).ok())?;

  (gone(started.get("supervisor")) && !group_running(group)).then(|| {
    Ending::new("cancelled", Some(1), "interrupted: supervisor and acknowledged process group gone")
  })
}

/// What a turn in a pane of its own closes on once the leftover sweep closed its pane, the collector that
/// watched it gone.
pub const PANE_SWEPT: &str = "interrupted: its collector gone, its pane closed by the leftover sweep";

/// What a turn in a pane of its own closes on once the leftover sweep finds its pane gone from herdr, the
/// collector that watched it gone.
pub const PANE_GONE: &str = "interrupted: its collector gone, its pane gone from herdr";

/// Whether what collected a turn in a pane says it is over: its collector published how it ended, or the
/// process that collected it is gone. A directory with no collector's envelope says nothing, and is not.
pub fn collector_ended(directory: &Path) -> bool {
  read_json(&directory.join(ENVELOPE))
    .is_some_and(|collector| collector.get("cause").is_some() || gone(collector.get("owner")))
}

/// The open row of a turn in a pane of its own whose collector is gone closed as interrupted, on the evidence
/// the leftover sweep found; whether it was open. A row its own writer closed meanwhile keeps that end.
pub async fn close_swept(
  db: &DatabaseConnection,
  id: &str,
  evidence: &'static str,
) -> Result<bool, Refusal> {
  let rows = all(
    db,
    r#"SELECT "body" FROM "agent_executions" WHERE "id" = $1 AND ("body"::jsonb)->>'endedAt' IS NULL"#,
    vec![id.into()],
  )
  .await?;
  let Some(body) = rows
    .first()
    .and_then(|row| row.try_get::<String>("", "body").ok())
    .and_then(|body| serde_json::from_str::<Map<String, Value>>(&body).ok())
  else {
    return Ok(false);
  };
  let mut row = Row { body };

  row.set("state", json!("cancelled"));
  row.set("endedAt", json!(now_ms()));
  row.set("exitCode", json!(1));
  row.set("reconciliationEvidence", json!(evidence));

  row.save_while_open(db).await
}

/// The open rows of turns in a pane of its own whose pane is none of those herdr holds and whose collector is
/// gone, by their ids: what a server's restart left of a run whose pane went before anything closed its row.
/// A resident turn's row and a native child's are another pass's.
pub async fn paneless(
  db: &DatabaseConnection,
  data_root: &Path,
  panes: &HashSet<String>,
) -> Result<Vec<String>, Refusal> {
  let rows = all(
    db,
    r#"SELECT "id", ("body"::jsonb)->>'paneId' AS "pane" FROM "agent_executions"
       WHERE ("body"::jsonb)->>'endedAt' IS NULL AND ("body"::jsonb)->>'paneId' IS NOT NULL
         AND COALESCE(("body"::jsonb)->'resident', 'null'::jsonb) = 'null'::jsonb
         AND COALESCE(("body"::jsonb)->>'sharedPane', 'false') <> 'true'"#,
    vec![],
  )
  .await?;
  let executions = data_root.join("executions");
  let mut gone = vec![];

  for row in rows {
    let id: String = row.try_get("", "id")?;
    let pane: String = row.try_get("", "pane")?;

    if plain(&id) && !panes.contains(&pane) && collector_ended(&executions.join(&id)) {
      gone.push(id);
    }
  }

  Ok(gone)
}

/// One pass over every open row, each one that has ended closed as it ended; how many were closed. A row its own
/// writer closed in the meantime keeps that end.
pub async fn pass(db: &DatabaseConnection, data_root: &Path) -> Result<usize, Refusal> {
  let rows: Vec<Map<String, Value>> = all(
    db,
    r#"SELECT "body" FROM "agent_executions" WHERE ("body"::jsonb)->>'endedAt' IS NULL"#,
    vec![],
  )
  .await?
  .iter()
  .filter_map(|row| row.try_get::<String>("", "body").ok())
  .filter_map(|body| serde_json::from_str(&body).ok())
  .collect();
  let open: HashSet<String> = rows
    .iter()
    .filter_map(|body| body.get("id").and_then(Value::as_str).map(str::to_owned))
    .collect();
  let executions = data_root.join("executions");
  let mut closed = 0;

  for body in rows {
    let mut row = Row { body };
    let parent_open = row.text("parent").is_some_and(|parent| open.contains(parent));
    let directory = executions.join(row.id());
    let Some(ending) = ending(&row.body, parent_open, &directory, now_ms()) else {
      continue;
    };

    row.set("state", json!(ending.state));
    row.set("endedAt", json!(now_ms()));
    row.set("reconciliationEvidence", json!(ending.evidence));

    if let Some(code) = ending.exit_code {
      row.set("exitCode", json!(code));
    }

    if row.save_while_open(db).await? {
      closed += 1;
    }
  }

  Ok(closed)
}
