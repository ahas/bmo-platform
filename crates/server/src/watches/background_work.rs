//! A supervisor's background work after the turn that launched it (docs/agent-control.md, 「Background work on a
//! line that carries it apart」; docs/supervisors.md, 「A supervisor's long work」). Every place's background
//! record is read as it grows: a task launched under a room is told to that room once, as an order for its
//! supervisor through the outbox, when its notice ends it, when a stop ended it, or when its program is gone
//! without a notice; and while it runs, its calls are held to the call clock, each cut written down so the
//! room's wake can say why. A notice the program took up without a submit is read off the task's session record,
//! and work a later stop of its session leaves out with no notice is taken as dropped once the grace passed. What
//! still runs is answered as room state, and its launch and its end are told on the stream (docs/api.md,
//! 「background-work」).

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use bmo_agency_client::BackgroundWork;

use policy::background_work::BACKGROUND_WORK;
use policy::fill_marks;
use policy::tool_clock::TOOL_CLOCK;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::door::place_run::{
  BackgroundLine, HookRecord, Launch, background_file, last_stop, read_background, transcript_since,
};
use crate::door::record::{TaskNotice, ended_notices, may_hold_notice, shells_left};
use crate::door::tool_clock::{
  BackgroundCall, BackgroundClock, BackgroundCut, Owns, ToolClock, subagent_uses,
};
use crate::events::{Bus, Named};
use crate::lines::place::{Place, places_dir};
use crate::outbox::{self, Keep, Notice};

/// What the watch keeps of a place in its own file beside the record: the tasks already settled — told to their
/// room, or ended where no room is told — and the clock's cuts on each task, written as the wake says them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kept {
  #[serde(default)]
  pub told: BTreeSet<String>,
  #[serde(default)]
  pub cuts: BTreeMap<String, Vec<String>>,
}

/// The file the watch keeps of a place.
pub fn kept_file(place: &Path) -> PathBuf {
  place.join("background-told.json")
}

fn read_kept(place: &Path) -> Kept {
  agent_host::disk::read_json(&kept_file(place)).unwrap_or_default()
}

fn write_kept(place: &Path, kept: &Kept) {
  let written = serde_json::to_vec(kept)
    .map_err(std::io::Error::other)
    .and_then(|bytes| agent_host::disk::publish_bytes(&kept_file(place), &bytes));

  if let Err(why) = written {
    eprintln!("background work: {}: {why}", kept_file(place).display());
  }
}

/// How a task stands, out of its place's background record.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Task {
  pub launch: Launch,
  /// The notice that ended it, where one did.
  pub notice: Option<TaskNotice>,
  /// Whether a stop ended it.
  pub stopped: bool,
}

/// The tasks a background record holds, each with what ended it; a notice or a stop for a task the record never
/// saw launched is passed over, and the first ending a task met is the one it keeps.
pub fn tasks_of(lines: &[BackgroundLine]) -> BTreeMap<String, Task> {
  let mut tasks: BTreeMap<String, Task> = BTreeMap::new();

  for line in lines {
    match line {
      BackgroundLine::Launched(launch) => {
        tasks.entry(launch.task.clone()).or_insert_with(|| Task {
          launch: launch.clone(),
          ..Task::default()
        });
      }
      BackgroundLine::Notice { notice, .. } if notice.ended() => {
        if let Some(task) = tasks.get_mut(&notice.task).filter(|task| task.ended().is_none()) {
          task.notice = Some(notice.clone());
        }
      }
      BackgroundLine::Stopped { task, .. } => {
        if let Some(task) = tasks.get_mut(task).filter(|task| task.ended().is_none()) {
          task.stopped = true;
        }
      }
      BackgroundLine::Notice { .. } => {}
    }
  }

  tasks
}

/// What became of a task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Came {
  /// Its notice ended it.
  Ended,
  /// A stop ended it.
  Stopped,
  /// Its program is gone with no notice of it.
  Lost,
  /// Its program dropped it, with no notice of it anywhere.
  Gone,
}

impl Task {
  fn ended(&self) -> Option<Came> {
    if self.stopped {
      return Some(Came::Stopped);
    }

    self.notice.as_ref().map(|_| Came::Ended)
  }

  /// What became of the task, its program standing as `program` names it — the moment the program standing now
  /// was started, or none where none stands: an ending its record holds, or lost where no program stands or the
  /// one standing was started after the task was launched; none while it still runs.
  pub fn came(&self, program: Option<i64>) -> Option<Came> {
    self.ended().or(match program {
      Some(started_at) if started_at <= self.launch.at => None,
      _ => Some(Came::Lost),
    })
  }
}

/// The word a task stands by once it came to be as `came` says: the word its notice ended on, `stopped` where a
/// stop ended it, `lost` where its program went without a notice, `gone` where its program dropped it.
pub fn ending_word(task: &Task, came: &Came) -> String {
  match came {
    Came::Ended => task.notice.as_ref().and_then(|notice| notice.status.clone()).unwrap_or_default(),
    Came::Stopped => "stopped".to_owned(),
    Came::Lost => "lost".to_owned(),
    Came::Gone => "gone".to_owned(),
  }
}

/// A task as `GET /background-work` and its events answer it, standing by `status`.
pub fn member(task: &Task, status: &str) -> BackgroundWork {
  let launch = &task.launch;

  BackgroundWork {
    task: launch.task.clone(),
    kind: launch.kind.clone(),
    description: launch.description.clone(),
    command: launch.command.clone(),
    room_id: launch.room.as_deref().and_then(|room| room.parse().ok()),
    agent_name: launch.agent.clone(),
    launched_at: launch.at,
    status: status.to_owned(),
  }
}

/// The word a task still running stands by.
pub const RUNNING: &str = "running";

// The places under the data root that hold a background record, each by its agent.
fn recorded_places(data_root: &Path) -> Vec<Place> {
  let Ok(entries) = std::fs::read_dir(places_dir(data_root)) else { return vec![] };

  entries
    .flatten()
    .map(|entry| entry.path())
    .filter(|path| background_file(path).exists())
    .filter_map(|path| path.file_name().and_then(|name| name.to_str()).map(str::to_owned))
    .map(|agent| Place::of(data_root, &agent))
    .collect()
}

/// The background work every place's program still runs, oldest first, of `room` alone where one is named.
pub fn running_work(data_root: &Path, room: Option<i64>) -> Vec<BackgroundWork> {
  let mut work: Vec<BackgroundWork> = recorded_places(data_root)
    .iter()
    .flat_map(|place| {
      let started_at = place.program_alive().map(|running| running.started_at as i64);
      let settled = read_kept(&place.path).told;

      tasks_of(&read_background(&place.path))
        .into_values()
        .filter(move |task| task.came(started_at).is_none() && !settled.contains(&task.launch.task))
        .map(|task| member(&task, RUNNING))
    })
    .filter(|work| room.is_none() || work.room_id == room)
    .collect();

  work
    .sort_by(|one, other| one.launched_at.cmp(&other.launched_at).then_with(|| one.task.cmp(&other.task)));
  work
}

// One task told on the stream under `kind`.
fn tell(events: &Bus, kind: &'static str, work: &BackgroundWork) {
  match serde_json::to_value(work) {
    Ok(body) => events.publish_named(Named {
      id: 0,
      at: 0,
      name: Cow::Borrowed(kind),
      body,
    }),
    Err(why) => eprintln!("background work: task {} was not told as {kind}: {why}", work.task),
  }
}

// The deployment's clock for a moment, as a person in the room reads it.
fn clock(at: i64) -> String {
  chrono::DateTime::from_timestamp_millis(at + 9 * 3_600_000)
    .map_or_else(String::new, |at| at.format("%H:%M").to_string())
}

fn kind_of(task: &Task) -> &'static str {
  let words = &BACKGROUND_WORK.words;

  if task.launch.kind == "shell" { &words.shell } else { &words.subagent }
}

/// The order the room's supervisor is handed for a task, as it came to be.
pub fn order(task: &Task, came: &Came, cuts: &[String]) -> String {
  let words = &BACKGROUND_WORK.words;
  let launched = clock(task.launch.at);
  let cuts = cuts.concat();
  let description = if task.launch.description.is_empty() {
    task.launch.task.as_str()
  } else {
    &task.launch.description
  };

  match (came, &task.notice) {
    (Came::Ended, Some(notice)) => {
      let result = notice.result.as_deref().or(notice.summary.as_deref()).map_or_else(
        || words.nothing_handed.clone(),
        |result| crate::text::within(result, BACKGROUND_WORK.result_chars).to_string(),
      );

      fill_marks(
        &words.ended,
        &[
          ("launched", &launched),
          ("kind", kind_of(task)),
          ("description", description),
          ("status", notice.status.as_deref().unwrap_or_default()),
          ("cuts", &cuts),
          ("result", &result),
          ("output", notice.output.as_deref().unwrap_or_default()),
        ],
      )
    }
    _ => fill_marks(
      &words.lost,
      &[
        ("launched", &launched),
        ("kind", kind_of(task)),
        ("description", description),
        ("why", if *came == Came::Stopped { &words.stopped } else { &words.program_gone }),
        ("cuts", &cuts),
      ],
    ),
  }
}

/// The instruction the receiver is handed for a task told to its room: an order for the supervisor that
/// launched it, in that room, opening a turn there. Work its program dropped is told nowhere: the program ended it
/// itself, and the turn that did so is where it was said.
pub fn instruction(task: &Task, came: &Came, cuts: &[String]) -> Option<Value> {
  let room = task.launch.room.as_deref()?;
  let words = &BACKGROUND_WORK.words;
  let description = if task.launch.description.is_empty() {
    task.launch.task.as_str()
  } else {
    &task.launch.description
  };
  let (kind, summary) = match came {
    Came::Ended => ("background-ended", &words.summary_ended),
    Came::Stopped | Came::Lost => ("background-lost", &words.summary_lost),
    Came::Gone => return None,
  };

  Some(json!({
    "room": room.parse::<i64>().map_or_else(|_| json!(room), |room| json!(room)),
    "agentName": task.launch.agent,
    "said": { "order": order(task, came, cuts) },
    "sort": "background",
    "kind": format!("{kind}:{}", task.launch.task),
    "summary": fill_marks(summary, &[("description", description)]),
    "cause": "reply",
    "publisher": "agency",
  }))
}

/// The line the wake carries for one cut of the clock.
pub fn cut_line(cut: &BackgroundCut) -> String {
  let words = &BACKGROUND_WORK.words;

  fill_marks(
    &words.cut,
    &[
      ("call", crate::text::within(&cut.command, 200)),
      ("minutes", &cut.minutes.to_string()),
      ("why", if cut.stalled { &words.stalled } else { &words.idle }),
    ],
  )
}

/// A subagent's own record, beside the session record its launch named.
pub fn subagent_record(launch: &Launch) -> Option<PathBuf> {
  let transcript = launch.transcript.as_deref()?;
  let session = transcript.strip_suffix(".jsonl")?;

  Some(Path::new(session).join("subagents").join(format!("agent-{}.jsonl", launch.task)))
}

/// The calls of a running task the clock holds: a background shell by its output file; a subagent's shell
/// calls still in flight by their commands, and the shells it put in its own background by their output files,
/// as its own record names them.
pub fn calls_of(task: &Task) -> Vec<BackgroundCall> {
  let launch = &task.launch;

  if launch.kind == "shell" {
    return vec![BackgroundCall {
      id: launch.task.clone(),
      command: launch.command.clone().unwrap_or_default(),
      owns: Owns::Output(launch.task.clone()),
      started_at: launch.at,
    }];
  }

  let Some(records) = subagent_record(launch).and_then(|path| transcript_since(&path, i64::MIN)) else {
    return vec![];
  };
  let in_flight =
    subagent_uses(&records)
      .into_iter()
      .filter(|used| used.name == "Bash")
      .filter_map(|used| {
        let command = used.input.get("command").and_then(Value::as_str)?.to_owned();
        let started_at = used.started_at.unwrap_or(launch.at);

        Some(BackgroundCall {
          id: format!("{}/{}", launch.task, used.id),
          owns: Owns::Command {
            command: command.clone(),
            started_at,
          },
          command,
          started_at,
        })
      });
  let left = shells_left(&records).into_iter().map(|shell| BackgroundCall {
    id: format!("{}/{}", launch.task, shell.task),
    command: shell.command,
    owns: Owns::Output(shell.task),
    started_at: shell.at,
  });

  in_flight.chain(left).collect()
}

/// What the watch holds between passes: how far it has read each place's record, the tasks read so far, the tasks
/// it has told the stream are running, how far it has read each running task's session record and the notices
/// found there, when a stop of its session first left each running task out, and the clock over each place's
/// program.
#[derive(Debug, Default)]
pub struct Watch {
  places: BTreeMap<PathBuf, Read>,
}

#[derive(Debug)]
struct Read {
  offset: u64,
  lines: Vec<BackgroundLine>,
  running: BTreeSet<String>,
  sessions: BTreeMap<PathBuf, Session>,
  left: BTreeMap<String, i64>,
  clock: BackgroundClock,
}

// How far a session record was read, and the ended notices found in it by task.
#[derive(Debug, Default)]
struct Session {
  offset: u64,
  ended: BTreeMap<String, TaskNotice>,
}

// The whole lines a file gained past `offset`, and where it ends now; a file shorter than `offset` was started
// again and is read from its start.
fn grown_text(path: &Path, offset: u64) -> Option<(String, u64, bool)> {
  use std::io::{Read as _, Seek as _, SeekFrom};

  let mut file = std::fs::File::open(path).ok()?;
  let size = file.metadata().ok()?.len();
  let again = size < offset;
  let from = if again { 0 } else { offset };

  file.seek(SeekFrom::Start(from)).ok()?;

  let mut bytes = vec![];

  file.read_to_end(&mut bytes).ok()?;

  // Only whole lines are read: a line still being appended is read on the next pass.
  let whole = bytes.iter().rposition(|byte| *byte == b'\n').map_or(0, |at| at + 1);

  Some((String::from_utf8_lossy(&bytes[..whole]).into_owned(), from + whole as u64, again))
}

// The lines a background record gained past `offset`, as `grown_text` reads them.
fn grown(path: &Path, offset: u64) -> Option<(Vec<BackgroundLine>, u64, bool)> {
  let (text, offset, again) = grown_text(path, offset)?;
  let lines = text.lines().filter_map(|line| serde_json::from_str(line).ok()).collect();

  Some((lines, offset, again))
}

impl Read {
  // The ended notices each running task's session record gained since the last look, kept by task: a task still
  // running under the program standing, and not settled. A line is parsed only where it may carry a notice, since
  // a session record grows by megabytes.
  fn read_sessions(
    &mut self,
    tasks: &BTreeMap<String, Task>,
    program: Option<i64>,
    settled: &BTreeSet<String>,
  ) {
    let paths: BTreeSet<PathBuf> = tasks
      .values()
      .filter(|task| task.came(program).is_none() && !settled.contains(&task.launch.task))
      .filter_map(|task| task.launch.transcript.as_deref().map(PathBuf::from))
      .collect();

    for path in paths {
      let session = self.sessions.entry(path.clone()).or_default();
      let Some((text, offset, again)) = grown_text(&path, session.offset) else { continue };

      if again {
        session.ended.clear();
      }

      session.offset = offset;

      for line in text.lines().filter(|line| may_hold_notice(line)) {
        let Ok(record) = serde_json::from_str::<Value>(line) else { continue };

        for notice in ended_notices(&record) {
          session.ended.entry(notice.task.clone()).or_insert(notice);
        }
      }
    }
  }

  // The notice a running task's session record holds for it, where its background record holds none.
  fn session_notice(&self, task: &Task) -> Option<TaskNotice> {
    let path = PathBuf::from(task.launch.transcript.as_deref()?);

    self.sessions.get(&path)?.ended.get(&task.launch.task).cloned()
  }

  // A running task a stop of its own session, made after its launch, leaves out: the moment the first such stop
  // was made is kept, so the grace counts from it.
  fn note_left(&mut self, task: &Task, stop: Option<&HookRecord>) {
    let Some(stop) = stop else { return };
    let same_session =
      stop.payload.get("session_id").and_then(Value::as_str) == task.launch.session.as_deref();
    let named = stop
      .payload
      .get("background_tasks")
      .and_then(Value::as_array)
      .into_iter()
      .flatten()
      .any(|named| {
        named.get("id").and_then(Value::as_str) == Some(task.launch.task.as_str())
          && named.get("status").and_then(Value::as_str) == Some(RUNNING)
      });

    if same_session && stop.at > task.launch.at && !named {
      self.left.entry(task.launch.task.clone()).or_insert(stop.at);
    }
  }
}

/// One pass over every place under the data root that holds a background record: what ended told to its room
/// through the outbox, and every launch and end told on `events`.
pub async fn pass(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  events: &Bus,
  data_root: &Path,
  watch: &mut Watch,
) {
  for place in recorded_places(data_root) {
    look_at(db, vars, events, &place, watch).await;
  }
}

async fn look_at(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  events: &Bus,
  place: &Place,
  watch: &mut Watch,
) {
  let read = watch.places.entry(place.path.clone()).or_insert_with(|| Read {
    offset: 0,
    lines: vec![],
    running: BTreeSet::new(),
    sessions: BTreeMap::new(),
    left: BTreeMap::new(),
    clock: BackgroundClock::new(ToolClock::from(&TOOL_CLOCK.call)),
  });

  if let Some((lines, offset, again)) = grown(&background_file(&place.path), read.offset) {
    if again {
      read.lines.clear();
    }

    read.lines.extend(lines);
    read.offset = offset;
  }

  let mut tasks = tasks_of(&read.lines);
  let mut kept = read_kept(&place.path);
  let before = kept.clone();
  let program = place.program_alive();
  let started_at = program.as_ref().map(|running| running.started_at as i64);
  let now = crate::now_ms();
  let gone_ms = (BACKGROUND_WORK.gone_seconds * 1_000) as i64;
  let stop = last_stop(&place.path);
  let mut running: Vec<BackgroundCall> = vec![];

  read.read_sessions(&tasks, started_at, &kept.told);

  for task in tasks.values_mut().filter(|task| task.ended().is_none()) {
    if let Some(notice) = read.session_notice(task) {
      task.notice = Some(notice);
    }
  }

  for (id, task) in &tasks {
    if task.came(started_at).is_none() && !kept.told.contains(id) {
      read.note_left(task, stop.as_ref());
    }

    let dropped = read.left.get(id).is_some_and(|left| now - left >= gone_ms);
    let came = task.came(started_at).or_else(|| dropped.then_some(Came::Gone));

    match came {
      None if kept.told.contains(id) => {}
      None => {
        if read.running.insert(id.clone()) {
          tell(events, "background-work.created", &member(task, RUNNING));
        }

        running.extend(calls_of(task));
      }
      Some(_) if kept.told.contains(id) => {}
      Some(came) => {
        let cuts = kept.cuts.get(id).cloned().unwrap_or_default();

        // A task with no room is told nowhere: its notice opened a turn no round carries.
        if let Some(body) = instruction(task, &came, &cuts) {
          let agent = task.launch.agent.as_deref().unwrap_or_default();
          let notice = Notice::instruction(body, Keep::Once).keyed(format!("background:{agent}:{id}"));

          if let Err(why) = outbox::owe(db, vars, notice).await {
            eprintln!("background work: {agent}: task {id} was not owed to its room: {why}");
            continue;
          }

          println!(
            "background work: {agent}: task {id} told to room {} as {came:?}",
            task.launch.room.as_deref().unwrap_or_default()
          );
        }

        kept.told.insert(id.clone());
        read.running.remove(id);
        tell(events, "background-work.ended", &member(task, &ending_word(task, &came)));
      }
    }
  }

  // The clock reads the processes of a program on this machine alone.
  if let Some(program) = program.filter(|_| place.node().is_none()) {
    for cut in read.clock.look(program.pid, &running, now).await {
      let task = cut.id.split('/').next().unwrap_or_default().to_owned();

      println!(
        "background work: {}: the clock cut {} of task {task} after {} minutes ({})",
        place.path.display(),
        cut.command,
        cut.minutes,
        if cut.stalled { "stalled" } else { "idle" }
      );
      kept.cuts.entry(task).or_default().push(cut_line(&cut));
    }
  }

  if kept != before {
    write_kept(&place.path, &kept);
  }
}
