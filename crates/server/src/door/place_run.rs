//! What an agent's place holds of the turns put into its resident program (docs/agent-control.md,
//! 「What a resident turn reads」, 「Runs and sessions in one pane」). The program writes into it through
//! the status-line command and the hooks its settings name — one line per call in `status.jsonl` and
//! `hooks.jsonl` — and the door writes one thing, the run it is putting in, in `run.json`, with a mark
//! beside it while it is handing that run a word (`handed`), so the line at the foot of the program's
//! screen can say what is going on and a hook can tell the bot's turn from a person's.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::door::record_secrets::{Readings, Tally};
use crate::lines::place::{FAILURE_KIND, PLACE_VARIABLE, Place, is_turn_variable};

pub fn run_file(place: &Path) -> PathBuf {
  place.join("run.json")
}

pub fn handed_file(place: &Path) -> PathBuf {
  place.join("handed")
}

/// The run standing in the place: the one the bot put in last, or a person's turn, which has no name of
/// the bot's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceRun {
  pub name: Option<String>,
  pub title: Option<String>,
  pub person: bool,
  pub started_at: i64,
  pub ended_at: Option<i64>,
  /// The run's own variables, which a program standing across many runs cannot carry in its own
  /// environment.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub env: Option<BTreeMap<String, String>>,
}

pub fn read_run(place: &Path) -> Option<PlaceRun> {
  serde_json::from_slice(&std::fs::read(run_file(place)).ok()?).ok()
}

fn write_run(place: &Path, run: &PlaceRun) -> std::io::Result<()> {
  std::fs::create_dir_all(place)?;

  let mut temporary = run_file(place).into_os_string();

  temporary.push(format!(".{}", std::process::id()));

  let mut bytes = serde_json::to_vec(run).map_err(std::io::Error::other)?;

  bytes.push(b'\n');
  std::fs::write(&temporary, bytes)?;
  std::fs::rename(&temporary, run_file(place))
}

/// The bot names the run it is about to put in before it puts it in, so the prompt the program takes is
/// already the bot's by the time its hook asks whose it is.
pub fn begin_run(
  place: &Path,
  name: &str,
  title: &str,
  at: i64,
  env: BTreeMap<String, String>,
) -> std::io::Result<()> {
  write_run(
    place,
    &PlaceRun {
      name: Some(name.to_owned()),
      title: Some(title.to_owned()),
      person: false,
      started_at: at,
      ended_at: None,
      env: Some(env),
    },
  )
}

/// The bot closes its run once it has read the turn's end — the only close an interrupted turn gets,
/// since an Escape fires no hook.
pub fn end_run(place: &Path, at: i64) -> std::io::Result<()> {
  match read_run(place) {
    Some(run) if run.ended_at.is_none() => write_run(
      place,
      &PlaceRun {
        ended_at: Some(at),
        ..run
      },
    ),
    _ => Ok(()),
  }
}

/// The bot marks its run as handed a word just before typing it into the program, and takes the mark
/// back where it does not type it after all: the word can land after the turn's end has closed the
/// run, and its submit is the bot's all the same. The next submit takes the mark away; a person's turn
/// is never marked. The mark is a file of its own, so the bot never writes the run while a hook may be
/// writing its end.
pub fn hand_run(place: &Path, handing: bool) {
  if !handing {
    let _ = std::fs::remove_file(handed_file(place));

    return;
  }

  if read_run(place).is_none_or(|run| run.person) {
    return;
  }

  let _ = std::fs::write(handed_file(place), format!("{}\n", crate::now_ms()));
}

/// How far out of their stamps' order two of a place's lines can land: each is stamped as its call arrives
/// and appended at once, so only calls arriving together cross, by moments, never by this much.
pub const CROSSED_MS: i64 = 60_000;

/// A stamped line of a place's record.
trait Stamped {
  fn at(&self) -> i64;
}

/// A place's record read back from its end, newest line first, one line at a time: the records only
/// grow, and a reader takes only as many lines as it asks for. A line that does not parse is passed over,
/// and a file that cannot be read has none.
fn records_back<T: for<'de> Deserialize<'de>>(path: &Path) -> impl Iterator<Item = T> {
  std::fs::File::open(path)
    .ok()
    .map(rev_lines::RawRevLines::new)
    .into_iter()
    .flatten()
    .map_while(Result::ok)
    .filter_map(|line| serde_json::from_slice(&line).ok())
}

/// A place's records stamped at or after `since`, oldest first, read back from the end no further than
/// a crossing's while before it.
fn records_since<T: for<'de> Deserialize<'de> + Stamped>(path: &Path, since: i64) -> Vec<T> {
  let mut records: Vec<T> = records_back(path)
    .take_while(|record: &T| record.at() >= since.saturating_sub(CROSSED_MS))
    .filter(|record| record.at() >= since)
    .collect();

  records.reverse();
  records
}

/// The login of the program that made a status reading, as its own environment named it: the
/// configuration home it reads its login from, and a fingerprint of the token it was handed instead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramLogin {
  pub config_dir: String,
  pub token: Option<String>,
}

/// One call the program made to its status-line command.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct StatusRecord {
  pub at: i64,
  #[serde(default)]
  pub payload: Value,
  #[serde(default)]
  pub login: Option<ProgramLogin>,
}

/// One call of the program's hooks: a turn's `stop`, the `failure` a failed request ended a turn with, a
/// prompt's `submit`, a session's `start`, and a `held` stop the hook answered with a block, on which the
/// program went on in the same turn.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct HookRecord {
  pub at: i64,
  pub kind: String,
  #[serde(default)]
  pub payload: Value,
}

/// Whether a hook call of this kind ends a turn.
pub fn ends_turn(kind: &str) -> bool {
  kind == "stop" || kind == FAILURE_KIND
}

impl Stamped for StatusRecord {
  fn at(&self) -> i64 {
    self.at
  }
}

impl Stamped for HookRecord {
  fn at(&self) -> i64 {
    self.at
  }
}

pub fn status_file(place: &Path) -> PathBuf {
  place.join("status.jsonl")
}

/// The status readings stamped at or after `since`, oldest first.
pub fn read_status(place: &Path, since: i64) -> Vec<StatusRecord> {
  records_since(&status_file(place), since)
}

pub fn hooks_file(place: &Path) -> PathBuf {
  place.join("hooks.jsonl")
}

/// The hook calls stamped at or after `since`, oldest first.
pub fn read_hooks(place: &Path, since: i64) -> Vec<HookRecord> {
  records_since(&hooks_file(place), since)
}

/// What the place's two records hold of a conversation taken out of them, where the conversation is taken:
/// every line of the session named, or every line where none is named. Each call the program made carries
/// its prompt, its answer or its session's title, and the session it was made in; where a session is named,
/// a line naming none, or one that does not read, stays. A program appending as the lines are taken out
/// appends after them.
pub fn forget_records(place: &Path, session: Option<&str>) -> std::io::Result<()> {
  for path in [status_file(place), hooks_file(place)] {
    let removed = match session {
      None => std::fs::OpenOptions::new().write(true).open(&path).and_then(|file| file.set_len(0)),
      Some(session) => forget_session(&path, session),
    };

    removed
      .or_else(|error| if error.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(error) })?;
  }

  Ok(())
}

// One record rewritten without the lines of a session, read a line at a time, since an agent's records grow
// large; left as it is where it holds none.
fn forget_session(path: &Path, session: &str) -> std::io::Result<()> {
  use std::io::{BufRead as _, Write as _};

  let of_session = |line: &str| {
    serde_json::from_str::<Value>(line)
      .ok()
      .is_some_and(|record| record.pointer("/payload/session_id").and_then(Value::as_str) == Some(session))
  };
  let reader = std::io::BufReader::new(std::fs::File::open(path)?);
  let mut temporary = path.as_os_str().to_owned();

  temporary.push(format!(".{}", std::process::id()));

  let temporary = PathBuf::from(temporary);
  let written = (|| {
    let mut kept = std::io::BufWriter::new(std::fs::File::create(&temporary)?);
    let mut dropped = false;

    for line in reader.lines() {
      let line = line?;

      if of_session(&line) {
        dropped = true;
      } else {
        writeln!(kept, "{line}")?;
      }
    }

    kept.flush()?;

    Ok::<bool, std::io::Error>(dropped)
  })();

  match written {
    Ok(true) => std::fs::rename(&temporary, path),
    Ok(false) => std::fs::remove_file(&temporary),
    Err(error) => {
      let _ = std::fs::remove_file(&temporary);

      Err(error)
    }
  }
}

/// A line appended to one of the place's records, every secret in its strings written as `[redacted]` by the
/// process's readings (docs/agent-control.md, 「A record holds no secret」).
pub fn append(path: &Path, record: &Value) -> std::io::Result<()> {
  append_read(path, record, crate::door::record_secrets::process())
}

/// A line appended to one of the place's records, read through `readings` first.
pub fn append_read(path: &Path, record: &Value, readings: &Readings) -> std::io::Result<()> {
  use std::io::Write as _;

  let mut record = record.clone();

  readings.value(&mut record);

  let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;

  file.write_all(format!("{record}\n").as_bytes())
}

/// The place's records that a line is appended to, each read through the readings as it is written.
pub fn record_files(place: &Path) -> [PathBuf; 3] {
  [status_file(place), hooks_file(place), background_file(place)]
}

/// One record written before read through `readings`, a line at a time since an agent's records grow large,
/// and rewritten where a line changed; a line that did not change is kept byte for byte, and a record holding
/// none is left as it is. A line appended while the record is read is read on and written too, and one
/// appended to the old record once it is replaced is appended to the new one. Answers how many lines changed,
/// each run taken out counted into `tally` by its source.
pub fn redact_record(path: &Path, readings: &Readings, tally: &mut Tally) -> std::io::Result<usize> {
  use std::io::{BufRead as _, Write as _};

  let source = std::fs::File::open(path)?;
  let permissions = source.metadata()?.permissions();
  let mut reader = std::io::BufReader::new(source);
  let mut temporary = path.as_os_str().to_owned();

  temporary.push(format!(".redact.{}", std::process::id()));

  let temporary = PathBuf::from(temporary);
  // Each line read on to the record's end, written as the readings leave it; answers how many changed.
  let mut read_on = |into: &mut dyn std::io::Write| -> std::io::Result<usize> {
    let mut changed = 0;
    let mut line = Vec::new();

    while reader.read_until(b'\n', &mut line)? > 0 {
      match redacted_line(&line, readings, tally) {
        Some(redacted) => {
          into.write_all(redacted.as_bytes())?;
          changed += 1;
        }
        None => into.write_all(&line)?,
      }

      line.clear();
    }

    Ok(changed)
  };
  let written = (|| {
    let mut kept = std::io::BufWriter::new(std::fs::File::create(&temporary)?);
    let changed = read_on(&mut kept)?;

    if changed > 0 {
      read_on(&mut kept)?;
      kept.flush()?;
      std::fs::set_permissions(&temporary, permissions)?;
    }

    Ok::<usize, std::io::Error>(changed)
  })();

  match written {
    Ok(0) => std::fs::remove_file(&temporary).map(|()| 0),
    Ok(changed) => {
      std::fs::rename(&temporary, path)?;

      let mut late = Vec::new();
      let late_changed = read_on(&mut late)?;

      if !late.is_empty() {
        std::fs::OpenOptions::new().append(true).open(path)?.write_all(&late)?;
      }

      Ok(changed + late_changed)
    }
    Err(error) => {
      let _ = std::fs::remove_file(&temporary);

      Err(error)
    }
  }
}

// A record's line as the readings leave it, its line end kept, or none where they change nothing: a line that
// reads as JSON has its strings read, any other line its words.
fn redacted_line(line: &[u8], readings: &Readings, tally: &mut Tally) -> Option<String> {
  let text = std::str::from_utf8(line).ok()?;
  let body = text.strip_suffix('\n').unwrap_or(text);
  let end = &text[body.len()..];

  match serde_json::from_str::<Value>(body) {
    Ok(mut record) => readings.value_tallied(&mut record, tally).then(|| format!("{record}{end}")),
    Err(_) => match readings.text_tallied(body, tally) {
      std::borrow::Cow::Owned(redacted) => Some(format!("{redacted}{end}")),
      std::borrow::Cow::Borrowed(_) => None,
    },
  }
}

/// The lines a Claude Code session's own record, its transcript, holds stamped at or after `since`, oldest
/// first; none where it cannot be read. The transcript is read a line at a time, so what is held is the
/// lines kept and never the whole file.
pub fn transcript_since(transcript: &Path, since: i64) -> Option<Vec<Value>> {
  std::fs::metadata(transcript).ok()?;

  let mut kept = vec![];

  crate::door::record::each_line(transcript, |_, record| {
    let stamped = record
      .get("timestamp")
      .and_then(Value::as_str)
      .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
      .is_some_and(|at| at.timestamp_millis() >= since);

    if stamped {
      kept.push(record);
    }
  });

  Some(kept)
}

/// The failure the program's latest turn since `since` ended with: none where that turn ended with a stop,
/// or a prompt came after its end.
pub fn latest_failure(place: &Path, since: i64) -> Option<HookRecord> {
  read_hooks(place, since)
    .into_iter()
    .rev()
    .find(|hook| hook.ends_turn() || hook.kind == "submit")
    .filter(|hook| hook.kind == FAILURE_KIND)
}

impl HookRecord {
  /// Whether the call ended a turn: a stop, or the failure a failed request ended it with.
  pub fn ends_turn(&self) -> bool {
    ends_turn(&self.kind)
  }

  /// What a failure says went wrong: the error the program wrote in the answer's place, else its details,
  /// else its kind; and the kind, where it names one.
  pub fn failure(&self) -> (String, Option<String>) {
    let text = |key: &str| {
      self
        .payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
    };
    let kind = text("error");
    let said = text("last_assistant_message")
      .or_else(|| text("error_details"))
      .or_else(|| kind.clone())
      .unwrap_or_else(|| "the request failed".to_owned());

    (said, kind)
  }
}

/// Whether a submit opens a person's turn: no run of the bot's is going in the place, and the bot has
/// handed it no word still to arrive. Asked before the submit is recorded, since recording it opens that
/// turn.
pub fn person_submits(place: &Path) -> bool {
  match read_run(place) {
    None => true,
    Some(run) => run.ended_at.is_some() && !handed_file(place).exists(),
  }
}

/// One call of the program's, as its hook or status-line command receives it, kept whole beside the moment
/// it arrived. A prompt submitted while no run of the bot's is going, and no word of the bot's is still to
/// arrive, is a person's turn; a submit the bot marked for opens its run again where a turn's end had
/// closed it, and takes the mark away. A turn's end, a stop or a failure, closes whatever run is going,
/// except a bot's run whose round still owes a turn (`keeps_run`). A status reading carries the login of
/// the program that made it.
pub fn record_call(
  place: &Path,
  kind: &str,
  payload: &Value,
  at: i64,
  keeps_run: bool,
  login: Option<&ProgramLogin>,
) -> std::io::Result<()> {
  std::fs::create_dir_all(place)?;

  if kind == "status" {
    let mut record = serde_json::json!({ "at": at, "payload": payload });

    if let Some(login) = login {
      record["login"] = serde_json::to_value(login).map_err(std::io::Error::other)?;
    }

    return append(&status_file(place), &record);
  }

  append(&hooks_file(place), &serde_json::json!({ "at": at, "kind": kind, "payload": payload }))?;

  let run = read_run(place);

  if kind == "submit" && person_submits(place) {
    write_run(
      place,
      &PlaceRun {
        name: None,
        title: None,
        person: true,
        started_at: at,
        ended_at: None,
        env: None,
      },
    )?;
  } else if kind == "submit"
    && let Some(run) = run.clone().filter(|run| !run.person && run.ended_at.is_some())
  {
    write_run(
      place,
      &PlaceRun {
        ended_at: None,
        ..run
      },
    )?;
  } else if ends_turn(kind)
    && let Some(run) = run.filter(|run| run.ended_at.is_none() && !(keeps_run && !run.person))
  {
    write_run(
      place,
      &PlaceRun {
        ended_at: Some(at),
        ..run
      },
    )?;
  }

  if kind == "submit" {
    hand_run(place, false);
  }

  Ok(())
}

/// Whether a submit is the program handing itself a background task's notice.
pub fn notice_submitted(payload: &Value) -> bool {
  payload
    .get("prompt")
    .and_then(Value::as_str)
    .is_some_and(crate::door::record::is_notice)
}

/// The place's last call that bounds a turn — a stop, a failure or a submit — read back from the record's
/// end no further than it.
pub fn last_bound(place: &Path) -> Option<HookRecord> {
  records_back::<HookRecord>(&hooks_file(place)).find(|hook| hook.ends_turn() || hook.kind == "submit")
}

/// Whether the program's foreground is at rest on a line that carries its background apart, whatever its
/// screen reads (docs/agent-control.md, 「Background work on a line that carries it apart」): the place's last
/// call bounding a turn is a turn's end stamped at or after `since`, and the session record's own end says
/// the same, no word handed over still queued.
pub fn foreground_rests(place: &Path, records: &[Value], since: i64) -> bool {
  last_bound(place).is_some_and(|hook| hook.ends_turn() && hook.at >= since)
    && crate::door::record::rests_apart(records)
}

/// The place's background record: the background work a line that carries it apart launched, and the notices
/// that ended it, one line each.
pub fn background_file(place: &Path) -> PathBuf {
  place.join("background.jsonl")
}

/// One line of the background record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum BackgroundLine {
  /// A task a stop named running that the record did not hold yet.
  Launched(Launch),
  /// A notice the program handed itself about a task.
  Notice {
    at: i64,
    #[serde(flatten)]
    notice: crate::door::record::TaskNotice,
  },
  /// A task a stop of the supervisor's turn ended, written before the stop ends it.
  Stopped { at: i64, task: String },
}

/// A background task as a stop the program made named it: its id, its kind and its description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunningTask {
  pub id: String,
  pub kind: String,
  pub description: String,
}

/// The place's last stop: the stop hook's call, or a stop it held.
pub fn last_stop(place: &Path) -> Option<HookRecord> {
  records_back::<HookRecord>(&hooks_file(place)).find(|hook| hook.kind == "stop" || hook.kind == "held")
}

/// The background tasks the place's last stop named running that no notice or stop in its background record has
/// ended since.
pub fn last_stop_tasks(place: &Path) -> Vec<RunningTask> {
  let Some(stop) = last_stop(place) else {
    return vec![];
  };
  let ended: std::collections::HashSet<String> = read_background(place)
    .into_iter()
    .filter_map(|line| match line {
      BackgroundLine::Notice { notice, .. } if notice.ended() => Some(notice.task),
      BackgroundLine::Stopped { task, .. } => Some(task),
      _ => None,
    })
    .collect();
  let text = |task: &Value, key: &str| task.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();

  stop
    .payload
    .get("background_tasks")
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter(|task| task.get("status").and_then(Value::as_str) == Some("running"))
    .map(|task| RunningTask {
      id: text(task, "id"),
      kind: text(task, "type"),
      description: text(task, "description"),
    })
    .filter(|task| !task.id.is_empty() && !ended.contains(&task.id))
    .collect()
}

/// The tasks a stop is about to end, written into the background record so their rooms are told of the stop
/// rather than of whatever notice the ending draws from the program.
pub fn write_stopped(place: &Path, tasks: &[String], at: i64) -> std::io::Result<()> {
  for task in tasks {
    let line = BackgroundLine::Stopped {
      at,
      task: task.clone(),
    };

    append(&background_file(place), &serde_json::to_value(line).map_err(std::io::Error::other)?)?;
  }

  Ok(())
}

/// A background task as the stop that first named it running found it, under the run standing in the place.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Launch {
  pub at: i64,
  pub task: String,
  /// `shell`, `subagent`, or whatever else the program names it.
  pub kind: String,
  pub description: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub command: Option<String>,
  /// The room the run that launched it was in, and its agent; none for a person's turn or a run naming no
  /// room.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub room: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub agent: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub run: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub session: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub transcript: Option<String>,
}

/// Every line of the background record, oldest first; a line that does not read is passed over.
pub fn read_background(place: &Path) -> Vec<BackgroundLine> {
  let Ok(text) = std::fs::read_to_string(background_file(place)) else { return vec![] };

  text.lines().filter_map(|line| serde_json::from_str(line).ok()).collect()
}

/// The tasks a stop names running that the background record does not hold yet, written into it under the run
/// standing in the place: its room and agent where it is a bot's run that names them.
pub fn write_launches(place: &Path, payload: &Value, at: i64) -> std::io::Result<()> {
  let text = |task: &Value, key: &str| task.get(key).and_then(Value::as_str).map(str::to_owned);
  let running: Vec<&Value> = payload
    .get("background_tasks")
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter(|task| task.get("status").and_then(Value::as_str) == Some("running"))
    .collect();

  if running.is_empty() {
    return Ok(());
  }

  let known: std::collections::HashSet<String> = read_background(place)
    .into_iter()
    .filter_map(|line| match line {
      BackgroundLine::Launched(launch) => Some(launch.task),
      _ => None,
    })
    .collect();
  let run = read_run(place).filter(|run| !run.person);
  let variable = |name: &str| run.as_ref().and_then(|run| run.env.as_ref()?.get(name).cloned());
  let room = variable("BMO_TURN_CHAT").filter(|room| !room.is_empty());

  for task in running {
    let Some(id) = text(task, "id").filter(|id| !known.contains(id)) else { continue };
    let launch = Launch {
      at,
      task: id,
      kind: text(task, "type").unwrap_or_default(),
      description: text(task, "description").unwrap_or_default(),
      command: text(task, "command"),
      agent: room.as_ref().and(variable("BMO_TURN_AGENT")),
      room: room.clone(),
      run: room.as_ref().and(run.as_ref().and_then(|run| run.name.clone())),
      session: text(payload, "session_id"),
      transcript: text(payload, "transcript_path"),
    };

    append(
      &background_file(place),
      &serde_json::to_value(BackgroundLine::Launched(launch)).map_err(std::io::Error::other)?,
    )?;
  }

  Ok(())
}

/// A submit of a background task's notice on a line that carries its background apart, kept as the program's
/// own turn: the call written into the hooks record as any submit is, and the notice into the background
/// record, while the place's run is left as it stands — the notice opens no person's turn, reopens no run of
/// the bot's and leaves the bot's mark for a word still to arrive.
pub fn record_notice(place: &Path, payload: &Value, at: i64) -> std::io::Result<()> {
  std::fs::create_dir_all(place)?;
  append(
    &hooks_file(place),
    &serde_json::json!({ "at": at, "kind": "submit", "payload": payload }),
  )?;

  let Some(notice) =
    payload.get("prompt").and_then(Value::as_str).and_then(crate::door::record::task_notice)
  else {
    return Ok(());
  };

  append(
    &background_file(place),
    &serde_json::to_value(BackgroundLine::Notice { at, notice }).map_err(std::io::Error::other)?,
  )
}

/// How many stops in a row are held before the next one is let through as it stands.
pub const STOP_HOLDS: usize = 3;

/// A stop that may still be held: how many holds it has had, when the bot's run going in the place started,
/// and whether the next hold is the last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenHold {
  pub held: usize,
  pub started_at: i64,
  pub last: bool,
}

/// Whether this stop may still be held: a bot's run going in the place and fewer than `limit` holds since
/// the last stop or prompt. Every hold of a stop counts against the one limit, whatever it was held for, so
/// a turn is never held without end.
pub fn open_hold(place: &Path, limit: usize) -> Option<OpenHold> {
  let run = read_run(place).filter(|run| !run.person && run.ended_at.is_none())?;
  let hooks = read_hooks(place, run.started_at);
  let since = hooks
    .iter()
    .rposition(|hook| hook.ends_turn() || hook.kind == "submit")
    .map_or(0, |at| at + 1);
  let held = hooks[since..].iter().filter(|hook| hook.kind == "held").count();

  (held < limit).then_some(OpenHold {
    held,
    started_at: run.started_at,
    last: held + 1 >= limit,
  })
}

/// The session's background tasks a stop found still running, out of what the program handed the hook: each
/// one's id and description.
pub fn running_tasks(payload: &Value) -> Vec<(String, String)> {
  let text = |task: &Value, key: &str| task.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();

  payload
    .get("background_tasks")
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter(|task| task.get("status").and_then(Value::as_str) == Some("running"))
    .map(|task| (text(task, "id"), text(task, "description")))
    .collect()
}

/// Why this stop is held rather than taken as the turn's end, or nothing: a bot's run going in the place, a
/// task of the session still running, and fewer than `limit` holds since the last stop or prompt
/// (docs/agent-control.md, 「What a resident turn reads」).
pub fn stop_hold(place: &Path, payload: &Value, limit: usize) -> Option<String> {
  let tasks = running_tasks(payload);
  let open = open_hold(place, limit)?;

  if tasks.is_empty() {
    return None;
  }

  let named = tasks
    .iter()
    .map(|(id, description)| {
      if description.is_empty() {
        id.clone()
      } else {
        format!("{id} ({})", crate::text::within(description, 120))
      }
    })
    .collect::<Vec<_>>()
    .join("; ");
  let mut said = vec![
    format!("Not ended: background work this session started is still running — {named}."),
    "A turn that ends now is read as the job's answer, and the work is cut off with it.".to_owned(),
    "Wait for each until it ends — on its end mark or its process exiting, however long it takes — and read what it wrote, or stop it with TaskStop where it is a helper you no longer need, a dev server or a stand-in; then answer.".to_owned(),
  ];

  if open.held + 1 >= limit {
    said.push(format!(
      "This is hold {} of {limit}; the next stop is let through with the work still running.",
      open.held + 1
    ));
  }

  Some(said.join(" "))
}

/// The one line at the foot of the program's screen: what is going and since when, or since when the agent
/// has been resting and which run it last did, on the deployment's clock (UTC+9).
pub fn status_line(run: Option<&PlaceRun>) -> String {
  use policy::door_words::DOOR_WORDS;
  use policy::fill_marks;

  let clock = |at: i64| {
    chrono::DateTime::from_timestamp_millis(at + 9 * 3_600_000)
      .map_or_else(String::new, |at| at.format("%H:%M").to_string())
  };
  let Some(run) = run else { return DOOR_WORDS.status_line.resting.clone() };
  let name = run.name.as_deref().unwrap_or_default();

  match (run.ended_at, run.person) {
    (None, true) => fill_marks(&DOOR_WORDS.status_line.person, &[("at", &clock(run.started_at))]),
    (None, false) => fill_marks(
      &DOOR_WORDS.status_line.running,
      &[
        ("name", name),
        ("title", run.title.as_deref().unwrap_or_default()),
        ("at", &clock(run.started_at)),
      ],
    ),
    (Some(ended), true) => {
      fill_marks(&DOOR_WORDS.status_line.rested_after_person, &[("at", &clock(ended))])
    }
    (Some(ended), false) => {
      fill_marks(&DOOR_WORDS.status_line.rested_after_run, &[("at", &clock(ended)), ("name", name)])
    }
  }
}

/// The two status readings either side of a turn: the last one made before it went in, and the first
/// one stamped after it ended, with the login of the program that made that one. A reading stamped in
/// the millisecond the turn ended may have been made before the end — the call as the program came up,
/// or one inside the turn — so it is never the one after. Named a session, only that session's
/// readings count: the place is the agent's, not the session's.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct StatusAround {
  pub before: Option<Value>,
  pub after: Option<Value>,
  pub login: Option<ProgramLogin>,
}

/// The readings around a turn out of the records given newest first, taking them only until the reading
/// before the turn: the first after the turn in the record's order is the last of them met going back.
pub fn status_around(
  newest_first: impl IntoIterator<Item = StatusRecord>,
  started_at: i64,
  ended_at: i64,
  session: Option<&str>,
) -> StatusAround {
  let mut before = None;
  let mut after: Option<StatusRecord> = None;

  for record in newest_first.into_iter().filter(|record| record.of_session(session)) {
    if record.at <= started_at {
      before = Some(record.payload);
      break;
    }

    if record.at > ended_at {
      after = Some(record);
    }
  }

  let mut around = StatusAround {
    before,
    ..StatusAround::default()
  };

  around.take_after(after);
  around
}

impl StatusAround {
  fn take_after(&mut self, after: Option<StatusRecord>) {
    self.login = after.as_ref().and_then(|record| record.login.clone());
    self.after = after.map(|record| record.payload);
  }
}

impl StatusRecord {
  // A reading of the session named, or of any where none is.
  fn of_session(&self, session: Option<&str>) -> bool {
    session.is_none_or(|session| self.payload.get("session_id").and_then(Value::as_str) == Some(session))
  }
}

/// The readings either side of a turn, waiting for the one after it until it comes, `alive` says the program
/// is gone, `bound` has passed or `stopped` says a stop stands over the run: the program makes that call about a
/// second after the turn's end, the reader goes without where the program ended before it or the call never
/// arrives — one made through a node's home bridge while its link was down is lost — and none is answered where
/// the run was stopped meanwhile, which ends the turn stopped.
/// The reading before is read once; each later look reads back no further than a crossing's while
/// before the turn's end.
#[allow(clippy::too_many_arguments)]
pub async fn status_after_turn<Stopped>(
  place: &Path,
  started_at: i64,
  ended_at: i64,
  session: Option<&str>,
  every: Duration,
  bound: Duration,
  alive: impl Fn() -> bool,
  stopped: impl Fn() -> Stopped,
) -> Option<StatusAround>
where
  Stopped: std::future::Future<Output = bool>,
{
  let path = status_file(place);
  let mut around = status_around(records_back(&path), started_at, ended_at, session);
  let by = tokio::time::Instant::now() + bound;

  while around.after.is_none() {
    if stopped().await {
      return None;
    }

    if !alive() || tokio::time::Instant::now() >= by {
      break;
    }

    tokio::time::sleep(every.min(by.saturating_duration_since(tokio::time::Instant::now()))).await;

    let after = records_back(&path)
      .take_while(|record: &StatusRecord| record.at > ended_at.saturating_sub(CROSSED_MS))
      .filter(|record| record.at > ended_at && record.of_session(session))
      .last();

    around.take_after(after);
  }

  Some(around)
}

/// Whether the program standing in the place has drawn itself since its loop started it: a status-line
/// call written at or after that moment, which Claude Code makes once its prompt footer is up. Until
/// then the pane can still read at rest from the program before it, and words put in reach no input
/// box.
pub fn program_drawn(place: &Place) -> bool {
  let Some(running) = place.running() else { return false };
  let since = running.started_at as i64;

  records_back(&status_file(&place.path))
    .take_while(|record: &StatusRecord| record.at >= since.saturating_sub(CROSSED_MS))
    .any(|record| record.at >= since)
}

/// A resident program's own environment: the pane's, without the variables of the run that happened to
/// start it, and with the place it answers to.
pub fn resident_environment(env: &BTreeMap<String, String>, place: &Path) -> BTreeMap<String, String> {
  let mut own: BTreeMap<String, String> = env
    .iter()
    .filter(|(name, _)| !is_turn_variable(name))
    .map(|(name, value)| (name.clone(), value.clone()))
    .collect();

  own.insert(PLACE_VARIABLE.to_owned(), place.to_string_lossy().into_owned());
  own
}

/// The run's own variables out of a pane's environment, for the place to hold while the run goes on.
pub fn run_variables(env: &BTreeMap<String, String>) -> BTreeMap<String, String> {
  env
    .iter()
    .filter(|(name, _)| is_turn_variable(name))
    .map(|(name, value)| (name.clone(), value.clone()))
    .collect()
}

/// The environment a process works under: its own, or, where it stands in an agent's place, its own
/// without the variables of whatever run started the pane and with those of the run standing in the
/// place now — none for a person's turn.
pub fn turn_environment(env: &BTreeMap<String, String>) -> BTreeMap<String, String> {
  let Some(place) = env.get(PLACE_VARIABLE).filter(|place| !place.is_empty()) else {
    return env.clone();
  };
  let mut own: BTreeMap<String, String> = env
    .iter()
    .filter(|(name, _)| !is_turn_variable(name))
    .map(|(name, value)| (name.clone(), value.clone()))
    .collect();

  if let Some(run) = read_run(Path::new(place)).filter(|run| !run.person) {
    own.extend(run.env.unwrap_or_default());
  }

  own
}
