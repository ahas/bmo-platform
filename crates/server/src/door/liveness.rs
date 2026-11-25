//! The life of a turn put into a program's pane (docs/agent-control.md, 「Telling that a turn's program is
//! gone」): which process the turn's program is, what its own signals last said, and the judgement each look
//! makes of them. A turn ends only on what it observes — its record closing the turn, its pane gone, its
//! program's process gone, or no program to name while the pane shows none and nothing the program writes
//! moves — and never on a span. What the turn read is kept as one row of the store's `turn_liveness`, opened
//! as the turn opens, written as its wait looks and closed with its ending, so a later reading says what
//! the turn was waiting on.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use agent_host::identity::{ProcessIdentity, process_birth};
use sea_orm::ConnectionTrait;
use serde_json::{Value, json};
use store::{Fault, all, run};

use super::looks::Look;
use super::place_run::{hooks_file, status_file};
use crate::lines::place::Place;
use crate::now_ms;

/// How long an ended row is kept: a week, as an execution row is.
const KEPT_MS: i64 = 7 * 24 * 3_600_000;

/// The ending a row the process before this one left open is closed with.
pub const RESTARTED: &str = "restarted";

/// A turn's program, by its process: its pid and its start time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
  pub pid: i32,
  pub birth: Option<String>,
}

/// What can be said of the turn's program on one look: the process it was is alive, it is gone, or no
/// process can be named for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Peer {
  Alive(Program),
  Gone(Program),
  Unnamed,
}

/// The program's own signals, as one look read them: how long its record is, and when its place last took
/// a status-line call and a hook event. Any of them moving between two looks is the program speaking.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Signals {
  pub record_size: u64,
  pub status_line_at: Option<i64>,
  pub hook_at: Option<i64>,
}

/// An ending a look observed that the record does not say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lost {
  /// The process the turn was put into is gone, or its place stands another in its stead.
  Program,
  /// No program can be named for the turn, the pane shows none, and nothing it writes moved.
  SilentPeer,
}

impl Lost {
  /// The word the ending goes by.
  pub fn word(self) -> &'static str {
    match self {
      Self::Program => "program-lost",
      Self::SilentPeer => "silent-peer",
    }
  }
}

/// The judgement the looks of one turn make, once its words went in: the signals read on the first of the
/// looks in a row that found no program to name and the pane showing none, against which the next such look
/// asks whether anything moved.
#[derive(Debug, Default)]
pub struct Judge {
  quiet: Option<Signals>,
}

impl Judge {
  /// What one look observed: none where the turn goes on. A record that closed the turn is the record's
  /// to read, so nothing is judged over it; a program gone is lost at once; a program alive is a live peer,
  /// however long it takes. Where no program can be named, a pane herdr reads as holding none (`unknown`),
  /// standing `blocked` or back at rest is the other end gone once a second such look in a row finds none of
  /// the program's signals moved.
  pub fn look(&mut self, peer: &Peer, closed: bool, herdr: &Look, signals: &Signals) -> Option<Lost> {
    if closed {
      self.quiet = None;
      return None;
    }

    match peer {
      Peer::Gone(_) => {
        self.quiet = None;
        Some(Lost::Program)
      }
      Peer::Alive(_) => {
        self.quiet = None;
        None
      }
      Peer::Unnamed => {
        let shows_none = herdr.is("unknown") || herdr.is("blocked") || herdr.at_rest();

        if !shows_none {
          self.quiet = None;
          return None;
        }

        if self.quiet.as_ref() == Some(signals) {
          return Some(Lost::SilentPeer);
        }

        self.quiet = Some(signals.clone());
        None
      }
    }
  }
}

/// Whether the process an argument list names carries a session: the session's id standing right after the
/// flag that opens it or the one that resumes it.
pub fn carries_session(args: &[String], session: &str) -> bool {
  args
    .windows(2)
    .any(|pair| matches!(pair[0].as_str(), "--session-id" | "--resume" | "-r") && pair[1] == session)
}

/// The process on this machine whose command line carries a session, the earliest started where several
/// do; none where none does. A pane of a run's own runs its program as the pane's own command, and the
/// session it opens is that run's alone.
pub fn program_of_session(session: &str) -> Option<Program> {
  if session.is_empty() {
    return None;
  }

  let entries = std::fs::read_dir("/proc").ok()?;
  let mut found: Vec<(u64, Program)> = entries
    .flatten()
    .filter_map(|entry| entry.file_name().to_str()?.parse::<i32>().ok())
    .filter_map(|pid| {
      let line = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
      let args: Vec<String> = line
        .split(|byte| *byte == 0)
        .filter(|arg| !arg.is_empty())
        .map(|arg| String::from_utf8_lossy(arg).into_owned())
        .collect();

      if !carries_session(&args, session) {
        return None;
      }

      let birth = process_birth(pid)?;
      let started = birth.parse::<u64>().unwrap_or(u64::MAX);

      Some((
        started,
        Program {
          pid,
          birth: Some(birth),
        },
      ))
    })
    .collect();

  found.sort_by_key(|(started, program)| (*started, program.pid));
  found.into_iter().next().map(|(_, program)| program)
}

/// Whether a process of this machine is still the one named.
pub fn alive_here(program: &Program) -> bool {
  program.birth.as_ref().is_some_and(|birth| {
    ProcessIdentity {
      pid: program.pid,
      birth: birth.clone(),
    }
    .alive()
  })
}

/// What can be said of a standing program the turn was put into, read off its place: alive while the place
/// still records that process and it runs, gone once it no longer runs or the place records another in its
/// stead.
pub fn standing_peer(place: &Place, program: Option<&Program>) -> Peer {
  let Some(program) = program else {
    return Peer::Unnamed;
  };
  let recorded = place.running();
  let replaced = recorded.as_ref().is_some_and(|running| running.pid != program.pid && running.pid > 0);

  if replaced || !place.alive(program.pid, program.birth.as_deref()) {
    Peer::Gone(program.clone())
  } else {
    Peer::Alive(program.clone())
  }
}

/// What can be said of a pane's own program: alive or gone where it was found, unnamed where it never was.
pub fn own_peer(program: Option<&Program>) -> Peer {
  match program {
    Some(program) if alive_here(program) => Peer::Alive(program.clone()),
    Some(program) => Peer::Gone(program.clone()),
    None => Peer::Unnamed,
  }
}

fn modified_ms(path: &Path) -> Option<i64> {
  let modified = std::fs::metadata(path).ok()?.modified().ok()?;

  i64::try_from(modified.duration_since(UNIX_EPOCH).ok()?.as_millis()).ok()
}

/// The program's signals now: its record's length, and when its place last took a status-line call and a
/// hook event, where it stands in a place.
pub fn signals_of(record: &Path, place: Option<&Path>) -> Signals {
  Signals {
    record_size: std::fs::metadata(record).map(|meta| meta.len()).unwrap_or(0),
    status_line_at: place.and_then(|place| modified_ms(&status_file(place))),
    hook_at: place.and_then(|place| modified_ms(&hooks_file(place))),
  }
}

/// The phase a turn's row is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
  /// Its program is being stood, or waited for to come to rest.
  Standing,
  /// Its words are going in.
  Prompting,
  /// Its words were seen taken.
  Working,
}

impl Phase {
  fn word(self) -> &'static str {
    match self {
      Self::Standing => "standing",
      Self::Prompting => "prompting",
      Self::Working => "working",
    }
  }
}

/// One turn's row, as its wait keeps it: written only where something in it changed.
#[derive(Clone, Debug)]
pub struct Liveness {
  pub execution: String,
  run: String,
  line: String,
  session: String,
  phase: Phase,
  pane: Option<String>,
  program: Option<Program>,
  records: Vec<PathBuf>,
  herdr: Option<String>,
  herdr_at: Option<i64>,
  signals: Signals,
  record_grew_at: Option<i64>,
  opened_at: i64,
  written: Option<Value>,
}

impl Liveness {
  /// A turn's row as it opens, not yet written.
  pub fn new(execution: &str, run: &str, line: &str, session: &str, records: Vec<PathBuf>) -> Self {
    Self {
      execution: execution.to_owned(),
      run: run.to_owned(),
      line: line.to_owned(),
      session: session.to_owned(),
      phase: Phase::Standing,
      pane: None,
      program: None,
      records,
      herdr: None,
      herdr_at: None,
      signals: Signals::default(),
      record_grew_at: None,
      opened_at: now_ms(),
      written: None,
    }
  }

  pub fn phase(&mut self, phase: Phase) {
    self.phase = phase;
  }

  pub fn pane(&mut self, pane: &str) {
    self.pane = Some(pane.to_owned());
  }

  pub fn program(&mut self, program: Option<&Program>) {
    self.program = program.cloned();
  }

  /// What herdr last read the pane as; a look herdr did not answer leaves the last reading standing.
  pub fn herdr(&mut self, look: &Look) {
    if matches!(look, Look::Silent(_)) {
      return;
    }

    self.herdr = Some(look.word().to_owned());
    self.herdr_at = Some(now_ms());
  }

  /// The program's signals as a look read them, the moment its record last grew among them.
  pub fn signals(&mut self, signals: &Signals) {
    if signals.record_size != self.signals.record_size {
      self.record_grew_at = Some(now_ms());
    }

    self.signals = signals.clone();
  }

  // What the row says, without the moments a look writes each time: what tells a change apart.
  fn state(&self) -> Value {
    json!({
      "phase": self.phase.word(),
      "pane": self.pane,
      "pid": self.program.as_ref().map(|program| program.pid),
      "birth": self.program.as_ref().and_then(|program| program.birth.clone()),
      "herdr": self.herdr,
      "size": self.signals.record_size,
      "status": self.signals.status_line_at,
      "hook": self.signals.hook_at,
    })
  }

  /// The row written where it changed since it was last written: a store that would not take it is said and
  /// never fails the turn.
  pub async fn write<C: ConnectionTrait>(&mut self, db: &C) {
    let state = self.state();

    if self.written.as_ref() == Some(&state) {
      return;
    }

    if let Err(why) = self.upsert(db, None).await {
      eprintln!("{}: its liveness row: {why:?}", self.run);
      return;
    }

    self.written = Some(state);
  }

  /// The row closed with the turn's ending and the reading it stood on.
  pub async fn close<C: ConnectionTrait>(&mut self, db: &C, ending: &str, detail: &str) {
    if let Err(why) = self.upsert(db, Some((ending, detail))).await {
      eprintln!("{}: its liveness row's ending ({ending}): {why:?}", self.run);
    }
  }

  async fn upsert<C: ConnectionTrait>(&self, db: &C, ending: Option<(&str, &str)>) -> Result<(), Fault> {
    let now = now_ms();
    let records: Vec<String> = self.records.iter().map(|path| path.display().to_string()).collect();
    let (ending, ended_at, detail) = match ending {
      Some((ending, detail)) => (Some(ending.to_owned()), Some(now), Some(detail.to_owned())),
      None => (None, None, None),
    };

    run(
      db,
      r#"INSERT INTO "turn_liveness" ("execution", "run", "line", "session", "phase", "pane", "program_pid",
           "program_birth", "records", "herdr", "herdr_at", "record_size", "record_grew_at", "status_line_at",
           "hook_at", "opened_at", "written_at", "ending", "ended_at", "detail")
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
         ON CONFLICT ("execution") DO UPDATE SET "phase" = EXCLUDED."phase", "pane" = EXCLUDED."pane",
           "program_pid" = EXCLUDED."program_pid", "program_birth" = EXCLUDED."program_birth",
           "records" = EXCLUDED."records", "herdr" = EXCLUDED."herdr", "herdr_at" = EXCLUDED."herdr_at",
           "record_size" = EXCLUDED."record_size", "record_grew_at" = EXCLUDED."record_grew_at",
           "status_line_at" = EXCLUDED."status_line_at", "hook_at" = EXCLUDED."hook_at",
           "written_at" = EXCLUDED."written_at", "ending" = EXCLUDED."ending",
           "ended_at" = EXCLUDED."ended_at", "detail" = EXCLUDED."detail""#,
      vec![
        self.execution.clone().into(),
        self.run.clone().into(),
        self.line.clone().into(),
        self.session.clone().into(),
        self.phase.word().into(),
        self.pane.clone().into(),
        self.program.as_ref().map(|program| i64::from(program.pid)).into(),
        self.program.as_ref().and_then(|program| program.birth.clone()).into(),
        json!(records).to_string().into(),
        self.herdr.clone().into(),
        self.herdr_at.into(),
        i64::try_from(self.signals.record_size).unwrap_or(i64::MAX).into(),
        self.record_grew_at.into(),
        self.signals.status_line_at.into(),
        self.signals.hook_at.into(),
        self.opened_at.into(),
        now.into(),
        ending.into(),
        ended_at.into(),
        detail.into(),
      ],
    )
    .await?;

    Ok(())
  }
}

/// The rows ended more than a week before `now` taken out.
pub async fn prune<C: ConnectionTrait>(db: &C, now: i64) -> Result<u64, Fault> {
  run(db, r#"DELETE FROM "turn_liveness" WHERE "ended_at" < $1"#, vec![(now - KEPT_MS).into()]).await
}

/// One row a process left open: its execution, its run, its phase, its pane and the program it named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeftOpen {
  pub execution: String,
  pub run: String,
  pub line: String,
  pub phase: String,
  pub pane: Option<String>,
  pub program: Option<Program>,
  pub herdr: Option<String>,
  pub record_grew_at: Option<i64>,
}

/// Every row no ending closed.
pub async fn left_open<C: ConnectionTrait>(db: &C) -> Result<Vec<LeftOpen>, Fault> {
  let rows = all(
    db,
    r#"SELECT "execution", "run", "line", "phase", "pane", "program_pid", "program_birth", "herdr",
         "record_grew_at" FROM "turn_liveness" WHERE "ended_at" IS NULL ORDER BY "opened_at""#,
    vec![],
  )
  .await?;
  let mut open = Vec::with_capacity(rows.len());

  for row in rows {
    let pid: Option<i64> = row.try_get("", "program_pid")?;

    open.push(LeftOpen {
      execution: row.try_get("", "execution")?,
      run: row.try_get("", "run")?,
      line: row.try_get("", "line")?,
      phase: row.try_get("", "phase")?,
      pane: row.try_get("", "pane")?,
      program: pid.and_then(|pid| i32::try_from(pid).ok()).map(|pid| Program {
        pid,
        birth: row.try_get("", "program_birth").ok().flatten(),
      }),
      herdr: row.try_get("", "herdr")?,
      record_grew_at: row.try_get("", "record_grew_at")?,
    });
  }

  Ok(open)
}

/// A row a process left open closed with an ending and what was found of its turn.
pub async fn close_left<C: ConnectionTrait>(
  db: &C,
  execution: &str,
  ending: &str,
  detail: &str,
) -> Result<(), Fault> {
  run(
    db,
    r#"UPDATE "turn_liveness" SET "ending" = $2, "ended_at" = $3, "detail" = $4, "written_at" = $3
       WHERE "execution" = $1 AND "ended_at" IS NULL"#,
    vec![execution.into(), ending.into(), now_ms().into(), detail.into()],
  )
  .await?;

  Ok(())
}

/// What a row left open says of its turn as a server finds it: its phase, its pane, its program and whether
/// that process still runs, what herdr last read and when the record last grew.
pub fn found_of(left: &LeftOpen) -> String {
  let program = match &left.program {
    Some(program) => {
      format!("program {} {}", program.pid, if alive_here(program) { "still alive" } else { "gone" })
    }
    None => "no program named".to_owned(),
  };

  format!(
    "{} on {}: phase {}, pane {}, {program}, herdr last read {}, record last grew at {}",
    left.run,
    left.line,
    left.phase,
    left.pane.as_deref().unwrap_or("none"),
    left.herdr.as_deref().unwrap_or("nothing"),
    left.record_grew_at.map_or_else(|| "never".to_owned(), |at| at.to_string()),
  )
}

/// Every row the process before this one left open closed as `restarted`, with what is found of each turn
/// now, each written to the log as well: how many. Run as a server starts, before any place that process
/// left is taken down.
pub async fn close_left_open<C: ConnectionTrait>(db: &C) -> Result<usize, Fault> {
  let open = left_open(db).await?;

  for left in &open {
    let found = found_of(left);

    println!("agency: a turn the last process left open: {found}");
    close_left(db, &left.execution, RESTARTED, &found).await?;
  }

  Ok(open.len())
}

/// What a place holds of its program as a sweep finds it, for the log before the place is taken down: the
/// program its record names and whether that process runs, and what the pane's head was last answered.
pub fn place_found(place: &Place) -> String {
  let program = match place.running() {
    Some(running) => format!(
      "program {} {} (identity {})",
      running.pid,
      if place.alive(running.pid, running.birth.as_deref()) { "alive" } else { "gone" },
      running.identity
    ),
    None => "no program recorded".to_owned(),
  };
  let head = std::fs::read_to_string(place.path.join("head-answer.txt"))
    .map(|said| said.trim().to_owned())
    .unwrap_or_else(|_| "nothing".to_owned());

  format!("{program}; its head was last answered: {head}")
}

tokio::task_local! {
  // The ending the last turn put through the door under one session turn's send ended on.
  static DOOR_ENDING: std::cell::RefCell<Option<&'static str>>;
}

/// A session turn's send run so the door's ending of every turn it puts through a program's pane is kept for
/// its end: the last of them is the word the send's wait ended on.
/// The send is held on the heap: a session turn's future is deep, and the scope around it adds to it.
pub async fn keeping_endings<F: Future>(send: F) -> F::Output {
  DOOR_ENDING.scope(std::cell::RefCell::new(None), Box::pin(send)).await
}

/// The ending a turn through the door came to, kept for the send it runs under; nothing where it runs under
/// none.
pub fn kept_ending(word: &'static str) {
  let _ = DOOR_ENDING.try_with(|kept| *kept.borrow_mut() = Some(word));
}

/// The ending the last turn through the door under this send came to, where one went through it.
pub fn door_ending() -> Option<&'static str> {
  DOOR_ENDING.try_with(|kept| *kept.borrow()).ok().flatten()
}
