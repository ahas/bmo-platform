//! A turn in flight across agency's own restart, picked up again (docs/sessions.md, 「A restart hands the
//! session over」): every turn sent is kept in the settings under its run while it goes, with what its end
//! needs to reach its caller — the session and the turn's id, the agent, the room, its run in the ledger and
//! its job. A server that starts reads each one left by the process before it in the place of the pane
//! program that carries it, the agent's or that of the thrown-away session it went on, as that program
//! writes its turns down: a dsh pane's in its record and inbox, a Claude Code pane's in the place's run,
//! the hooks' calls and the session's transcript. A turn whose end the program wrote is handed back through
//! the path a live turn's end takes, a turn the program is still working on, or has not taken yet, is
//! attached to and waited on with the program held, its thrown-away session's place kept until its end is
//! handed back and ended then, one no job holds cut at the bound of an adopted turn, and a turn nothing names,
//! or its program never took, is ended as lost.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use agent_session::{Apart, SessionId, TurnEnd};
use policy::fill_marks;
use policy::resident_turn::RESIDENT_TURN;
use policy::supervisor_runner::SUPERVISOR_RUNNER;
use policy::tool_clock::TOOL_CLOCK;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use store::{all, one};

use crate::door::aside;
use crate::door::execution::{Row, cancelled, publish_envelope};
use crate::door::inbox::{
  State, provider_named, read_turn_on, record_of, span_named, span_named_after, waiting_in_inbox,
};
use crate::door::place_run::{end_run, latest_failure, read_hooks, read_run, transcript_since};
use crate::door::record::{
  CLAUDE_CODE, Notices, Reply, RoundState, Tail, UsageSpent, answer_since, answered_since, prompted_since,
  read_round, read_turn, turn_closed_since, turn_error,
};
use crate::door::tool_clock::{CallVerdict, ClockNote, InProcessCut, ToolClock, Watch as ToolWatch};
use crate::door::turn::{AsideOutcome, takes_words_mid_turn};
use crate::herdr_backend::{RoadWalked, TurnEnded, close_turn_run, spent_of};
use crate::job_stall::JobStall;
use crate::lines::place::Place;
use crate::places::Ground;
use crate::places::locks::{agent_lock_path, hold};
use crate::queue::orderer::JobService;
use crate::queue::settings;
use crate::queue::stops::Stops;
use crate::run_clock::{CallAsks, PickedUpQuestion};
use crate::run_log::lines::Line;
use crate::run_log::{RunLog, Target};
use crate::worker_session::records::{note_answer, slot_account};
use crate::{Refusal, now_ms};

/// The settings family a turn in flight is kept under, by its run.
pub const PREFIX: &str = "turn_in_flight:";

/// What a turn's end needs to reach its caller after the process that sent it is gone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InFlight {
  pub session: String,
  pub turn: i64,
  pub run: String,
  /// The key its caller sent it under, which a repeat of the send attaches by (docs/api.md, 「sessions」).
  #[serde(default)]
  pub key: Option<String>,
  pub agent: String,
  #[serde(default)]
  pub chat_id: Option<i64>,
  /// The turn's run in the ledger, a chat turn's or a role session's; a job's run is the job's own.
  #[serde(default)]
  pub ledger: Option<i64>,
  #[serde(default)]
  pub job: Option<i64>,
  pub started_at: i64,
}

/// A turn kept while it goes; a store that would not take it is said and never fails the turn.
pub async fn keep(db: &sea_orm::DatabaseConnection, turn: &InFlight) {
  let text = serde_json::to_string(turn).unwrap_or_default();

  if let Err(why) = settings::set(db, &format!("{PREFIX}{}", turn.run), &text).await {
    eprintln!("{}: the turn in flight was not kept: {}", turn.run, why.detail);
  }
}

/// The settings family the node a kept turn waits on is kept under while it waits, by its run.
pub const NODE_WAIT_PREFIX: &str = "turn_node_wait:";

/// The node a kept turn waits on marked beside its record while it waits, or the mark taken away once it waits
/// no more (docs/person-nodes.md, 「Which node is a person's」); a store that would not take it is said.
pub async fn mark_node_wait(db: &sea_orm::DatabaseConnection, run: &str, node: Option<&str>) {
  let key = format!("{NODE_WAIT_PREFIX}{run}");
  let marked = match node {
    Some(node) => settings::set(db, &key, node).await,
    None => settings::forget(db, &key).await,
  };

  if let Err(why) = marked {
    eprintln!("{run}: the node it waits on was not marked beside it: {}", why.detail);
  }
}

/// The node a kept turn was waiting on, where it was marked so; a store that cannot be read names none.
pub async fn node_waited(db: &sea_orm::DatabaseConnection, run: &str) -> Option<String> {
  settings::get(db, &format!("{NODE_WAIT_PREFIX}{run}")).await.ok().flatten()
}

// Why a kept turn nothing names is ended: the node it waited on, where it waited for one.
fn lost_why(waited: Option<&str>) -> String {
  waited.map_or_else(
    || LOST.to_owned(),
    |node| format!("agency restarted while the turn waited for {node}'s link, so no program took it"),
  )
}

/// A turn no longer kept, the run and why written to the log: its end reached its caller, was handed back,
/// or nothing names it any more (docs/sessions.md, 「A restart hands the session over」). The node it waited on is
/// let go with it.
pub async fn forget(db: &sea_orm::DatabaseConnection, run: &str, why: &str) {
  mark_node_wait(db, run, None).await;

  match settings::forget(db, &format!("{PREFIX}{run}")).await {
    Ok(()) => println!("{run}: the turn is no longer kept in flight: {why}"),
    Err(refusal) => eprintln!("{run}: the turn kept in flight was not let go ({why}): {}", refusal.detail),
  }
}

// The runs of the turns this process carries now.
static CARRIED: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<String>>> =
  std::sync::LazyLock::new(Default::default);

fn carried() -> std::sync::MutexGuard<'static, std::collections::HashSet<String>> {
  CARRIED.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A turn this process carries, marked from its start until the mark goes with its end: its own end closes
/// it, so the queue's pass leaves it alone, though its place's run ends between the roads its walk goes out
/// on (docs/sessions.md, 「A turn whose program dies」).
pub struct Carrying(String);

impl Carrying {
  pub fn of(run: &str) -> Self {
    carried().insert(run.to_owned());

    Self(run.to_owned())
  }
}

impl Drop for Carrying {
  fn drop(&mut self) {
    carried().remove(&self.0);
  }
}

/// Whether this process carries a turn under a run now.
pub fn carried_here(run: &str) -> bool {
  carried().contains(run)
}

/// Whether a run's turn is kept in flight, which the pass ending a job left running leaves to this one.
pub async fn kept(db: &sea_orm::DatabaseConnection, run: &str) -> bool {
  settings::get(db, &format!("{PREFIX}{run}")).await.ok().flatten().is_some()
}

/// Whether a turn of an agent's is kept in flight, its program carrying it whoever waits on it.
pub async fn carried_for(db: &sea_orm::DatabaseConnection, agent: &str) -> bool {
  in_flight(db).await.is_ok_and(|turns| turns.iter().any(|turn| turn.agent == agent))
}

/// The turns kept in flight, as the store holds them.
pub async fn in_flight(db: &sea_orm::DatabaseConnection) -> Result<Vec<InFlight>, Refusal> {
  let mut turns = vec![];

  for run in settings::named(db, PREFIX).await? {
    if let Some(text) = settings::get(db, &format!("{PREFIX}{run}")).await? {
      match serde_json::from_str::<InFlight>(&text) {
        Ok(turn) => turns.push(turn),
        Err(_) => forget(db, &run, "its kept row does not read").await,
      }
    }
  }

  Ok(turns)
}

/// How a picked-up turn came out: what the record holds of its end, or why it has none.
#[derive(Clone, Debug, PartialEq)]
pub enum Came {
  Answered(Box<Reply>),
  Failed(String),
}

/// Where a kept turn stands in its agent's pane program's record.
#[derive(Clone, Debug, PartialEq)]
pub enum Standing {
  /// Its end is in the record.
  Ended(Came),
  /// The program is working on it, or has it waiting in its inbox.
  Going,
  /// No record names it.
  Lost,
}

/// The name of the place whose program carries a kept turn: the thrown-away session's own place where the
/// turn went on one, a job's turn among them (docs/api.md, 「sessions」), the agent's otherwise.
pub fn place_name_of(turn: &InFlight) -> String {
  let session = SessionId(turn.session.clone());

  if crate::herdr_backend::is_apart(&session) {
    crate::apart_places::place_name(&session)
  } else {
    turn.agent.clone()
  }
}

/// Where a kept turn stands, read from the place its program carries it in, as that program writes its
/// turns down: a dsh pane's in its record and its inbox, a Claude Code pane's in the place's run and the
/// hooks' calls (docs/sessions.md, 「A restart hands the session over」).
pub fn standing(place: &Place, turn: &InFlight) -> Standing {
  let records = record_of(&place.path).1;
  let Some(span) = span_named(&records, &turn.run) else {
    if waiting_in_inbox(&place.path, &turn.run) {
      return Standing::Going;
    }

    return standing_on_hooks(place, turn);
  };

  standing_in(&records[span])
}

// Where a turn of a Claude Code pane stands, which keeps no record of agency's but the place's run, its
// hooks' calls and the session's own transcript: going while the place's run is the turn's, the program
// took it — a submit since the run began — and no end has closed it; ended once the hooks have closed the
// run, its answer or its error read from the transcript the hooks name, from the run's start; lost where
// the run is another's or the program never took it.
fn standing_on_hooks(place: &Place, turn: &InFlight) -> Standing {
  let Some(run) =
    read_run(&place.path).filter(|run| !run.person && run.name.as_deref() == Some(turn.run.as_str()))
  else {
    return Standing::Lost;
  };
  let hooks = read_hooks(&place.path, run.started_at);

  if !hooks.iter().any(|hook| hook.kind == "submit") {
    return Standing::Lost;
  }

  if run.ended_at.is_none() {
    return Standing::Going;
  }

  // The hooks of the run's session name its transcript, the latest naming it the one it ended in.
  let named = |key: &str| {
    hooks
      .iter()
      .rev()
      .find_map(|hook| hook.payload.get(key).and_then(Value::as_str).map(str::to_owned))
  };
  let session = named("session_id").unwrap_or_default();
  let lines = named("transcript_path")
    .map(|transcript| transcript_here(place, &transcript, &session))
    .and_then(|transcript| transcript_since(&transcript, run.started_at))
    .unwrap_or_default();
  let failure = latest_failure(&place.path, run.started_at).map(|failure| failure.failure());

  let notices = if crate::lines::place::carries_background(&place.path) {
    Notices::Apart
  } else {
    Notices::Joined
  };

  Standing::Ended(came_of(&lines, &session, failure, notices))
}

/// The transcript a Claude Code pane in a place writes its turn into, as this machine holds it: the one its hooks
/// name, where the place stands here; and for a place on a node, whose hooks name the node's path, that session's
/// record under the configuration directory and folder of the place's program as this machine reads them, where
/// the link brings the node's transcript back (docs/sessions.md, 「A restart hands the session over」).
pub fn transcript_here(place: &Place, named: &str, session: &str) -> PathBuf {
  let on_node = place.node().is_some() && !session.is_empty();
  let mirrored = on_node.then(|| place.program()).flatten().and_then(|program| {
    let config = program.command.env.get("CLAUDE_CONFIG_DIR").filter(|dir| !dir.is_empty())?;

    harness_providers::claude_code::ClaudeCode::records_under(
      Path::new(config),
      &program.command.cwd,
      session,
    )
    .into_iter()
    .next()
  });

  mirrored.unwrap_or_else(|| PathBuf::from(named))
}

// How a Claude Code turn came out, read from its transcript's lines since it went in: its error, the
// failure its place recorded, no answer, or its round's answer.
fn came_of(
  lines: &[Value],
  session: &str,
  failure: Option<(String, Option<String>)>,
  notices: Notices,
) -> Came {
  let read = read_turn(lines, 0);
  let answered = answered_since(lines, 0);

  if let Some((error, _)) = turn_error(&read, answered, failure) {
    return Came::Failed(error);
  }

  if !answered {
    return Came::Failed(NO_ANSWER.to_owned());
  }

  let mut reply = read.reply(session, read.spend.clone());
  let round = read_round(lines, notices);

  if !round.text.is_empty() {
    reply.text = round.text;
  }

  reply.answers = round.answers;

  Came::Answered(Box::new(reply))
}

/// A turn's pane of its own, which no place stands for — a role's turn on Claude Code's own provider, a
/// job's among them: the execution its run opened, the pane and the program its liveness row names, the
/// session's transcript it reads, when the execution began, and the road and folder the row names.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnPane {
  pub execution: String,
  pub pane: Option<String>,
  pub transcript: std::path::PathBuf,
  pub session: String,
  pub program: Option<crate::door::liveness::Program>,
  pub started_at: i64,
  pub provider: String,
  pub model: String,
  pub effort: Option<String>,
  pub role: Option<String>,
  pub line: Option<String>,
  pub cwd: Option<String>,
}

/// The pane of its own a kept turn went out in, found through the open execution its run opened and that
/// execution's liveness row; none where the run opened no such execution. The program is the one the row
/// names, else the process whose command line carries the session.
pub async fn own_pane(db: &sea_orm::DatabaseConnection, run: &str) -> Option<OwnPane> {
  let row = one(
    db,
    r#"SELECT l."execution", l."pane", l."session", l."records", l."program_pid", l."program_birth",
              e."body"
       FROM "turn_liveness" l JOIN "agent_executions" e ON e."id" = l."execution"
       WHERE l."run" = $1 AND (e."body"::jsonb)->>'endedAt' IS NULL
         AND COALESCE((e."body"::jsonb)->'resident', 'null'::jsonb) = 'null'::jsonb
       ORDER BY l."opened_at" DESC LIMIT 1"#,
    vec![run.into()],
  )
  .await
  .ok()
  .flatten()?;
  let text = |column: &str| row.try_get::<Option<String>>("", column).ok().flatten();
  let records: Vec<String> = text("records").and_then(|records| serde_json::from_str(&records).ok())?;
  let body: Value = text("body").and_then(|body| serde_json::from_str(&body).ok())?;
  let session = text("session").unwrap_or_default();
  let pid = row.try_get::<Option<i64>>("", "program_pid").ok().flatten();
  let program = match pid.and_then(|pid| i32::try_from(pid).ok()) {
    Some(pid) => Some(crate::door::liveness::Program {
      pid,
      birth: text("program_birth"),
    }),
    None => crate::door::liveness::program_of_session(&session),
  };

  let named = |key: &str| body.get(key).and_then(Value::as_str).map(str::to_owned);

  Some(OwnPane {
    execution: text("execution")?,
    pane: text("pane"),
    transcript: records.first().map(std::path::PathBuf::from)?,
    session,
    program: program.clone(),
    started_at: body.get("startedAt").and_then(Value::as_i64).unwrap_or_default(),
    provider: named("provider").unwrap_or_default(),
    model: named("model").unwrap_or_default(),
    effort: named("effort"),
    role: named("role"),
    line: named("line"),
    cwd: program.and_then(|program| program_cwd(program.pid)),
  })
}

// The folder a program stands in, as its own process names it.
fn program_cwd(pid: i32) -> Option<String> {
  std::fs::read_link(format!("/proc/{pid}/cwd"))
    .ok()
    .map(|path| path.to_string_lossy().into_owned())
}

// A thrown-away session's scope, remembered so its state answers by id while its picked-up turn goes on
// (docs/sessions.md, 「A restart hands the session over」): the provider, model and folder its execution row
// names, and the role, line and folder the row and its program hold.
async fn carried_apart(ground: &Ground, turn: &InFlight, place: &Place, pane: Option<&OwnPane>) {
  let session = SessionId(turn.session.clone());

  if !crate::herdr_backend::is_apart(&session) {
    return;
  }

  let scope = match pane {
    Some(pane) => Some(Apart {
      provider: pane.provider.clone(),
      model: pane.model.clone(),
      effort: pane.effort.clone(),
      cwd: pane.cwd.clone().unwrap_or_default(),
      role: pane.role.clone(),
      line: pane.line.clone(),
      ..Apart::default()
    }),
    None => {
      let cwd = place.running().and_then(|running| program_cwd(running.pid));
      let row = open_rows(ground, &turn.run).await.into_iter().next();

      row.map(|row| Apart {
        provider: row.text("provider").unwrap_or_default().to_owned(),
        model: row.text("model").unwrap_or_default().to_owned(),
        effort: row.text("effort").map(str::to_owned),
        cwd: cwd.unwrap_or_default(),
        role: row.text("role").map(str::to_owned),
        line: row.text("line").map(str::to_owned),
        ..Apart::default()
      })
    }
  };

  if let Some(scope) = scope {
    crate::herdr_backend::remember_carried(ground, session, turn.agent.clone(), scope);
  }
}

/// Where a turn in a pane of its own stands, read from its session's transcript since its execution began:
/// lost where the program never took it, no words handed over standing there, whatever the program is doing;
/// going while the round there is still working and its program stands; ended once the round has ended, its
/// answer or its error read from it; ended failed where the program is gone with the round open.
pub fn standing_in_pane(pane: &OwnPane) -> Standing {
  let lines = transcript_since(&pane.transcript, pane.started_at).unwrap_or_default();

  standing_in_lines(pane, &lines)
}

// Where a kept turn in a pane of its own stands, read off its transcript's lines from the turn's beginning.
fn standing_in_lines(pane: &OwnPane, lines: &[Value]) -> Standing {
  if !prompted_since(lines, 0) {
    return Standing::Lost;
  }

  // A pane of its own carries a role's turn, whose line never carries its background apart.
  let round = read_round(lines, Notices::Joined);

  match round.state {
    RoundState::Interrupted => Standing::Ended(Came::Failed("the turn was interrupted".to_owned())),
    RoundState::Ended if turn_closed_since(lines, 0) => {
      Standing::Ended(came_of(lines, &pane.session, None, Notices::Joined))
    }
    _ if pane.program.as_ref().is_some_and(crate::door::liveness::alive_here) => Standing::Going,
    _ => Standing::Ended(Came::Failed(GONE.to_owned())),
  }
}

// Why a turn whose end its program wrote with no answer ended.
const NO_ANSWER: &str = "the pane program wrote no answer";

/// Where a turn put into a place under a name stands, the turns put in under that name before it being
/// `before`: read from the place's record where the program took it, going where it waits in the inbox, and
/// lost where neither holds it.
pub fn standing_after(place: &Place, name: &str, before: usize) -> Standing {
  let records = record_of(&place.path).1;
  let Some(span) = span_named_after(&records, name, before) else {
    let taken = (0..).take_while(|count| span_named_after(&records, name, *count).is_some()).count();

    return if taken == before && waiting_in_inbox(&place.path, name) {
      Standing::Going
    } else {
      Standing::Lost
    };
  };

  standing_in(&records[span])
}

// Where a turn stands, read from its lines of the record.
fn standing_in(lines: &[Value]) -> Standing {
  let provider = provider_named(lines).unwrap_or_default();
  let (state, error, reply) = read_turn_on(lines, "", &provider);

  match state {
    State::Working => Standing::Going,
    State::Interrupted => Standing::Ended(Came::Failed("the turn was interrupted".to_owned())),
    State::Ended => Standing::Ended(match error {
      Some(error) => Came::Failed(error),
      None if reply.text.is_empty() => Came::Failed(NO_ANSWER.to_owned()),
      None => Came::Answered(Box::new(reply)),
    }),
  }
}

// Why a turn no record names ended.
const LOST: &str = "agency restarted while the turn went, and no record its program keeps names it";
// Why a turn whose program went away while it was attached to ended.
const GONE: &str = "the pane program went away while the turn was picked up after agency restarted";

// What a picked-up turn's log says where it begins to follow the turn, and where it cannot.
const FOLLOWS: &str = "The turn was picked up again after agency restarted. Its log follows it from here.";
const CANNOT_FOLLOW: &str = "The turn was picked up again after agency restarted, and its log cannot follow it: \
                             the record its program writes holds nothing this process can read.";
const NOTED: &str = "The turn was picked up again after agency restarted.";

// Whether the line saying where the log stands with a picked-up turn is written yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Said {
  Nothing,
  Follows,
  Cannot,
}

/// A picked-up turn's record read into its run log while the turn goes on (docs/sessions.md, 「A restart
/// hands the session over」): what the process before this one filed is passed over, and each look files
/// what the record gained since, so a screen following the log follows the turn rather than standing at the
/// line the restart left. Where no record this process can read names the turn, the log says so instead.
struct Following {
  log: RunLog,
  source: String,
  // A Claude Code pane's transcript, read as it grows, how many of its lines are filed, and where the turn's
  // own lines begin in it.
  tail: Option<Tail>,
  fed: usize,
  begins: usize,
  // A dsh pane's record, and how many of the turn's progress and answer lines are filed.
  place: Option<PathBuf>,
  place_fed: usize,
  said: Said,
}

impl Following {
  /// The writer of a picked-up turn's lines, armed on the record this look names where one names it.
  async fn of(ground: &Ground, turn: &InFlight, place: &Place, pane: Option<&OwnPane>) -> Self {
    let mut following = Self {
      log: RunLog::new(Target {
        agent: turn.agent.clone(),
        run: turn.run.clone(),
        chat_id: turn.chat_id,
        job_id: turn.job,
      }),
      source: String::new(),
      tail: None,
      fed: 0,
      begins: 0,
      place: None,
      place_fed: 0,
      said: Said::Nothing,
    };

    following.arm(ground, turn, place, pane).await;

    following
  }

  // Arms the reader on the record this look names, where one names it and no reader stands yet.
  async fn arm(&mut self, ground: &Ground, turn: &InFlight, place: &Place, pane: Option<&OwnPane>) {
    if self.tail.is_some() || self.place.is_some() {
      return;
    }

    if let Some(pane) = pane {
      self.source = CLAUDE_CODE.to_owned();

      let (tail, fed, begins) = reading(ground, turn, &pane.transcript, pane.started_at).await;

      self.begins = begins;
      self.tail = Some(tail);
      self.fed = fed;

      return;
    }

    let (_, records) = record_of(&place.path);

    if let Some(span) = span_named(&records, &turn.run) {
      self.source = provider_named(&records[span]).unwrap_or_default();
      self.place = Some(place.path.clone());
      self.place_fed = filed(ground, turn).await;

      return;
    }

    // A Claude Code pane in a place keeps its turn in the session's transcript its hooks name, not in the
    // place's own record.
    if let Some((transcript, since)) = claude_place(place, turn) {
      self.source = CLAUDE_CODE.to_owned();

      let (tail, fed, begins) = reading(ground, turn, &transcript, since).await;

      self.begins = begins;
      self.tail = Some(tail);
      self.fed = fed;
    }
  }

  /// Files what the turn's record gained since the last look, saying once where the log stands with it.
  async fn feed(&mut self, ground: &Ground, turn: &InFlight, place: &Place, pane: Option<&OwnPane>) {
    self.arm(ground, turn, place, pane).await;

    let armed = self.tail.is_some() || self.place.is_some();
    let say = match (armed, self.said) {
      (true, Said::Nothing | Said::Cannot) => Some((FOLLOWS, Said::Follows)),
      (false, Said::Nothing) => Some((CANNOT_FOLLOW, Said::Cannot)),
      _ => None,
    };

    if let Some((line, said)) = say {
      self.log.write(&ground.db, Line::note(line)).await;
      self.said = said;
    }

    if let Some(tail) = &mut self.tail {
      let count = tail.look().len();

      // A record replaced under the reader is read again from where its newest filed line stands.
      if count < self.fed {
        let newest = newest_source(ground, turn).await;

        self.fed = Spots::of(tail.path(), i64::MAX, newest.as_deref()).resumed.unwrap_or(0);
      }

      let raw = tail.raw();

      if raw.len() > self.fed {
        self.log.feed(&ground.db, &raw[self.fed..], &self.source).await;
        self.fed = raw.len();
      }

      // The reader holds the turn's lines from its beginning, and the lines as written until they are filed.
      tail.forget_before(self.begins.min(self.fed));
      tail.forget_raw_before(self.fed);

      // What the turn has written so far answers the progress read under its own id.
      shown(ground, turn, answer_since(tail.look(), self.begins));

      return;
    }

    let Some(path) = &self.place else { return };
    let (written, records) = record_of(path);
    let Some(span) = span_named(&records, &turn.run) else { return };

    shown(ground, turn, read_turn_on(&records[span.clone()], "", &self.source).2.text);

    let mut seen = 0usize;
    let mut fresh = vec![];

    for (line, record) in written[span.clone()].iter().zip(&records[span]) {
      if matches!(record.get("kind").and_then(Value::as_str), Some("progress" | "answer")) {
        if seen >= self.place_fed {
          fresh.push(line.clone());
        }

        seen += 1;
      }
    }

    if !fresh.is_empty() {
      self.log.feed(&ground.db, &fresh, &self.source).await;
      self.place_fed += fresh.len();
    }
  }
}

impl Following {
  // Where the turn in a pane of its own stands, read off the lines its reader holds rather than the transcript
  // parsed again; the transcript is read whole only where no reader is armed on it.
  fn standing_in(&mut self, pane: &OwnPane) -> Standing {
    let begins = self.begins;

    match self.tail.as_mut() {
      Some(tail) => standing_in_lines(pane, tail.look().get(begins..).unwrap_or_default()),
      None => standing_in_pane(pane),
    }
  }
}

// What a picked-up turn has written so far, shown to the progress read under its own id.
fn shown(ground: &Ground, turn: &InFlight, text: String) {
  ground.outputs.shown(&SessionId(turn.session.clone()), turn.turn, text);
}

// One whole transcript read at a time by the pickup: every turn in flight is picked up at once at start,
// several of them on the supervisor's one long transcript, and each is read through twice, a line at a time,
// before its reader holds only the lines from where the turn begins.
static WHOLE_READS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

// A transcript read from the line the run's log last took out of it, else from the turn's beginning: the
// transcript as it stands now, holding only the lines from where either begins, how many of its lines are
// filed, and where the turn's lines begin.
async fn reading(ground: &Ground, turn: &InFlight, transcript: &Path, since: i64) -> (Tail, usize, usize) {
  let newest = newest_source(ground, turn).await;
  let _one = WHOLE_READS.acquire().await;
  let spots = Spots::of(transcript, since, newest.as_deref());
  let fed = spots.resumed.unwrap_or(spots.begins).min(spots.count);
  let mut tail = Tail::from_line(transcript, spots.begins.min(fed));

  tail.look();

  (tail, fed, spots.begins)
}

// Where a picked-up turn's lines stand in its transcript, read a line at a time so no more than one line is
// held while the whole is read.
struct Spots {
  // The first line stamped at or after the turn began, else the end.
  begins: usize,
  // The line after the newest one the run's log filed, by the id that line carried; none where the
  // transcript no longer names it.
  resumed: Option<usize>,
  count: usize,
}

impl Spots {
  fn of(transcript: &Path, since: i64, newest: Option<&str>) -> Self {
    let mut begins = None;
    let mut resumed = None;
    let mut count = 0;

    crate::door::record::each_line(transcript, |at, record| {
      if begins.is_none() && stamped_since(&record, since) {
        begins = Some(at);
      }

      if newest.is_some_and(|newest| names_source(&record, newest)) {
        resumed = Some(at + 1);
      }

      count = at + 1;
    });

    Self {
      begins: begins.unwrap_or(count),
      resumed,
      count,
    }
  }
}

// The newest line of the run's log that came out of its program's record, by the id the record carried: where
// a transcript's lines are filed again, they begin after the record that id names.
async fn newest_source(ground: &Ground, turn: &InFlight) -> Option<String> {
  one(
    &ground.db,
    r#"SELECT "source_id" FROM "agent_log"
       WHERE "agent" = $1 AND "run" = $2 AND "source_id" IS NOT NULL
       ORDER BY "id" DESC LIMIT 1"#,
    vec![turn.agent.clone().into(), turn.run.clone().into()],
  )
  .await
  .ok()
  .flatten()
  .and_then(|row| row.try_get::<Option<String>>("", "source_id").ok().flatten())
}

// Whether a record is the one a filed line came from, by the id it carried.
fn names_source(record: &Value, newest: &str) -> bool {
  record.pointer("/message/id").and_then(Value::as_str) == Some(newest)
    || record.get("uuid").and_then(Value::as_str) == Some(newest)
}

// Whether a record was stamped at or after a moment.
fn stamped_since(record: &Value, since: i64) -> bool {
  record
    .get("timestamp")
    .and_then(Value::as_str)
    .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
    .is_some_and(|at| at.timestamp_millis() >= since)
}

// The transcript a Claude Code pane in a place writes its turn into, and when that run began, where the
// place's run is the turn's and its hooks name the session's transcript.
fn claude_place(place: &Place, turn: &InFlight) -> Option<(PathBuf, i64)> {
  let run =
    read_run(&place.path).filter(|run| !run.person && run.name.as_deref() == Some(turn.run.as_str()))?;
  let hooks = read_hooks(&place.path, run.started_at);
  let transcript = hooks
    .iter()
    .rev()
    .find_map(|hook| hook.payload.get("transcript_path").and_then(Value::as_str))?;
  let session = hooks
    .iter()
    .rev()
    .find_map(|hook| hook.payload.get("session_id").and_then(Value::as_str))
    .unwrap_or_default();

  Some((transcript_here(place, transcript, session), run.started_at))
}

/// Every turn the process before this one left in flight, read at start from the place its program carries
/// it in: handed back where it ended, attached to where it goes on, the lease bound to its run then standing
/// as seen and its open execution rows kept as this process's own, ended as lost where nothing names it or
/// its program never took it, the pane of its own closed then. A job's lost turn is let go, and its job is
/// launched again from the words it keeps.
pub async fn at_start(ground: &Arc<Ground>) -> Result<usize, Refusal> {
  let turns = in_flight(&ground.db).await?;
  let count = turns.len();

  for turn in turns {
    let name = place_name_of(&turn);
    let place = Place::of(&ground.roots.data, &name);

    // The turn answers the progress read under the id it went in with, and a repeat of its send attaches to
    // it by its caller's key, from before its end is read until that end is handed back.
    ground.outputs.begun(&SessionId(turn.session.clone()), turn.turn, &turn.run);

    if let Some(key) = &turn.key {
      ground.turn_keys.stand(&turn.session, key);
    }

    let (now, pane) = found(ground, &place, &turn).await;
    // The pane of its own the turn went out in, closed once its end is handed back, though the turn is read
    // in its place: a job's turn has its hooks write into a place of its own and stands in a pane of its own.
    let own = match &pane {
      Some(pane) => Some(pane.clone()),
      None => own_pane(&ground.db, &turn.run).await,
    };

    match now {
      Standing::Ended(came) => {
        let mut following = Following::of(ground, &turn, &place, pane.as_ref()).await;

        following.feed(ground, &turn, &place, pane.as_ref()).await;
        hand_back(ground, &place, &turn, came, true).await;
        close_pane(ground, own.as_ref()).await;
      }
      // No program took a job's turn, so the job is launched again from the words it keeps; the pane of its
      // own that stands on nothing is closed with its rows.
      Standing::Lost if turn.job.is_some() => {
        let session = SessionId(turn.session.clone());

        ground.outputs.ended(&session, turn.turn);

        // A job's caller is the job itself, launched again from the words it keeps; a send repeated under
        // its key is answered failed as the turn's end.
        if let Some(key) = &turn.key {
          let end = TurnEnd::failed(&turn.run, LOST.to_owned(), None);

          ground
            .turn_keys
            .ended(
              &ground.db,
              &turn.session,
              key,
              &TurnEnd {
                turn: turn.turn,
                ..end
              },
            )
            .await;
        }

        close_rows(ground, &turn.run, false).await;
        close_pane(ground, own.as_ref()).await;
        forget(&ground.db, &turn.run, "no program took it; its job is launched again").await
      }
      Standing::Lost => {
        let why = lost_why(node_waited(&ground.db, &turn.run).await.as_deref());

        hand_back(ground, &place, &turn, Came::Failed(why), false).await;
        close_pane(ground, own.as_ref()).await;
      }
      Standing::Going => {
        // The adopted turn stands as seen: the lease bound to its run is given back by its end, or by its
        // program found gone, and never waits on a sight of it this process would not have.
        if let Err(why) = crate::order_run::dispatch::seen_standing(&ground.db, &turn.run).await {
          eprintln!("{}: the lease bound to the adopted turn was not marked seen: {why}", turn.run);
        }

        // Its rows are this process's from now, before any pass of the executions reads their collector.
        own_rows(ground, &turn.run).await;
        carried_apart(ground, &turn, &place, pane.as_ref()).await;

        let ground = ground.clone();

        tokio::spawn(async move { attach(&ground, &name, place, pane, own, turn).await });
      }
    }
  }

  Ok(count)
}

// Where a kept turn stands as the pickup reads it: in its place's record, else in a pane of its own, which a
// turn no place names may have gone out in.
async fn found(ground: &Ground, place: &Place, turn: &InFlight) -> (Standing, Option<OwnPane>) {
  let now = standing(place, turn);

  if now != Standing::Lost {
    return (now, None);
  }

  match own_pane(&ground.db, &turn.run).await {
    Some(pane) => (standing_in_pane(&pane), Some(pane)),
    None => (Standing::Lost, None),
  }
}

/// The turns this process carries that a restart would cut (docs/sessions.md, 「A restart hands the session
/// over」, what the deployment's quiet counts): each no job's, and one no program's record names yet, so the
/// pickup at the next start, reading as it does here, would find nothing to attach to. A job's turn is left
/// out, since the lease pass takes its item again; a turn waiting for its node's link, which the next start ends; a turn this process does not carry, which the reap or the
/// next start ends; and a turn of a supervisor removed, which ends with it.
pub async fn unresumable(ground: &Ground) -> Result<Vec<InFlight>, Refusal> {
  let mut cut = vec![];

  for turn in in_flight(&ground.db).await? {
    if turn.job.is_some() || !carried_here(&turn.run) || !agent_stands(ground, &turn).await {
      continue;
    }

    let place = Place::of(&ground.roots.data, &place_name_of(&turn));

    // A turn waiting for its node's link has started nothing, and the next server ends it with why
    // (docs/sessions.md, 「A restart hands the session over」), so the deployment waits on none.
    if found(ground, &place, &turn).await.0 == Standing::Lost
      && node_waited(&ground.db, &turn.run).await.is_none()
    {
      cut.push(turn);
    }
  }

  Ok(cut)
}

// Whether the agent a kept turn is carried for still stands: a thrown-away session's turn stands with its
// session, any other with its agent's row, so the turns of a supervisor removed are no work a restart would cut
// (docs/supervisors.md, 「Adding and removing a supervisor」). A row that cannot be read counts the turn.
async fn agent_stands(ground: &Ground, turn: &InFlight) -> bool {
  if crate::herdr_backend::is_apart(&SessionId(turn.session.clone())) {
    return true;
  }

  crate::places::AgentRow::named(&ground.db, &turn.agent)
    .await
    .map_or(true, |row| row.is_some())
}

// A turn the program goes on with, waited on with its place's program held, so no turn of this process
// goes into it meanwhile, and handed back once the record says it ended; a program gone is a turn gone. A
// cancellation of its run cuts it short with its surface's interrupt, as the door carrying a live turn
// does, and the record's end of it is handed back as any other. A thrown-away session's place it was
// carried in is ended once the end is handed back, its session ending with its turn, and the pane of the
// turn's own, `own`, is closed, as the door closes it with its run, whether the turn was read in it (`pane`) or
// in its place.
async fn attach(
  ground: &Ground,
  name: &str,
  place: Place,
  pane: Option<OwnPane>,
  own: Option<OwnPane>,
  turn: InFlight,
) {
  {
    let _held = hold(agent_lock_path(&ground.roots.data, name)).await;

    waited(ground, &place, pane.as_ref(), &turn, ToolClock::from(&TOOL_CLOCK.call)).await;
  }

  close_pane(ground, own.as_ref()).await;

  if crate::apart_places::is_apart_place(&ground.roots.data, name) {
    crate::apart_places::end(ground, name).await;
  }

  crate::apart_places::end_run_places(ground, &turn.run, name).await;
}

// A pane of the turn's own closed once its end is handed back.
async fn close_pane(ground: &Ground, pane: Option<&OwnPane>) {
  if let (Some(herdr), Some(pane)) = (&ground.herdr, pane.and_then(|pane| pane.pane.as_deref())) {
    let _ = herdr.call("pane.close", json!({ "pane_id": pane }), None).await;
  }

  // Whatever the adopted run started ends with it (docs/run-groups.md).
  if let Some(own) = pane
    && let Some(line) = crate::run_groups::end_of_run(&ground.roots.data, &own.execution).await
  {
    println!("{line}");
  }
}

/// Whether a picked-up turn's tool calls are held to the call clock, as its live turn's were: a job's turn,
/// a role session's and a supervisor's are, and a conversation a caller opened is not
/// (docs/agent-control.md, 「A run's tool calls are held to a clock」).
pub fn holds_call_clock(turn: &InFlight) -> bool {
  let session = agent_session::SessionId(turn.session.clone());

  turn.job.is_some() || !crate::herdr_backend::is_conversation(&session)
}

// An adopted turn waited on until its end is handed back: in its place, or in its pane of its own. Its
// record is read into the run log on every look, so the log follows the turn as a live one's would. A
// job's turn, a role session's and a supervisor's are held to the same tool clock as a door's: a blocked
// call is cut so the turn can go on, and a stop cuts it so the stop's words can be read. A job's turn past
// its bound is cut and its item released, and a call of a job's gone silent and idle is asked about. The
// asks opened on its calls close with it.
pub async fn waited(
  ground: &Ground,
  place: &Place,
  pane: Option<&OwnPane>,
  turn: &InFlight,
  clock: ToolClock,
) {
  let mut asks = turn
    .job
    .map(|job| CallAsks::new(&turn.run, &turn.agent, job, JobService::of(ground.vars())));
  let mut stall = turn.job.map(JobStall::new);

  followed(ground, place, pane, turn, clock, asks.as_mut(), stall.as_mut()).await;

  if let Some(asks) = asks.as_mut() {
    asks.ended(&ground.db, &ground.events, None).await;
  }

  if let Some(stall) = stall.as_mut() {
    stall.ended(&ground.db, ground.vars()).await;
  }
}

async fn followed(
  ground: &Ground,
  place: &Place,
  pane: Option<&OwnPane>,
  turn: &InFlight,
  clock: ToolClock,
  mut asks: Option<&mut CallAsks>,
  mut stall: Option<&mut JobStall>,
) {
  let every = Duration::from_millis(RESIDENT_TURN.inbox_poll_ms);
  let mut unseen = 0u32;
  let mut cut = false;
  let mut stop_seen_at: Option<i64> = None;
  let clocked = holds_call_clock(turn);
  let stop_bound_ms = clocked.then_some(clock.stop_bound_ms);
  let grace_ms = clock.in_process_grace_ms;
  let grace_cut = clock.in_process_grace_cut.clone();
  let mut in_process_cut: Option<InProcessCut> = None;
  let mut tools = clocked.then(|| ToolWatch::taken_up(clock));
  let mut following = Following::of(ground, turn, place, pane).await;
  let provider = pane
    .map(|pane| pane.provider.as_str())
    .filter(|provider| !provider.is_empty())
    .unwrap_or(CLAUDE_CODE);
  // Words offered to the run while it goes, as the live door listens for them (docs/sessions.md, 「A restart
  // hands the session over」): every offer it did not take is answered refused as the turn's end lets it go.
  let listening = takes_words_mid_turn(provider).then(|| aside::listen(&turn.run));

  own_rows(ground, &turn.run).await;

  loop {
    let now = now_ms();
    let standing = match pane {
      Some(pane) => following.standing_in(pane),
      None => standing(place, turn),
    };

    match standing {
      Standing::Ended(came) => {
        following.feed(ground, turn, place, pane).await;

        return hand_back(ground, place, turn, came, true).await;
      }
      Standing::Lost => {
        following.feed(ground, turn, place, pane).await;

        return hand_back(ground, place, turn, Came::Failed(LOST.to_owned()), true).await;
      }
      Standing::Going => {}
    }

    // A turn past its bound is cut, as the door cuts a live one: a job's with its item released, its stop
    // mark written first, so the end of the turn reads it and the job ends stopped.
    if let Some(why) = over_bound(ground, turn, now).await {
      // The turn's program is cut short as the door cuts one past its bound, so it does not go on
      // working a job whose item and queue have been released.
      surface_interrupt(ground, &turn.run, pane).await;
      following.feed(ground, turn, place, pane).await;

      return hand_back(ground, place, turn, Came::Failed(why), true).await;
    }

    following.feed(ground, turn, place, pane).await;

    if let (Some(watch), Some(tail)) = (tools.as_mut(), following.tail.as_ref()) {
      watch.subagent_records(tail.path(), now);
    }

    let records = following.tail.as_mut().map(Tail::look);
    let program = tool_program(place, pane);

    if let (Some(watch), Some(records)) = (tools.as_mut(), records) {
      watch_tools(ground, turn, records, program, watch, asks.as_deref_mut(), &mut following.log, now)
        .await;
    }

    // A call with no process of its own is cut by words put into the working turn, as the door cuts one:
    // the program ends the waiting call on them and hands the model a failed result. A program that does
    // not take them is ended at the cut's grace.
    if let Some(watch) = tools.as_mut() {
      let target = target_of(place, pane);

      while let Some(cut) = watch.take_in_process_cut() {
        let said = format!("{}: the call {} was cut, its words put into the turn", turn.run, cut.tool);

        println!("{said}");
        following.log.write(&ground.db, Line::note(&said)).await;

        if takes_words_mid_turn(provider) {
          if let (Some(herdr), Some(target)) = (&ground.herdr, target.as_deref()) {
            let _ = herdr.call("agent.prompt", json!({ "target": target, "text": cut.words }), None).await;
          }
        } else {
          surface_interrupt(ground, &turn.run, pane).await;
        }

        if in_process_cut.is_none() {
          in_process_cut = Some(cut);
        }
      }

      if !watch.in_process_cut_still() {
        in_process_cut = None;
      }

      // A call gone silent and idle is asked about as the door asks about one, and only a cut on it stops
      // the call, with words put into the turn.
      while let Some(silent) = watch.take_silent() {
        let verdict = match asks.as_deref_mut() {
          Some(asks) => asks.hear(&ground.db, &ground.events, &silent).await,
          None => CallVerdict::NotAsked,
        };
        let answered = watch.answer(&turn.run, &silent.id, verdict, now).await;

        keep_notes(ground, &answered.lines, &mut following.log).await;

        if let (Some(words), true, Some(herdr), Some(target)) =
          (answered.words, takes_words_mid_turn(provider), &ground.herdr, target.as_deref())
        {
          let _ = herdr.call("agent.prompt", json!({ "target": target, "text": words }), None).await;
        }
      }

      for ended in watch.take_closed_asks() {
        if let Some(asks) = asks.as_deref_mut() {
          asks.ended(&ground.db, &ground.events, Some(&ended)).await;
        }
      }

      // A job's turn standing still is told to its item's orderer as a live turn's is.
      while let Some(change) = watch.take_stall() {
        if let Some(stall) = stall.as_deref_mut() {
          stall.heard(&ground.db, ground.vars(), &change).await;
        }
      }
    }

    if let Some(cut) = &in_process_cut
      && tools.as_ref().is_some_and(|watch| watch.in_process_cut_still())
      && now - cut.at >= grace_ms
    {
      let why = fill_marks(&grace_cut, &[("tool", &cut.tool), ("seconds", &cut.seconds.to_string())]);

      if let Some(job) = turn.job
        && let Err(refused) = crate::queue::stops::mark_job_stopped(&ground.db, job, &why).await
      {
        eprintln!(
          "{}: job {job} was not marked stopped at a cut call's grace: {}",
          turn.run, refused.detail
        );
      }

      println!("{}: {}", turn.run, why);
      following.log.write(&ground.db, Line::note(&why)).await;
      surface_interrupt(ground, &turn.run, pane).await;
      following.feed(ground, turn, place, pane).await;

      return hand_back(ground, place, turn, Came::Failed(why), true).await;
    }

    if let Some(listening) = &listening {
      took_words(ground, place, pane, listening).await;
    }

    if !cut && cancelled(&ground.db, &[turn.run.as_str()]).await.unwrap_or(false) {
      cut = true;
      stop_seen_at = Some(now);

      if let (Some(watch), Some(records)) = (tools.as_mut(), records) {
        cut_tools(ground, turn, records, program, watch, &mut following.log, now).await;
      }

      interrupt(ground, &turn.run, pane).await;
    } else if let (Some(seen), Some(bound)) = (stop_seen_at, stop_bound_ms)
      && now - seen >= bound
    {
      // Past the stop's bound the surface's own interrupt ends the turn, and its end is handed back as a
      // stop. A job's stop mark was already written before the cancellation.
      surface_interrupt(ground, &turn.run, pane).await;
      following.feed(ground, turn, place, pane).await;

      let why =
        format!("{}: the stop was not ended within its bound, so the turn is interrupted", turn.run);

      println!("{why}");

      return hand_back(ground, place, turn, Came::Failed(why), true).await;
    }

    // A pane of its own reads its program on every look above.
    if pane.is_some() {
      tokio::time::sleep(every).await;
      continue;
    }

    // The program gone is the turn's end; read so on as many looks in a row as a pane gone is confirmed
    // on, so one read caught between its record's writes does not end a turn that goes on.
    unseen = if place.program_alive().is_some() { 0 } else { unseen + 1 };

    if unseen >= RESIDENT_TURN.gone_looks {
      following.feed(ground, turn, place, pane).await;

      return hand_back(ground, place, turn, Came::Failed(GONE.to_owned()), true).await;
    }

    tokio::time::sleep(every).await;
  }
}

// The pane words for a picked-up turn are typed into: its own pane, else the pane of its place's program.
fn target_of(place: &Place, pane: Option<&OwnPane>) -> Option<String> {
  pane
    .and_then(|pane| pane.pane.clone())
    .or_else(|| place.running().and_then(|running| running.pane_id))
}

// The words offered to a picked-up turn typed into its program while herdr reads it working, one offer a look,
// each answered with what became of it; an offer stands while the program does not read working.
async fn took_words(ground: &Ground, place: &Place, pane: Option<&OwnPane>, listening: &aside::Listening) {
  let target = target_of(place, pane);
  let (Some(herdr), Some(target)) = (&ground.herdr, target.as_deref()) else { return };

  if !crate::door::looks::working(herdr, target).await {
    return;
  }

  let Some(offered) = listening.take() else { return };
  let outcome = match herdr
    .call("agent.prompt", json!({ "target": target, "text": offered.words }), None)
    .await
  {
    Ok(_) => AsideOutcome::Delivered,
    Err(fault) if crate::door::looks::is_blocked(&fault) => AsideOutcome::Waiting,
    Err(_) => AsideOutcome::Refused,
  };

  listening.said(offered, outcome);
}

/// The program whose process tree a picked-up turn's calls are read in: the pane's own program where the
/// turn went out in a pane of its own, the place's standing program otherwise.
fn tool_program(place: &Place, pane: Option<&OwnPane>) -> Option<i32> {
  pane
    .and_then(|pane| pane.program.as_ref())
    .map(|program| program.pid)
    .or_else(|| place.program_alive().map(|running| running.pid))
}

/// A picked-up turn's calls watched once, its lines kept as a live turn's are: the unit's log and the run's
/// log. `records` is the turn program's transcript as it stands, and `asks` the job's, through which a call
/// due for its stall is asked whether it waits on the job's own question.
#[allow(clippy::too_many_arguments)]
pub async fn watch_tools(
  ground: &Ground,
  turn: &InFlight,
  records: &[Value],
  program: Option<i32>,
  watch: &mut ToolWatch,
  asks: Option<&mut CallAsks>,
  log: &mut RunLog,
  now: i64,
) -> Vec<String> {
  let Some(program) = program else { return vec![] };
  let mut question = PickedUpQuestion {
    asks,
    db: &ground.db,
  };
  let notes = watch.look(&turn.run, records, 0, program, now, &mut question).await;

  keep_notes(ground, &notes, log).await
}

/// A stop on a picked-up turn: a call that is making no progress is cut so the stop's words can be read at
/// the next boundary.
pub async fn cut_tools(
  ground: &Ground,
  turn: &InFlight,
  records: &[Value],
  program: Option<i32>,
  watch: &mut ToolWatch,
  log: &mut RunLog,
  now: i64,
) -> Vec<String> {
  let Some(program) = program else { return vec![] };
  let notes = watch.cut_for_stop(&turn.run, records, 0, program, now).await;

  keep_notes(ground, &notes, log).await
}

// The clock's notes kept as a live turn's are: each sentence in the unit's log, and each note with its fields
// in the run's log. Answers the sentences.
async fn keep_notes(ground: &Ground, notes: &[ClockNote], log: &mut RunLog) -> Vec<String> {
  let mut sentences = vec![];

  for note in notes {
    let sentence = note.sentence();

    println!("{sentence}");
    log.write(&ground.db, Line::clock(note)).await;
    sentences.push(sentence);
  }

  sentences
}

/// A picked-up turn past its bound, with why it is cut; none for one within it. A job's turn is held to the
/// job's own bound and its job marked stopped with the reason, so the turn's end reads it and the job ends
/// stopped with its item's lease given back. A turn no job holds is held to the bound of an adopted turn, since
/// its caller went away with the process before (docs/sessions.md, 「A restart hands the session over」).
pub async fn over_bound(ground: &Ground, turn: &InFlight, now: i64) -> Option<String> {
  let Some(job) = turn.job else {
    let minutes = RESIDENT_TURN.adopted_bound_minutes.max(1);

    if now.saturating_sub(turn.started_at) < minutes_ms(minutes) {
      return None;
    }

    let why = fill_marks(&RESIDENT_TURN.adopted_cut, &[("minutes", &minutes.to_string())]);

    println!("{}: {why}", turn.run);

    return Some(why);
  };
  let bound_ms = minutes_ms(TOOL_CLOCK.turn.bound_minutes);

  if now.saturating_sub(turn.started_at) < bound_ms {
    return None;
  }

  let minutes = (bound_ms / 60_000).max(1);
  let why = fill_marks(&TOOL_CLOCK.turn.cut, &[("minutes", &minutes.to_string())]);

  if let Err(refused) = crate::queue::stops::mark_job_stopped(&ground.db, job, &why).await {
    eprintln!("{}: job {job} was not marked stopped at its turn's bound: {}", turn.run, refused.detail);
  }

  println!(
    "{}: the turn passed its {minutes}-minute bound, so job {job} is marked stopped and the turn is cut",
    turn.run
  );

  Some(why)
}

// A number of minutes in milliseconds, held at the largest a moment holds.
fn minutes_ms(minutes: u64) -> i64 {
  i64::try_from(minutes).unwrap_or(i64::MAX / 60_000).saturating_mul(60_000)
}

// A picked-up turn ended by the surface's own interrupt: the keys its provider cuts a turn with, typed into
// the pane its execution row names.
async fn surface_interrupt(ground: &Ground, run: &str, pane: Option<&OwnPane>) {
  let Some(herdr) = &ground.herdr else { return };
  let Some(keys) = harness_providers::provider("claude-code")
    .and_then(|provider| provider.declared().resident.as_ref())
    .map(|resident| resident.interrupt.clone())
  else {
    return;
  };
  let target = match pane.and_then(|pane| pane.pane.clone()) {
    Some(pane) => Some(pane),
    None => open_rows(ground, run)
      .await
      .into_iter()
      .next()
      .and_then(|row| row.text("paneId").map(str::to_owned)),
  };
  let Some(target) = target else { return };

  let _ = herdr.call("pane.send_keys", json!({ "pane_id": target, "keys": keys }), None).await;
}

// A kept turn cut short: its surface's interrupt typed into the pane of each open row it put in, at once;
// a role's turn in a pane of its own is told the stop words, as the door tells a run that is no chat round.
async fn interrupt(ground: &Ground, run: &str, pane: Option<&OwnPane>) {
  let Some(herdr) = &ground.herdr else { return };

  if let Some(pane) = pane.and_then(|pane| pane.pane.as_deref()) {
    let words = &policy::run_stop::RUN_STOP.instruction;
    let _ = herdr.call("agent.prompt", json!({ "target": pane, "text": words }), None).await;

    return;
  }

  for row in open_rows(ground, run).await {
    let directory = ground.roots.data.join("executions").join(row.id());
    let body = Value::Object(row.body);

    Stops::interrupt_resident(&ground.db, herdr, &body, &directory, true).await;
  }
}

/// The open execution rows a turn put in under a run left behind, kept as this process's own while it waits on
/// them, so no pass of the executions reads their collector as gone.
pub async fn own_rows(ground: &Ground, run: &str) {
  for row in open_rows(ground, run).await {
    publish_envelope(&ground.roots.data, row.id(), None);
  }
}

/// The open execution rows a turn put in under a run left behind, closed on how the turn came out.
pub async fn close_rows(ground: &Ground, run: &str, answered: bool) {
  for mut row in open_rows(ground, run).await {
    let state = if answered { "exited" } else { "cancelled" };

    publish_envelope(&ground.roots.data, row.id(), Some(state));
    row.set("state", json!(state));
    row.set("endedAt", json!(now_ms()));
    row.set("reconciliationEvidence", json!("picked up again after agency restarted"));

    if let Err(why) = row.save(&ground.db).await {
      eprintln!("{run}: the execution row was not closed: {}", why.detail);
    }
  }
}

// The open execution rows a turn's program put in under its run: a dsh pane's turn names its run on its
// row, and a Claude Code pane's is named by the turn's liveness row, which names the run and its execution.
async fn open_rows(ground: &Ground, run: &str) -> Vec<Row> {
  all(
    &ground.db,
    r#"SELECT "body" FROM "agent_executions"
       WHERE ("body"::jsonb)->>'endedAt' IS NULL
         AND (("body"::jsonb)->'inboxTurn'->>'name' = $1
              OR "id" IN (SELECT "execution" FROM "turn_liveness" WHERE "run" = $1))"#,
    vec![run.into()],
  )
  .await
  .unwrap_or_default()
  .iter()
  .filter_map(|row| row.try_get::<String>("", "body").ok())
  .filter_map(|body| serde_json::from_str::<Map<String, Value>>(&body).ok())
  .map(|body| Row { body })
  .collect()
}

// How many of a turn's record lines its run log already holds: each progress and answer line is filed once,
// as the line it is, by whichever process read it.
async fn filed(ground: &Ground, turn: &InFlight) -> usize {
  one(
    &ground.db,
    r#"SELECT COUNT(*) AS "filed" FROM "agent_log"
       WHERE "agent" = $1 AND "run" = $2 AND "kind" IN ('progress', 'answer')"#,
    vec![turn.agent.clone().into(), turn.run.clone().into()],
  )
  .await
  .ok()
  .flatten()
  .and_then(|row| row.try_get::<i64>("", "filed").ok())
  .unwrap_or(0)
  .max(0) as usize
}

/// A kept turn's end handed back through the path a live turn's end takes: what of its record its run log
/// lacks filed there with the end, its execution row and its place's run closed, its road and spend
/// published for a chat turn, its run in the ledger closed where it is no job's, its end published as `turn.ended` under its
/// turn's id, a job ended with it, and the turn no longer kept.
pub async fn hand_back(ground: &Ground, place: &Place, turn: &InFlight, came: Came, noted: bool) {
  let db = &ground.db;
  let (written, records) = record_of(&place.path);
  let rows = open_rows(ground, &turn.run).await;
  let source = rows
    .first()
    .and_then(|row| row.text("provider").map(str::to_owned))
    .or_else(|| span_named(&records, &turn.run).and_then(|span| provider_named(&records[span])))
    .unwrap_or_default();
  let mut log = RunLog::new(Target {
    agent: turn.agent.clone(),
    run: turn.run.clone(),
    chat_id: turn.chat_id,
    job_id: turn.job,
  });

  if let Some(span) = span_named(&records, &turn.run) {
    let skip = filed(ground, turn).await;
    let mut seen = 0;
    let rest: Vec<String> = written[span.clone()]
      .iter()
      .zip(&records[span])
      .filter(|(_, record)| {
        let filed = matches!(record.get("kind").and_then(Value::as_str), Some("progress" | "answer"));

        if filed {
          seen += 1;
        }

        filed && seen > skip
      })
      .map(|(line, _)| line.clone())
      .collect();

    log.feed(db, &rest, &source).await;
  }

  let answered = matches!(came, Came::Answered(_));

  if !noted {
    log.write(db, Line::note(NOTED)).await;
  }

  log
    .write(
      db,
      Line {
        kind: "end",
        text: match &came {
          Came::Answered(_) => SUPERVISOR_RUNNER.log.answered.clone(),
          Came::Failed(why) => why.clone(),
        },
        state: Some(if answered { "succeeded" } else { "failed" }),
        ..Line::default()
      },
    )
    .await;

  close_rows(ground, &turn.run, answered).await;

  let _ = end_run(&place.path, now_ms());
  crate::herdr_backend::forget_carried(ground, &SessionId(turn.session.clone()));

  let (mut end, spent) = match &came {
    Came::Answered(reply) => (TurnEnd::answered(&turn.run, reply.text.clone()), spent_of(&reply.spend)),
    Came::Failed(why) => (TurnEnd::failed(&turn.run, why.clone(), None), None),
  };

  end.turn = turn.turn;

  if turn.job.is_none() {
    // A turn no program's record names walked no road anybody could read.
    if let Some(chat_id) = turn.chat_id.filter(|_| !source.is_empty()) {
      let reply = match &came {
        Came::Answered(reply) => Some(reply.as_ref()),
        Came::Failed(_) => None,
      };

      ground.events.publish(road_walked(chat_id, turn, &source, reply));
    }

    if let Came::Answered(reply) = &came {
      spend_told(ground, turn, reply).await;
    }

    close_turn_run(db, turn.ledger, &end, spent, None).await;
  }

  if end.ending.is_none() {
    end.ending = Some(crate::herdr_backend::ending_of(&end).to_owned());
  }

  ground.outputs.ended(&SessionId(turn.session.clone()), turn.turn);
  ground.events.publish(TurnEnded {
    session: turn.session.clone(),
    key: turn.key.clone(),
    end: end.clone(),
  });

  // Kept under the caller's key before the turn is let go of, so a repeat of its send is answered with it.
  if let Some(key) = &turn.key {
    ground.turn_keys.ended(db, &turn.session, key, &end).await;
  }

  if let Some(job) = turn.job {
    crate::control::work::job_turn_ended(ground, job, end).await;
  }

  forget(db, &turn.run, "its end was handed back").await;
}

/// A turn whose program died with its run still open: closed with the moment and the reason, as the
/// program's own end would close it, so the room's turn is free and the words it did not answer wait for
/// the next turn. A pane that is still standing but whose turn's collector is gone is left alone. Each turn
/// is read in the place its program carries it in, and only where that place's run is the turn's: the run
/// of another turn, an earlier one in the agent's own place among them, says nothing of this one. A turn
/// this process still carries is its own end's to close, and is left to it.
pub async fn reap_dead(ground: &Ground) -> usize {
  let mut closed = 0;

  for turn in in_flight(&ground.db).await.unwrap_or_default() {
    if carried_here(&turn.run) {
      continue;
    }

    let place = Place::of(&ground.roots.data, &place_name_of(&turn));
    let Some(run) = crate::door::place_run::read_run(&place.path)
      .filter(|run| run.name.as_deref() == Some(turn.run.as_str()))
    else {
      continue;
    };

    if run.ended_at.is_some() {
      forget(&ground.db, &turn.run, "its place's run has ended").await;
      continue;
    }

    let rows = open_rows(ground, &turn.run).await;
    let dead = rows.iter().any(|row| crate::queue::reconcile::resident_gone(&row.body));

    if !dead {
      continue;
    }

    let (pid, birth) = rows
      .iter()
      .find_map(|row| {
        let resident = row.body.get("resident")?;
        Some((
          resident.get("pid").and_then(Value::as_i64),
          resident.get("birth").and_then(Value::as_str).map(str::to_owned),
        ))
      })
      .unwrap_or((None, None));
    let reason = format!(
      "The turn died silent: run {} in room {:?} on pid {:?} birth {:?} had no end.",
      turn.run, turn.chat_id, pid, birth
    );

    crate::door::place_run::end_run(&place.path, now_ms()).ok();
    hand_back(ground, &place, &turn, Came::Failed(reason), false).await;
    closed += 1;
  }

  closed
}

// The road a picked-up chat turn walked, from its start to now.
fn road_walked(chat_id: i64, turn: &InFlight, provider: &str, reply: Option<&Reply>) -> RoadWalked {
  let spend = reply.map_or(&[][..], |reply| reply.spend.as_slice());
  let reported = !spend.is_empty();
  let sum = |pick: fn(&crate::door::record::ModelSpend) -> i64| {
    reported.then(|| spend.iter().map(pick).sum::<i64>())
  };

  RoadWalked {
    stage: SUPERVISOR_RUNNER.turn_stage.clone(),
    chat_id,
    run: turn.run.clone(),
    provider: reply.map_or_else(|| provider.to_owned(), |reply| reply.mode.clone()),
    model: reply.and_then(|reply| reply.model.clone()).unwrap_or_default(),
    outcome: if reply.is_some() { "answered" } else { "error" },
    ms: now_ms() - turn.started_at,
    input_tokens: sum(|one| one.input_tokens),
    output_tokens: sum(|one| one.output_tokens),
    cache_read_tokens: sum(|one| one.cache_read_tokens),
    cache_write_tokens: sum(|one| one.cache_write_tokens),
    context_ratio: reply.and_then(|reply| reply.context_ratio),
  }
}

// What a picked-up chat turn spent, written and published as a live turn's is.
async fn spend_told(ground: &Ground, turn: &InFlight, reply: &Reply) {
  if let Err(why) =
    note_answer(&ground.db, &turn.agent, &reply.mode, reply.model.as_deref(), reply.effort.as_deref()).await
  {
    eprintln!("{}: its last answer was not written: {}", turn.agent, why.detail);
  }

  let account = slot_account(&reply.mode, reply.account.as_deref());

  if let Some(spent) =
    UsageSpent::of(&turn.agent, Some(&turn.run), now_ms() / 1000, &reply.spend, |_| account.clone())
  {
    ground.events.publish(spent);
  }
}
