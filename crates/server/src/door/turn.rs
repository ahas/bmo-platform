//! One turn put into an agent's standing Claude Code program and read back out of its record
//! (docs/agent-control.md, 「The order for putting in one turn and waiting for it to end」). A run that is
//! nobody's standing session takes its turn the same way in a pane of its own, opened for the turn and
//! closed with it: nothing of it outlives the turn, so nothing is waited out, stood again or interrupted,
//! and what it cost is read off the account line its program writes as it ends.
//!
//! The order is the measured one and every step of it is there for a reason. The program is stood
//! first, because a run told its work on the command line cannot be told a second thing; it is waited
//! for until it is ready, because a prompt pushed at an agent that is not ready is refused; the prompt
//! goes in without a wait attached, because a wait would hold the call for the whole of the turn; and
//! the turn is over when the agent has been seen working and has stopped, because the idle it starts
//! in looks exactly like the idle it ends in. What replaces a print-mode stream is the record the
//! program writes for itself, named on the way in, and the status line's readings either side of the
//! turn; nothing is scraped off the screen, and a screen is kept only as evidence.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{FixedOffset, TimeZone};
use harness_providers::claude_code::ClaudeCode;
use herdr::{Fault, Herdr, RUN_TOKEN, TOO_LARGE};
use policy::door_words::DOOR_WORDS;
use policy::fill_marks;
use policy::resident_turn::RESIDENT_TURN;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::aside;
use super::confirm::{Asking, Confirmation, Questions};
use super::execution::{Children, Identity, Row, cancelled, publish_envelope};
use super::guard::{socket_refusal, turn_refusal};
use super::liveness::{
  Judge, Liveness, Lost, Peer, Phase, Program, Signals, own_peer, program_of_session, signals_of,
  standing_peer,
};
use super::looks::{
  GoneConfirm, Look, folder_trust_keys, is_blocked, pane_listed, plugin_offer_keys, prompt,
  prompt_with_lead, screen_of, send_keys, status_of,
};
use super::node_take::{NodeTake, Take};
use super::place_run::{
  HookRecord, ProgramLogin, begin_run, end_run, foreground_rests, hand_run, hooks_file, last_stop_tasks,
  latest_failure, program_drawn, read_hooks, read_run, run_variables, status_after_turn, status_file,
  write_stopped,
};
use super::record::{
  COST_STATE, ChildReader, ModelSpend, Notices, Reply, ReplyWindow, RoundState, Tail, answered_since,
  command_closing_since, command_words, compacted_since, ended_on_error, ends_on_turn_end, last_activity,
  prompted_since, read_round, read_turn, rests_on_error, status_figures, turn_closed_since, turn_error,
  turns_owed, word_written_down,
};
use super::stand::{Interrupt, interrupt_program, person_turn_done, stand_resident, stood_again};
use crate::control::claude_accounts;
use crate::lines::place::{Place, resident_of};
use crate::now_ms;
use crate::places::Ground;
use crate::places::home::{Home, METADATA_SOURCE, short_title};
use crate::places::trust::{Trust, config_file, trust_folder};
use crate::worker_session::fold::FOLD_WORD;

/// What the typed lead says about the words pasted after it, where the caller says nothing else: a
/// job's and an upkeep turn's are the owner's own words.
pub fn pasted_lead() -> String {
  RESIDENT_TURN.pasted_lead.clone()
}

// The work key a pushed word's guard names: the job worker's.
const JOB_WORK_KEY: &str = "job-worker";

/// The paces a turn keeps, as the shared policy declares them; a test shortens them. None of them ends
/// the turn or a wait in it: each says when to look again.
#[derive(Clone, Debug)]
pub struct Timings {
  /// How often the agent is asked what it is doing.
  pub poll: Duration,
  /// How long a question just answered may still stand on the screen before it is answered again.
  pub redraw: Duration,
  /// How long after the words went in, and between presses after that, `enter` is pressed again while
  /// the prompt is not seen submitted.
  pub submit: Duration,
  pub gone: GoneConfirm,
}

impl Default for Timings {
  fn default() -> Self {
    let declared = &*RESIDENT_TURN;

    Self {
      poll: Duration::from_millis(declared.poll_ms),
      redraw: Duration::from_millis(declared.redraw_ms),
      submit: Duration::from_millis(declared.submit_ms),
      gone: GoneConfirm::default(),
    }
  }
}

/// Where a turn is put: the program standing in an agent's place, which outlives the turn, or a pane of
/// the run's own, opened for the turn in a workspace of its own on the herdr given and closed with it, for
/// a run that is nobody's standing session (docs/agent-control.md, 「One standing pane per agent」).
#[derive(Clone, Debug)]
pub enum Seat {
  Standing(Home),
  Own(Herdr),
}

impl Seat {
  /// The herdr the turn's pane is on.
  pub fn herdr(&self) -> &Herdr {
    match self {
      Self::Standing(home) => &home.herdr,
      Self::Own(herdr) => herdr,
    }
  }
}

/// One turn as its caller names it.
#[derive(Clone, Debug)]
pub struct Turn {
  /// The run's name, which its place names it by and its execution row is a child of.
  pub name: String,
  /// What the turn is, for a person watching herdr.
  pub title: String,
  /// Where the turn is put.
  pub seat: Seat,
  pub cwd: String,
  /// The command the program is stood on where it must be started for this turn.
  pub command: Vec<String>,
  pub env: BTreeMap<String, String>,
  pub prompt: String,
  /// The session the command opens or resumes, whose record the turn is read out of.
  pub session: String,
  pub identity: Identity,
  /// The line the turn runs on, whose session token the program is handed.
  pub line: String,
  /// The provider the turn goes out on, for its execution row.
  pub provider: String,
  /// What is typed before the words pasted after it: a job's and an upkeep turn's vouch for the words,
  /// and a room's round names nothing, since its words are pasted behind the head line alone.
  pub lead: String,
  /// Whether the run trusts the directory it works in, which a caller says of its place.
  pub trusts_folder: bool,
  /// How long the turn goes before its caller's overtime policy is asked about it; none for a turn
  /// whose caller asks nothing. The clock ends nothing by itself.
  pub clock: Option<Duration>,
  /// How the program's working turn is cut short.
  pub interrupt: Interrupt,
  pub timings: Timings,
}

/// What became of a word said to a turn that was already going: `Delivered`, the run has it; `Waiting`,
/// the one refusal that is not a fault — the run sits on a question a person has to answer, and asking
/// again in the same turn meets the same refusal; `Refused`, anything else, and the words stay unsent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsideOutcome {
  Delivered,
  Waiting,
  Refused,
}

/// What a turn past its clock tells its caller: when it started, its clock, and the last thing its
/// record says it did.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overrun {
  pub started_at: i64,
  pub bound_ms: i64,
  pub last_activity_at: Option<i64>,
  pub last_activity: Option<String>,
}

/// What a caller answers a turn past its clock. Only `Cut` ends the turn, as the caller's own stop of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Overtime {
  /// The caller asks nothing at the clock: the turn goes on, and is asked nothing again.
  NotAsked,
  /// The caller stops the turn.
  Cut,
  /// Go on, and ask again at this moment.
  AskAgainAt(i64),
  /// The one asked could not answer: the turn goes on, asked again once the clock's length has passed.
  Failed(String),
}

/// What a turn's caller is told while the turn runs and may say into it: the words it still has for
/// the working turn, what became of them, what to do at the clock, and the notes a log would keep.
/// Every method has the answer a caller with nothing to say gives.
pub trait Watch {
  /// The execution row the turn runs under, once it is open: the name a stop of the turn is aimed at.
  fn opened(&mut self, _execution: &str) -> impl Future<Output = ()> + Send {
    async {}
  }

  /// Words to say to the turn while it works, asked on every look at a working program.
  fn words(&mut self) -> impl Future<Output = Option<String>> + Send {
    async { None }
  }

  /// What became of the words last handed over.
  fn said(&mut self, _outcome: AsideOutcome) -> impl Future<Output = ()> + Send {
    async {}
  }

  /// Asked each time the turn's clock passes.
  fn overtime(&mut self, _asked: &Overrun) -> impl Future<Output = Overtime> + Send {
    async { Overtime::NotAsked }
  }

  /// A line for whoever reads the run afterwards: a screen kept as evidence, or what the turn did.
  fn note(&mut self, _text: &str) -> impl Future<Output = ()> + Send {
    async {}
  }

  /// What the call clock said of a tool call, as its sentence and its note, for whoever reads the run
  /// afterwards: a caller that keeps a log keeps the note's fields beside the sentence; any other takes the
  /// sentence as a note.
  fn clock(
    &mut self,
    sentence: &str,
    _note: &super::tool_clock::ClockNote,
  ) -> impl Future<Output = ()> + Send {
    self.note(sentence)
  }

  /// Whether a question the program stops on is put to this caller rather than left for a person.
  fn puts_questions(&self) -> bool {
    false
  }

  /// A question the program stopped on in the middle of the turn, put to the caller, which answers while
  /// the turn is watched on; none where it puts nothing to anybody, and the question stands for a person.
  fn confirm(&mut self, _asked: &Confirmation) -> impl Future<Output = Option<Asking>> + Send {
    async { None }
  }

  /// What the record gained since it was last handed over, from the moment the prompt was seen taken:
  /// every line as the program wrote it, for a run log to read as it reads a stream.
  fn record(&mut self, _lines: &[String]) -> impl Future<Output = ()> + Send {
    async {}
  }

  /// Whether the caller has been told to stop the run, asked on every look at it: a run told so is stopped
  /// as a cancellation over its names stops it.
  fn stopping(&mut self) -> bool {
    false
  }

  /// The clock the turn's tool calls are held to, where its caller keeps one: a job's turn and a role
  /// session's turn do, and no other turn does.
  fn tool_clock(&self) -> Option<super::tool_clock::ToolClock> {
    None
  }

  /// The longest the turn may go before it is cut and its caller told, where its caller keeps such a bound.
  fn turn_clock(&self) -> Option<Duration> {
    None
  }

  /// A tool call gone silent and idle, or one whose ask stands, handed to the caller's overtime policy on
  /// each read: the caller that asks opens an ask on it, or reads the verdict on the ask it opened. A caller
  /// that asks nothing leaves the call running and is not asked about it again.
  fn silent_call(
    &mut self,
    _call: &super::tool_clock::SilentCall,
  ) -> impl Future<Output = super::tool_clock::CallVerdict> + Send {
    async { super::tool_clock::CallVerdict::NotAsked }
  }

  /// Whether a tool call waits on the job's own question, still waiting for its answer: asked before the call
  /// is stopped for its stall, which would end the wait and withdraw the question. A caller whose run asks
  /// no question says it does not.
  fn waits_on_question(&mut self, _call: &str) -> impl Future<Output = bool> + Send {
    async { false }
  }

  /// A call asked about has ended, or with none the turn has: the asks on it close as ended.
  fn call_ended(&mut self, _call: Option<&str>) -> impl Future<Output = ()> + Send {
    async {}
  }

  /// The numbers the turn's stall is judged by where its tool calls are held to a clock: the policy's.
  fn stall_clock(&self) -> super::stall::StallClock {
    super::stall::StallClock::policy()
  }

  /// What became of the turn's progress as its call clock read it: a stall past the stall span, or a move after
  /// one. A caller that tells nobody of it lets it pass; one that does lets its notice go as the turn ends.
  fn stalled(&mut self, _change: &super::stall::StallChange) -> impl Future<Output = ()> + Send {
    async {}
  }

  /// A call that held no process of its own did not take its cut within the grace: the reason a caller
  /// marks its job stopped by, where it keeps one; none where the turn is only stopped.
  fn held_by_processless_call(
    &mut self,
    _tool: &str,
    _seconds: u64,
  ) -> impl Future<Output = Option<String>> + Send {
    async { None }
  }

  /// What the turn waits for before its program is put to work, as it begins, and none as it clears: a program on
  /// a node that has stood past its notice without taking the turn (docs/work-nodes.md, 「When a node drops」).
  fn waiting(&mut self, _waiting: Option<agent_session::Waiting>) -> impl Future<Output = ()> + Send {
    async {}
  }
}

// The caller asked whether a call due for its stall waits on the job's own question.
struct CallerQuestion<'w, W>(&'w mut W);

impl<W: Watch> super::tool_clock::QuestionWait for CallerQuestion<'_, W> {
  fn waits_on_question(&mut self, call: &str) -> impl Future<Output = bool> + Send {
    self.0.waits_on_question(call)
  }
}

/// A caller with nothing to say and nothing to keep.
pub struct Quiet;

impl Watch for Quiet {}

/// What one turn came to.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "lowercase")]
pub enum Ending {
  /// The ordinary ending.
  Answered { reply: Box<Reply>, records: usize },
  /// The program is asking a person something and nothing was delivered.
  Blocked { screen: String },
  /// The program would not get past the folder-trust question, trusted and stood up again.
  Untrusted {
    screen: String,
    folder: String,
    config: String,
  },
  /// The pane went away under the run.
  Vanished { screen: String },
  /// The turn was cut on its caller's verdict once its clock had passed.
  Overran { screen: String },
  /// The turn came back to rest with no answer in its record.
  Silent { screen: String },
  /// The turn's last word was the program's own error in the answer's place, or a failed request ended it:
  /// the error, and its kind where the program names one.
  Errored {
    error: String,
    // Read where the turn ends, never written out: what an ending is written as stays the same for every
    // reader.
    #[serde(skip)]
    kind: Option<String>,
  },
  /// The prompt never became a turn, and why.
  Unsubmitted {
    screen: String,
    // Read where the turn ends, never written out, like the error's kind above.
    #[serde(skip)]
    why: String,
  },
  /// Ended from outside: a cancellation stood over this run's name or its root.
  Stopped { screen: String },
  /// The program's process the turn was put into is gone, or its place stands another in its stead, and its
  /// record never closed the turn (docs/agent-control.md, 「Telling that a turn's program is gone」).
  #[serde(rename = "program-lost")]
  ProgramLost {
    screen: String,
    // Read where the turn ends, never written out, like the error's kind above.
    #[serde(skip)]
    why: String,
  },
  /// No program can be named for the turn, its pane shows none, and nothing the program writes moved
  /// between two looks: the other end is gone.
  #[serde(rename = "silent-peer")]
  SilentPeer {
    screen: String,
    #[serde(skip)]
    why: String,
  },
}

impl Ending {
  /// The word the ending goes by.
  pub fn word(&self) -> &'static str {
    match self {
      Self::Answered { .. } => "answered",
      Self::Blocked { .. } => "blocked",
      Self::Untrusted { .. } => "untrusted",
      Self::Vanished { .. } => "vanished",
      Self::Overran { .. } => "overran",
      Self::Silent { .. } => "silent",
      Self::Errored { .. } => "errored",
      Self::Unsubmitted { .. } => "unsubmitted",
      Self::Stopped { .. } => "stopped",
      Self::ProgramLost { .. } => "program-lost",
      Self::SilentPeer { .. } => "silent-peer",
    }
  }

  /// What the ending stood on, for the turn's liveness row: why it lost its program, the error, why its
  /// words never became a turn, or its word.
  pub fn detail(&self) -> String {
    match self {
      Self::ProgramLost { why, .. } | Self::SilentPeer { why, .. } | Self::Unsubmitted { why, .. } => {
        why.clone()
      }
      Self::Errored { error, .. } => error.clone(),
      other => other.word().to_owned(),
    }
  }
}

/// Whether a turn's wait is over on an error of its program's: a failed request the program reported through
/// its failure hook ends it at once, whatever herdr reads the pane as; the error the program wrote in the
/// answer's place ends it once the pane is no longer working (docs/agent-control.md, 「Telling that a turn's
/// program is gone」).
pub fn over_on_error(working: bool, error_in_record: bool, failure_reported: bool) -> bool {
  failure_reported || (error_in_record && !working)
}

/// Whether a turn come back to rest has nothing more to write of its end: the line a turn ends with is in its
/// record, the interrupted marker is, or its program reported the failed request that ended it, after which no
/// such line is written (docs/agent-control.md, 「Telling that a turn's program is gone」).
pub fn end_written(closed: bool, interrupted: bool, failure_reported: bool) -> bool {
  closed || interrupted || failure_reported
}

/// Whether a round's program at rest may be read as owing no further turn once its record stands unchanged
/// between two looks: the record ends on a turn's end, or the program reported the failed request its latest
/// turn ended with.
pub fn rests_on_an_end(at_rest: bool, ends_on_end: bool, failure_reported: bool) -> bool {
  at_rest && (ends_on_end || failure_reported)
}

/// How a turn ended, for its execution row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum End {
  Exited,
  Cancelled,
}

impl End {
  fn word(self) -> &'static str {
    match self {
      Self::Exited => "exited",
      Self::Cancelled => "cancelled",
    }
  }
}

/// The first line of a prompt put into a resident program: which run it is, what it is and when it
/// went in, on the deployment's clock (UTC+9).
pub fn run_header(name: &str, title: &str, at: i64) -> String {
  let seoul = FixedOffset::east_opt(9 * 3600).expect("a fixed offset");
  let clock = seoul
    .timestamp_millis_opt(at)
    .single()
    .map_or_else(String::new, |at| at.format("%H:%M").to_string());

  format!("[{name} · {title} · {clock}]")
}

/// The login a program runs under, as its environment names it: the configuration home it reads its
/// login from, and a fingerprint of a token it was handed instead — a digest cut short, from which no
/// token can be made back.
pub fn program_login(machine_config_dir: &Path, env: &BTreeMap<String, String>) -> ProgramLogin {
  ProgramLogin {
    config_dir: env
      .get("CLAUDE_CONFIG_DIR")
      .filter(|dir| !dir.is_empty())
      .cloned()
      .unwrap_or_else(|| machine_config_dir.to_string_lossy().into_owned()),
    token: env
      .get("CLAUDE_CODE_OAUTH_TOKEN")
      .filter(|token| !token.trim().is_empty())
      .map(|token| fingerprint(token)),
  }
}

fn fingerprint(token: &str) -> String {
  format!("{:x}", Sha256::digest(token.trim().as_bytes()))[..16].to_owned()
}

/// The registered account a program's login belongs to: a token account by its token alone, a directory
/// or endpoint account by the configuration home of its own it holds, and where it was handed a token
/// beside it — a program on a work node — only where that token is the account's token for the nodes,
/// kept under the data root named, which an endpoint account has none of.
pub fn account_of_login(
  accounts: &[claude_accounts::Account],
  login: &ProgramLogin,
  data_root: &Path,
) -> Option<String> {
  let held = |path: &Path| std::fs::read_to_string(path).ok().map(|held| fingerprint(&held));

  if let Some(token) = &login.token
    && let Some(account) = accounts.iter().find(|account| {
      account.credential.token_file().is_some_and(|file| held(file).as_ref() == Some(token))
    })
  {
    return Some(account.id.clone());
  }

  let absolute = |path: &Path| std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
  let home = absolute(Path::new(&login.config_dir));

  accounts
    .iter()
    .find(|account| {
      account.credential.own_home().is_some_and(|own| absolute(own) == home)
        && login.token.as_ref().is_none_or(|token| {
          account
            .node_token_file(data_root)
            .is_some_and(|file| held(&file).as_ref() == Some(token))
        })
    })
    .map(|account| account.id.clone())
}

// The answers one wait types to the questions the line answers itself: counted against their bound, and
// paced so a question still on screen a redraw after its answer is answered again.
struct OwnAnswers {
  typed: u32,
  at: Option<tokio::time::Instant>,
  redraw: Duration,
}

impl OwnAnswers {
  fn new(redraw: Duration) -> Self {
    Self {
      typed: 0,
      at: None,
      redraw,
    }
  }

  // Whether a question still standing is past the redraw its last answer was given.
  fn due(&self) -> bool {
    self.at.is_none_or(|at| at.elapsed() >= self.redraw)
  }

  fn left(&self) -> bool {
    self.typed < RESIDENT_TURN.screen_answers
  }
}

// One turn's state as it is watched.
struct Door<'a, W: Watch> {
  ground: &'a Ground,
  turn: &'a Turn,
  watch: &'a mut W,
  herdr: &'a Herdr,
  // The standing program's place and home; none for a turn in a pane of its own.
  standing: Option<(Place, &'a Home)>,
  tail: Tail,
  row: Row,
  children: Children,
  reader: ChildReader,
  observed: usize,
  // Where the record was last handed to the watch, once the prompt was seen taken.
  fed: Option<usize>,
  questions: Questions,
  trust_config: PathBuf,
  // The turn's row in the store's liveness table, and the program its words went into: for a standing
  // program the process its place records as they go in, for a pane of the turn's own the process whose
  // command line carries the session, once found.
  liveness: Liveness,
  program: Option<Program>,
  judge: Judge,
  // The bound a standing program on a node is held to until it takes the turn; none here and once it has.
  node_take: Option<NodeTake>,
  // How the turn's rounds read the turns its program opens on its own, as its line declares.
  notices: Notices,
}

// What the wait for a program to be told something came to: ready, as a look read it with the screen it kept, or
// its program on a node past its bound, and why the turn ends.
enum Rest {
  Ready(Look, String),
  Overdue(String),
}

impl<W: Watch> Door<'_, W> {
  // What can be said of the turn's program now: a pane of the turn's own looks for its process until it
  // finds it.
  fn peer(&mut self) -> Peer {
    if let Some((place, _)) = &self.standing {
      return standing_peer(place, self.program.as_ref());
    }

    if self.program.is_none() {
      self.program = program_of_session(&self.turn.session);
      self.liveness.program(self.program.as_ref());
    }

    own_peer(self.program.as_ref())
  }

  // The program's own signals now: its record's length, and its place's status-line calls and hook events.
  fn signals(&mut self) -> Signals {
    let signals =
      signals_of(self.tail.path(), self.standing.as_ref().map(|(place, _)| place.path.as_path()));

    self.liveness.signals(&signals);
    signals
  }

  // A look's reading of herdr kept on the turn's row, written where something in it changed.
  async fn looked(&mut self, look: &Look) {
    self.liveness.herdr(look);
    self.liveness.write(&self.ground.db).await;
  }

  // The program standing in the place named as the one the turn's words go into.
  fn name_standing(&mut self) {
    if let Some((place, _)) = &self.standing {
      self.program = place.program_alive().map(|running| Program {
        pid: running.pid,
        birth: running.birth,
      });
      self.liveness.program(self.program.as_ref());
    }
  }

  // The turn ended on what a look observed and the record does not say, its screen kept as evidence.
  async fn lost(&mut self, pane: &str, lost: Lost, herdr: &str) -> Ending {
    let why = match (lost, &self.program) {
      (Lost::Program, Some(program)) => {
        let instead = self
          .standing
          .as_ref()
          .and_then(|(place, _)| place.running())
          .filter(|running| running.pid != program.pid && running.pid > 0)
          .map(|running| format!(", and its place stands program {} in its stead", running.pid))
          .unwrap_or_default();

        format!(
          "{}: the program {} the turn was put into is gone{instead}, and its record never closed the turn",
          lost.word(),
          program.pid
        )
      }
      (Lost::Program, None) => format!("{}: the turn's program is gone", lost.word()),
      (Lost::SilentPeer, _) => format!(
        "{}: no program can be named for the turn, herdr reads its pane {herdr}, and nothing its program writes moved between two looks",
        lost.word()
      ),
    };
    let shown = self.screen(pane).await;
    let screen = self.kept_screen(&why, shown).await;

    match lost {
      Lost::Program => Ending::ProgramLost { screen, why },
      Lost::SilentPeer => Ending::SilentPeer { screen, why },
    }
  }

  fn lines(&mut self) -> &[Value] {
    self.tail.look()
  }

  // The bound a program on a node is held to before it takes the turn, looked at once: the turn's waiting told, its
  // log and agency's written, as the notice passes, and why the turn ends once the bound has; none here, within it
  // and once the words are taken.
  async fn node_overdue(&mut self) -> Option<String> {
    let take = self.node_take.as_mut()?;
    let node = take.node().to_owned();
    let heard = crate::watches::links::link_heard(&self.ground.roots.data, &node);
    let refused = crate::watches::links::link_refused(&self.ground.roots.data, &node);
    let refused = refused.first().map(|(port, why)| (*port, why.as_str()));

    match take.look(heard, refused, tokio::time::Instant::now()) {
      Take::Within => None,
      Take::Cleared => {
        self.watch.waiting(None).await;

        None
      }
      Take::Notice { reason, stood } => {
        eprintln!("{}: {reason}", self.turn.name);
        self.watch.note(&reason).await;
        self
          .watch
          .waiting(Some(agent_session::Waiting {
            kind: agent_session::WaitKind::NodeProgram,
            service: node,
            reason,
            since: now_ms() - stood.as_millis() as i64,
          }))
          .await;

        None
      }
      Take::Overdue(why) => {
        eprintln!("{}: {why}", self.turn.name);
        self.watch.note(&why).await;

        Some(why)
      }
    }
  }

  // The turn's words taken, or the turn ended: no bound reads the program any more, and a waiting told is cleared.
  async fn node_settled(&mut self) {
    if self.node_take.take().is_some_and(|take| take.noticed()) {
      self.watch.waiting(None).await;
    }
  }

  // The turn ended on its program's bound: the program ended, so the next turn stands a new one, and the words that
  // never became a turn said so.
  async fn node_cut(&mut self, pane: &str, why: String) -> Ending {
    self.interrupt(pane, &Interrupt::EndProgram).await;
    self.node_settled().await;

    let shown = self.screen(pane).await;
    let screen = self.kept_screen(&why, shown).await;

    Ending::Unsubmitted { screen, why }
  }

  async fn cancelled(&self) -> bool {
    let names = [self.turn.name.as_str(), self.row.id(), self.row.root()];

    cancelled(&self.ground.db, &names).await.unwrap_or(false)
  }

  // What a launch asks first, each time: not cancelled, and the guard's word.
  async fn check_launch(&self) -> Result<(), String> {
    if self.cancelled().await {
      return Err("execution cancelled".to_owned());
    }

    match turn_refusal(&self.ground.db, &self.ground.machine(), "pane", &self.turn.env).await {
      Some(why) => Err(why),
      None => Ok(()),
    }
  }

  async fn save(&self) {
    if let Err(why) = self.row.save(&self.ground.db).await {
      eprintln!(
        "{}: the execution row ({}): {why}",
        self.turn.name,
        self.row.text("state").unwrap_or("")
      );
    }
  }

  // The row closed with how the turn ended: the envelope first, so a collector killed before the
  // archive's write still leaves the cause behind.
  async fn close(&mut self, state: &str) {
    publish_envelope(&self.ground.roots.data, self.row.id(), Some(state));
    self.row.set("state", json!(state));
    self.row.set("endedAt", json!(now_ms()));
    self.save().await;
  }

  // What the record's account line written after `from` says the run spent, written as the program of a
  // pane closed ends: waited for until it comes, herdr says the pane is gone or the resident turn's bound on
  // the reading after a turn has passed, and read once more then, empty where it never came; none at all
  // where a stop stood over the run meanwhile, which ends the turn stopped.
  async fn account_line(&mut self, pane: &str, from: usize) -> Option<Option<Vec<ModelSpend>>> {
    let written = |records: &[Value]| {
      records
        .iter()
        .skip(from)
        .any(|record| record.get("type").and_then(Value::as_str) == Some(COST_STATE))
    };
    let by = tokio::time::Instant::now() + Duration::from_millis(RESIDENT_TURN.status_after_ms);

    loop {
      let records = self.lines();

      if written(records) {
        return Some(spend_since(records, from));
      }

      if self.cancelled().await {
        return None;
      }

      // herdr saying the pane is gone, or the program's own process gone, is the program done writing.
      if pane_listed(self.herdr, pane).await == Some(false)
        || matches!(own_peer(self.program.as_ref()), Peer::Gone(_))
        || tokio::time::Instant::now() >= by
      {
        let records = self.lines();

        return Some(written(records).then(|| spend_since(records, from)).flatten());
      }

      tokio::time::sleep(Duration::from_millis(100)).await;
    }
  }

  async fn finish_children(&mut self) {
    let row = self.row.clone();

    if let Err(why) = self.children.finish(&self.ground.db, &row).await {
      eprintln!("{}: the child runs' rows: {why}", self.turn.name);
    }
  }

  // The row of a turn whose own pane its collector has closed: one whose prompt never went in did not
  // launch; one whose pane herdr says is gone ended the way the turn read it; one herdr still holds, or
  // does not answer about, may still be working, so it stays open and unknown.
  async fn settle_own(&mut self, pane: &str, attempted: bool, ending: End) {
    if !attempted {
      self.close("launch-error").await;
      return;
    }

    if pane_listed(self.herdr, pane).await == Some(false) {
      self.close(ending.word()).await;
    } else {
      self.row.set("state", json!("unknown"));
      self.save().await;
    }
  }

  async fn kept_screen(&mut self, why: &str, screen: String) -> String {
    let text = if screen.is_empty() { why.to_owned() } else { format!("{why}\n\n{screen}") };

    self.watch.note(&text).await;
    screen
  }

  async fn screen(&self, pane: &str) -> String {
    screen_of(self.herdr, pane).await
  }

  async fn record_children(&mut self) {
    let records = self.tail.look();

    for record in records.iter().skip(self.observed) {
      let said = {
        let children = &self.children;

        self.reader.read(record, |id| children.known(id))
      };

      for evidence in said {
        if let Err(why) = self.children.update(&self.ground.db, &self.row, &evidence).await {
          eprintln!("{}: a child run's row: {why}", self.turn.name);
        }
      }
    }

    self.observed = records.len();
    self.feed().await;
  }

  // What the record gained handed to the watch, once the turn is under way.
  async fn feed(&mut self) {
    let Some(at) = self.fed else {
      return;
    };
    self.tail.look();

    let lines = self.tail.raw();

    if lines.len() > at {
      self.watch.record(&lines[at..]).await;
    }

    let fed = lines.len().max(at);

    self.fed = Some(fed);
    self.tail.forget_raw_before(fed);
  }

  // A run that trusts its folder has the folder written as trusted, in the file its own login reads,
  // before any program comes up in it, so the question never stands.
  async fn trust_here(&mut self) {
    if !self.turn.trusts_folder {
      return;
    }

    let config = self.trust_config.display().to_string();
    let cwd = self.turn.cwd.clone();
    let trust = if Path::new(&cwd).is_absolute() {
      trust_folder(&self.trust_config, &cwd)
    } else {
      Trust::Unreadable
    };

    match trust {
      Trust::Trusted => {}
      Trust::Written => {
        self
          .watch
          .note(&fill_marks(
            &DOOR_WORDS.notes.trust_written,
            &[("config", &config.to_string()), ("cwd", &cwd.to_string())],
          ))
          .await;
      }
      Trust::NoConfigDir | Trust::Unreadable => {
        let word = match trust {
          Trust::NoConfigDir => "no-config-dir".to_owned(),
          _ if Path::new(&cwd).is_absolute() => "unreadable".to_owned(),
          _ => format!("Error: folder trust is kept by absolute path, not {cwd}"),
        };

        self
          .watch
          .note(&fill_marks(
            &DOOR_WORDS.notes.trust_unwritten,
            &[("config", &config.to_string()), ("cwd", &cwd.to_string()), ("why", &word.to_string())],
          ))
          .await;
      }
    }
  }

  // The standing program's working turn cut short, or the program ended; a pane of the turn's own is closed
  // with the turn instead, so nothing is done to it here.
  async fn interrupt(&self, pane: &str, interrupt: &Interrupt) {
    if let Some((place, home)) = &self.standing {
      interrupt_program(place, home, pane, interrupt).await;
    }
  }

  // A command cut short stands back in the program's input box, where the next turn's words would become its
  // arguments: the program is ended, and its loop starts it on the same session with the box empty.
  async fn clear_command(&self, pane: &str) {
    self.interrupt(pane, &Interrupt::EndProgram).await;
  }

  // Whether the place's run is marked as handed words the bot put in; a pane of the turn's own has no
  // place.
  fn hand(&self, handed: bool) {
    if let Some((place, _)) = &self.standing {
      hand_run(&place.path, handed);
    }
  }

  // The failure a failed request ended the program's latest turn with since a moment, where it ended so; a
  // pane of the turn's own has no place, and its record alone says how its turn ended.
  fn failure_since(&self, since: i64) -> Option<HookRecord> {
    self.standing.as_ref().and_then(|(place, _)| latest_failure(&place.path, since))
  }

  // Bringing a wedged program back to rest: it is ended, its loop starts it again on the same session
  // with an empty input box, and the place is watched until the program standing is a different one,
  // because until its record says so a prompt put in would go into the program just ended. A pane of the
  // turn's own has nothing standing to reset.
  async fn stand_again(&mut self, pane: &str, why: &str) -> bool {
    let Some((place, home)) = &self.standing else { return false };

    if self.cancelled().await {
      return false;
    }

    let was = place.running();

    self.watch.note(&fill_marks(&DOOR_WORDS.notes.stand_again, &[("why", why)])).await;
    interrupt_program(place, home, pane, &Interrupt::EndProgram).await;

    let ground = self.ground;
    let names = [self.turn.name.clone(), self.row.id().to_owned(), self.row.root().to_owned()];

    stood_again(place, was.as_ref(), self.turn.timings.poll, || async {
      let names: Vec<&str> = names.iter().map(String::as_str).collect();

      cancelled(&ground.db, &names).await.unwrap_or(false)
    })
    .await
  }

  // The keys answering a question on screen the line answers itself, and the note saying so: the
  // folder-trust question in a run that trusts its folder, and the plugin offer in every run.
  fn own_answer(&self, screen: &str) -> Option<(Vec<&'static str>, &'static str)> {
    if self.turn.trusts_folder
      && let Some(keys) = folder_trust_keys(screen)
    {
      return Some((keys, DOOR_WORDS.notes.trusted_folder.as_str()));
    }

    plugin_offer_keys(screen).map(|keys| (keys, DOOR_WORDS.notes.declined_offer.as_str()))
  }

  // A question on screen the line answers itself answered, where one stands, answers are left and one is
  // due: whether its keys went in.
  async fn answer_own(&mut self, pane: &str, screen: &str, answers: &mut OwnAnswers) -> bool {
    if !answers.left() || !answers.due() {
      return false;
    }

    let Some((keys, note)) = self.own_answer(screen) else { return false };

    if send_keys(self.herdr, pane, &keys).await.is_err() {
      return false;
    }

    self.watch.note(note).await;
    answers.typed += 1;
    answers.at = Some(tokio::time::Instant::now());

    true
  }

  // A look at the pane as the turn reads it. On a line that carries its program's background apart, a
  // working reading while the program's foreground has rested since `since` is its background work's, and
  // is read as at rest (docs/agent-control.md, 「Background work on a line that carries it apart」).
  async fn look(&mut self, pane: &str, confirm: GoneConfirm, since: i64) -> Look {
    let now = status_of(self.herdr, pane, confirm).await;
    let Some(place) = self.standing.as_ref().map(|(place, _)| place.path.clone()) else {
      return now;
    };

    if self.notices == Notices::Apart && now.is("working") && foreground_rests(&place, self.lines(), since)
    {
      return Look::Status("idle".to_owned());
    }

    now
  }

  // Before anything is typed into a program that carries its background apart, its input taken back to the
  // main conversation where its screen addresses one of its background agents: the list opened, its cursor
  // moved up to the main row and that row taken, the screen read after each key. Answers whether the input
  // addresses the main conversation now.
  async fn input_on_main(&mut self, pane: &str) -> bool {
    let words = &DOOR_WORDS.agent_view;

    if self.notices != Notices::Apart || self.standing.is_none() {
      return true;
    }

    let mut moved = false;

    for _ in 0..words.steps {
      let screen = self.screen(pane).await;

      if !screen.contains(&words.addressed) {
        if moved {
          self.watch.note(&words.returned).await;
        }

        return true;
      }

      let on_main = screen.lines().any(|line| {
        line.trim_start().strip_prefix(words.cursor.as_str()).is_some_and(|row| {
          row.trim_start_matches(|one: char| one.is_whitespace() || "◯●".contains(one)).trim()
            == words.main_row
        })
      });
      let key = if on_main {
        &words.take
      } else if screen.contains(&words.list_focused) {
        &words.up
      } else {
        &words.open_list
      };

      moved = true;
      let _ = send_keys(self.herdr, pane, &[key.as_str()]).await;
      tokio::time::sleep(self.turn.timings.poll).await;
    }

    let screen = self.screen(pane).await;

    if screen.contains(&words.addressed) {
      return false;
    }

    self.watch.note(&words.returned).await;
    true
  }

  // On a stop of a turn on a line that carries its background apart, the background work the place's last stop
  // named running ended within the policy's bound: written down as stopped first, then, where a subagent is
  // among it, the program itself ended, which takes its subagents and shells with it, and otherwise the shells'
  // own processes; every process of the work still standing is then ended, a polite end and a kill past the
  // grace. The note names what was ended and what could not be. A place on a node is reached through the
  // program's end alone.
  async fn end_background(&mut self, pane: &str) {
    use policy::background_work::BACKGROUND_WORK;

    let Some((place, home)) = self.standing.as_ref() else { return };
    let tasks = last_stop_tasks(&place.path);

    if tasks.is_empty() {
      return;
    }

    let ids: Vec<String> = tasks.iter().map(|task| task.id.clone()).collect();
    let named = tasks
      .iter()
      .map(|task| format!("{} ({} {})", task.id, task.kind, task.description))
      .collect::<Vec<_>>()
      .join("; ");
    let bound = Duration::from_secs(BACKGROUND_WORK.stop_bound_seconds);
    let grace = Duration::from_millis(BACKGROUND_WORK.stop_grace_ms);
    let subagent = tasks.iter().any(|task| task.kind != "shell");
    let local = place.node().is_none();
    let program = place.program_alive().map(|running| running.pid);

    if let Err(why) = write_stopped(&place.path, &ids, now_ms()) {
      eprintln!("{}: the background record: {why}", self.turn.name);
    }

    let words = &DOOR_WORDS.background_stop;
    let (pids, said) = if subagent {
      let pids = program.filter(|_| local).map(super::tool_clock::tree_processes).unwrap_or_default();
      let _ =
        tokio::time::timeout(bound, interrupt_program(place, home, pane, &Interrupt::EndProgram)).await;

      (pids, fill_marks(&words.ended_program, &[("tasks", &named)]))
    } else {
      let pids = program
        .filter(|_| local)
        .map(|program| super::tool_clock::task_processes(program, &ids));

      (pids.unwrap_or_default(), fill_marks(&words.ended_shells, &[("tasks", &named)]))
    };
    let left = tokio::time::timeout(bound, super::tool_clock::end_processes(&pids, grace))
      .await
      .unwrap_or(pids);

    println!("{}: {said}", self.turn.name);
    self.watch.note(&said).await;

    if !left.is_empty() {
      let pids = left.iter().map(i32::to_string).collect::<Vec<_>>().join(", ");
      let said = fill_marks(&words.left, &[("pids", &pids)]);

      println!("{}: {said}", self.turn.name);
      self.watch.note(&said).await;
    }
  }

  // Waiting for the program to be ready to be told something, keeping what its screen said meanwhile —
  // the most worth reading is said by a program about to be gone. The wait lasts until the program is at
  // rest, stands at a question, or its pane is gone, and a herdr that does not answer is waited out. A
  // question the line answers itself is answered here, a counted number of times, and the wait goes on.
  async fn settled(&mut self, pane: &str) -> Result<Rest, String> {
    let timings = self.turn.timings.clone();
    let mut screen = String::new();
    let mut answers = OwnAnswers::new(timings.redraw);
    let mut silent = false;

    loop {
      self.check_launch().await?;

      if let Some(why) = self.node_overdue().await {
        return Ok(Rest::Overdue(why));
      }

      let now = self.look(pane, timings.gone, i64::MIN).await;

      if now == Look::Gone {
        return Ok(Rest::Ready(now, screen));
      }

      if let Look::Silent(cause) = &now {
        if !silent {
          silent = true;
          self
            .watch
            .note(&fill_marks(&DOOR_WORDS.notes.herdr_silent, &[("cause", cause.as_str())]))
            .await;
        }

        tokio::time::sleep(timings.poll).await;
        continue;
      }

      silent = false;

      let seen = self.screen(pane).await;

      if !seen.trim().is_empty() {
        screen.clone_from(&seen);
      }

      // A program started again for this turn reads at rest from the one before it until it is up, and
      // words put in meanwhile are lost whole. A pane of the turn's own has only its own program.
      if now.at_rest() && self.standing.as_ref().is_none_or(|(place, _)| program_drawn(place)) {
        return Ok(Rest::Ready(now, screen));
      }

      let due = answers.due();

      if now.is("blocked") && !self.answer_own(pane, &seen, &mut answers).await && due {
        // A question the line does not answer itself, or one its answers did not clear: the turn's to
        // handle.
        return Ok(Rest::Ready(now, screen));
      }

      tokio::time::sleep(timings.poll).await;
    }
  }

  // Whether a prompt put in was submitted: the record gained a person's prompt after the turn began, or
  // the pane went to work, within the submit pace; `Unsent` once the pace has passed with neither, for
  // `enter` to be pressed again. A pane gone, or stopped on a question, counts as taken, so the turn's own
  // watching reads what it is doing.
  async fn taken(&mut self, pane: &str, from: usize) -> Submit {
    let by = tokio::time::Instant::now() + self.turn.timings.submit;

    loop {
      if prompted_since(self.lines(), from) {
        return Submit::Seen;
      }

      let now = self.look(pane, GoneConfirm::default(), i64::MIN).await;

      if now.is("working") {
        return Submit::Working;
      }

      if now == Look::Gone || now.is("blocked") {
        return Submit::Seen;
      }

      if tokio::time::Instant::now() >= by {
        return Submit::Unsent;
      }

      tokio::time::sleep(self.turn.timings.poll).await;
    }
  }

  // A word pushed into the working turn. herdr's working can trail the turn's real end by a look, and
  // making the push takes seconds more, so whether the turn still works is read again just before the
  // keys go: off the place, whose stop hook closes the run at the end, and off the record. The run is
  // marked as handed the word first, so an end landing between that reading and the keys still finds
  // the word the bot's; a push that does not go in takes the mark back, and one that does is waited on
  // until the record holds it.
  async fn push_word(&mut self, pane: &str, from: usize, text: &str) -> Result<Value, Fault> {
    self.hand(true);

    let run = self.standing.as_ref().and_then(|(place, _)| read_run(&place.path));
    let notices = self.notices;
    let records = self.lines();
    let over = run.is_some_and(|run| run.person || run.ended_at.is_some())
      || read_round(records.get(from..).unwrap_or_default(), notices).state != RoundState::Working;
    let at = records.len();
    let said = if over {
      Err(Fault {
        code: "turn_ended".to_owned(),
        message: "the turn ended before the words could go in".to_owned(),
      })
    } else if !self.input_on_main(pane).await {
      Err(Fault {
        code: "agent_view".to_owned(),
        message: DOOR_WORDS.agent_view.stuck.clone(),
      })
    } else {
      prompt_with_lead(self.herdr, pane, &self.turn.lead, text).await
    };

    if said.is_err() {
      self.hand(false);

      return said;
    }

    // Waited on until the record holds it, or the round no longer works and the word is the next
    // round's.
    while !word_written_down(self.lines(), at) && self.round_state(from) == RoundState::Working {
      tokio::time::sleep(self.turn.timings.poll).await;
    }

    said
  }

  // Whether the turn's program still stands: the process its words went into not gone, and for a pane of
  // the turn's own a pane herdr has not said is gone.
  async fn program_stands(&mut self, pane: &str) -> bool {
    if matches!(self.peer(), Peer::Gone(_)) {
      return false;
    }

    self.standing.is_some() || status_of(self.herdr, pane, GoneConfirm::default()).await != Look::Gone
  }

  fn round_state(&mut self, from: usize) -> RoundState {
    let notices = self.notices;
    let records = self.lines();

    read_round(records.get(from..).unwrap_or_default(), notices).state
  }
}

// What a command of the program's own came to, out of its own words (docs/agent-control.md, 「How a command of
// the program's own ends」).
async fn command_ended<W: Watch>(
  door: &mut Door<'_, W>,
  pane: &str,
  records: &[Value],
  from: usize,
  read: &super::record::TurnRead,
  failure: Option<(String, Option<String>)>,
) -> Ending {
  let turn = door.turn;
  let said = command_words(records, from);

  // Said as its error, or ended by a failure its hook reported, whichever came first.
  if let Some((error, kind)) = said.as_ref().filter(|said| said.error).map_or_else(
    || failure.clone(),
    |said| Some((said.text.clone(), failure.clone().and_then(|(_, kind)| kind))),
  ) {
    return Ending::Errored { error, kind };
  }

  let Some(said) = said else {
    door.clear_command(pane).await;

    let shown = door.screen(pane).await;
    let screen = door.kept_screen(&DOOR_WORDS.screens.silent, shown).await;

    return Ending::Silent { screen };
  };

  // A fold word the program answered without folding is a failed compaction, whatever words it chose.
  if turn.prompt == FOLD_WORD && !compacted_since(records, from) {
    return Ending::Errored {
      error: said.text,
      kind: None,
    };
  }

  door.close("exited").await;

  let mut reply = read.reply(&turn.session, read.spend.clone());

  reply.text = said.text;

  Ending::Answered {
    reply: Box::new(reply),
    records: records.len(),
  }
}

// Whether a prompt put in was seen submitted.
enum Submit {
  Working,
  Seen,
  Unsent,
}

/// One turn, start to finish, in the agent's standing program or in a pane of its own opened for it. An
/// error is a turn that never got under way or was thrown out of — cancelled before it went in, refused by
/// the guard, a pane herdr would not take — and its row says so.
pub async fn put<W: Watch>(ground: &Ground, turn: &Turn, watch: &mut W) -> Result<Ending, String> {
  let herdr = turn.seat.herdr();

  let machine = ground.machine();

  if let Some(why) = socket_refusal(herdr.socket(), &machine) {
    return Err(why);
  }

  let machine_config = ClaudeCode::machine_config_dir(&machine);
  let records_dir = turn
    .env
    .get("CLAUDE_CONFIG_DIR")
    .filter(|dir| !dir.is_empty())
    .map_or_else(|| machine_config.clone(), PathBuf::from);
  let record = ClaudeCode::records_under(&records_dir, Path::new(&turn.cwd), &turn.session).swap_remove(0);
  let trust_config = config_file(
    &turn
      .env
      .get("CLAUDE_CONFIG_DIR")
      .filter(|dir| !dir.is_empty())
      .map_or_else(|| PathBuf::from(machine.var("HOME").unwrap_or_default()), PathBuf::from),
  );
  let mut tail = Tail::at_end(&record);
  let from = tail.count();
  // A turn of a standing program is for the agent whose program it is, where the caller names none.
  let identity = Identity {
    agent: turn.identity.agent.clone().or_else(|| match &turn.seat {
      Seat::Standing(home) => Some(home.agent.clone()),
      Seat::Own(_) => None,
    }),
    ..turn.identity.clone()
  };
  let row = Row::begin(
    &ground.db,
    &turn.provider,
    &turn.line,
    &turn.name,
    Some(&turn.session),
    &turn.command,
    &identity,
  )
  .await
  .map_err(|why| why.detail)?;

  publish_envelope(&ground.roots.data, row.id(), None);
  watch.opened(row.id()).await;

  let standing = match &turn.seat {
    Seat::Standing(home) => Some((Place::of(&ground.roots.data, &home.agent), home)),
    Seat::Own(_) => None,
  };
  // The turn's liveness row names every record its wait reads: the session's, and a standing program's
  // status line and hooks in its place.
  let mut read = vec![record.clone()];

  if let Some((place, _)) = &standing {
    read.extend([status_file(&place.path), hooks_file(&place.path)]);
  }

  let mut liveness = Liveness::new(row.id(), &turn.name, &turn.line, &turn.session, read);

  if let Err(why) = super::liveness::prune(&ground.db, now_ms()).await {
    eprintln!("{}: the liveness rows past their week: {why:?}", turn.name);
  }

  liveness.write(&ground.db).await;

  let mut door = Door {
    ground,
    turn,
    watch,
    herdr,
    standing,
    tail,
    row,
    children: Children::default(),
    reader: ChildReader::default(),
    observed: from,
    fed: None,
    questions: Questions::new(turn.timings.redraw),
    trust_config,
    liveness,
    program: None,
    judge: Judge::default(),
    node_take: None,
    notices: Notices::of_line(&turn.line),
  };

  door.node_take = door
    .standing
    .as_ref()
    .and_then(|(place, _)| place.node())
    .map(|node| NodeTake::of_policy(&node));
  let opened = match &turn.seat {
    Seat::Standing(home) => stand(&mut door, home).await,
    Seat::Own(_) => open_own(&mut door).await,
  };
  // Whatever a co-work branch checkout's run starts once its program stands is the run's
  // (docs/run-groups.md, 「A run in a standing program」); a person's turn waited out before it is not.
  let began = crate::run_groups::ticks_now();
  let pane = match opened {
    Ok(pane) => pane,
    Err(why) => {
      door.node_settled().await;
      door.close("launch-error").await;
      door.liveness.close(&ground.db, "failed", &why).await;

      return Err(why);
    }
  };

  door.row.set("paneId", json!(pane));
  door.liveness.pane(&pane);
  door.liveness.write(&ground.db).await;

  // A turn of a standing program says so on its row, with where its record is read from and which process
  // the program was, since the pane outlives the turn and cannot be what closes it.
  if let Some((place, _)) = &door.standing {
    let running = place.running();
    let resident = json!({
      "place": place.path,
      "record": record,
      "from": from,
      "pid": running.as_ref().map(|running| running.pid),
      "birth": running.as_ref().and_then(|running| running.birth.clone()),
    });

    door.row.set("resident", resident);
  }

  let mut attempted = false;
  let mut ending = End::Exited;
  // Held on the heap: the wait is the deepest future of a turn.
  let watched = Box::pin(watch_turn(&mut door, &pane, from, &mut attempted, &mut ending)).await;

  // The asks opened on the turn's calls close with it, and so does a wait it told.
  door.watch.call_ended(None).await;
  door.node_settled().await;

  let watched = match watched {
    Ok(done) => Ok(done),
    Err(why) => {
      ending = End::Cancelled;
      Err(why)
    }
  };

  door.feed().await;
  // A question still being put is about a pane that is going away.
  door.questions.withdraw();

  match &watched {
    Ok(ended) => {
      super::liveness::kept_ending(ended.word());
      door.liveness.close(&ground.db, ended.word(), &ended.detail()).await
    }
    Err(why) => door.liveness.close(&ground.db, "failed", why).await,
  }

  match &turn.seat {
    Seat::Standing(home) => {
      // The program and its pane stay for the next turn; the run it was ends here, and the place says so
      // to the program's status line.
      if let Some((place, _)) = &door.standing {
        let _ = end_run(&place.path, now_ms());
      }

      if !door.row.ended() {
        door.close(if attempted { ending.word() } else { "launch-error" }).await;
      }

      door.finish_children().await;

      // Only a co-work branch checkout's run is taken down: a supervisor's program keeps what its turns
      // start. A place on a node runs its processes there, out of this machine's reach.
      let branch = crate::places::branch::place_of(&ground.roots.data, &turn.cwd).is_some();

      if let Some((place, _)) = door.standing.as_ref().filter(|(place, _)| branch && place.node().is_none())
      {
        let program = place.running().map(|running| running.pid);
        let ended = crate::run_groups::end_of_standing_run(
          &ground.roots.data,
          &turn.name,
          &place.path,
          began,
          program,
        )
        .await;

        if let Some(line) = ended {
          println!("{line}");
        }
      }

      // A label that named the run goes back to what the agent is, once the row is closed.
      home.rest(&pane).await;
    }
    Seat::Own(_) => {
      // Closed before the archive's writes, so a broken archive never strands the program.
      let _ = herdr.call("pane.close", json!({ "pane_id": pane }), None).await;

      if !door.row.ended() {
        door.settle_own(&pane, attempted, ending).await;
      }

      door.finish_children().await;

      // Whatever the run started ends with it before the turn is handed back (docs/run-groups.md).
      if let Some(line) = crate::run_groups::end_of_run(&ground.roots.data, door.row.id()).await {
        println!("{line}");
      }
    }
  }

  watched
}

// Where a standing program is stood for the turn: a person's turn working in it waited out first, before
// anything is stood, replaced or reset; then the program the turn needs, whose pane is answered.
async fn stand<W: Watch>(door: &mut Door<'_, W>, home: &Home) -> Result<String, String> {
  let ground = door.ground;
  let turn = door.turn;
  let place = Place::of(&ground.roots.data, &home.agent);
  let program =
    resident_of(&ground.machine(), &place.path, &turn.cwd, &turn.command, &turn.env, &turn.session);

  door.check_launch().await?;
  door.trust_here().await;

  let waited = {
    let door = &*door;

    person_turn_done(&place, home, turn.timings.poll, || door.check_launch()).await?
  };

  if waited {
    door.watch.note(&DOOR_WORDS.notes.person_turn).await;
  }

  let names = [turn.name.clone(), door.row.id().to_owned(), door.row.root().to_owned()];
  let every = Duration::from_millis(250);
  let road = turn.identity.model.as_deref().map(|model| (turn.provider.as_str(), model));
  let standing =
    stand_resident(ground, &place, home, &program, &turn.env, &turn.line, road, every, || async {
      let names: Vec<&str> = names.iter().map(String::as_str).collect();

      cancelled(&ground.db, &names).await.unwrap_or(false)
    });
  let mut standing = std::pin::pin!(standing);

  // A program on a node that does not come up is held to the bound it is held to until it takes the turn.
  let stood = loop {
    tokio::select! {
      stood = &mut standing => break stood,
      () = tokio::time::sleep(turn.timings.poll) => {
        if let Some(why) = door.node_overdue().await {
          return Err(why);
        }
      }
    }
  };

  stood
    .map(|stood| stood.pane)
    .map_err(|fault| format!("herdr would not take {}: {}: {}", turn.name, fault.code, fault.message))
}

// A pane of the turn's own, in a workspace of its own labelled with what the run is, on what model and on
// which attempt, its pane named with the run's execution as its token.
async fn open_own<W: Watch>(door: &mut Door<'_, W>) -> Result<String, String> {
  door.check_launch().await?;
  door.trust_here().await;

  let turn = door.turn;
  let model = door.row.text("model").unwrap_or("claude-code").to_owned();
  let attempt = door.row.body.get("attempt").and_then(Value::as_i64).unwrap_or(1);
  let label = short_title(
    &fill_marks(
      &DOOR_WORDS.notes.title,
      &[("title", &turn.title), ("model", &model), ("attempt", &attempt.to_string())],
    ),
    TITLE_WIDTH,
  );

  // The program started through the run-group launcher where the code tree holds it, so whatever the run
  // starts ends with it (docs/run-groups.md).
  let binary = door.ground.verbs_binary();
  let command = if binary.exists() {
    crate::run_groups::launch_line(
      &binary,
      &door.ground.roots.data,
      &turn.name,
      door.row.id(),
      &turn.command,
    )
  } else {
    turn.command.clone()
  };

  open_pane(door.herdr, door.row.id(), &label, &command, turn)
    .await
    .map_err(|fault| format!("herdr would not take {}: {}: {}", turn.name, fault.code, fault.message))
}

/// What a run spent since `from`, by the record's account lines: the last one after it less the last one
/// before it, model by model, each kind never below nought; none where no account line follows `from` or
/// nothing moved.
pub fn spend_since(records: &[Value], from: usize) -> Option<Vec<ModelSpend>> {
  let usage_at = |records: &mut dyn Iterator<Item = &Value>| {
    records
      .filter(|record| record.get("type").and_then(Value::as_str) == Some(COST_STATE))
      .last()
      .map(|record| record.get("modelUsage").cloned().unwrap_or(Value::Null))
  };
  let after = usage_at(&mut records.iter().skip(from))?;
  let before = usage_at(&mut records.iter().take(from)).unwrap_or(Value::Null);
  let number = |usage: Option<&Value>, name: &str| {
    usage.and_then(|usage| usage.get(name)).and_then(Value::as_f64).unwrap_or(0.0)
  };
  let moved: Vec<ModelSpend> = after
    .as_object()
    .into_iter()
    .flatten()
    .filter_map(|(model, total)| {
      let earlier = before.get(model);
      let less = |name: &str| (number(Some(total), name) - number(earlier, name)).max(0.0);
      let (input, output) = (less("inputTokens"), less("outputTokens"));
      let (read, write) = (less("cacheReadInputTokens"), less("cacheCreationInputTokens"));
      let cost = less("costUSD");

      [input, output, read, write, cost].iter().any(|value| *value > 0.0).then(|| {
        let (input, output, read, write) = (input as i64, output as i64, read as i64, write as i64);

        ModelSpend {
          mode: "claude-code".to_owned(),
          model: model.clone(),
          tokens: input + output + read + write,
          input_tokens: input,
          output_tokens: output,
          cache_read_tokens: read,
          cache_write_tokens: write,
          cost,
        }
      })
    })
    .collect();

  (!moved.is_empty()).then_some(moved)
}

// How wide a workspace's label is kept, as a sidebar can show it.
const TITLE_WIDTH: usize = 48;

// The workspace made and the run laid out alone in its tab, so no shell is left standing beside it to keep
// the workspace up after the run; a workspace that ends up with nothing running in it, and a pane that
// cannot be named, are closed rather than left behind with nobody to close them.
async fn open_pane(
  herdr: &Herdr,
  name: &str,
  label: &str,
  command: &[String],
  turn: &Turn,
) -> Result<String, Fault> {
  let made = herdr
    .call("workspace.create", json!({ "label": label, "cwd": turn.cwd, "env": turn.env }), None)
    .await?;
  let (Some(workspace), Some(tab)) = (
    made.pointer("/workspace/workspace_id").and_then(Value::as_str),
    made.pointer("/tab/tab_id").and_then(Value::as_str),
  ) else {
    return Err(Fault {
      code: "unreadable".to_owned(),
      message: "workspace.create named no workspace and tab".to_owned(),
    });
  };
  let root = json!({ "type": "pane", "command": command, "cwd": turn.cwd, "env": turn.env });
  let applied = herdr.call("layout.apply", json!({ "tab_id": tab, "root": root }), None).await;
  let pane = applied
    .as_ref()
    .ok()
    .and_then(|applied| applied.pointer("/layout/root/pane_id"))
    .and_then(Value::as_str)
    .map(str::to_owned);
  let Some(pane) = pane else {
    let _ = herdr.call("workspace.close", json!({ "workspace_id": workspace }), None).await;

    return Err(applied.err().unwrap_or_else(|| Fault {
      code: "unreadable".to_owned(),
      message: "layout.apply named no pane".to_owned(),
    }));
  };
  let named = herdr
    .call(
      "pane.report_metadata",
      json!({ "pane_id": pane, "source": METADATA_SOURCE, "tokens": { RUN_TOKEN: name } }),
      None,
    )
    .await;

  if let Err(fault) = named {
    let _ = herdr.call("pane.close", json!({ "pane_id": pane }), None).await;

    return Err(fault);
  }

  Ok(pane)
}

// Whether the provider's standing program takes words put in while a turn works into that turn.
pub(crate) fn takes_words_mid_turn(provider: &str) -> bool {
  harness_providers::provider(provider)
    .and_then(|declared| declared.declared().resident.as_ref())
    .is_some_and(|resident| resident.words_mid_turn)
}

async fn watch_turn<W: Watch>(
  door: &mut Door<'_, W>,
  pane: &str,
  opened_from: usize,
  attempted: &mut bool,
  ending: &mut End,
) -> Result<Ending, String> {
  let turn = door.turn;
  let timings = turn.timings.clone();
  let every = timings.poll;
  // The clock the caller keeps on the turn: the run's own, else the bound its caller sets. A job's turn
  // carries the bound that cuts it and releases its item; every other turn's clock only asks.
  let clock = turn.clock.or_else(|| door.watch.turn_clock());
  let tool_policy = door.watch.tool_clock();
  let stop_bound_ms = tool_policy.as_ref().map(|clock| clock.stop_bound_ms);
  let in_process_grace_ms = tool_policy.as_ref().map_or(i64::MAX, |clock| clock.in_process_grace_ms);
  let stall_clock = door.watch.stall_clock();
  let mut tools = tool_policy
    .map(|clock| super::tool_clock::Watch::stalling(clock, super::stall::Progress::new(stall_clock)));
  // Where this turn's own lines begin in the record, read again as the prompt goes in: a program
  // started again for this turn writes lines of its own on the way up.
  let mut from = opened_from;
  let mut prompted_at;
  // Words offered to the run from outside it, taken while its turn works, where its provider takes them.
  let listening = takes_words_mid_turn(&turn.provider).then(|| aside::listen(&turn.name));

  door.save().await;

  // One reset and one retry per turn, never a loop.
  let mut reset = false;

  loop {
    let (ready, screen) = match door.settled(pane).await? {
      Rest::Ready(ready, screen) => (ready, screen),
      Rest::Overdue(why) => {
        *ending = End::Cancelled;

        return Ok(door.node_cut(pane, why).await);
      }
    };

    if ready == Look::Gone {
      *ending = End::Cancelled;

      let screen = door.kept_screen(&DOOR_WORDS.screens.vanished, screen).await;

      return Ok(Ending::Vanished { screen });
    }

    if !ready.at_rest() {
      // A question nobody is coming to answer, or no state at all: a wedge, given its one reset. The
      // folder-trust question in a run that trusts its folder is nobody's to answer: the trust is
      // written again and the program stood up again, and where even that leaves it standing the turn
      // names it. A plugin offer its answers did not clear is nobody's either: a wedge like any other.
      let untrusted = turn.trusts_folder && ready.is("blocked") && folder_trust_keys(&screen).is_some();
      let offered = ready.is("blocked") && plugin_offer_keys(&screen).is_some();
      // A question is left standing for whoever is to answer it, and only a turn that can put one to
      // somebody has such a question; for any other turn a blocked program is a wedge like any other.
      let asks_somebody = ready.is("blocked") && door.watch.puts_questions() && !untrusted && !offered;
      let shown = if screen.is_empty() { door.screen(pane).await } else { screen };
      let screen = door.kept_screen(&DOOR_WORDS.screens.not_ready, shown).await;

      if !reset && !asks_somebody {
        reset = true;

        if untrusted {
          door.trust_here().await;
        }

        let why = if untrusted { &DOOR_WORDS.notes.untrusted } else { &DOOR_WORDS.notes.not_ready };

        if door.stand_again(pane, why).await {
          continue;
        }
      }

      *ending = End::Cancelled;

      return Ok(if untrusted {
        Ending::Untrusted {
          screen,
          folder: turn.cwd.clone(),
          config: door.trust_config.display().to_string(),
        }
      } else {
        Ending::Blocked { screen }
      });
    }

    if door.cancelled().await {
      return Err("execution cancelled".to_owned());
    }

    door.check_launch().await?;
    *attempted = true;
    // The run this turn is goes into the place before the prompt, and the prompt's first line names it
    // where the screen shows it. A command of the program's own is the whole of what it is told, since
    // the program reads it as a command only when nothing stands before it.
    prompted_at = now_ms();

    if let Some((place, _)) = &door.standing
      && let Err(why) =
        begin_run(&place.path, &turn.name, &turn.title, prompted_at, run_variables(&turn.env))
    {
      eprintln!("{}: the place's run: {why}", turn.name);
    }

    from = from.max(door.lines().len());

    if let Some(resident) = door.row.body.get_mut("resident") {
      resident["from"] = json!(from);
    }

    // The program the words go into is the one standing now, a reset before them included.
    door.name_standing();
    door.liveness.phase(Phase::Prompting);
    door.liveness.write(&door.ground.db).await;

    // Nothing is typed where the program's input addresses one of its background agents.
    if !door.input_on_main(pane).await {
      *ending = End::Cancelled;

      let shown = door.screen(pane).await;
      let screen = door.kept_screen(&DOOR_WORDS.agent_view.stuck, shown).await;

      return Ok(Ending::Blocked { screen });
    }

    let sent = if turn.prompt.starts_with('/') {
      prompt(door.herdr, pane, &turn.prompt).await
    } else if door.standing.is_some() {
      // A resident program's turn is typed behind the line naming the run. A round's lead is nothing, so
      // the head line is the whole of what is typed before its words are pasted.
      let header = run_header(&turn.name, &turn.title, prompted_at);
      let lead = if turn.lead.is_empty() { header } else { format!("{header} {}", turn.lead) };

      prompt_with_lead(door.herdr, pane, &lead, &turn.prompt).await
    } else {
      // A pane of the turn's own runs nothing else, so its first line needs no name of the run.
      prompt_with_lead(door.herdr, pane, &turn.lead, &turn.prompt).await
    };

    if let Err(fault) = sent {
      let shown = door.screen(pane).await;
      let screen = door
        .kept_screen(
          &fill_marks(
            &DOOR_WORDS.screens.not_taken,
            &[("code", &fault.code.to_string()), ("message", &fault.message)],
          ),
          shown,
        )
        .await;

      // A refusal that is a question somebody is to be asked is left standing like the one above.
      if is_blocked(&fault) && door.watch.puts_questions() {
        *ending = End::Cancelled;

        return Ok(Ending::Blocked { screen });
      }

      // Words too long for herdr are refused the same way however often the program is stood up again.
      if !reset && fault.code != TOO_LARGE {
        reset = true;

        if door.stand_again(pane, &DOOR_WORDS.notes.not_taken).await {
          continue;
        }
      }

      // Refused for good, the words never became a turn. The lead typed before the refusal is still in
      // the box, so the program is started again with it empty.
      door.interrupt(pane, &Interrupt::EndProgram).await;
      *ending = End::Cancelled;

      return Ok(Ending::Unsubmitted {
        screen,
        why: fault.to_string(),
      });
    }

    break;
  }

  // Accepted is not submitted: pasted words can stand in the input box unsent, and a program at rest
  // with words in its box reads exactly like one that has finished.
  // `enter` is pressed again at each submit pace until the words are seen taken; an unsent box ends
  // nothing.
  let mut taken = door.taken(pane, from).await;

  while matches!(taken, Submit::Unsent) {
    if door.cancelled().await {
      return Err("execution cancelled".to_owned());
    }

    // A program gone takes no `enter`: the words went into nothing that can answer them.
    if matches!(door.peer(), Peer::Gone(_)) && !turn_closed_since(door.lines(), from) {
      *ending = End::Exited;

      return Ok(door.lost(pane, Lost::Program, "unsubmitted").await);
    }

    if let Some(why) = door.node_overdue().await {
      *ending = End::Cancelled;

      return Ok(door.node_cut(pane, why).await);
    }

    door.watch.note(&DOOR_WORDS.notes.enter_pressed).await;
    let _ = send_keys(door.herdr, pane, &["enter"]).await;
    taken = door.taken(pane, from).await;
  }

  door.row.set("state", json!("running"));
  door.save().await;
  door.fed = Some(from);
  door.node_settled().await;
  door.liveness.phase(Phase::Working);
  door.liveness.write(&door.ground.db).await;

  let mut worked = matches!(taken, Submit::Working);
  // When the caller's overtime policy is asked next; none once it asks nothing.
  let mut ask_at = clock.map(|clock| now_ms() + clock.as_millis() as i64);
  // A command of the program's own writes no answer and closes no turn: its own words in the record are its
  // end, and one that writes none is over at rest, its record still between two looks.
  let mut command = turn.prompt.starts_with('/');
  let mut rested_at: Option<usize> = None;
  let mut answer_from = from;
  // The moment since which a failed request's end is this turn's, moved with the words owed after a
  // refusal.
  let mut failed_from = prompted_at;
  let mut stop_asked = false;
  let mut stop_seen_at: Option<i64> = None;
  let mut stop_cut = false;
  let mut in_process_cut: Option<super::tool_clock::InProcessCut> = None;
  let mut silent_since: Option<i64> = None;
  // The answers typed to a plugin offer while the turn is watched.
  let mut offers = OwnAnswers::new(timings.redraw);

  loop {
    // A job's or a role session's tool calls, read off the record as it goes: each start with its pid and
    // summary, a still-running line past its threshold, each end with its duration and outcome, and a call
    // that has stopped burning processor time cut so it cannot hold the turn for ever.
    let program = door.program.as_ref().map(|program| program.pid);

    if let (Some(watching), Some(program)) = (tools.as_mut(), program) {
      watching.subagent_records(door.tail.path(), now_ms());

      let records = door.tail.look();
      let mut question = CallerQuestion(&mut *door.watch);
      let said = watching.look(&turn.name, records, from, program, now_ms(), &mut question).await;

      for note in &said {
        let sentence = note.sentence();

        println!("{sentence}");
        door.watch.clock(&sentence, note).await;
      }

      // A call with no process of its own is cut by words put into the working turn: the program ends the
      // waiting call on them and hands the model a failed result, and the turn goes on.
      while let Some(cut) = watching.take_in_process_cut() {
        let said = format!("{}: the call {} was cut, its words put into the turn", turn.name, cut.tool);

        println!("{said}");
        door.watch.note(&said).await;

        if takes_words_mid_turn(&turn.provider) {
          if let Err(fault) = prompt_with_lead(door.herdr, pane, &turn.lead, &cut.words).await {
            let said = format!(
              "{}: the call {}'s cut words did not go in: {}: {}",
              turn.name, cut.tool, fault.code, fault.message
            );

            println!("{said}");
            door.watch.note(&said).await;
          }
        } else {
          door.interrupt(pane, &turn.interrupt).await;
        }

        if in_process_cut.is_none() {
          in_process_cut = Some(cut);
        }
      }

      if !watching.in_process_cut_still() {
        in_process_cut = None;
      }

      // A call gone silent and idle is asked about, never cut by the clock: the caller's policy opens the
      // ask and reads its verdict, and only a cut on it stops the call, with words put into the turn.
      while let Some(silent) = watching.take_silent() {
        let verdict = door.watch.silent_call(&silent).await;
        let answered = watching.answer(&turn.name, &silent.id, verdict, now_ms()).await;

        for note in &answered.lines {
          let sentence = note.sentence();

          println!("{sentence}");
          door.watch.clock(&sentence, note).await;
        }

        if let Some(words) = answered.words
          && takes_words_mid_turn(&turn.provider)
          && let Err(fault) = prompt_with_lead(door.herdr, pane, &turn.lead, &words).await
        {
          let said = format!(
            "{}: the cut words of the call {} did not go in: {}: {}",
            turn.name, silent.tool, fault.code, fault.message
          );

          println!("{said}");
          door.watch.note(&said).await;
        }
      }

      for ended in watching.take_closed_asks() {
        door.watch.call_ended(Some(&ended)).await;
      }

      while let Some(change) = watching.take_stall() {
        door.watch.stalled(&change).await;
      }
    }

    // Stopped from outside while the turn was running. A job is asked to end its work and waited on
    // until it has; a chat round has no work of its own to wind down and is cut short at once. A pane of
    // the turn's own is closed with the turn, which ends its program.
    if door.watch.stopping() || door.cancelled().await {
      let now = now_ms();
      let seen = *stop_seen_at.get_or_insert(now);
      let asks_first = turn.line != "chat";

      // A call blocked on a read or a write cannot take the stop's words; its processes are cut so the
      // words are read at the next boundary. A call still burning processor time is left to the bound.
      if !stop_cut {
        let program = door.program.as_ref().map(|program| program.pid);

        if let (Some(watching), Some(program)) = (tools.as_mut(), program) {
          stop_cut = true;

          let records = door.lines();
          let said = watching.cut_for_stop(&turn.name, records, from, program, now).await;

          for note in &said {
            let sentence = note.sentence();

            println!("{sentence}");
            door.watch.clock(&sentence, note).await;
          }
        }
      }

      // Past the stop's bound the turn is ended by the surface's interrupt, whatever the program is
      // doing: an item and the queue behind it are not held by a turn that will not end.
      if stop_bound_ms.is_some_and(|bound| now - seen >= bound) {
        let said =
          format!("{}: the stop was not ended within its bound, so the turn is interrupted", turn.name);

        println!("{said}");
        door.watch.note(&said).await;
        door.interrupt(pane, &turn.interrupt).await;
        *ending = End::Cancelled;

        let shown = door.screen(pane).await;
        let screen = door.kept_screen(&DOOR_WORDS.screens.stopped, shown).await;

        return Ok(Ending::Stopped { screen });
      }

      if door.standing.is_some() && !stop_asked {
        stop_asked = true;

        if asks_first {
          if let Some(why) =
            turn_refusal(&door.ground.db, &door.ground.machine(), JOB_WORK_KEY, &turn.env).await
          {
            return Err(why);
          }

          let _ =
            prompt_with_lead(door.herdr, pane, &turn.lead, &policy::run_stop::RUN_STOP.instruction).await;
          tokio::time::sleep(every).await;
          continue;
        }
      }

      if stop_asked {
        let standing = door.look(pane, GoneConfirm::default(), prompted_at).await;

        // A job told to stop ends its turn itself; a herdr that does not answer has not said it has.
        if asks_first && (standing.is("working") || matches!(standing, Look::Silent(_))) {
          tokio::time::sleep(every).await;
          continue;
        }

        if standing.is("working") {
          door.interrupt(pane, &turn.interrupt).await;

          // The row closes as a stop on the interrupted marker the program writes, looked for until it
          // is written or the program reads at rest or gone.
          while door.round_state(from) == RoundState::Working
            && door.look(pane, GoneConfirm::default(), prompted_at).await.is("working")
          {
            tokio::time::sleep(every).await;
          }

          if command {
            door.clear_command(pane).await;
          }
        }
      }

      *ending = End::Cancelled;

      let shown = door.screen(pane).await;
      let screen = door.kept_screen(&DOOR_WORDS.screens.stopped, shown).await;

      // What the program left running in its own background ends with the stop on a line that carries it
      // apart.
      if door.notices == Notices::Apart {
        door.end_background(pane).await;
      }

      return Ok(Ending::Stopped { screen });
    }

    // A call cut with no process of its own whose words the program did not take within its grace: the
    // turn is ended there, so a program that cannot take the words does not hold its item to its bound.
    if let Some(cut) = &in_process_cut
      && tools.as_ref().is_some_and(|watching| watching.in_process_cut_still())
      && now_ms() - cut.at >= in_process_grace_ms
    {
      let why = door.watch.held_by_processless_call(&cut.tool, cut.seconds).await;
      let said = match &why {
        Some(reason) => format!("{}: {reason}", turn.name),
        None => format!(
          "{}: the call {} did not take its cut within {}s, so the turn is stopped",
          turn.name,
          cut.tool,
          in_process_grace_ms / 1_000
        ),
      };

      println!("{said}");
      door.watch.note(&said).await;
      door.interrupt(pane, &turn.interrupt).await;
      *ending = End::Cancelled;

      let shown = door.screen(pane).await;
      let screen = door.kept_screen(&said, shown).await;

      return Ok(if why.is_some() { Ending::Overran { screen } } else { Ending::Stopped { screen } });
    }

    // Past its clock the caller's policy is asked, and only its verdict cuts the turn.
    if let (Some(at), Some(clock)) = (ask_at, clock)
      && now_ms() >= at
    {
      let activity = last_activity(door.lines(), from);
      let asked = Overrun {
        started_at: prompted_at,
        bound_ms: clock.as_millis() as i64,
        last_activity_at: activity.at,
        last_activity: activity.what,
      };

      match door.watch.overtime(&asked).await {
        Overtime::AskAgainAt(again) => ask_at = Some(again.max(now_ms() + 1)),
        Overtime::NotAsked => ask_at = None,
        Overtime::Failed(why) => {
          door
            .watch
            .note(&fill_marks(&DOOR_WORDS.notes.overrun_unasked, &[("why", &why.to_string())]))
            .await;
          ask_at = Some(now_ms() + clock.as_millis() as i64);
        }
        Overtime::Cut => {
          door.interrupt(pane, &turn.interrupt).await;
          *ending = End::Cancelled;

          let shown = door.screen(pane).await;
          let screen = door.kept_screen(&DOOR_WORDS.screens.overran, shown).await;

          return Ok(Ending::Overran { screen });
        }
      }
    }

    // The program is read before herdr, so a program gone ends the turn while herdr does not answer as
    // well; a record that closed the turn meanwhile is read as it stands.
    let signals = door.signals();
    let peer = door.peer();
    let closed = {
      let records = door.lines();

      turn_closed_since(records, answer_from)
        || ended_on_error(records, answer_from)
        || rests_on_error(records, answer_from)
        || (command && command_words(records, answer_from).is_some())
    } || door.failure_since(failed_from).is_some();

    if matches!(peer, Peer::Gone(_)) {
      if closed {
        break;
      }

      *ending = End::Exited;

      return Ok(door.lost(pane, Lost::Program, "").await);
    }

    let now = door.look(pane, timings.gone, prompted_at).await;

    door.looked(&now).await;

    if now == Look::Gone {
      // Gone mid-turn, as herdr has said look after look: somebody closing it.
      *ending = End::Cancelled;

      return Ok(Ending::Vanished {
        screen: "the pane went away while the turn was running".to_owned(),
      });
    }

    // herdr said nothing either way: the record is read on and herdr asked again, however long the
    // silence lasts; it ends nothing.
    if let Look::Silent(cause) = &now {
      if silent_since.is_none() {
        door
          .watch
          .note(&fill_marks(&DOOR_WORDS.notes.herdr_silent, &[("cause", &cause.to_string())]))
          .await;
        silent_since = Some(now_ms());
      }

      tokio::time::sleep(every).await;
      continue;
    }

    // The silence's end is written as its start was.
    if let Some(since) = silent_since.take() {
      door
        .watch
        .note(&fill_marks(
          &DOOR_WORDS.notes.herdr_back,
          &[
            ("seconds", &((now_ms() - since) as f64 / 1000.0).round().to_string()),
            ("state", now.word()),
          ],
        ))
        .await;
    }

    door.record_children().await;

    if now.is("working") {
      worked = true;
    }

    // No program to name while the pane shows none, and nothing it writes moved since the look before:
    // the other end is gone.
    if let Some(lost) = door.judge.look(&peer, closed, &now, &signals) {
      *ending = End::Cancelled;

      return Ok(door.lost(pane, lost, now.word()).await);
    }

    // Over where the program ended the turn on its own error, whatever the pane shows: a full window leaves
    // the program standing at its own choice of what to do next, which nobody is coming to answer and one
    // of whose answers spends usage credits, so it is closed with the keys the provider cuts a turn
    // with — on Claude Code its 「stop and wait」 — and the program stands at rest for the next turn.
    // It is read before any question the pane holds is put to anybody, since this one is no question of
    // the turn's, and before any word is pushed in: a failure the program reported is its own word that
    // the turn is over, however herdr still reads the pane, and a word pushed after it would open a turn of
    // the program's own that hides it.
    if over_on_error(
      now.is("working"),
      ended_on_error(door.lines(), answer_from) || rests_on_error(door.lines(), answer_from),
      door.failure_since(failed_from).is_some(),
    ) {
      if now.is("blocked") {
        door.watch.note(&DOOR_WORDS.notes.error_menu).await;
        door.interrupt(pane, &turn.interrupt).await;
      }

      break;
    }

    // The plugin offer is no question of the turn's: it is declined from the screen ahead of any question
    // put to a person, and the watching waits for its answer to clear it. One its answers did not clear
    // is left to the questions.
    if now.is("blocked") && offers.left() {
      let seen = door.screen(pane).await;

      if plugin_offer_keys(&seen).is_some() {
        door.answer_own(pane, &seen, &mut offers).await;
        tokio::time::sleep(every).await;
        continue;
      }
    }

    door.questions.look(door.herdr, pane, &now, door.watch).await;

    // Words led by a slash the program took for a prompt answer as any prompt does, and are read as one.
    command = command && !answered_since(door.lines(), answer_from);

    if command {
      let records = door.lines();

      if command_words(records, answer_from).is_some() {
        break;
      }

      if now.at_rest() && (worked || command_closing_since(records, answer_from)) {
        if rested_at == Some(records.len()) {
          break;
        }

        rested_at = Some(records.len());
      } else {
        rested_at = None;
      }

      tokio::time::sleep(every).await;
      continue;
    }

    // Words owed after a refusal the screen could not carry go in once the run is back at rest, and the
    // turn goes on: what it does with them is still this turn's work.
    if now.at_rest()
      && let Some(owed) = door.questions.owed()
    {
      door.check_launch().await?;
      door.hand(true);

      let told = prompt_with_lead(door.herdr, pane, &turn.lead, &owed).await;

      door.questions.paid();

      if told.is_err() {
        door.hand(false);
      } else {
        answer_from = door.lines().len();
        failed_from = now_ms();
        worked = false;
        tokio::time::sleep(every).await;
        continue;
      }
    }

    // Over when it has come back to rest having done something: seen at work, or with an answer in the
    // record written after this turn began, which the idle before the prompt cannot have.
    if now.at_rest() && (worked || answered_since(door.lines(), answer_from)) {
      break;
    }

    // Back at rest never seen working and with no answer: over once the record closes a turn after the
    // prompt.
    if now.at_rest() && !worked && turn_closed_since(door.lines(), answer_from) {
      break;
    }

    // Only while it is working: a word pushed at an agent that has not started is one the turn's own
    // prompt has not reached yet, and one pushed after it has stopped belongs to the next turn.
    if now.is("working") {
      let offered = listening.as_ref().and_then(aside::Listening::take);
      let words = match &offered {
        Some(offered) => Some(offered.words.clone()),
        None => door.watch.words().await,
      };

      if let Some(words) = words {
        if let Some(why) =
          turn_refusal(&door.ground.db, &door.ground.machine(), JOB_WORK_KEY, &BTreeMap::new()).await
        {
          if let (Some(listening), Some(offered)) = (&listening, offered) {
            listening.said(offered, AsideOutcome::Refused);
          }

          return Err(why);
        }

        let outcome = match door.push_word(pane, from, &words).await {
          Ok(_) => AsideOutcome::Delivered,
          Err(fault) if is_blocked(&fault) => AsideOutcome::Waiting,
          Err(_) => AsideOutcome::Refused,
        };

        match (&listening, offered) {
          (Some(listening), Some(offered)) => listening.said(offered, outcome),
          _ => door.watch.said(outcome).await,
        }
      }
    }

    tokio::time::sleep(every).await;
  }

  // At rest is not over: the program's screen can come to rest before the answer of a turn of a second or
  // two has reached its record, and the record's closing line comes after the last of the answer's words.
  // So a turn seen at work, or one that answered, is over once that line is written, waited for until it
  // is; one a person interrupted writes the interrupted marker instead, one a failed request ended is over on
  // its program's failure hook, which no such line follows, and a turn stopped meanwhile, one
  // whose pane herdr says is gone, or one whose program is gone, is read as it stands. No program to name,
  // a pane showing none and nothing it writes moving between two looks is the other end gone. Where
  // nothing of an answer was written, a program gone or a silent peer is the turn's ending.
  let mut lost_after_rest: Option<(Lost, String)> = None;

  if !command && (worked || answered_since(door.lines(), answer_from)) {
    loop {
      if end_written(
        turn_closed_since(door.lines(), answer_from),
        door.round_state(answer_from) == RoundState::Interrupted,
        door.failure_since(failed_from).is_some(),
      ) || door.cancelled().await
      {
        break;
      }

      let signals = door.signals();
      let peer = door.peer();

      if matches!(peer, Peer::Gone(_)) {
        lost_after_rest = Some((Lost::Program, String::new()));
        break;
      }

      let now = door.look(pane, GoneConfirm::default(), prompted_at).await;

      door.looked(&now).await;

      if now == Look::Gone {
        break;
      }

      // A program at rest on its own error writes no closing line after it.
      if !now.is("working") && rests_on_error(door.lines(), answer_from) {
        break;
      }

      if let Some(lost) = door.judge.look(&peer, false, &now, &signals) {
        lost_after_rest = Some((lost, now.word().to_owned()));
        break;
      }

      tokio::time::sleep(every).await;
    }
  }

  if let Some((lost, herdr)) = lost_after_rest
    && !answered_since(door.lines(), answer_from)
    && !turn_closed_since(door.lines(), answer_from)
    && !ended_on_error(door.lines(), answer_from)
    && !rests_on_error(door.lines(), answer_from)
    && door.failure_since(failed_from).is_none()
  {
    *ending = if lost == Lost::Program { End::Exited } else { End::Cancelled };

    return Ok(door.lost(pane, lost, &herdr).await);
  }

  // The program holds on to what it was handed and takes it as a turn of its own once the turn it was
  // in has ended, so the round is over only once nothing it was handed is still queued and every turn
  // it owes has written its end. The count can expect a turn none is coming for — a notice queued twice
  // under one task — so a program reading at rest on two looks in a row, its record unchanged between
  // them and ending on a turn's end, has nothing still to come, and the wait ends there and says what the
  // count still expected. A program of the turn's own is handed nothing past its prompt.
  if door.standing.is_some() && answered_since(door.lines(), from) {
    let mut rested_at: Option<usize> = None;

    while !door.cancelled().await {
      let notices = door.notices;
      let (working, count, ends_on_end, owed, ended) = {
        let records = door.lines();
        let round = records.get(from..).unwrap_or_default();
        let ended = round
          .iter()
          .filter(|record| {
            record.get("type").and_then(Value::as_str) == Some("system")
              && record.get("subtype").and_then(Value::as_str) == Some("turn_duration")
          })
          .count();

        (
          read_round(round, notices).state == RoundState::Working,
          records.len(),
          ends_on_turn_end(round),
          turns_owed(round, notices),
          ended,
        )
      };

      if !working {
        break;
      }

      let now = door.look(pane, GoneConfirm::default(), prompted_at).await;

      // A program gone owes nothing more.
      if now == Look::Gone || !door.program_stands(pane).await {
        break;
      }

      if rests_on_an_end(now.at_rest(), ends_on_end, door.failure_since(failed_from).is_some()) {
        if rested_at == Some(count) {
          door
            .watch
            .note(&fill_marks(
              &DOOR_WORDS.notes.quiet,
              &[
                ("queued", &owed.queued.to_string()),
                ("owed", &owed.owed.to_string()),
                ("ended", &ended.to_string()),
              ],
            ))
            .await;
          break;
        }

        rested_at = Some(count);
      } else {
        rested_at = None;
      }

      tokio::time::sleep(every).await;
    }
  }

  // The round is read to its end, so no word of it is still to arrive, and a mark left by a word whose
  // submit never came would take the next person's line for the bot's.
  door.hand(false);

  // The turn's lines as they stand at its end, read across the waits below: the reader holds them from the
  // turn's beginning, so this is the one copy a turn makes of them.
  let records = door.lines().to_vec();
  let answered = answered_since(&records, from);
  let read = read_turn(&records, from);
  let failure = door.failure_since(failed_from).map(|failure| failure.failure());

  if command {
    return Ok(command_ended(door, pane, &records, from, &read, failure).await);
  }

  if let Some((error, kind)) = turn_error(&read, answered, failure) {
    if answered {
      door.record_children().await;
    }

    crate::keepalive::logins::node_token_refused(door.ground, &turn.env, &error).await;

    return Ok(Ending::Errored { error, kind });
  }

  if !answered {
    let shown = door.screen(pane).await;
    let screen = door.kept_screen(&DOOR_WORDS.screens.silent, shown).await;

    return Ok(Ending::Silent { screen });
  }

  door.record_children().await;

  let accounts = claude_accounts::accounts(&door.ground.db).await.unwrap_or_default();
  let machine_config = ClaudeCode::machine_config_dir(&door.ground.machine());
  let login = program_login(&machine_config, &turn.env);
  let Some(place) = door.standing.as_ref().map(|(place, _)| place.path.clone()) else {
    // What a program of the turn's own cost it writes as it ends, so its pane is closed first and the
    // record watched for that line; a run whose account never comes keeps what its own lines say, with no
    // price.
    let _ = door.herdr.call("pane.close", json!({ "pane_id": pane }), None).await;

    let Some(spend) = door.account_line(pane, records.len()).await else {
      return Ok(stopped_after_end(door, pane, ending).await);
    };

    door.close("exited").await;

    let mut reply = read.reply(&turn.session, spend.unwrap_or_else(|| read.spend.clone()));

    reply.account = account_of_login(&accounts, &login, &door.ground.roots.data);

    return Ok(Ending::Answered {
      reply: Box::new(reply),
      records: records.len(),
    });
  };

  // The program stays up, so what the turn cost is its status line's running total after the turn less
  // the total before it, with the account's windows beside it; a reading that does not come keeps what
  // the record says, with no price. The turn ended where its Stop hook says.
  let stopped_at = read_hooks(&place, prompted_at)
    .iter()
    .rev()
    .find(|hook| hook.ends_turn())
    .map_or_else(now_ms, |hook| hook.at);
  let asked: &Door<'_, W> = door;
  let standing = asked.standing.as_ref().map(|(standing, _)| standing);
  let bound = Duration::from_millis(RESIDENT_TURN.status_after_ms);
  let around = status_after_turn(
    &place,
    prompted_at,
    stopped_at,
    Some(&turn.session),
    every,
    bound,
    || standing.is_some_and(|standing| standing.program_alive().is_some()),
    || asked.cancelled(),
  )
  .await;
  // A stop that came while the reading was waited on ends the turn stopped, as one that came while it worked does.
  let Some(around) = around else {
    return Ok(stopped_after_end(door, pane, ending).await);
  };
  // The windows are the login's that made the reading, named by the reading itself; a reading that
  // names no registered login is written under nobody.
  let data = &door.ground.roots.data;
  let wrote = around.login.as_ref().and_then(|login| account_of_login(&accounts, login, data));
  let figures = status_figures(around.before.as_ref(), around.after.as_ref());
  let spend = match figures.money {
    Some(money) if read.spend.len() == 1 => read
      .spend
      .iter()
      .cloned()
      .map(|mut row| {
        row.cost = money;
        row
      })
      .collect(),
    _ => read.spend.clone(),
  };
  let round = read_round(records.get(from..).unwrap_or_default(), door.notices);
  let answered_by = wrote.clone().or_else(|| account_of_login(&accounts, &login, data));

  door.close("exited").await;

  let mut reply = read.reply(&turn.session, spend);

  reply.text = if round.text.is_empty() { read.text.clone() } else { round.text };
  reply.answers = round.answers;

  if wrote.is_some() {
    reply.session_usage = figures.session_usage.clone();
    reply.windows = figures.windows.iter().cloned().map(ReplyWindow::Status).collect();
  }

  reply.context_ratio = figures.context_ratio;
  reply.account = answered_by;
  reply.context_tokens = figures.context_tokens.or(read.context_tokens);

  Ok(Ending::Answered {
    reply: Box::new(reply),
    records: records.len(),
  })
}

// A turn its record had closed, ended stopped all the same: a stop stood over its run while the reading of its
// price after its end was waited on, and its caller asked for the turn to be over (docs/agent-control.md,
// 「Telling that a turn's program is gone」). The program is at rest or gone, so nothing is cut.
async fn stopped_after_end<W: Watch>(door: &mut Door<'_, W>, pane: &str, ending: &mut End) -> Ending {
  *ending = End::Cancelled;

  let shown = door.screen(pane).await;
  let screen = door.kept_screen(&DOOR_WORDS.screens.stopped, shown).await;

  Ending::Stopped { screen }
}
