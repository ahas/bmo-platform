//! The herdr backend of the session contract (docs/api.md, 「Sessions」): the ten operations answered over
//! the door, the places and the lines. A worker's session is its own, one per worker, carried by the
//! worker's session turn on the line a job runs on; a supervisor's session is one per room, carried by its
//! resident program on the chat line with the system prompt its settings compose; a session across rooms is
//! the one its agent's row keeps, carried by its resident program on the chat line around the room block,
//! the road and the base persona, its brain's or else the one the turn's caller hands in, or by a stand-in
//! where the agent's key walks onto one; a thrown-away session, held in memory from its start to its end,
//! carries a co-work branch's turns or a runner line's, each on a fresh harness session; and a conversation
//! the caller opened, held there too, carries ordinary turns on the provider and model it names, resuming the
//! session its road keeps. A session is known by its agent and its scope — `<agent>@worker`,
//! `<agent>@<room>`, `<agent>`, `<agent>@run:<n>`, `<agent>@conversation:<n>` — and stands across the
//! generations its harness opens.
//!
//! A turn is carried whole before *send a turn* answers with its end: whether the program took the words,
//! or refused them while it waits on a person, and how it ended. Its end is published as `turn-ended`, and
//! its start as
//! `turn-started`; a conversational turn's road as `road-walked`. Words pushed into a session are offered to
//! the run its turn carries, which its door takes at its next look at the working program.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use agent_session::EngineError;
use agent_session::{
  Answer, Apart, Aside, Context, Conversation, Fault, FaultKind, GateAsk, Outcome, Output, RoomBlock,
  Scope, Session, SessionId, Sessions, Start, State, Turn, TurnAnswer, TurnContext, TurnEnd, WentOut,
  WorkParts,
};
use indexmap::IndexMap;
use policy::fill_marks;
use policy::gate_places::GATE_PLACES;
use policy::handoff::HANDOFF;
use policy::supervisor_runner::SUPERVISOR_RUNNER;
use sea_orm::TransactionTrait;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use store::{all, one, run as written};

use crate::chat_stand_in::road_turn;
use crate::control::supervisor_settings::Settings;
use crate::door::aside;
use crate::door::record::{ModelSpend, UsageSpent};
use crate::door::turn::AsideOutcome;
use crate::events::Event;
use crate::handoff::{Asking, Cause};
use crate::job_run::claude::turn::{Shows, TASK_LOCKED, TURN_AGENT, WorkAsked, WorkSeat, carry, continues};
use crate::lines::Trees;
use crate::lines::chat::{ChatLaunch, RoomContext, compose_prepared, compose_turn, gate_run};
use crate::lines::pieces::{ComposedTurn, SupervisorTurn, compose, lost_session, registration_named};
use crate::places::gate_places;
use crate::places::{AgentRow, Ground, Places};
use crate::run_ledger;
use crate::run_log::lines::Line;
use crate::run_log::{RunLog, Target};
use crate::session_output::{OutputWatch, Outputs};
use crate::session_questions::Questions;
use crate::supervisor_turn::guard::{Spent, spent_out};
use crate::supervisor_turn::handoff::{Due, Speaker, Why, hand_over};
use crate::supervisor_turn::overtaken;
use crate::supervisor_turn::run::{NoAnswer, Reply, Tally, let_go, run_turn};
use crate::supervisor_turn::sessions::{self, Harness, Kept, RoomSession, harness_road};
use crate::watches::machine::keep_worker_job;
use crate::worker_session;
use crate::worker_session::fold::{Folded, fold};
use crate::worker_session::records::{self, note_answer, slot_account};
use crate::{Refusal, now_ms};

// What a worker's session is told apart by in its id, after its agent.
const WORKER: &str = "worker";

// What a thrown-away session is told apart by in its id, after its agent.
const APART: &str = "run";

// What a conversation the caller opened is told apart by in its id, after its agent.
const CONVERSATION: &str = "conversation";

/// Remembers a thrown-away session a picked-up turn still runs on, with the scope its execution row names, so
/// its state answers by id until that turn's end is handed back.
pub fn remember_carried(ground: &Ground, session: SessionId, agent: String, scope: Apart) {
  ground
    .carried
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner)
    .insert(session, (agent, scope));
}

/// Lets go of a carried thrown-away session as its turn ends.
pub fn forget_carried(ground: &Ground, session: &SessionId) {
  ground.carried.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(session);
}

// The scope a carried thrown-away session stands on, where the pickup adopted its turn.
fn carried_scope_of(ground: &Ground, id: &SessionId) -> Option<(String, Apart)> {
  ground
    .carried
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner)
    .get(id)
    .cloned()
}

/// A turn put into a session began: the session, its id, and the run it goes out as.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStarted {
  pub session: String,
  pub run: String,
  pub turn_id: i64,
  pub at: i64,
}

impl Event for TurnStarted {
  const NAME: &'static str = "turn.created";
}

/// What a running turn waits for, as it begins, changes and clears (docs/api.md, 「sessions」): the session,
/// the turn, the run it goes out as, and the wait, none once it has cleared.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnChanged {
  pub session: String,
  pub turn_id: i64,
  pub run: String,
  pub waiting: Option<agent_session::Waiting>,
}

impl Event for TurnChanged {
  const NAME: &'static str = "turn.changed";
}

/// What a turn came to, as its send answers it (docs/api.md, 「Sessions」, a turn's end), and the session
/// it went on.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TurnEnded {
  pub session: String,
  /// The key its caller sent it under, where it named one (docs/api.md, 「sessions」).
  #[serde(skip_serializing_if = "Option::is_none")]
  pub key: Option<String>,
  #[serde(flatten)]
  pub end: TurnEnd,
}

impl Event for TurnEnded {
  const NAME: &'static str = "turn.ended";
}

/// A session's system prompt changed, as the turn that sent the changed prompt records it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPromptChanged {
  pub session: String,
  pub run: String,
}

impl Event for SystemPromptChanged {
  const NAME: &'static str = "system-prompt-changed";
}

/// The road a conversational turn walked (docs/run-records.md, 「The roads a turn walked」): its stage, the
/// room and the run, the provider and model it went out on, how it ended, how long it held the run, its
/// tokens summed over the models that answered — none where it reported none — and how full the session's
/// window stood after it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoadWalked {
  pub stage: String,
  pub chat_id: i64,
  pub run: String,
  pub provider: String,
  pub model: String,
  pub outcome: &'static str,
  pub ms: i64,
  pub input_tokens: Option<i64>,
  pub output_tokens: Option<i64>,
  pub cache_read_tokens: Option<i64>,
  pub cache_write_tokens: Option<i64>,
  pub context_ratio: Option<f64>,
}

impl Event for RoadWalked {
  const NAME: &'static str = "road-walked";
}

impl RoadWalked {
  /// The road a turn walked from `started`, its tokens added over what each model reported.
  #[allow(clippy::too_many_arguments)]
  pub fn of(
    chat_id: i64,
    run: &str,
    provider: &str,
    model: &str,
    answered: bool,
    started: Instant,
    spend: &[ModelSpend],
    context_ratio: Option<f64>,
  ) -> Self {
    let reported = !spend.is_empty();
    let sum = |pick: fn(&ModelSpend) -> i64| reported.then(|| spend.iter().map(pick).sum::<i64>());

    Self {
      stage: SUPERVISOR_RUNNER.turn_stage.clone(),
      chat_id,
      run: run.to_owned(),
      provider: provider.to_owned(),
      model: model.to_owned(),
      outcome: if answered { "answered" } else { "error" },
      ms: i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX),
      input_tokens: sum(|one| one.input_tokens),
      output_tokens: sum(|one| one.output_tokens),
      cache_read_tokens: sum(|one| one.cache_read_tokens),
      cache_write_tokens: sum(|one| one.cache_write_tokens),
      context_ratio,
    }
  }
}

// The provider a turn across rooms goes out on: the chat line is mapped on Claude Code alone.
const CHAT_PROVIDER: &str = "claude-code";

/// The model a repository's pin names now, or none where the repository pins none. A job there stands on
/// this model (docs/repositories.md, 「A pinned model」).
pub async fn pinned_worker_model(
  db: &sea_orm::DatabaseConnection,
  repository: &str,
) -> Result<Option<String>, store::Fault> {
  Ok(
    crate::repositories::named(db, repository)
      .await?
      .and_then(|repository| repository.pin)
      .map(|pin| pin.model),
  )
}

/// The harness session a supervisor's row keeps across rooms, where it keeps one.
pub async fn agent_session(
  db: &sea_orm::DatabaseConnection,
  agent: &str,
) -> Result<Option<String>, Refusal> {
  Ok(
    one(db, r#"SELECT "session_id" FROM "supervisors" WHERE "name" = $1"#, vec![agent.into()])
      .await?
      .and_then(|row| row.try_get::<Option<String>>("", "session_id").ok().flatten())
      .filter(|session| !session.is_empty()),
  )
}

/// The provider a supervisor's last answer went out on, where it answered: the road of the session its row
/// keeps, so a stand-in's session is resumed on the same road and never handed to another.
pub async fn agent_last_mode(
  db: &sea_orm::DatabaseConnection,
  agent: &str,
) -> Result<Option<String>, Refusal> {
  Ok(
    one(db, r#"SELECT "last_mode" FROM "supervisors" WHERE "name" = $1"#, vec![agent.into()])
      .await?
      .and_then(|row| row.try_get::<Option<String>>("", "last_mode").ok().flatten())
      .filter(|mode| !mode.is_empty()),
  )
}

/// The harness session a supervisor keeps across rooms written on its row, with the room its last turn
/// answered; none lets it go.
pub async fn keep_agent_session(
  db: &sea_orm::DatabaseConnection,
  agent: &str,
  kept: Option<(&str, i64)>,
) -> Result<(), Refusal> {
  let let_go = kept.is_none();
  let (session, chat_id) =
    kept.map_or((None, None), |(session, chat_id)| (Some(session.to_owned()), Some(chat_id)));
  let transaction = db.begin().await?;

  written(
    &transaction,
    r#"UPDATE "supervisors" SET "session_id" = $2, "chat_id" = COALESCE($3, "chat_id") WHERE "name" = $1"#,
    vec![agent.into(), session.into(), chat_id.into()],
  )
  .await
  .map(drop)
  .map_err(Refusal::from)?;

  // A session let go takes nothing it had loaded into the one that replaces it.
  if let_go {
    sessions::forget_loads(&transaction, agent).await?;
  }

  transaction.commit().await?;

  Ok(())
}

/// The session contract over herdr, on the places the server stands.
pub struct HerdrSessions {
  places: Places,
  // The turn each session carries while it goes, with the run it goes out as, which the interrupt stops:
  // one a session, a second turn refused while it goes.
  going: Mutex<HashMap<SessionId, Vec<Running>>>,
  // The sessions whose turn waits on the receiver's handoff turn. The session is lent to that turn, which
  // would otherwise be refused as a second turn of it (docs/sessions.md, 「A restart hands the session
  // over」).
  handing: Mutex<HashSet<SessionId>>,
  // The turns a handoff turn displaced, put back when it ends.
  parked: Mutex<HashMap<SessionId, Vec<Running>>>,
  // What each session's turn going now shows of itself to a reading of its state: what it went out on,
  // once its line is settled.
  showing: Mutex<HashMap<SessionId, WentOut>>,
  // The thrown-away sessions started and not ended, each with its agent and its scope, and the number the
  // next one is given.
  apart: Mutex<BTreeMap<SessionId, (String, Apart)>>,
  // The conversations the callers opened and have not ended, each with its agent and what it stands on,
  // and the number the next one is given.
  conversations: Mutex<BTreeMap<SessionId, (String, HeldConversation)>>,
  numbered: AtomicU64,
  // The id the next turn begun is given, past every id given before.
  turns: AtomicI64,
  // The questions the sessions' turns stand on, put to their callers.
  questions: Questions,
  // The sessions whose turn going now hears no words pushed into it.
  deaf: Mutex<HashSet<SessionId>>,
  // What each session's turns wrote of their answers, shared with the pickup of the turns a process before
  // this one left in flight.
  outputs: Arc<Outputs>,
  // The context each room's and agent's session was left with by its last turn that read one.
  left: Mutex<HashMap<SessionId, Context>>,
  // The runs cut because the slot their session was bound to was given back, each with the reason its end
  // carries.
  cut: Mutex<HashMap<String, String>>,
  // The runs this backend put a stop to, whose cancellation it takes back once their end is handed back
  // (docs/agent-control.md, 「What a cancellation reaches, and how long it stands」).
  interrupted: Mutex<HashSet<String>>,
  // The agents whose one session across rooms is being replaced now, outside any turn, and the word that
  // wakes a turn waiting for one of them (docs/sessions.md, 「Session replacement」).
  replacing: Mutex<HashSet<String>>,
  replaced: tokio::sync::Notify,
  // The room sessions whose owed handover is being made now, by the agent and the room, so two passes never
  // make one twice. What is owed is written down (docs/sessions.md, 「Session replacement」).
  settling_rooms: Mutex<HashSet<(String, i64)>>,
  // The least time between two asks for the same session's handoff, in milliseconds: the policy's, unless a
  // caller paces the asks otherwise (docs/sessions.md, 「A restart hands the session over」).
  handoff_pace: AtomicU64,
  // The key of the command each role session's turn going now went out under, read into its end.
  prefixes: Mutex<HashMap<SessionId, String>>,
  // This backend as the server shares it, for the work a turn's end leaves to run apart from the answer.
  me: std::sync::OnceLock<std::sync::Weak<HerdrSessions>>,
}

/// One turn a session carries while it goes: its id, and the run it goes out as.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Running {
  turn: i64,
  run: String,
}

/// A conversation the caller opened: what its turns go out on, and the session its last answered turn on
/// a road that keeps one left for the next turn to resume.
#[derive(Clone, Debug)]
struct HeldConversation {
  conversation: Conversation,
  harness: Option<String>,
}

fn fault(kind: FaultKind, message: impl Into<String>) -> Fault {
  Fault::new(kind, message)
}

fn store_fault(refusal: Refusal) -> Fault {
  fault(FaultKind::Unreachable, refusal.detail)
}

/// A session's id: its agent and its scope. A thrown-away session has none to name here, since its id
/// carries the number the backend gives it as it starts (`apart_id`).
pub fn id_of(agent: &str, scope: &Scope) -> Option<SessionId> {
  let id = match scope {
    Scope::Agent => agent.to_owned(),
    Scope::Room(room) => format!("{agent}@{room}"),
    Scope::Worker => format!("{agent}@{WORKER}"),
    Scope::Apart(_) | Scope::Conversation(_) => return None,
  };

  Some(SessionId(id))
}

// A session the list names, where its scope gives it an id.
fn listed(agent: String, scope: Scope) -> Option<Session> {
  id_of(&agent, &scope).map(|id| Session { id, agent, scope })
}

/// The agent and scope a session's id names.
pub fn scope_of(id: &SessionId) -> (String, Scope) {
  match id.0.split_once('@') {
    None => (id.0.clone(), Scope::Agent),
    Some((agent, WORKER)) => (agent.to_owned(), Scope::Worker),
    Some((agent, room)) => (agent.to_owned(), Scope::Room(room.to_owned())),
  }
}

/// The context a worker's session record kept of its last answered turn: the tokens it carried, against
/// the window they filled at the share they filled it; none where its record keeps neither, or its share
/// reads as nothing.
pub fn carried_of(tokens: Option<i64>, ratio: Option<f64>) -> Option<Context> {
  let (tokens, ratio) = (tokens?, ratio?);
  let carried = u64::try_from(tokens).ok()?;

  (ratio > 0.0).then(|| Context {
    carried,
    window: (carried as f64 / ratio).round() as u64,
  })
}

// What a session's job turn shows of itself to a reading of the session while it goes.
struct Showing<'s> {
  sessions: &'s HerdrSessions,
  session: &'s SessionId,
  turn: i64,
}

impl Shows for Showing<'_> {
  fn went_out(&self, went_out: &WentOut) {
    self.sessions.showing().insert(self.session.clone(), went_out.clone());
    self.sessions.outputs.attempted(self.session, self.turn);
  }

  fn wrote(&self, texts: &[String]) {
    self.sessions.outputs.wrote(self.session, self.turn, texts);
  }
}

/// Whether words pushed into a session reach the turn it carries: a job's turn says so in its work parts,
/// and every other turn hears them.
pub fn hears_words(work: Option<&WorkParts>) -> bool {
  work.is_none_or(|work| work.hears_words)
}

/// The id a thrown-away session is started under: its agent and the number the backend gave it.
pub fn apart_id(agent: &str, number: u64) -> SessionId {
  SessionId(format!("{agent}@{APART}:{number}"))
}

/// Whether a session's id names a thrown-away session.
pub fn is_apart(id: &SessionId) -> bool {
  numbered_scope(id, APART)
}

/// The id a conversation the caller opened is started under: its agent and the number the backend gave
/// it.
pub fn conversation_id(agent: &str, number: u64) -> SessionId {
  SessionId(format!("{agent}@{CONVERSATION}:{number}"))
}

/// Whether a session's id names a conversation the caller opened.
pub fn is_conversation(id: &SessionId) -> bool {
  numbered_scope(id, CONVERSATION)
}

// The number a thrown-away session's or a conversation's id was given.
fn session_number(id: &str) -> Option<i64> {
  let (_, scope) = id.split_once('@')?;
  let number = scope
    .strip_prefix(APART)
    .or_else(|| scope.strip_prefix(CONVERSATION))?
    .strip_prefix(':')?;

  number.parse().ok()
}

// Whether a session's id names a numbered session of the scope word given.
fn numbered_scope(id: &SessionId, word: &str) -> bool {
  id.0
    .split_once('@')
    .and_then(|(_, scope)| scope.strip_prefix(word))
    .is_some_and(|rest| rest.strip_prefix(':').is_some_and(|number| number.parse::<u64>().is_ok()))
}

// Whether a role's session's turns go into a place of its own: every road but Claude Code's own, whose runs
// each stand in a pane of their own (docs/api.md, 「sessions」).
fn in_places(apart: &Apart) -> bool {
  apart.provider != worker_session::PROVIDER
}

// A thrown-away session asked of what is no thrown-away session's to answer.
fn apart_refused(agent: &str) -> Fault {
  fault(FaultKind::Refused, format!("{agent}: a thrown-away session keeps no agent's row"))
}

// A room's id as the store keeps it.
fn room_number(room: &str) -> Result<i64, Fault> {
  room
    .parse()
    .map_err(|_| fault(FaultKind::Refused, format!("{room} is not a room's id")))
}

// What a supervisor's row holds that its turn reads: who it is, its order, its prompts and its threshold.
struct Pinned {
  speaker: Speaker,
  supervisor: crate::supervisors::Supervisor,
  prompts: Option<String>,
}

async fn pinned(ground: &Ground, agent: &str) -> Result<Option<Pinned>, Fault> {
  let held = crate::supervisors::named(&ground.db, agent)
    .await
    .map_err(|why| store_fault(why.into()))?;
  let Some(supervisor) = held else { return Ok(None) };

  Ok(Some(Pinned {
    speaker: Speaker {
      name: agent.to_owned(),
      label: supervisor.label.clone(),
      pinned_model: None,
    },
    prompts: supervisor.prompts.clone(),
    supervisor,
  }))
}

// A supervisor's turn carried on the supply its road takes (docs/supply.md): a person's supervisor asks its
// source first and fails with the one sentence where none is left; a reserved source is metered while the turn
// goes, the turn cancelled once it ran out, and settled as it ends. The deployment's own supervisors go out as
// they always went, their source kept on the ledger's run.
async fn supplied_turn<F>(
  ground: &Ground,
  agent: &str,
  room: Option<i64>,
  run: &str,
  ledger: Option<i64>,
  turn: F,
) -> Result<Came, Fault>
where
  F: std::future::Future<Output = Result<Came, Fault>> + Unpin,
{
  use crate::supply::meter::{self, Reading};

  let supervisor = crate::supervisors::named(&ground.db, agent).await.ok().flatten();
  let Some((supervisor, road)) =
    supervisor.and_then(|supervisor| supervisor.road.clone().map(|road| (supervisor, road)))
  else {
    return turn.await;
  };
  let asked = crate::supply::Asked {
    kind: crate::supply::Kind::Turn,
    id: ledger.unwrap_or_else(now_ms),
    agent,
    account: supervisor.owner_account,
    provider: &road.provider,
    model: &road.model,
    job: None,
    supervisor: Some(agent),
    room,
  };
  let chosen = match crate::supply::choose(ground, &asked).await {
    Ok(crate::supply::Choice::Go(chosen)) => chosen,
    Ok(crate::supply::Choice::Closed(closed)) => {
      let told = crate::supply::told::Told {
        supervisor: Some(agent.to_owned()),
        room,
      };

      crate::supply::told::closed(
        ground,
        run,
        supervisor.owner_account,
        None,
        &road.provider,
        &road.model,
        &told,
        &closed,
      )
      .await;
      return Ok(Came::Failed(closed.sentence, None));
    }
    Err(refused) => return Ok(Came::Failed(refused.detail, None)),
  };

  let told = crate::supply::told::Told {
    supervisor: Some(agent.to_owned()),
    room,
  };

  crate::supply::settle_choice(ground, agent, &chosen, None, &told).await;

  if let Some(ledger) = ledger
    && let Err(why) = run_ledger::mark_supply(&ground.db, ledger, chosen.source.word()).await
  {
    eprintln!("{run}: its supply source was not kept: {why}");
  }

  let since = now_ms();
  let reading = if road.provider == "claude-code" {
    Reading::StatusLine {
      place: ground.roots.place(agent),
      since,
    }
  } else {
    Reading::Gateway {
      run: agent.to_owned(),
      since,
    }
  };
  let Some(shared) = meter::metered(&chosen, reading, agent, None, room) else {
    return turn.await;
  };
  let watch = meter::watch(ground, shared.clone());

  let mut turn = turn;
  let watch = Box::pin(watch);

  tokio::pin!(watch);

  let came = tokio::select! {
    came = &mut turn => came,
    closed = &mut watch => {
      let _ = crate::queue::stops::Stops::of(ground, None).cancel(run).await;
      let came = turn.await;

      match Some(closed) {
        Some(closed) => {
          let told = crate::supply::told::Told {
            supervisor: Some(agent.to_owned()),
            room,
          };

          crate::supply::told::closed(
            ground,
            run,
            supervisor.owner_account,
            None,
            &road.provider,
            &road.model,
            &told,
            &closed,
          )
          .await;
          came.map(|_| Came::Failed(closed.sentence, None))
        }
        None => came,
      }
    }
  };

  if let Err(why) = meter::finish(ground, &shared).await {
    eprintln!("{run}: its supply reservation was not settled: {why}");
  }

  came
}

// The provider and model a supervisor's turn goes out on: its one road, where the road's provider has quota
// left and can answer now (docs/supervisors.md, 「Where a supervisor's turn runs」). A road that cannot
// take the turn fails it with the sentence that says why: nothing stands in and nothing waits.
async fn choice_of(
  ground: &Ground,
  pinned: &Pinned,
) -> Result<(&'static dyn harness_providers::Provider, String), String> {
  let supervisor = &pinned.supervisor;

  if supervisor.road.is_none() {
    return Err(format!("{} names no road to go out on", supervisor.name));
  }

  let order = supervisor.order();
  let mut ask = crate::road::Ask::of(crate::road::Row::Supervisor(&supervisor.name), "chat", &order);

  // A person's supervisor walks the logins its supply took: its own, or the lendable ones.
  ask.user =
    crate::supply::carrying(crate::supply::chosen_for(&ground.roots.data, &supervisor.name).as_ref());
  let resolution = crate::road::resolve(ground, &ask).await?;
  let Some(road) = resolution.road() else {
    return Err(resolution.sentence());
  };
  let provider = harness_providers::provider(&road.provider)
    .ok_or_else(|| format!("{} is no provider of this build", road.provider))?;

  Ok((provider, road.model.clone()))
}

// Whether a room's harness session stands on another road than the one its supervisor answers on now: another
// provider or model, or a place its registration no longer holds.
async fn harness_moved(
  ground: &Ground,
  harness: &Harness,
  provider: &dyn harness_providers::Provider,
  model: &str,
) -> bool {
  harness.road != harness_road(provider.name(), model)
    || match (&harness.placement, provider.not_registered()) {
      (Some(node), Some(_)) => registration_named(ground, model)
        .await
        .map(|held| !held.holds_node(node))
        .unwrap_or(false),
      _ => false,
    }
}

// The system prompt a supervisor's settings compose, out of the documents of the tree the process runs, for
// the provider and model it answers on, with the layers its account's standing and the delegation chain give it
// (docs/prompt-layers.md).
async fn system_of(
  ground: &Ground,
  pinned: &Pinned,
  settings: &Settings,
  provider: &dyn harness_providers::Provider,
  model: &str,
) -> Result<String, String> {
  let chosen = one(
    &ground.db,
    "SELECT p.text FROM agent_base_prompts c JOIN base_prompts p ON p.name = c.prompt WHERE c.agent = $1",
    vec![pinned.speaker.name.clone().into()],
  )
  .await
  .map_err(|why| why.detail)?
  .and_then(|row| row.try_get::<String>("", "text").ok());
  let base = chosen.map(|text| harness_machine::supervisor_prompt::Base {
    text,
    model: model.to_owned(),
  });
  let roots = harness_machine::supervisor_prompt::Roots {
    assets: ground.code_root(),
    data: ground.roots.data.clone(),
  };
  let call_names = pinned.supervisor.call_names();
  let supervisor = harness_machine::supervisor_prompt::Supervisor {
    name: &pinned.speaker.name,
    label: pinned.supervisor.label.as_deref(),
    call_names: &call_names,
    prompts: pinned.prompts.as_deref(),
  };

  let layers =
    crate::prompt_layers::of(&ground.db, &crate::apps::service(ground), pinned.supervisor.owner_account)
      .await?;

  harness_machine::supervisor_prompt::system(
    &roots,
    supervisor,
    &settings.composition(),
    base.as_ref(),
    crate::lines::pieces::takes_harness(provider.name()),
    &layers,
  )
  .map_err(|why| why.to_string())
  .map(|system| (system, pinned.supervisor.owner_account))
  .map(|(system, account)| match account {
    Some(account) => format!("{}\n{system}", crate::supervisors::account_mark(account)),
    None => system,
  })
}

// The generation settings a supervisor's row holds, in the order they were saved.
fn generation_of(settings: &Settings) -> IndexMap<String, Value> {
  settings
    .generation()
    .iter()
    .filter(|(_, value)| value.is_number() || value.is_boolean())
    .map(|(name, value)| (name.clone(), value.clone()))
    .collect()
}

// What a turn on a session came to, before it is published: a failure carries why, and the engine's error
// where its engine ended the turn with one.
enum Came {
  /// The answer, with what the turn spent where its road reported it, the context it left where it read and
  /// each turn's answer it was built from where its road read them.
  Answered(String, Option<run_ledger::Spent>, Option<Context>, Vec<TurnAnswer>),
  Stopped,
  Failed(String, Option<EngineError>),
  /// A job's turn, whose end is read whole where it is carried.
  Ended(Box<TurnEnd>),
  /// A turn put back to its caller before anything went in: the session it names was opened under another
  /// command than the turn's.
  Back,
}

impl Came {
  // The turn's end under the run it went out as.
  fn end(self, run: &str) -> TurnEnd {
    match self {
      Self::Answered(text, _, _, answers) => TurnEnd::answered_of(run, text, answers),
      Self::Stopped => TurnEnd::of(run, Outcome::Stopped),
      Self::Failed(why, engine) => TurnEnd::failed(run, why, engine),
      Self::Ended(end) => *end,
      Self::Back => TurnEnd::of(run, Outcome::Reprepared),
    }
  }

  // What the turn spent, read off the answer where it carried it and off a job's end where that did.
  fn spent(&self) -> Option<run_ledger::Spent> {
    match self {
      Self::Answered(_, spent, _, _) => *spent,
      Self::Ended(end) => turn_spent(end),
      _ => None,
    }
  }

  // The context the turn left, where its answer read it.
  fn context(&self) -> Option<Context> {
    match self {
      Self::Answered(_, _, context, _) => *context,
      Self::Ended(end) => end
        .reply
        .as_ref()
        .and_then(|reply| carried_of(reply.context_tokens, reply.context_ratio)),
      _ => None,
    }
  }
}

// The context a supervisor run's answer left.
fn left(reply: &Reply) -> Option<Context> {
  carried_of(reply.context_tokens, reply.context_ratio)
}

/// What a turn's models spent, each figure summed over them; none where they reported nothing.
pub fn spent_of(spend: &[ModelSpend]) -> Option<run_ledger::Spent> {
  (!spend.is_empty()).then(|| {
    spend.iter().fold(run_ledger::Spent::default(), |sum, one| run_ledger::Spent {
      input_tokens: sum.input_tokens + one.input_tokens,
      output_tokens: sum.output_tokens + one.output_tokens,
      cache_read_tokens: sum.cache_read_tokens + one.cache_read_tokens,
      cache_write_tokens: sum.cache_write_tokens + one.cache_write_tokens,
      cost_micros: sum.cost_micros + crate::supply::reservations::micros(one.cost),
    })
  })
}

/// The word a turn's end goes by: an answer and a turn put back go by their outcome; any other by the
/// ending the door's last turn under its send came to, where one went through it and did not answer, else
/// by its outcome.
pub fn ending_of(end: &TurnEnd) -> &'static str {
  let outcome = match end.outcome {
    Outcome::Answered => "answered",
    Outcome::Blocked => "blocked",
    Outcome::Stopped => "stopped",
    Outcome::Failed => "failed",
    Outcome::Reprepared => "reprepared",
  };

  match end.outcome {
    Outcome::Answered | Outcome::Reprepared => outcome,
    _ => match crate::door::liveness::door_ending() {
      Some("answered") | None => outcome,
      Some(word) => word,
    },
  }
}

/// Why a run came to no answer, and the engine's error where its engine ended it with one.
pub fn failure_of(no: NoAnswer) -> (String, Option<EngineError>) {
  match no {
    NoAnswer::Errored { error, kind } => (error, Some(EngineError { kind })),
    NoAnswer::Cut(why) | NoAnswer::Stopped(why) | NoAnswer::Failed(why) | NoAnswer::Unsubmitted(why) => {
      (why, None)
    }
    NoAnswer::Overtaken => ("the supervisor's reset overtook the turn".to_owned(), None),
  }
}

// Whether an agent's name is a gate work key: a gate's conversation carries the caller's whole system prompt.
fn gate_work_key(agent: &str) -> bool {
  policy::work_keys::WORK_KEYS
    .profile(agent)
    .is_some_and(|profile| profile.kind == "gate")
}

impl HerdrSessions {
  pub fn new(places: Places) -> Self {
    let outputs = places.ground.outputs.clone();

    Self {
      places,
      going: Mutex::default(),
      handing: Mutex::default(),
      parked: Mutex::default(),
      showing: Mutex::default(),
      apart: Mutex::default(),
      conversations: Mutex::default(),
      numbered: AtomicU64::new(1),
      turns: AtomicI64::new(0),
      questions: Questions::default(),
      deaf: Mutex::default(),
      outputs,
      left: Mutex::default(),
      cut: Mutex::default(),
      interrupted: Mutex::default(),
      replacing: Mutex::default(),
      replaced: tokio::sync::Notify::new(),
      settling_rooms: Mutex::default(),
      handoff_pace: AtomicU64::new(HANDOFF.again_seconds * 1000),
      prefixes: Mutex::default(),
      me: std::sync::OnceLock::new(),
    }
  }

  /// The backend behind the shared handle the server holds it by, for what its turns leave to run apart.
  pub fn shared_as(self: Arc<Self>) -> Arc<Self> {
    let _ = self.me.set(Arc::downgrade(&self));
    self
  }

  /// The least time between two asks for the same session's handoff set to `every` in place of the policy's.
  pub fn pace_handoffs(&self, every: Duration) {
    self
      .handoff_pace
      .store(u64::try_from(every.as_millis()).unwrap_or(u64::MAX), Ordering::Relaxed);
  }

  // The shared handle, where the server holds one; a backend held by nothing else runs such work inline.
  fn shared(&self) -> Option<Arc<Self>> {
    self.me.get().and_then(std::sync::Weak::upgrade)
  }

  // --- An owed replacement of an agent's one session across rooms (docs/sessions.md, 「Session replacement」)

  // Held while a turn of `agent` must wait: a replacement of its session being made outside any turn.
  async fn wait_replaced(&self, agent: &str) {
    loop {
      let notified = self.replaced.notified();

      if !self.replacing.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).contains(agent) {
        return;
      }

      notified.await;
    }
  }

  /// The replacement an agent's one session owes made now, where it owes one, holds a session and carries no
  /// turn: the session is asked for its handoff, let go, and the next turn opens fresh. Answers whether it was
  /// made. One already being made is not made twice, and an agent in a turn keeps its session until the
  /// turn ends, when its end makes the replacement.
  pub async fn settle_owed(&self, agent: &str) -> bool {
    let ground = self.ground();
    let db = &ground.db;
    let Some(why) = crate::supervisor_turn::replacement::owed(db, agent).await.ok().flatten() else {
      return false;
    };
    let session = SessionId(agent.to_owned());

    if self.turn_run(&session).is_some() || self.handing().contains(&session) {
      return false;
    }

    if !self
      .replacing
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .insert(agent.to_owned())
    {
      return false;
    }

    let made = async {
      let held = agent_session(db, agent).await.map_err(store_fault)?;
      let kept_session = held.is_some();

      if let Some(held_session) = held {
        let mut log = RunLog::new(Target {
          agent: agent.to_owned(),
          run: format!("replacement-{}", now_ms()),
          chat_id: None,
          job_id: None,
        });

        let asking = Asking {
          reason: &why,
          room: None,
          cause: cause_of(&why),
          clean: false,
          by: None,
        };

        self.handed_off(&mut log, agent, false, true, asking).await;
        keep_agent_session(db, agent, None).await.map_err(store_fault)?;
        // The program stood on the session that just went, so it ends with it rather than waiting for the
        // next turn to stand it again: the pane stops showing the road it went out on, the session token it
        // carried goes back to the tool host, and the next turn stands the road the row names now
        // (docs/supervisors.md, 「The program on an ended session」).
        let_go(ground, agent, &[held_session]).await;
        // The line the run log carries names what made the session go.
        let note = if why == HANDOFF.reasons.moved {
          &SUPERVISOR_RUNNER.log.system_changed
        } else {
          &SUPERVISOR_RUNNER.log.road_changed
        };

        log.write(db, Line::note(note.clone())).await;

        if let Some(session) = id_of(agent, &Scope::Agent) {
          ground.events.publish(SystemPromptChanged {
            session: session.0,
            run: log.run().to_owned(),
          });
        }
      }

      crate::supervisor_turn::replacement::settle(db, agent).await.map_err(store_fault)?;
      println!("{agent}: its session was replaced outside any turn ({why})");

      Ok::<bool, Fault>(kept_session)
    }
    .await;

    self.replacing.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(agent);
    self.replaced.notify_waiters();

    match made {
      Ok(made) => made,
      Err(why) => {
        eprintln!("{agent}: the owed replacement of its session was not made: {}", why.message);

        false
      }
    }
  }

  /// The one session across rooms replaced because the owner moved the supervisor's road: the debt is
  /// written first, so a supervisor carrying a turn keeps its session and its end makes the replacement,
  /// and it is made at once where the agent is quiet.
  pub async fn replaced_road(&self, agent: &str) {
    let ground = self.ground();

    if let Err(why) =
      crate::supervisor_turn::replacement::owe(&ground.db, agent, &HANDOFF.reasons.road).await
    {
      eprintln!("{agent}: the replacement its moved road owes was not written down: {why}");
    }

    self.settle_owed(agent).await;
  }

  /// What a supervisor's sessions held replaced because its road moved — its provider, model, effort, thinking or
  /// answer ceiling, a registration pinned anew, or a registration that lost the address its program stood on —
  /// each through its handoff while the agent is quiet: the one session across rooms owes its replacement, and
  /// every room's own session owes its handover (docs/supervisors.md, 「Where a supervisor's turn runs」).
  pub async fn road_moved(&self, agent: &str) {
    let scope = crate::supervisors::named(&self.ground().db, agent)
      .await
      .ok()
      .flatten()
      .map(|held| held.session_scope);

    match scope.as_deref() {
      Some(scope) if scope == crate::conversational::Scope::Agent.word() => self.replaced_road(agent).await,
      Some(_) => self.replace_rooms(agent, Why::Road).await,
      None => {}
    }
  }

  /// Every room session of `agent` that holds a harness session owes its handover for `why`, each made at once
  /// in a room that carries no turn and otherwise as that room's turn ends (docs/supervisors.md, 「A session in
  /// each room」).
  pub async fn replace_rooms(&self, agent: &str, why: Why) {
    let held = match sessions::sessions_of(&self.ground().db, agent).await {
      Ok(held) => held,
      Err(why) => {
        return eprintln!("supervisor {agent}: the sessions its rooms hold were not read: {}", why.detail);
      }
    };

    for session in held.iter().filter(|session| session.harness.is_some()) {
      self.owe_room_handover(session, why).await;
    }

    self.settled_apart().await;
  }

  /// Every room session a waiting reset asks to clear, where it holds a harness session, owes its handover for
  /// the reset, each made as `replace_rooms` makes it (docs/session-resets.md, 「Who holds a session」).
  pub async fn replace_reset_rooms(&self) {
    let db = &self.ground().db;
    let waiting = match all(
      db,
      r#"SELECT DISTINCT "s"."agent", "s"."chat_id" FROM "session_resets" "r"
         JOIN "room_sessions" "s" ON "s"."agent" = "r"."agent" AND "s"."chat_id" = "r"."chat_id"
         WHERE "r"."done_at" IS NULL AND "s"."harness_session" IS NOT NULL"#,
      vec![],
    )
    .await
    {
      Ok(rows) => rows,
      Err(why) => {
        return eprintln!("agency: the room sessions a reset waits for were not read: {}", why.detail);
      }
    };

    for row in waiting {
      let (Ok(agent), Ok(chat_id)) =
        (row.try_get::<String>("", "agent"), row.try_get::<i64>("", "chat_id"))
      else {
        continue;
      };

      if let Ok(Some(session)) = sessions::session(db, &agent, chat_id).await {
        self.owe_room_handover(&session, reset_why(db, &agent, chat_id).await).await;
      }
    }

    self.settled_apart().await;
  }

  // The handovers the rooms owe made apart from the caller where the server shares this backend, since each is
  // a turn of its own; inline otherwise.
  async fn settled_apart(&self) {
    match self.shared() {
      Some(backend) => {
        tokio::spawn(async move { backend.settle_rooms().await });
      }
      None => self.settle_rooms().await,
    }
  }

  // The handover a room's session owes for `why` written down, composed for the road the session was opened on,
  // so its handoff turn goes out on that road and the prompt composed for it. One owed already on the same
  // session stands as it was written. Answers whether one is owed.
  async fn owe_room_handover(&self, session: &RoomSession, why: Why) -> bool {
    let ground = self.ground();
    let db = &ground.db;
    let (agent, chat_id) = (session.agent.as_str(), session.chat_id);
    let Some(harness) = &session.harness else { return false };
    let said = |what: String| {
      eprintln!("supervisor {agent} in chat {chat_id}: the handover owed was not written down: {what}");
      false
    };

    match crate::supervisor_turn::replacement::room_owed(db, agent, chat_id).await {
      Ok(Some(owed)) if owed.session == harness.id => return true,
      Ok(_) => {}
      Err(why) => return said(why.detail),
    }

    let Some((name, model)) = harness.road.split_once("::") else {
      return said(format!("its session's road {} names no provider", harness.road));
    };
    let Some(provider) = harness_providers::provider(name) else {
      return said(format!("its session stands on {name}, which this build does not hold"));
    };
    let pinned = match pinned(ground, agent).await {
      Ok(Some(pinned)) => pinned,
      Ok(None) => return said(format!("no agent is named {agent}")),
      Err(fault) => return said(fault.message),
    };
    let settings = match Settings::of(db, agent).await {
      Ok(settings) => settings,
      Err(refused) => return said(refused.error),
    };
    let system = match system_of(ground, &pinned, &settings, provider, model).await {
      Ok(system) => system,
      Err(why) => return said(why),
    };
    let due = Due {
      provider,
      model: model.to_owned(),
      system,
      generation: generation_of(&settings),
      session: harness.id.clone(),
      placement: harness.placement.clone(),
      ratio: 1.0,
      why,
    };
    let owed = crate::supervisor_turn::replacement::RoomOwed::of(&pinned.speaker, &due);

    match crate::supervisor_turn::replacement::owe_room(db, agent, chat_id, &owed).await {
      Ok(()) => true,
      Err(why) => said(why.detail),
    }
  }

  // Ahead of a room's turn: a handover its session still owes, or one a waiting reset or a road moved under the
  // session calls for, made before the turn opens anything, so the turn opens on its note (docs/supervisors.md,
  // 「A session in each room」). One under way is waited for.
  async fn room_replaced_ahead(
    &self,
    agent: &str,
    chat_id: i64,
    provider: &'static dyn harness_providers::Provider,
    model: &str,
  ) {
    let ground = self.ground();
    let db = &ground.db;
    let Ok(Some(held)) = sessions::session(db, agent, chat_id).await else { return };
    let Some(harness) = held.harness.clone() else { return };
    let owed = crate::supervisor_turn::replacement::room_owed(db, agent, chat_id)
      .await
      .ok()
      .flatten()
      .is_some_and(|owed| owed.session == harness.id);

    if !owed {
      let why = if sessions::reset_waiting(db, agent, chat_id).await.unwrap_or(false) {
        reset_why(db, agent, chat_id).await
      } else if harness_moved(ground, &harness, provider, model).await {
        Why::Road
      } else {
        return;
      };

      if !self.owe_room_handover(&held, why).await {
        return;
      }
    }

    self.wait_room_settled(agent, chat_id).await;
    self.settle_room(agent, chat_id).await;
  }

  // Held while a room's owed handover is being made by another pass.
  async fn wait_room_settled(&self, agent: &str, chat_id: i64) {
    let key = (agent.to_owned(), chat_id);

    loop {
      let notified = self.replaced.notified();

      if !self
        .settling_rooms
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains(&key)
      {
        return;
      }

      notified.await;
    }
  }

  /// A pass over every conversational agent: one whose session owes a replacement, or holds a prompt that has
  /// moved under it, is replaced while it is quiet, and every room whose own session owes a handover is handed
  /// over while it carries no turn. Run at the server's start, when documents are reloaded, and on the
  /// keepalive clock, so a moved document is settled before anyone asks.
  pub async fn settle_pass(&self) {
    self.settle_rooms().await;

    let ground = self.ground();
    let db = &ground.db;
    let agents = match crate::conversational::agents_where(db, |settings| {
      matches!(settings.scope, crate::conversational::Scope::Agent)
    })
    .await
    {
      Ok(agents) => agents,
      Err(why) => {
        return eprintln!(
          "agency: the agents whose sessions may owe a replacement were not read: {}",
          why.detail
        );
      }
    };

    for agent in agents {
      let owed = crate::supervisor_turn::replacement::owed(db, &agent).await.ok().flatten().is_some();

      if !owed {
        let held = match agent_session(db, &agent).await {
          Ok(held) => held,
          Err(_) => continue,
        };

        if held.is_none() || self.turn_run(&SessionId(agent.clone())).is_some() {
          continue;
        }

        let trees = Trees::of(ground);
        let composed = match crate::lines::chat::composed_system(ground, &trees, &agent, None, None).await {
          Ok(composed) => composed,
          Err(why) => {
            eprintln!(
              "{agent}: its system prompt was not composed to tell whether it moved: {}",
              why.detail
            );
            continue;
          }
        };
        let recorded = sessions::agent_system_hash(db, &agent).await.ok().flatten();

        if !sessions::prompt_moved(true, recorded.as_deref(), &composed.hash) {
          continue;
        }

        let _ = crate::supervisor_turn::replacement::owe(db, &agent, &HANDOFF.reasons.moved).await;
      }

      self.settle_owed(&agent).await;
    }
  }

  /// Every room whose own session owes a handover made while the room carries no turn, one picked up across
  /// agency's restart included: at the server's start and on the keepalive clock, so a handover a restart cut
  /// off between the turn that found it and its making is made all the same.
  async fn settle_rooms(&self) {
    let db = self.ground().db.clone();
    let owing = match crate::supervisor_turn::replacement::rooms_owing(&db).await {
      Ok(owing) => owing,
      Err(why) => {
        return eprintln!("agency: the rooms whose sessions owe a handover were not read: {}", why.detail);
      }
    };
    let kept = crate::turn_pickup::in_flight(&db).await.unwrap_or_default();

    for (agent, chat_id) in owing {
      let busy = id_of(&agent, &Scope::Room(chat_id.to_string()))
        .is_some_and(|session| self.turn_run(&session).is_some())
        || kept.iter().any(|turn| turn.agent == agent && turn.chat_id == Some(chat_id));

      if !busy {
        self.settle_room(&agent, chat_id).await;
      }
    }
  }

  /// A room session's owed handover made, read from where it was written down: the session writes its note in
  /// one more turn and the next generation opens on it; one that writes no note opens the next generation all the
  /// same, so it reads what moved, and is marked as having lost its handoff. The program stood on the session
  /// that went is taken down, and a reset waiting for the room is settled by it. One owed on a session the room
  /// no longer holds is let go, that session replaced already. The debt is let go once the handover is made, so a
  /// restart under it leaves it owed.
  async fn settle_room(&self, agent: &str, chat_id: i64) {
    let ground = self.places.ground.clone();
    let db = ground.db.clone();
    let owed = match crate::supervisor_turn::replacement::room_owed(&db, agent, chat_id).await {
      Ok(Some(owed)) => owed,
      Ok(None) => return,
      Err(why) => {
        return eprintln!(
          "supervisor {agent} in chat {chat_id}: the handover owed was not read: {}",
          why.detail
        );
      }
    };
    let key = (agent.to_owned(), chat_id);

    if !self
      .settling_rooms
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .insert(key.clone())
    {
      return;
    }

    self.room_handed_over(agent, chat_id, owed).await;
    self.settling_rooms.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&key);
    self.replaced.notify_waiters();
  }

  // The handover a room's session owed made and let go.
  async fn room_handed_over(
    &self,
    agent: &str,
    chat_id: i64,
    owed: crate::supervisor_turn::replacement::RoomOwed,
  ) {
    let ground = self.places.ground.clone();
    let hold = self.places.kept.reset.clone();
    let db = ground.db.clone();
    let before = sessions::session(&db, agent, chat_id)
      .await
      .ok()
      .flatten()
      .and_then(|session| session.harness.map(|harness| harness.id));
    let settled = async {
      if let Err(why) = crate::supervisor_turn::replacement::settle_room(&db, agent, chat_id).await {
        eprintln!("supervisor {agent} in chat {chat_id}: the handover owed was not let go: {}", why.detail);
      }
    };

    if before.as_deref() != Some(owed.session.as_str()) {
      println!(
        "supervisor {agent} in chat {chat_id}: the handover owed is let go: its session was replaced already"
      );
      return settled.await;
    }

    let Some((speaker, due)) = owed.due() else {
      eprintln!(
        "supervisor {agent} in chat {chat_id}: the handover owed names the provider {}, which this build does not hold",
        owed.provider
      );
      return settled.await;
    };

    let why = due.why;

    hand_over(&ground, &hold, &speaker, chat_id, due).await;

    let after = sessions::session(&db, agent, chat_id).await.ok().flatten();
    let unwritten = after.is_some_and(|session| session.harness.map(|harness| harness.id) == before);

    if unwritten {
      if let Err(why) = sessions::open_generation(&db, agent, chat_id).await {
        eprintln!(
          "supervisor {agent} in chat {chat_id}: the next generation did not open after the handover: {}",
          why.detail
        );
      }

      // The generation opened without the note it was owed: the receiver opens its first turn on what it
      // keeps of the room's answers (docs/sessions.md, 「A restart hands the session over」), unless the reset
      // it carries out is clean, whose generation opens on nothing.
      if why != Why::CleanReset
        && let Err(lost) =
          crate::supervisor_turn::replacement::lose(&db, &sessions::room_session_id(agent, chat_id)).await
      {
        eprintln!(
          "supervisor {agent} in chat {chat_id}: the handoff it lost was not written down: {}",
          lost.detail
        );
      }

      crate::supervisor_turn::handoff::told(&ground, agent, chat_id, why).await;
    }

    // The program stood on the session that went, so it ends with it (docs/supervisors.md, 「The program on an
    // ended session」), and a reset the room waited for is what the handover carried out.
    let_go(&ground, agent, std::slice::from_ref(&owed.session)).await;

    if sessions::reset_waiting(&db, agent, chat_id).await.unwrap_or(false)
      && let Err(why) = sessions::settle_reset(&db, agent, chat_id).await
    {
      eprintln!(
        "supervisor {agent} in chat {chat_id}: the reset its handover carried out was not settled: {}",
        why.detail
      );
    }

    settled.await;
  }

  // The context an agent's resident program stands at where it carries its own turns, as its record says of its
  // last answer (docs/sessions.md, 「Session compaction」); none on any other provider.
  fn resident_context(&self, agent: &str, provider: &str) -> Option<Context> {
    if !crate::chat_stand_in::inbox_declared(provider) {
      return None;
    }

    let place = crate::lines::place::Place::of(&self.ground().roots.data, agent);
    let (carried, window) = crate::door::inbox::latest_context(&place.path)?;

    Some(Context { carried, window })
  }

  // The context a session's last turn left it with, where one read it.
  fn left_of(&self, session: &SessionId) -> Option<Context> {
    self.left.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(session).copied()
  }

  // What a session's last turn left, no longer what it carries once it has folded or ended.
  fn forget_left(&self, session: &SessionId) {
    self.left.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(session);
  }

  fn deaf(&self) -> std::sync::MutexGuard<'_, HashSet<SessionId>> {
    self.deaf.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  fn showing(&self) -> std::sync::MutexGuard<'_, HashMap<SessionId, WentOut>> {
    self.showing.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  /// The numbers this process gives its thrown-away sessions and conversations continued past every number a
  /// process before it gave that the store still names, so one session id never names two sessions across
  /// agency's restarts and words said to a job's session reach that job (docs/sessions.md, 「A restart hands the
  /// session over」). Run as the server starts, before any session is started.
  pub async fn number_past_kept(&self) {
    let db = &self.ground().db;
    let named = one(
      db,
      &format!(
        r#"SELECT MAX((regexp_match("id", '@(?:{APART}|{CONVERSATION}):([0-9]+)$'))[1]::bigint) AS "past"
           FROM (SELECT "session_id" AS "id" FROM "jobs" UNION ALL SELECT "session_id" FROM "job_sessions"
                 UNION ALL SELECT "session" FROM "role_slot_requests") AS "ids""#
      ),
      vec![],
    )
    .await;
    let mut past = match named {
      Ok(row) => row.and_then(|row| row.try_get::<Option<i64>>("", "past").ok().flatten()).unwrap_or(0),
      Err(why) => {
        eprintln!("agency: the session numbers given before were not read: {}", why.detail);
        0
      }
    };

    for turn in crate::turn_pickup::in_flight(db).await.unwrap_or_default() {
      if let Some(number) = session_number(&turn.session) {
        past = past.max(number);
      }
    }

    let next = u64::try_from(past).unwrap_or(0).saturating_add(1);

    self.numbered.fetch_max(next, Ordering::Relaxed);
  }

  fn apart(&self) -> std::sync::MutexGuard<'_, BTreeMap<SessionId, (String, Apart)>> {
    self.apart.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  // The agent and scope a session's id names: a numbered session's as it was started, which one that
  // has ended or was never started names none of.
  fn scope_held(&self, id: &SessionId) -> Result<(String, Scope), Fault> {
    if is_apart(id) {
      return match self.apart().get(id) {
        Some((agent, apart)) => Ok((agent.clone(), Scope::Apart(apart.clone()))),
        None => Err(fault(FaultKind::Unknown, format!("{id} is no thrown-away session standing now"))),
      };
    }

    if is_conversation(id) {
      return match self.conversations().get(id) {
        Some((agent, held)) => Ok((agent.clone(), Scope::Conversation(held.conversation.clone()))),
        None => Err(fault(FaultKind::Unknown, format!("{id} is no conversation standing now"))),
      };
    }

    Ok(scope_of(id))
  }

  fn conversations(&self) -> std::sync::MutexGuard<'_, BTreeMap<SessionId, (String, HeldConversation)>> {
    self.conversations.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  // The agent and scope the backend holds a session under, for a route that reads or moves a session
  // whose id a numbered scope names.
  pub async fn held_scope(&self, session: &SessionId) -> Result<(String, Scope), Fault> {
    self.scope_held(session)
  }

  /// The scope a thrown-away session the pickup carries stands on, where this process adopted its turn: its
  /// state is read by id until that turn's end is handed back (docs/sessions.md, 「A restart hands the session
  /// over」).
  pub fn carried_scope(&self, session: &SessionId) -> Option<(String, Apart)> {
    carried_scope_of(self.ground(), session)
  }

  fn ground(&self) -> &Ground {
    &self.places.ground
  }

  fn going(&self) -> std::sync::MutexGuard<'_, HashMap<SessionId, Vec<Running>>> {
    self.going.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  // The sessions lent to the receiver's handoff turn while this process waits on it.
  fn handing(&self) -> std::sync::MutexGuard<'_, HashSet<SessionId>> {
    self.handing.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  // A session lent to the receiver's handoff no longer: the turns its handoff turns displaced take it back.
  fn lent_back(&self, session: &SessionId) {
    self.handing().remove(session);

    let mut going = self.going();

    if let Some(parked) = self.parked().remove(session) {
      going.entry(session.clone()).or_default().extend(parked);
    }

    if going.get(session).is_some_and(|runs| runs.is_empty()) {
      going.remove(session);
    }
  }

  // The turns a handoff turn displaced, put back once the session is lent no longer.
  fn parked(&self) -> std::sync::MutexGuard<'_, HashMap<SessionId, Vec<Running>>> {
    self.parked.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  // The id the next turn begun is given: the time it began, past every id given before it.
  fn next_turn(&self) -> i64 {
    let at = now_ms();
    let last = self
      .turns
      .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |last| Some(at.max(last + 1)))
      .unwrap_or(0);

    at.max(last + 1)
  }

  // Whether a session carries a turn now.
  fn in_turn(&self, session: &SessionId) -> bool {
    self.going().get(session).is_some_and(|runs| !runs.is_empty())
  }

  /// The run the turn a session carries now goes out as, none while it carries no turn.
  pub fn turn_run(&self, session: &SessionId) -> Option<String> {
    self
      .going()
      .get(session)
      .and_then(|runs| runs.first())
      .map(|running| running.run.clone())
  }

  /// The turn a session carries now: its id and the run it goes out as, none while it carries no turn.
  pub fn turn_going(&self, session: &SessionId) -> Option<(i64, String)> {
    self
      .going()
      .get(session)
      .and_then(|runs| runs.first())
      .map(|running| (running.turn, running.run.clone()))
  }

  /// A role session ended under whatever turn it carries, as giving back the slot it is bound to ends it
  /// (docs/role-slots.md, 「The line」): each turn running in it cut, ending `stopped` with the reason given,
  /// waited for until it has ended — on its end, never on a span — and the session then ended.
  pub async fn end_under_turns(&self, session: &SessionId, reason: &str) -> Result<(), Fault> {
    let mut ended = self.ground().events.subscribe::<TurnEnded>();

    loop {
      let runs = self.going().get(session).cloned().unwrap_or_default();

      if runs.is_empty() {
        break;
      }

      for running in &runs {
        self
          .cut
          .lock()
          .unwrap_or_else(|poisoned| poisoned.into_inner())
          .insert(running.run.clone(), reason.to_owned());

        match self.interrupt(session, running.turn).await {
          // A turn that ended between the read and the cut is no longer there to cut.
          Ok(()) => {}
          Err(fault) if fault.kind == FaultKind::UnknownTurn => {}
          Err(fault) => return Err(fault),
        }
      }

      while self.in_turn(session) {
        if ended.next().await.is_none() {
          return Err(fault(FaultKind::Busy, format!("{session} is in a turn whose end cannot be heard")));
        }
      }
    }

    self.end(session).await
  }

  /// Whether a worker's own session carries a turn of this process's now, whatever job does or does not
  /// name it.
  pub fn worker_in_turn(&self, worker: &str) -> bool {
    worker_session_id(worker).is_ok_and(|session| self.in_turn(&session))
  }

  // A run let go of its session's turns as it ends, and the words pushed into it with it.
  fn ended(&self, session: &SessionId, run: &str) {
    aside::forget(run);
    self.showing().remove(session);
    self.questions.clear(session);
    self.deaf().remove(session);

    let lending = self.handing().contains(session);
    let mut going = self.going();

    if let Some(runs) = going.get_mut(session)
      && let Some(at) = runs.iter().position(|going| going.run == run)
    {
      let ended = runs.remove(at);

      self.outputs.ended(session, ended.turn);
    }

    // A turn that lent its session ended while a turn of the receiver's handoff still goes: it leaves the parked
    // turns too, so the handoff turn's end hands back nothing of it to hold the session busy.
    if let Some(parked) = self.parked().get_mut(session)
      && let Some(at) = parked.iter().position(|going| going.run == run)
    {
      let ended = parked.remove(at);

      self.outputs.ended(session, ended.turn);
    }

    // A turn of the receiver's handoff ended while the session is still lent: the turn that lent it waits on,
    // since the receiver may send another. Lent no longer, the waiting turn takes the session back.
    if !lending && let Some(parked) = self.parked().remove(session) {
      going.entry(session.clone()).or_default().extend(parked);
    }

    if going.get(session).is_some_and(|runs| runs.is_empty()) {
      going.remove(session);
    }
  }

  // The agent a session is asked for, where it can hold a session at that scope.
  async fn checked(&self, agent: &str, scope: &Scope) -> Result<AgentRow, Fault> {
    let ground = self.ground();
    let Some(row) = AgentRow::named(&ground.db, agent).await.map_err(store_fault)? else {
      return Err(fault(FaultKind::Unknown, format!("no agent is named {agent}")));
    };
    let conversational = crate::conversational::settings(&ground.db, agent).await.map_err(store_fault)?;

    match scope {
      Scope::Worker if row.kind == "worker" => Ok(row),
      Scope::Worker => Err(fault(FaultKind::Refused, format!("{agent} is not a worker"))),
      Scope::Room(room) if conversational.is_some() => room_number(room).map(|_| row),
      Scope::Room(_) => Err(fault(FaultKind::Refused, format!("{agent} is not a conversational agent"))),
      Scope::Agent
        if conversational
          .as_ref()
          .is_some_and(|settings| settings.scope == crate::conversational::Scope::Agent) =>
      {
        Ok(row)
      }
      Scope::Agent => Err(fault(FaultKind::Refused, format!("{agent} keeps no session across rooms"))),
      Scope::Conversation(_) if conversational.as_ref().is_some() => Ok(row),
      Scope::Conversation(_) if gate_work_key(agent) => Ok(row),
      Scope::Conversation(_) => {
        Err(fault(FaultKind::Refused, format!("{agent} is not a conversational agent")))
      }
      Scope::Apart(_) => Err(apart_refused(agent)),
    }
  }

  // The agent a session's scope stands on, where it stands on one; a thrown-away session stands on the
  // name its turns run under, which is no agent's row.
  async fn admitted(&self, agent: &str, scope: &Scope) -> Result<Option<AgentRow>, Fault> {
    match scope {
      // A role's run goes out on whichever provider its slot or its caller names, each a provider this build
      // carries and the owner has on (docs/provider-order.md, 「Whether a road can answer」).
      Scope::Apart(apart) => {
        self.provider_ready(&apart.provider).await?;

        Ok(None)
      }
      Scope::Conversation(conversation) => {
        self.provider_ready(&conversation.provider).await?;

        let Some(row) = AgentRow::named(&self.ground().db, agent).await.map_err(store_fault)? else {
          return Err(fault(FaultKind::Unknown, format!("no agent is named {agent}")));
        };

        if crate::conversational::settings(&self.ground().db, agent)
          .await
          .map_err(store_fault)?
          .is_none()
        {
          return Err(fault(FaultKind::Refused, format!("{agent} is not a conversational agent")));
        }

        Ok(Some(row))
      }
      _ => self.checked(agent, scope).await.map(Some),
    }
  }

  // Whether a provider is one this build carries and the owner has switched on.
  async fn provider_ready(&self, provider: &str) -> Result<(), Fault> {
    if harness_providers::provider(provider).is_none() {
      return Err(fault(FaultKind::Refused, format!("no provider named {provider} in this build")));
    }

    if !crate::control::harness::provider_enabled(&self.ground().db, provider)
      .await
      .map_err(|why| fault(FaultKind::Refused, why.error))?
    {
      return Err(fault(FaultKind::Refused, format!("{provider} is switched off")));
    }

    Ok(())
  }

  // The supervisor's session in the room its turn goes on in: the one it holds, or a new generation where it
  // holds none, where the harness session it holds was opened on another provider or model or stands on a
  // place its registration no longer has, or where a generation that kept turns holds no harness session.
  async fn room_session(
    &self,
    agent: &str,
    chat_id: i64,
    provider: &'static dyn harness_providers::Provider,
    model: &str,
  ) -> Result<RoomSession, Refusal> {
    let ground = self.ground();
    let db = &ground.db;
    let Some(held) = sessions::session(db, agent, chat_id).await? else {
      return sessions::open_generation(db, agent, chat_id).await;
    };
    let moved = match &held.harness {
      None => false,
      Some(harness) => harness_moved(ground, harness, provider, model).await,
    };
    let unresumable =
      held.harness.is_none() && sessions::keeps_turns(db, agent, chat_id, held.generation).await?;

    if moved || unresumable {
      sessions::open_generation(db, agent, chat_id).await
    } else {
      Ok(held)
    }
  }

  // The handoff asked for where a chat-line turn is about to open a fresh session in place of one the agent
  // had, and what came of it said in the turn's run log (docs/sessions.md, 「A restart hands the session
  // over」); a turn that resumes its session asks nothing. One whose ask was not had leaves its session marked
  // so, for the receiver to open the fresh one on its transcript, unless the reset it carries out is clean. A
  // session gone with no program left to ask is told to the receiver all the same, so its room shows the line.
  async fn handed_off(
    &self,
    log: &mut RunLog,
    agent: &str,
    resuming: bool,
    kept: bool,
    asking: Asking<'_>,
  ) {
    let ground = self.ground();
    let gone = asking.cause == Cause::Other;

    // A room's own session is not asked through the receiver. When its prompt or its road moved, a reset of it
    // was asked or its window filled, agency asks it for its own handoff note and tells the receiver once the
    // generation opens; a generation that opens because the session it had is gone has no session left to ask,
    // so it carries nothing and the receiver is told alone.
    if asking.room.is_some() {
      if gone {
        crate::handoff::tell(ground.vars(), agent, asking).await;
      }

      return;
    }

    let session = SessionId(agent.to_owned());

    // A turn the receiver's handoff sends on the session lent to it is that handoff's own: it asks for none,
    // since the receiver lines an ask up behind the one under way, which waits on this very turn.
    if self.handing().contains(&session) {
      log.write(&ground.db, Line::note(SUPERVISOR_RUNNER.log.handoff_inside.clone())).await;
      return;
    }

    let standing = crate::lines::place::Place::of(&ground.roots.data, agent).program_alive().is_some();

    if !crate::handoff::replaces(kept, resuming, standing) {
      if gone && !resuming {
        crate::handoff::tell(ground.vars(), agent, asking).await;
      }

      return;
    }

    // Asks for the same session are paced, never skipped: one sooner than the pace after the last waits out the
    // rest of it and is then made, so a turn that keeps failing does not leave the room in a handoff loop and no
    // replacement lets a session go without asking (docs/sessions.md, 「A restart hands the session over」).
    let key = format!("handoff_asked:{agent}:0");
    let pace = i64::try_from(self.handoff_pace.load(Ordering::Relaxed)).unwrap_or(i64::MAX);
    let asked_at = crate::queue::settings::get(&ground.db, &key)
      .await
      .ok()
      .flatten()
      .and_then(|at| at.parse::<i64>().ok());

    if let Some(left) = asked_at.map(|at| at.saturating_add(pace) - now_ms()).filter(|left| *left > 0) {
      log
        .write(
          &ground.db,
          Line::note(fill_marks(
            &SUPERVISOR_RUNNER.log.handoff_paced,
            &[("seconds", &(left / 1000 + 1).to_string())],
          )),
        )
        .await;
      tokio::time::sleep(Duration::from_millis(left.unsigned_abs())).await;
    }

    let _ = crate::queue::settings::set(&ground.db, &key, &now_ms().to_string()).await;

    // The handoff a supervisor's one session across rooms is asked for is one more turn of that same session,
    // so the session is lent to it while this turn waits: the receiver finds the session by the state agency
    // answers for it, and that turn would otherwise be refused as a second turn of the session.
    self.handing().insert(session.clone());

    // A stop asked of the waiting turn ends the wait; the turn then ends stopped, as its run reads the stop.
    let handed = tokio::select! {
      handed = crate::handoff::asked(ground.vars(), agent, asking) => handed,
      () = stop_asked(ground, log.run()) => {
        crate::handoff::Handed::Failed(SUPERVISOR_RUNNER.log.handoff_stopped.clone())
      }
    };

    self.lent_back(&session);

    log.write(&ground.db, Line::note(handed.said())).await;

    if matches!(handed, crate::handoff::Handed::Failed(_))
      && !asking.clean
      && let Err(why) = crate::supervisor_turn::replacement::lose(&ground.db, agent).await
    {
      eprintln!("{agent}: the handoff it lost was not written down: {}", why.detail);
    }
  }

  // One turn of a supervisor in a room, carried by its resident program on its room's session.
  async fn room_turn(
    &self,
    agent: &str,
    chat_id: i64,
    run: &str,
    prompt: &str,
    context: &TurnContext,
    shown: OutputWatch<'_>,
  ) -> Result<Came, Fault> {
    let ground = self.ground();
    let db = ground.db.clone();
    let failed = |why: String| Ok(Came::Failed(why, None));
    let detail = |refusal: Refusal| refusal.detail;
    let Some(pinned) = pinned(ground, agent).await? else {
      return Err(fault(FaultKind::Unknown, format!("no agent is named {agent}")));
    };
    let (provider, model) = match choice_of(ground, &pinned).await {
      Ok(choice) => choice,
      Err(why) => return failed(why),
    };
    let began = context.began.unwrap_or_else(now_ms);

    // A handover the room's session owes, or one a reset or a road moved under it calls for, is made before
    // anything is opened, so the turn opens on its note (docs/supervisors.md, 「A session in each room」).
    self.room_replaced_ahead(agent, chat_id, provider, &model).await;

    let hold = &self.places.kept.reset;
    let keeping = hold.keeping().await;

    if overtaken::overtaken(&db, agent, began).await {
      return Ok(Came::Stopped);
    }

    let settings = Settings::of(&db, agent)
      .await
      .map_err(|refused| fault(FaultKind::Refused, refused.error))?;
    let system = match system_of(ground, &pinned, &settings, provider, &model).await {
      Ok(system) => system,
      Err(why) => return failed(why),
    };
    let mut log = RunLog::new(Target {
      agent: agent.to_owned(),
      run: run.to_owned(),
      chat_id: Some(chat_id),
      job_id: None,
    });

    if sessions::reset_waiting(&db, agent, chat_id).await.map_err(store_fault)? {
      if sessions::session(&db, agent, chat_id).await.map_err(store_fault)?.is_some() {
        sessions::open_generation(&db, agent, chat_id).await.map_err(store_fault)?;
      }

      sessions::settle_reset(&db, agent, chat_id).await.map_err(store_fault)?;
      log.write(&db, Line::note(SUPERVISOR_RUNNER.log.cleared.clone())).await;
    }

    let mut session = self
      .room_session(agent, chat_id, provider, &model)
      .await
      .map_err(|why| fault(FaultKind::Refused, detail(why)))?;
    let limits = &SUPERVISOR_RUNNER.limits;
    let guarded = provider.declared().resident.as_ref().is_some_and(|resident| resident.turn_limits);

    // A room session carried over its net by a turn that never ended opens a fresh generation rather
    // than judging every new turn over the net at once.
    if guarded && session.spent.calls >= limits.calls {
      session = sessions::open_generation(&db, agent, chat_id).await.map_err(store_fault)?;
      log.write(&db, Line::note(SUPERVISOR_RUNNER.log.cleared.clone())).await;
    }

    let hash = format!("{:x}", Sha256::digest(system.as_bytes()));
    let moved = sessions::prompt_moved(session.harness.is_some(), session.system_hash.as_deref(), &hash);

    drop(keeping);

    if moved {
      // The documents the session's prompt is composed from moved: this turn goes on in the session as it
      // stands, sent the prompt as it now is, and the handover is owed, made once the turn has ended
      // (docs/supervisors.md, 「A session in each room」; docs/sessions.md, 「Session replacement」).
      let due = Due {
        provider,
        model: model.clone(),
        system: system.clone(),
        generation: generation_of(&settings),
        session: session.harness.as_ref().map(|harness| harness.id.clone()).unwrap_or_default(),
        placement: session.harness.as_ref().and_then(|harness| harness.placement.clone()),
        ratio: 1.0,
        why: Why::Documents,
      };

      let owed = crate::supervisor_turn::replacement::RoomOwed::of(&pinned.speaker, &due);

      if let Err(why) = crate::supervisor_turn::replacement::owe_room(&db, agent, chat_id, &owed).await {
        eprintln!(
          "supervisor {agent} in chat {chat_id}: the handover owed was not written down: {}",
          why.detail
        );
      }

      log.write(&db, Line::note(SUPERVISOR_RUNNER.log.system_moved_owed.clone())).await;
    }

    let changed =
      moved || (session.system_hash.is_some() && session.system_hash.as_deref() != Some(hash.as_str()));

    if session.system_hash.as_deref() != Some(hash.as_str()) {
      session = sessions::mark_system(&db, agent, chat_id, &hash).await.map_err(store_fault)?;
    }

    if changed {
      log.write(&db, Line::note(SUPERVISOR_RUNNER.log.system_changed.clone())).await;

      if let Some(session) = id_of(agent, &Scope::Room(chat_id.to_string())) {
        ground.events.publish(SystemPromptChanged {
          session: session.0,
          run: run.to_owned(),
        });
      }
    }

    // A session opened by a handoff begins with the note its last generation wrote, ahead of the turn's
    // words.
    let prompt = match (&session.harness, &session.handoff) {
      (None, Some(note)) => format!("{note}\n\n{prompt}"),
      _ => prompt.to_owned(),
    };
    let generation = generation_of(&settings);

    let started = Instant::now();

    // A fresh generation in place of one the supervisor's program still stands on asks for its handoff first;
    // a prompt that moved already wrote one above.
    if !moved {
      self
        .handed_off(
          &mut log,
          agent,
          session.harness.is_some(),
          false,
          Asking {
            reason: &HANDOFF.reasons.replaced,
            room: Some(chat_id),
            cause: Cause::Road,
            clean: false,
            by: None,
          },
        )
        .await;
    }

    let mut asked = self
      .room_run(
        &mut log,
        &pinned.speaker,
        provider,
        &model,
        &session,
        &system,
        &prompt,
        &generation,
        guarded,
        began,
        context,
        shown,
      )
      .await;

    // A session its harness no longer holds or cannot continue is let go, and the turn is asked again once
    // on a fresh one.
    if let Err(NoAnswer::Failed(said)) = &asked
      && session.harness.is_some()
      && lost_session(said)
    {
      session = sessions::open_generation(&db, agent, chat_id).await.map_err(store_fault)?;
      self
        .handed_off(
          &mut log,
          agent,
          false,
          false,
          Asking {
            reason: &HANDOFF.reasons.lost,
            room: Some(chat_id),
            cause: Cause::Other,
            clean: false,
            by: None,
          },
        )
        .await;
      asked = self
        .room_run(
          &mut log,
          &pinned.speaker,
          provider,
          &model,
          &session,
          &system,
          &aside::carried(run, &prompt),
          &generation,
          guarded,
          began,
          context,
          shown,
        )
        .await;
    }

    let _keeping = hold.keeping().await;
    let (reply, placement) = match asked {
      Ok(asked) if !overtaken::overtaken(&db, agent, began).await => asked,
      Ok(_) | Err(NoAnswer::Overtaken | NoAnswer::Stopped(_)) => return Ok(Came::Stopped),
      Err(no) => {
        let (reason, engine) = failure_of(no);
        let _ = sessions::mark_spent(&db, agent, chat_id, Spent::default()).await;

        ground.events.publish(RoadWalked::of(
          chat_id,
          run,
          provider.name(),
          &model,
          false,
          started,
          &[],
          None,
        ));
        log
          .write(
            &db,
            Line {
              kind: "end",
              text: reason.clone(),
              state: Some("failed"),
              ..Line::default()
            },
          )
          .await;
        return Ok(Came::Failed(reason, engine));
      }
    };

    ground.events.publish(RoadWalked::of(
      chat_id,
      run,
      &reply.mode,
      &reply.model,
      true,
      started,
      &reply.spend,
      reply.context_ratio,
    ));
    self.record_usage(agent, &reply, run).await;
    sessions::record_turn(
      &db,
      &session,
      &[
        Kept {
          role: "user",
          text: &prompt,
        },
        Kept {
          role: "assistant",
          text: &reply.text,
        },
      ],
      Some(&Harness {
        id: reply.session_id.clone(),
        road: harness_road(provider.name(), &model),
        placement: placement.clone(),
      }),
      None,
    )
    .await
    .map_err(store_fault)?;
    log
      .write(
        &db,
        Line {
          kind: "end",
          text: SUPERVISOR_RUNNER.log.answered.clone(),
          state: Some("succeeded"),
          ..Line::default()
        },
      )
      .await;

    // A turn whose context reached its fold ratio hands its session over before the next one, where its
    // provider declares the handoff (docs/supervisors.md, 「Its limit」).
    let hands_off = provider
      .declared()
      .resident
      .as_ref()
      .is_some_and(|resident| resident.handover == policy::providers::Handover::Handoff);
    let mark = pinned.supervisor.fold_ratio;

    if let Some(ratio) = reply.context_ratio.filter(|ratio| hands_off && *ratio >= mark) {
      let due = Due {
        provider,
        model,
        system,
        generation,
        session: reply.session_id.clone(),
        placement,
        ratio,
        why: Why::Limit,
      };
      let (ground, hold, speaker) = (self.places.ground.clone(), hold.clone(), pinned.speaker);

      tokio::spawn(async move { hand_over(&ground, &hold, &speaker, chat_id, due).await });
    }

    Ok(Came::Answered(
      reply.text.clone(),
      spent_of(&reply.spend),
      left(&reply),
      reply.answers.clone(),
    ))
  }

  #[allow(clippy::too_many_arguments)]
  async fn room_run(
    &self,
    log: &mut RunLog,
    speaker: &Speaker,
    provider: &'static dyn harness_providers::Provider,
    model: &str,
    session: &RoomSession,
    system: &str,
    prompt: &str,
    generation: &IndexMap<String, Value>,
    guarded: bool,
    began: i64,
    context: &TurnContext,
    shown: OutputWatch<'_>,
  ) -> Result<(Reply, Option<String>), NoAnswer> {
    let ground = self.ground();

    if let Some(reason) = spent_out(&SUPERVISOR_RUNNER.limits, session.spent).filter(|_| guarded) {
      return Err(NoAnswer::Cut(reason));
    }

    let resume = session
      .harness
      .as_ref()
      .map(|harness| (harness.id.as_str(), harness.placement.as_deref()));
    let run = log.run().to_owned();
    // A registration whose engine the arbiter stands is asked a card first, held for the turn and given
    // back at its end (docs/own-llm.md, 「An engine the arbiter stands」).
    let card = crate::cards::hold_for_model(
      ground,
      provider.name(),
      model,
      &speaker.name,
      resume.map(|(id, _)| id),
      log,
      Some(shown),
    )
    .await
    .map_err(|refused| NoAnswer::Failed(refused.detail))?;
    let watching = card.as_ref().map(|card| crate::cards::watch_turn(ground, card, &run));
    let turned = async {
      let composed = compose(
        ground,
        &ground.machine(),
        &SupervisorTurn {
          agent: &speaker.name,
          chat_id: session.chat_id,
          provider: provider.name(),
          model,
          system,
          prompt,
          privileged: context.privileged,
          resume,
          card: card.as_ref().map(|card| card.node.as_str()),
          run: &run,
          answers: &context.answers,
          generation,
          began,
        },
      )
      .await
      .map_err(|refused| NoAnswer::Failed(refused.detail))?;

      log.launched(&ground.db, Some(&composed.system_path), prompt, &[]).await;

      let tally = Tally {
        agent: &speaker.name,
        chat_id: session.chat_id,
        carried: session.spent,
      };
      let reply = run_turn(
        ground,
        &speaker.name,
        crate::lines::chat::LINE,
        provider.name(),
        &composed,
        resume.map(|(id, _)| id),
        log,
        guarded,
        Some(&tally),
        Some(shown),
      )
      .await?;

      Ok((reply, composed.placement))
    }
    .await;

    if let Some(watching) = watching {
      watching.finish(ground).await;
    }

    if let Some(card) = &card {
      crate::cards::give_back(ground, card).await;
    }

    turned
  }

  // One turn of an agent across rooms, carried by its resident program on the chat line around the room
  // block, the road and the base persona, on the one session its row keeps; a session
  // its harness no longer holds is let go, and the turn asked again once on a fresh one.
  async fn agent_turn(
    &self,
    agent: &str,
    run: &str,
    prompt: &str,
    context: &TurnContext,
    shown: OutputWatch<'_>,
    handoff: bool,
  ) -> Result<Came, Fault> {
    let ground = self.ground();
    let db = ground.db.clone();
    let Some(room) = &context.room else {
      return Err(fault(
        FaultKind::Refused,
        format!("{agent}: a turn across rooms names the room it answers"),
      ));
    };
    let began = context.began.unwrap_or_else(now_ms);
    let mut log = RunLog::new(Target {
      agent: agent.to_owned(),
      run: run.to_owned(),
      chat_id: Some(room.chat_id),
      job_id: None,
    });
    // A replacement of the session being made outside any turn is waited for, and the turn opens on what it
    // leaves (docs/sessions.md, 「Session replacement」).
    if !handoff {
      self.wait_replaced(agent).await;
    }

    let mut held = agent_session(&db, agent).await.map_err(store_fault)?;
    let last_mode = agent_last_mode(&db, agent).await.map_err(store_fault)?;
    // A session the last answer left on a road other than Claude Code is resumed on that road; one left on
    // Claude Code is Claude's, and a road other than Claude Code opens a session of its own.
    let road_resume = if last_mode.as_deref() == Some(CHAT_PROVIDER) { None } else { held.clone() };
    // Claude Code's own session resumes only a session Claude Code left: one another road left is held by no
    // Claude Code login, so a fresh session opens in its place.
    let claude_held = if last_mode.as_deref().is_none_or(|mode| mode == CHAT_PROVIDER) {
      held.clone()
    } else {
      None
    };
    // A turn of the receiver's handoff goes out on the road the session last answered on where the row now names
    // another, so the program holding the session writes the note (docs/sessions.md, 「A restart hands the session
    // over」).
    let handoff_road = if handoff { answered_road(&db, agent).await } else { None };
    let handoff_model = handoff_road.as_ref().map(|road| road.model.as_str());

    // A reset the owner asked of the supervisor's one session closes it before this turn opens a new one,
    // and the session is asked for its handoff while agency still names it, as before any fresh session
    // replaces one the agent had (docs/session-resets.md, 「Who holds a session」).
    let waiting = if handoff {
      None
    } else {
      sessions::waiting_reset(&db, agent, None).await.map_err(store_fault)?
    };

    if let Some(reset) = waiting {
      // The room the turn answers is the round under way the receiver tells the reset in, so the ask names none:
      // a room named here is a room's own session, which is not asked.
      let asking = Asking {
        reason: &HANDOFF.reasons.reset,
        room: None,
        cause: Cause::Reset,
        clean: reset.clean,
        by: reset.by,
      };

      self.handed_off(&mut log, agent, false, true, asking).await;
      keep_agent_session(&db, agent, None).await.map_err(store_fault)?;
      sessions::settle_reset_agent(&db, agent).await.map_err(store_fault)?;
      held = None;
      log
        .write(&db, Line::note(policy::supervisor_runner::SUPERVISOR_RUNNER.log.cleared.clone()))
        .await;
    }

    let started = Instant::now();
    // The turn goes out on the supervisor's one road: on a provider other than Claude Code its program carries
    // the turn, and Claude Code's session stays as it stands; on Claude Code its own session carries it. The run
    // is the chat line's (docs/cache-fallback.md, 「A supervisor's turn on a road other than Claude Code」).
    let line_run = match crate::lines::chat::chat_run(
      ground,
      &ground.machine(),
      &launch_on(agent, room, context, road_resume, handoff_model, None),
      prompt,
      false,
    )
    .await
    {
      Ok(line_run) => line_run,
      Err(why) => return Ok(Came::Failed(why.detail, None)),
    };
    let hash = line_run.system_hash.clone();
    let recorded = sessions::agent_system_hash(&db, agent).await.map_err(store_fault)?;
    // The receiver's handoff turn is one more turn of the session whose prompt moved: it reads the prompt the
    // session holds, so it is no second move.
    let moved = !handoff && sessions::prompt_moved(held.is_some(), recorded.as_deref(), &hash);

    if moved {
      // The documents the prompt is composed from moved under a session the agent already had: this turn goes
      // on in that session, sent the prompt as it now is, and the replacement is owed, made once the turn has
      // ended and the agent is quiet (docs/sessions.md, 「Session replacement」).
      crate::supervisor_turn::replacement::owe(&db, agent, &HANDOFF.reasons.moved)
        .await
        .map_err(store_fault)?;
      log.write(&db, Line::note(SUPERVISOR_RUNNER.log.system_moved_owed.clone())).await;
    }

    sessions::mark_agent_system(&db, agent, &hash).await.map_err(store_fault)?;

    let asked = match road_turn(
      ground,
      &mut log,
      &line_run,
      agent,
      &context.answers,
      began,
      Some(shown),
      handoff_road.as_ref(),
    )
    .await
    {
      Ok(Some(reply)) => Ok(reply),
      Err(no) => Err(no),
      Ok(None) => {
        // A fresh session in place of one the agent's program still stands on asks for its handoff first.
        if !moved {
          self
            .handed_off(
              &mut log,
              agent,
              held.is_some(),
              false,
              Asking {
                reason: &HANDOFF.reasons.replaced,
                room: None,
                cause: Cause::Road,
                clean: false,
                by: None,
              },
            )
            .await;
        }

        let mut asked = self
          .agent_run(
            &mut log,
            agent,
            room,
            context,
            prompt,
            claude_held.clone(),
            handoff_model,
            began,
            shown,
          )
          .await;

        if let Err(NoAnswer::Failed(said)) = &asked
          && claude_held.is_some()
          && lost_session(said)
        {
          // The session the turn could not resume is asked for its handoff while agency still names it,
          // then let go.
          self
            .handed_off(
              &mut log,
              agent,
              false,
              false,
              Asking {
                reason: &HANDOFF.reasons.lost,
                room: None,
                cause: Cause::Other,
                clean: false,
                by: None,
              },
            )
            .await;
          keep_agent_session(&db, agent, None).await.map_err(store_fault)?;
          asked = self
            .agent_run(
              &mut log,
              agent,
              room,
              context,
              &aside::carried(run, prompt),
              None,
              handoff_model,
              began,
              shown,
            )
            .await;
        }

        asked
      }
    };

    let reply = match asked {
      Ok(reply) => reply,
      Err(NoAnswer::Overtaken | NoAnswer::Stopped(_)) => return Ok(Came::Stopped),
      Err(no) => {
        let (reason, engine) = failure_of(no);
        let road = crate::supervisors::named(&db, agent).await.ok().flatten().and_then(|held| held.road);
        let (provider, model) = road
          .map_or_else(|| (CHAT_PROVIDER.to_owned(), String::new()), |road| (road.provider, road.model));

        ground.events.publish(RoadWalked::of(
          room.chat_id,
          run,
          &provider,
          &model,
          false,
          started,
          &[],
          None,
        ));
        log
          .write(
            &db,
            Line {
              kind: "end",
              text: reason.clone(),
              state: Some("failed"),
              ..Line::default()
            },
          )
          .await;

        return Ok(Came::Failed(reason, engine));
      }
    };

    ground.events.publish(RoadWalked::of(
      room.chat_id,
      run,
      &reply.mode,
      &reply.model,
      true,
      started,
      &reply.spend,
      reply.context_ratio,
    ));
    self.record_usage(agent, &reply, run).await;

    if !reply.session_id.is_empty() {
      keep_agent_session(&db, agent, Some((&reply.session_id, room.chat_id)))
        .await
        .map_err(store_fault)?;
    }

    log
      .write(
        &db,
        Line {
          kind: "end",
          text: SUPERVISOR_RUNNER.log.answered.clone(),
          state: Some("succeeded"),
          ..Line::default()
        },
      )
      .await;

    Ok(Came::Answered(
      reply.text.clone(),
      spent_of(&reply.spend),
      left(&reply),
      reply.answers.clone(),
    ))
  }

  // One turn of a conversation the caller opened, on the provider and model its scope names: the chat
  // line on Claude Code, with the agent's own documents and thinking, or the stand-in road the provider
  // declares. The session the last answered turn left is resumed where the road keeps one, and one the
  // program no longer holds is let go and the turn asked again on a fresh one.
  async fn conversation_turn(
    &self,
    turn: &Turn,
    agent: &str,
    conversation: &Conversation,
    run: &str,
    shown: OutputWatch<'_>,
  ) -> Result<Came, Fault> {
    let ground = self.ground();
    let began = turn.context.began.unwrap_or_else(now_ms);
    let room = turn.context.room.clone().unwrap_or(RoomBlock {
      chat_id: 0,
      text: String::new(),
      mark: String::new(),
    });
    let held = self.held_harness(&turn.session);
    let launch = launch_on(
      agent,
      &room,
      &turn.context,
      held.clone(),
      Some(&conversation.model),
      Some(&conversation.cwd),
    );
    let mut log = RunLog::new(Target {
      agent: agent.to_owned(),
      run: run.to_owned(),
      chat_id: Some(room.chat_id),
      job_id: None,
    });
    let started = Instant::now();
    let mut asked = self
      .conversation_asked(&mut log, agent, &launch, conversation, turn, began, shown)
      .await;

    if let Err(NoAnswer::Failed(said)) = &asked
      && held.is_some()
      && lost_session(said)
    {
      let fresh =
        launch_on(agent, &room, &turn.context, None, Some(&conversation.model), Some(&conversation.cwd));
      asked = self.conversation_asked(&mut log, agent, &fresh, conversation, turn, began, shown).await;
    }

    let reply = match asked {
      Ok(reply) => reply,
      Err(NoAnswer::Overtaken | NoAnswer::Stopped(_)) => return Ok(Came::Stopped),
      Err(no) => {
        let (reason, engine) = failure_of(no);

        ground.events.publish(RoadWalked::of(
          room.chat_id,
          run,
          &conversation.provider,
          &conversation.model,
          false,
          started,
          &[],
          None,
        ));
        log
          .write(
            &ground.db,
            Line {
              kind: "end",
              text: reason.clone(),
              state: Some("failed"),
              ..Line::default()
            },
          )
          .await;

        return Ok(Came::Failed(reason, engine));
      }
    };

    ground.events.publish(RoadWalked::of(
      room.chat_id,
      run,
      &reply.mode,
      &reply.model,
      true,
      started,
      &reply.spend,
      reply.context_ratio,
    ));
    self.record_usage(agent, &reply, run).await;
    self.keep_conversation_harness(&turn.session, &conversation.provider, &reply);
    log
      .write(
        &ground.db,
        Line {
          kind: "end",
          text: SUPERVISOR_RUNNER.log.answered.clone(),
          state: Some("succeeded"),
          ..Line::default()
        },
      )
      .await;

    Ok(Came::Answered(
      reply.text.clone(),
      spent_of(&reply.spend),
      left(&reply),
      reply.answers.clone(),
    ))
  }

  // One gate's turn of a conversation: the gate's instructions as the program's whole system prompt, on the
  // model and effort its mapping names, with no tools, no MCP server and the thinking the gate asks for.
  #[allow(clippy::too_many_arguments)]
  async fn gate_turn(
    &self,
    turn: &Turn,
    agent: &str,
    provider: &str,
    cwd: &str,
    gate: &GateAsk,
    run: &str,
    shown: OutputWatch<'_>,
  ) -> Result<Came, Fault> {
    let ground = self.ground();
    let machine = ground.machine();
    let began = turn.context.began.unwrap_or_else(now_ms);
    let chat_id = turn.context.room.as_ref().map_or(0, |room| room.chat_id);
    let mut log = RunLog::new(Target {
      agent: agent.to_owned(),
      run: run.to_owned(),
      chat_id: turn.context.room.as_ref().map(|room| room.chat_id),
      job_id: None,
    });
    let mut prepared = gate_run(ground, agent, gate, &turn.prompt, cwd)
      .map_err(|refused| fault(FaultKind::Refused, refused.detail))?;
    // On a road whose program carries its own turns the turn stands in a place kept for its program
    // (docs/agent-control.md, 「A gate's places」), its run's records staying the agent's.
    let seat = if provider != CHAT_PROVIDER && crate::chat_stand_in::inbox_declared(provider) {
      let program = gate_places::Asked {
        agent,
        provider,
        model: &prepared.model,
        effort: &prepared.effort,
        thinking: prepared.thinking,
        system: gate.system.as_deref().unwrap_or_default(),
        cwd,
      }
      .key();
      let seat = self.places.kept.gate_places.take(ground, agent, &program).await;

      prepared.agent = seat.name.clone();
      Some(seat)
    } else {
      None
    };
    let started = Instant::now();
    let asked = if provider == CHAT_PROVIDER {
      let composed =
        compose_prepared(ground, &machine, &prepared, &turn.prompt, run, &turn.context.answers, began)
          .await;

      match composed {
        Ok(composed) => {
          log.launched(&ground.db, Some(&composed.system_path), &turn.prompt, &[]).await;

          run_turn(
            ground,
            agent,
            crate::lines::chat::LINE,
            CHAT_PROVIDER,
            &composed,
            None,
            &mut log,
            false,
            None,
            Some(shown),
          )
          .await
        }
        Err(refused) => Err(NoAnswer::Failed(refused.detail)),
      }
    } else {
      crate::chat_stand_in::gate_turn_on(
        ground,
        &mut log,
        &prepared,
        provider,
        &turn.context.answers,
        began,
        gate,
        Some(shown),
      )
      .await
    };

    let reply = match asked {
      Ok(reply) => reply,
      Err(NoAnswer::Overtaken | NoAnswer::Stopped(_)) => return Ok(Came::Stopped),
      Err(no) => {
        let (reason, engine) = failure_of(no);

        ground.events.publish(RoadWalked::of(
          chat_id,
          run,
          provider,
          &prepared.model,
          false,
          started,
          &[],
          None,
        ));
        log
          .write(
            &ground.db,
            Line {
              kind: "end",
              text: reason.clone(),
              state: Some("failed"),
              ..Line::default()
            },
          )
          .await;

        return Ok(Came::Failed(reason, engine));
      }
    };

    ground.events.publish(RoadWalked::of(
      chat_id,
      run,
      &reply.mode,
      &reply.model,
      true,
      started,
      &reply.spend,
      reply.context_ratio,
    ));
    self.record_usage(agent, &reply, run).await;
    log
      .write(
        &ground.db,
        Line {
          kind: "end",
          text: SUPERVISOR_RUNNER.log.answered.clone(),
          state: Some("succeeded"),
          ..Line::default()
        },
      )
      .await;

    // A session past its bound starts afresh once the answer is on its way, its place held until it has.
    let held: i64 = reply
      .spend
      .iter()
      .map(|spent| spent.input_tokens + spent.cache_read_tokens + spent.output_tokens)
      .sum();

    if let Some(seat) = seat
      && held >= GATE_PLACES.session_tokens
    {
      tokio::spawn(seat.start_afresh());
    }

    Ok(Came::Answered(
      reply.text.clone(),
      spent_of(&reply.spend),
      left(&reply),
      reply.answers.clone(),
    ))
  }

  // One turn of a conversation on the road its launch names: Claude Code's chat line on the deployment's
  // own program, or the stand-in road the provider declares.
  #[allow(clippy::too_many_arguments)]
  async fn conversation_asked(
    &self,
    log: &mut RunLog,
    agent: &str,
    launch: &ChatLaunch<'_>,
    conversation: &Conversation,
    turn: &Turn,
    began: i64,
    shown: OutputWatch<'_>,
  ) -> Result<Reply, NoAnswer> {
    if conversation.provider == CHAT_PROVIDER {
      return self
        .agent_run_launched(log, agent, launch, &turn.prompt, &turn.context.answers, began, shown)
        .await;
    }

    crate::chat_stand_in::turn_on(
      self.ground(),
      log,
      launch,
      &conversation.provider,
      &conversation.model,
      &turn.prompt,
      &turn.context.answers,
      began,
      Some(shown),
    )
    .await
  }

  // The session a conversation's next turn on the provider named would resume, none where its road keeps
  // none.
  fn held_harness(&self, session: &SessionId) -> Option<String> {
    self.conversations().get(session).and_then(|(_, held)| held.harness.clone())
  }

  // The session a conversation's answered turn left, kept for its next turn where its road keeps one.
  fn keep_conversation_harness(&self, session: &SessionId, provider: &str, reply: &Reply) {
    let keeps = harness_providers::provider(provider)
      .and_then(|declared| declared.declared().resident.as_ref())
      .is_some_and(|resident| resident.turns != policy::providers::Turns::Inbox);

    if !keeps {
      return;
    }

    if let Some((_, held)) = self.conversations().get_mut(session) {
      held.harness = Some(reply.session_id.clone());
    }
  }

  #[allow(clippy::too_many_arguments)]
  async fn agent_run(
    &self,
    log: &mut RunLog,
    agent: &str,
    room: &RoomBlock,
    context: &TurnContext,
    prompt: &str,
    resume: Option<String>,
    model: Option<&str>,
    began: i64,
    shown: OutputWatch<'_>,
  ) -> Result<Reply, NoAnswer> {
    let launch = launch_on(agent, room, context, resume, model, None);
    self
      .agent_run_launched(log, agent, &launch, prompt, &context.answers, began, shown)
      .await
  }

  // One chat-line turn on the launch as it stands, which names the road its run goes out on.
  #[allow(clippy::too_many_arguments)]
  async fn agent_run_launched(
    &self,
    log: &mut RunLog,
    agent: &str,
    launch: &ChatLaunch<'_>,
    prompt: &str,
    answers: &[i64],
    began: i64,
    shown: OutputWatch<'_>,
  ) -> Result<Reply, NoAnswer> {
    let ground = self.ground();
    let composed = compose_turn(ground, &ground.machine(), launch, prompt, log, answers, began)
      .await
      .map_err(|refused| NoAnswer::Failed(refused.detail))?;

    log.launched(&ground.db, Some(&composed.system_path), prompt, &[]).await;

    run_turn(
      ground,
      agent,
      crate::lines::chat::LINE,
      CHAT_PROVIDER,
      &composed,
      launch.resume.as_deref(),
      log,
      false,
      None,
      Some(shown),
    )
    .await
  }

  // What a supervisor's run spent is its own: its last answer written and its usage published.
  async fn record_usage(&self, agent: &str, reply: &Reply, run: &str) {
    let ground = self.ground();

    if let Err(why) =
      note_answer(&ground.db, agent, &reply.mode, Some(&reply.model), reply.effort.as_deref()).await
    {
      eprintln!("supervisor {agent}: its last answer was not written: {}", why.detail);
    }

    let account = slot_account(&reply.mode, reply.account.as_deref());

    if let Some(spent) =
      UsageSpent::of(agent, Some(run), now_ms() / 1000, &reply.spend, |_| account.clone())
    {
      ground.events.publish(spent);
    }
  }

  // A turn's end published as `turn-ended`, before its send is answered with it.
  // One turn sent into a session and carried to its end.
  async fn sent(&self, turn: Turn) -> Result<TurnEnd, Fault> {
    let (agent, scope) = self.scope_held(&turn.session)?;

    // There is no worker scope: a run in a repository is a role's session on its slot (docs/api.md,
    // 「sessions」).
    if matches!(scope, Scope::Worker) {
      return Err(no_worker_scope(&turn.session));
    }

    let row = self.admitted(&agent, &scope).await?;
    let at = now_ms();
    let room = match &scope {
      Scope::Room(room) => Some(room.clone()),
      Scope::Agent => turn.context.room.as_ref().map(|room| room.chat_id.to_string()),
      Scope::Worker | Scope::Apart(_) | Scope::Conversation(_) => None,
    };

    // A job's turn goes out under the run name its runner gives it, on Claude Code alone; a thrown-away
    // session carries nothing else.
    let work = match scope {
      Scope::Worker | Scope::Apart(_) => turn.context.work.as_ref(),
      _ => None,
    };

    // A gate's turn carries the gate's ask instead of work, and goes out on the gate road.
    if matches!(scope, Scope::Apart(_)) && work.is_none() && turn.context.gate.is_none() {
      return Err(fault(
        FaultKind::Refused,
        format!("{}: a thrown-away session's turn names its work parts", turn.session),
      ));
    }

    if matches!(scope, Scope::Conversation(_)) && turn.context.work.is_some() {
      return Err(fault(
        FaultKind::Refused,
        format!("{}: a conversation's turn carries no work parts", turn.session),
      ));
    }

    let run = match (&room, work) {
      (_, Some(work)) => work.name.clone(),
      (Some(room), None) => {
        fill_marks(&SUPERVISOR_RUNNER.runs.turn, &[("chat", room.as_str()), ("at", &at.to_string())])
      }
      (None, None) => format!("turn:{agent}:{at}"),
    };

    let turn_id;
    // A session its turn lent the receiver's handoff takes each turn the receiver sends for it, one at a
    // time, for as long as the ask stands; the turn that lent it stands waiting and takes the session back
    // once the ask is answered.
    let lent = self.handing().contains(&turn.session);

    {
      let mut going = self.going();
      let runs = going.entry(turn.session.clone()).or_default();
      let mut parked = self.parked();

      // Busy: a second turn of a session not lent, or of a lent one while a handoff turn of it still goes.
      if !runs.is_empty() && (!lent || parked.contains_key(&turn.session)) {
        return Err(fault(FaultKind::Busy, format!("{} is in a turn", turn.session)));
      }

      if lent {
        parked.entry(turn.session.clone()).or_default().extend(std::mem::take(runs));
      }

      turn_id = self.next_turn();
      runs.push(Running {
        turn: turn_id,
        run: run.clone(),
      });
    }

    self.outputs.begun(&turn.session, turn_id, &run);

    // A turn agency carries stands in the ledger from its start, wherever it is no job's and no gate's: a chat
    // turn into a room, and a role session's turn under its title, in the room it names, under the name its log
    // lines are written under (docs/run-records.md, 「The run ledger」).
    let carried = match (&room, work, &turn.context.gate, &turn.context.job) {
      (Some(room), None, None, _) => Some(CarriedRun {
        unit: run.clone(),
        room: room.parse().ok(),
        title: None,
      }),
      (None, Some(work), None, None) => Some(CarriedRun {
        unit: work.unit.clone().unwrap_or_else(|| work.name.clone()),
        room: (work.chat != 0).then_some(work.chat),
        title: Some(work.title.clone()),
      }),
      _ => None,
    };
    let ledger = match carried {
      Some(carried) => carried.open(&self.ground().db, &agent, &turn.context, at).await,
      None => None,
    };

    if !hears_words(work) {
      self.deaf().insert(turn.session.clone());
    }

    // The turn is kept while it goes, so a server started after this one goes down hands its end back to
    // the caller (docs/sessions.md, 「A restart hands the session over」).
    crate::turn_pickup::keep(
      &self.ground().db,
      &crate::turn_pickup::InFlight {
        session: turn.session.0.clone(),
        turn: turn_id,
        run: run.clone(),
        agent: agent.clone(),
        key: turn.context.key.clone(),
        chat_id: room.as_deref().and_then(|room| room.parse().ok()),
        ledger,
        job: turn.context.job.as_ref().map(|job| job.job),
        started_at: at,
      },
    )
    .await;

    // Carried by this process until its end, which closes it; the queue's pass leaves it to that end.
    let _carrying = crate::turn_pickup::Carrying::of(&run);

    // A room turn of a conversational agent is what an item it files is filed under.
    if let (Some(room), None, None) = (&room, work, &turn.context.gate) {
      crate::turn_marks::carrying(
        &agent,
        &run,
        crate::turn_marks::Mark {
          chat_id: room.parse().unwrap_or_default(),
          privileged: turn.context.privileged,
        },
      );
    }

    self.ground().events.publish(TurnStarted {
      session: turn.session.0.clone(),
      run: run.clone(),
      turn_id,
      at,
    });

    let shown = OutputWatch {
      outputs: &self.outputs,
      session: &turn.session,
      turn: turn_id,
    };
    let came = match (&scope, &row) {
      (Scope::Apart(apart), _) => match (&turn.context.gate, work) {
        (Some(gate), _) => {
          self.gate_turn(&turn, &agent, &apart.provider, &apart.cwd, gate, &run, shown).await
        }
        (None, Some(work)) => {
          let db = &self.ground().db;

          if turn.context.job.is_some() {
            keep_worker_job(db, &agent, turn.context.job).await;
          }

          // The seat's home node stands over every other claim to where the run goes: the administrator placed
          // this agent on it (docs/role-agents.md, 「What an agent is」). A run of an account with a node and no
          // home of its own goes the place-based road whatever its provider, so the link carries its place to
          // the node (docs/person-nodes.md, 「Which node is a person's」); a JOB without a home stands where its
          // worker does, whose checkout it works (docs/worker-slots.md, 「Names and places」). A node that
          // cannot be read ends the turn refused, as any other turn refused once it began.
          // Boxed: the turn's future is large, and wrapped once more it would no longer fit a test thread's stack.
          let seat = crate::role_agents::session_seat(db, &turn.session.0).await;
          let home = match seat.as_ref().ok().cloned().flatten() {
            Some(name) => crate::role_agents::named(db, &name)
              .await
              .ok()
              .flatten()
              .and_then(|agent| agent.home_node),
            None => None,
          };
          let going = Box::pin(async {
            let mut placed =
              match crate::person_nodes::run_placed(db, apart.role.as_deref(), &apart.cwd).await {
                Err(why) => return Err(fault(FaultKind::Refused, why.detail)),
                Ok(placed) => placed,
              };

            if let Some(home) = home.clone() {
              placed.node = Some(home);
            } else if turn.context.job.is_some() {
              placed = match crate::queue::workers::worker_row(db, &agent, &self.ground().source).await {
                Ok(worker) => crate::person_nodes::RunPlaced {
                  account: None,
                  node: worker.and_then(|worker| worker.node).filter(|node| !node.trim().is_empty()),
                },
                Err(why) => {
                  eprintln!("{run}: where its worker stands could not be read: {}", why.detail);
                  placed
                }
              };
            }

            match placed.node.is_none() && apart.provider == worker_session::PROVIDER {
              true => self.carried(&turn, &agent, WorkSeat::Apart(apart), work, turn_id).await,
              false => {
                let began = turn.context.began.unwrap_or(at);

                self
                  .apart_in_place(
                    &turn.session,
                    &agent,
                    apart,
                    work,
                    &turn.prompt,
                    began,
                    turn.context.job,
                    placed,
                    shown,
                  )
                  .await
              }
            }
          });
          // The turn runs as the seat of the slot its session was started on, whatever folder it works in
          // (docs/role-agents.md, 「What a run reads off its agent」), so a slot asked with no path still goes out
          // on its seat's account and node.
          let came = match seat {
            Ok(Some(seat)) => crate::role_agents::composed_as(seat, going).await,
            Ok(None) => going.await,
            Err(why) => Err(fault(FaultKind::Refused, why.to_string())),
          };

          if turn.context.job.is_some() {
            keep_worker_job(db, &agent, None).await;
          }

          came
        }
        (None, None) => Err(apart_refused(&agent)),
      },
      (Scope::Conversation(conversation), _) => match &turn.context.gate {
        Some(gate) => {
          self
            .gate_turn(&turn, &agent, &conversation.provider, &conversation.cwd, gate, &run, shown)
            .await
        }
        None => self.conversation_turn(&turn, &agent, conversation, &run, shown).await,
      },
      (Scope::Worker, _) => Err(no_worker_scope(&turn.session)),
      (Scope::Room(room), _) => match room_number(room) {
        Ok(chat_id) => {
          let turn_of = self.room_turn(&agent, chat_id, &run, &turn.prompt, &turn.context, shown);

          Box::pin(supplied_turn(self.ground(), &agent, Some(chat_id), &run, ledger, Box::pin(turn_of)))
            .await
        }
        Err(refused) => Err(refused),
      },
      (Scope::Agent, _) => {
        let turn_of = self.agent_turn(&agent, &run, &turn.prompt, &turn.context, shown, lent);

        Box::pin(supplied_turn(self.ground(), &agent, None, &run, ledger, Box::pin(turn_of))).await
      }
    };

    self.ended(&turn.session, &run);
    crate::turn_marks::ended(&agent, &run);

    // A session opened without its handoff is told so until a turn begun after it has answered.
    if matches!(came, Ok(Came::Answered(..)))
      && matches!(scope, Scope::Agent | Scope::Room(_))
      && let Err(why) =
        crate::supervisor_turn::replacement::answered_since(&self.ground().db, &turn.session.0, at).await
    {
      eprintln!("{}: the handoff it lost was not let go: {}", turn.session.0, why.detail);
    }

    // A replacement this turn found owed is made now that the turn has ended, apart from the answer going
    // back (docs/sessions.md, 「Session replacement」).
    match (&scope, self.shared()) {
      (Scope::Agent, Some(backend)) => {
        let agent = agent.clone();

        tokio::spawn(async move {
          backend.settle_owed(&agent).await;
        });
      }
      (Scope::Agent, None) => {
        self.settle_owed(&agent).await;
      }
      (Scope::Room(room), backend) => {
        if let Ok(chat_id) = room_number(room) {
          match backend {
            Some(backend) => {
              let agent = agent.clone();

              tokio::spawn(async move {
                backend.settle_room(&agent, chat_id).await;
              });
            }
            None => self.settle_room(&agent, chat_id).await,
          }
        }
      }
      _ => {}
    }

    // A turn cut because its session's slot was given back says so.
    let cut = self.cut.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&run);

    // A turn refused once its start was published still ends, failed with the refusal.
    match came {
      Ok(came) => {
        let spent = came.spent();
        let context = came.context();

        if let (Some(context), Scope::Room(_) | Scope::Agent) = (context, &scope) {
          self
            .left
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(turn.session.clone(), context);
        }

        let mut end = came.end(&run);

        end.turn = turn_id;

        let prefix = self
          .prefixes
          .lock()
          .unwrap_or_else(|poisoned| poisoned.into_inner())
          .remove(&turn.session);

        end.prefix = end.prefix.take().or(prefix);

        // Ended by that stop, whatever its program made of it: a stop is never a failure.
        if let Some(reason) = cut
          && end.outcome != Outcome::Answered
        {
          end.outcome = Outcome::Stopped;
          end.ending = Some("stopped".to_owned());
          end.reason = Some(reason);
          end.failure = None;
          end.engine_error = None;
        }

        close_turn_run(&self.ground().db, ledger, &end, spent, context).await;

        let end = self.published(&turn.session, turn.context.key.as_deref(), end);

        self.handed_back(&turn, &run, &end).await;
        Ok(end)
      }
      Err(refused) => {
        let mut end = TurnEnd::failed(&run, refused.message.clone(), None);

        end.turn = turn_id;
        close_turn_run(&self.ground().db, ledger, &end, None, None).await;

        let end = self.published(&turn.session, turn.context.key.as_deref(), end);

        self.handed_back(&turn, &run, &end).await;
        Err(refused)
      }
    }
  }

  // A turn's end handed back: kept under its caller's key, where it named one, for a repeat of its send, and
  // only then no longer kept in flight, so a process that ends between the two leaves the turn for the next
  // one's pickup rather than losing its end (docs/sessions.md, 「A restart hands the session over」).
  async fn handed_back(&self, turn: &Turn, run: &str, end: &TurnEnd) {
    let ground = self.ground();

    if let Some(key) = &turn.context.key {
      ground.turn_keys.ended(&ground.db, &turn.session.0, key, end).await;
    }

    crate::turn_pickup::forget(&ground.db, run, "its end reached its caller").await;

    // A stop this backend put to the turn is taken back now the turn is gone, so it refuses no later turn
    // that goes out under the same name.
    let stopped = self.interrupted.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(run);

    if stopped && let Err(refusal) = crate::queue::cancels::take_back(&ground.db, run).await {
      eprintln!("{run}: the stop put to it was not taken back: {}", refusal.detail);
    }
  }

  // A turn's end published, its `ending` the word the door's last turn under the send ended on where the
  // turn did not answer, else its outcome's own.
  fn published(&self, session: &SessionId, key: Option<&str>, mut end: TurnEnd) -> TurnEnd {
    // A turn stopped because its card was taken says so, whatever its program made of the stop.
    if let Some(by) = crate::cards::taken_from(&end.run) {
      end.ending = Some(crate::cards::CARD_TAKEN.to_owned());
      end.reason = Some(format!("the card was taken by {by}"));
    }

    if end.ending.is_none() {
      end.ending = Some(ending_of(&end).to_owned());
    }

    self.ground().events.publish(TurnEnded {
      session: session.0.clone(),
      key: key.map(str::to_owned),
      end: end.clone(),
    });

    end
  }

  // A state given the turn its session carries now and what that turn went out on.
  fn going_shown(&self, session: &SessionId, state: &mut State) {
    state.run = match self.going().get(session).map(Vec::as_slice) {
      Some([run]) => Some(run.run.clone()),
      _ => None,
    };
    state.went_out = self.showing().get(session).cloned();
    state.question = self.questions.shown(session);
  }

  // A job's turn carried on the seat given, what it went out on and what it writes shown to a reading of its
  // session while it goes.
  async fn carried(
    &self,
    turn: &Turn,
    agent: &str,
    seat: WorkSeat<'_>,
    work: &WorkParts,
    turn_id: i64,
  ) -> Result<Came, Fault> {
    let asked = WorkAsked {
      ground: self.ground(),
      agent,
      seat,
      work,
      job: turn.context.job,
      prompt: &turn.prompt,
      session: &turn.session,
      questions: &self.questions,
    };
    let shown = Showing {
      sessions: self,
      session: &turn.session,
      turn: turn_id,
    };

    carry(&asked, &shown).await.map(|end| Came::Ended(Box::new(end)))
  }

  // The session a run in a place of its own kept, folded where its turn left it at or past its role's
  // compaction threshold (docs/sessions.md, 「Session compaction」): one more turn on the run's own road into the
  // same session, the fold word, before the run gives its slot back. A fold that answered leaves the session
  // kept and valid with its context unread; one that did not leaves it as it stood.
  #[allow(clippy::too_many_arguments)]
  async fn fold_in_place(
    &self,
    run: &crate::lines::chat::ChatRun,
    role: &str,
    taken: &crate::road::Road,
    supervisor: Option<&str>,
    session: &str,
    share: Option<f64>,
    log: &mut RunLog,
  ) {
    let ground = self.ground();
    let held = match crate::apart_sessions::take_for_fold(&ground.db, role, session, share).await {
      Ok(Some(held)) => held,
      Ok(None) => return,
      Err(why) => {
        eprintln!("{session}: whether it is due a fold could not be read: {why:?}");
        return;
      }
    };
    let fold = crate::lines::chat::ChatRun {
      prompt: crate::worker_session::fold::FOLD_WORD.to_owned(),
      fold: true,
      resume: Some(held.clone()),
      ..run.clone()
    };
    // The fold reached the session only where it answered on that same session.
    let folded =
      crate::chat_stand_in::job_turn_on(ground, log, &fold, role, taken, supervisor, now_ms(), None)
        .await
        .is_ok_and(|reply| reply.session_id == held);

    ground.events.publish(crate::worker_session::told::SessionFolded {
      session: held.clone(),
    });

    let given = if folded {
      crate::apart_sessions::folded(&ground.db, &held, now_ms()).await
    } else {
      eprintln!("compact {role} {held}: the fold did not answer, the session stands as it was");
      crate::apart_sessions::give_back(&ground.db, &held).await
    };

    if let Err(why) = given {
      eprintln!("{session}: the session it folded was not given back: {why:?}");
    }
  }

  // One turn of a role's session in the place of its own, on a provider other than Claude Code's own or, for a
  // run of an account with a node, on any provider
  // (docs/api.md, 「sessions」): the runner line it names with the instructions its caller handed in, or else
  // the worker line opening on the platform's harness and the role's prompt, with the repository's own where
  // its folder is a repository's checkout; on the session the role keeps at that folder on that provider and
  // model where it keeps sessions and one is valid, a new one otherwise; the role's bundles and the record its
  // caller named on its token.
  #[allow(clippy::too_many_arguments)]
  async fn apart_in_place(
    &self,
    session: &SessionId,
    agent: &str,
    apart: &Apart,
    work: &WorkParts,
    prompt: &str,
    began: i64,
    job: Option<agent_session::JobMark>,
    placed: crate::person_nodes::RunPlaced,
    shown: OutputWatch<'_>,
  ) -> Result<Came, Fault> {
    let ground = self.ground();
    let trees = Trees::of(ground);
    let place = crate::apart_places::place_name(session);
    let role = apart.role.as_deref().unwrap_or_else(|| crate::agent_roles::job_role());
    let refused = |why: String| fault(FaultKind::Refused, why);
    let composed = crate::lines::role::compose(ground, &trees, apart, role, &work.mapping)
      .await
      .map_err(|why| refused(why.detail))?;
    let mut env = ground.env().clone();

    crate::apart_places::mark(ground, &place, session, placed.account.as_deref()).map_err(refused)?;

    // Placed on its account's node, the place is mirrored there from its first file on.
    if let Some(node) = &placed.node {
      crate::watches::links::mark_place_on(&ground.roots.place(&place), Some(node), &composed.cwd)
        .map_err(refused)?;

      if let Err(why) =
        crate::watches::links::write_node_spec(&ground.db, ground.peers.node.as_ref(), &ground.roots, node)
          .await
      {
        eprintln!("{place}: {node}'s link spec was not written again: {why}");
      }
    }
    env.extend(work.variables.iter().map(|(name, value)| (name.clone(), value.clone())));
    env.extend(crate::lines::token::record_variables(apart));
    env.insert(TURN_AGENT.to_owned(), agent.to_owned());

    if job.is_some() {
      env.insert(TASK_LOCKED.to_owned(), "1".to_owned());
    }

    // The key of the command the turn goes out under: a session its caller names, opened under another, is put
    // back to the caller before anything goes in, its prefix having moved (docs/sessions.md, 「The handoff a job's
    // session leaves」).
    let prefix = crate::apart_sessions::key_of(
      composed.line,
      &composed.cwd,
      &composed.system_path,
      &composed.mcp_config,
      &composed.tools_allowed,
    );

    if apart.resume.is_some() && apart.resume_prefix.as_ref().is_some_and(|held| *held != prefix) {
      return Ok(Came::Back);
    }

    self
      .prefixes
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .insert(session.clone(), prefix);

    // The session the role keeps at this folder on this provider and model, where it keeps sessions.
    let key = if crate::apart_sessions::setting(&ground.db, role, &composed.cwd).await.keeps() {
      Some(crate::apart_sessions::SessionKey {
        agent: crate::apart_sessions::agent_at(&ground.db, role, &composed.cwd).await,
        role: role.to_owned(),
        path: composed.cwd.clone(),
        provider: apart.provider.clone(),
        model: apart.model.clone(),
        effort: composed.effort.clone(),
        account: None,
        command: crate::apart_sessions::key_of(
          composed.line,
          &composed.cwd,
          &composed.system_path,
          &composed.mcp_config,
          &composed.tools_allowed,
        ),
      })
    } else {
      None
    };
    let resume = match (&apart.resume, &key) {
      (Some(named), _) => Some(named.clone()),
      (None, Some(key)) => {
        crate::apart_sessions::take(&ground.db, key, now_ms()).await.unwrap_or_else(|why| {
          eprintln!("{session}: its kept session could not be read, so it opens a new one: {why:?}");
          None
        })
      }
      (None, None) => None,
    };
    let chat_run = composed.run(crate::lines::role::Turned {
      place: place.clone(),
      model: apart.model.clone(),
      prompt: prompt.to_owned(),
      resume: resume.clone(),
      env,
      cards: crate::cards::Ask {
        own: apart.demand,
        within: apart.within,
        terms: crate::cards::terms(apart.card_rank, apart.card_pool.clone()),
      },
    });
    let mut log = RunLog::new(Target {
      agent: agent.to_owned(),
      run: work.name.clone(),
      chat_id: (work.chat != 0).then_some(work.chat),
      job_id: job.map(|job| job.job),
    });
    let taken = crate::road::Road {
      provider: apart.provider.clone(),
      model: apart.model.clone(),
      account: None,
    };
    let opened_at = now_ms();
    let supervisor = apart.asked_by.as_deref();
    let came = crate::chat_stand_in::job_turn_on(
      ground,
      &mut log,
      &chat_run,
      role,
      &taken,
      supervisor,
      began,
      Some(shown),
    )
    .await;

    if let Some(key) = &key {
      let kept = match &came {
        Ok(reply) if !reply.session_id.is_empty() => {
          let held = crate::apart_sessions::Answered {
            at: now_ms(),
            opened_at,
            context_tokens: reply.context_tokens,
            context_share: reply.context_ratio,
          };

          crate::apart_sessions::keep(&ground.db, key, &reply.session_id, held).await
        }
        Ok(_) => Ok(()),
        Err(_) => match &resume {
          Some(held) => crate::apart_sessions::give_back(&ground.db, held).await,
          None => Ok(()),
        },
      };

      if let Err(why) = kept {
        eprintln!("{session}: the session it ran on was not kept: {why:?}");
      }

      if let Ok(reply) = &came
        && !reply.session_id.is_empty()
      {
        self
          .fold_in_place(
            &chat_run,
            role,
            &taken,
            supervisor,
            &reply.session_id,
            reply.context_ratio,
            &mut log,
          )
          .await;
      }
    }

    match came {
      Ok(reply) => Ok(Came::Answered(
        reply.text.clone(),
        spent_of(&reply.spend),
        left(&reply),
        reply.answers.clone(),
      )),
      Err(NoAnswer::Overtaken | NoAnswer::Stopped(_)) => Ok(Came::Stopped),
      Err(no) => {
        let (reason, engine) = failure_of(no);

        Ok(Came::Failed(reason, engine))
      }
    }
  }

  // An agent's session across rooms folded in the program standing on it (docs/sessions.md, 「Session
  // compaction」): the fold word put in on the command that program stands on, so that nothing of the turn
  // restarts it or moves the session, under the lock every turn of that program holds. An inbox program takes
  // no command and folds itself.
  async fn fold_resident(&self, agent: &str, session: &SessionId) -> Result<(), Fault> {
    let ground = self.ground();
    let supervisor = crate::supervisors::named(&ground.db, agent)
      .await
      .map_err(|why| store_fault(why.into()))?;
    let (provider, model) = supervisor
      .as_ref()
      .and_then(road_of)
      .unwrap_or_else(|| (CHAT_PROVIDER.to_owned(), None));

    if let Some(refused) = folds_itself(session, crate::chat_stand_in::inbox_declared(&provider)) {
      return Err(refused);
    }

    let Some(held) = agent_session(&ground.db, agent).await.map_err(store_fault)? else {
      return Err(fault(FaultKind::Unknown, format!("{session} has not been opened")));
    };
    let Some(standing) = crate::lines::place::Place::of(&ground.roots.data, agent).program() else {
      return Err(fault(FaultKind::Refused, format!("{session} has no program standing to fold it")));
    };

    if crate::door::endpoint::session_named(&standing.command.args) != Some(held.as_str()) {
      return Err(fault(
        FaultKind::Refused,
        format!("{session}: its program stands on another session than {held}"),
      ));
    }

    let began = now_ms();
    let mut command = standing.command;

    // The token its place handed it is the place's own to keep: the program is known by the command without it.
    command.env.remove(crate::lines::token::TOKEN_VARIABLE);

    let composed = ComposedTurn {
      system_path: PathBuf::new(),
      command,
      session: held.clone(),
      prompt: crate::worker_session::fold::FOLD_WORD.to_owned(),
      model: model.unwrap_or_default(),
      effort: None,
      placement: None,
      window: None,
      account: None,
      began,
      run_env: None,
      given: None,
    };
    let mut log = RunLog::new(Target {
      agent: agent.to_owned(),
      run: format!("bmo-fold-{agent}-{began}"),
      chat_id: None,
      job_id: None,
    });
    let came = run_turn(
      ground,
      agent,
      crate::lines::chat::LINE,
      &provider,
      &composed,
      None,
      &mut log,
      false,
      None,
      None,
    )
    .await;
    let told = |ground: &Ground| {
      ground.events.publish(crate::worker_session::told::SessionFolded {
        session: held.clone(),
      });
    };

    match came {
      Ok(_) => {
        println!("compact {agent} {held}: compacted");
        told(ground);
        self.forget_left(session);

        Ok(())
      }
      Err(no) => {
        // Only a fold whose end could not be read may have reached the session.
        if matches!(no, NoAnswer::Failed(_) | NoAnswer::Cut(_)) {
          told(ground);
        }

        let (reason, _) = failure_of(no);

        eprintln!("compact {agent} {held}: failed — {reason}");

        Err(fault(FaultKind::Refused, format!("{session} could not be folded: {reason}")))
      }
    }
  }

  // A worker's latest session record, the one a fold or a reading of its state goes by.
  async fn latest_worker(&self, worker: &str) -> Result<Option<records::WorkerSession>, Fault> {
    Ok(records::sessions(&self.ground().db, worker).await.map_err(store_fault)?.pop())
  }
}

impl Sessions for HerdrSessions {
  async fn start(&self, request: Start) -> Result<Session, Fault> {
    if matches!(&request.scope, Scope::Apart(_) | Scope::Conversation(_))
      && (request.agent.trim().is_empty() || request.agent.contains('@'))
    {
      return Err(fault(FaultKind::Refused, format!("{} is no name a turn runs under", request.agent)));
    }

    self.admitted(&request.agent, &request.scope).await?;

    let (id, scope) = match &request.scope {
      Scope::Apart(apart) => {
        let mut apart = apart.clone();

        // A session naming no folder goes out in its line's own, the worker line's where it names no runner
        // line: a folder by its absolute path, which is how the program keeps its trust and its records.
        if apart.cwd.trim().is_empty() {
          let line = apart.line.as_deref().unwrap_or(crate::lines::worker::LINE);
          let folder = Trees::of(self.ground()).agents.join(line);

          std::fs::create_dir_all(&folder)
            .map_err(|why| fault(FaultKind::Refused, format!("{}: {why}", folder.display())))?;
          apart.cwd = folder.to_string_lossy().into_owned();
        }

        // A number whose place still stands, kept for a turn the process before left in flight, is passed
        // over, so no new session goes into it.
        let data = &self.ground().roots.data;
        let id = loop {
          let id = apart_id(&request.agent, self.numbered.fetch_add(1, Ordering::Relaxed));

          if !crate::apart_places::is_apart_place(data, &crate::apart_places::place_name(&id)) {
            break id;
          }
        };

        self.apart().insert(id.clone(), (request.agent.clone(), apart.clone()));
        (id, Scope::Apart(apart))
      }
      Scope::Conversation(conversation) => {
        let mut conversation = conversation.clone();

        if conversation.cwd.trim().is_empty() {
          conversation.cwd = self.ground().source.to_string_lossy().into_owned();
        }

        let id = conversation_id(&request.agent, self.numbered.fetch_add(1, Ordering::Relaxed));

        self.conversations().insert(
          id.clone(),
          (
            request.agent.clone(),
            HeldConversation {
              conversation: conversation.clone(),
              harness: None,
            },
          ),
        );
        (id, Scope::Conversation(conversation))
      }
      scope => (
        id_of(&request.agent, scope).ok_or_else(|| {
          fault(FaultKind::Refused, format!("{}: its scope names no session", request.agent))
        })?,
        scope.clone(),
      ),
    };

    Ok(Session {
      id,
      agent: request.agent,
      scope,
    })
  }

  async fn switch(
    &self,
    session: &SessionId,
    provider: Option<&str>,
    model: Option<&str>,
  ) -> Result<Session, Fault> {
    let (agent, scope) = self.scope_held(session)?;

    if self.in_turn(session) {
      return Err(fault(FaultKind::Busy, format!("{session} is in a turn")));
    }

    match scope {
      Scope::Conversation(conversation) => {
        let provider = provider.map(str::trim).filter(|provider| !provider.is_empty());
        let model = model.map(str::trim).filter(|model| !model.is_empty());
        let next = Conversation {
          provider: provider.unwrap_or(&conversation.provider).to_owned(),
          model: model.unwrap_or(&conversation.model).to_owned(),
          cwd: conversation.cwd.clone(),
        };

        self.provider_ready(&next.provider).await?;

        let moved = next.provider != conversation.provider || next.model != conversation.model;

        if let Some((_, held)) = self.conversations().get_mut(session) {
          held.conversation = next.clone();

          if moved {
            held.harness = None;
          }
        }

        Ok(Session {
          id: session.clone(),
          agent,
          scope: Scope::Conversation(next),
        })
      }
      Scope::Apart(apart) => {
        let provider = provider.map(str::trim).filter(|provider| !provider.is_empty());
        let model = model.map(str::trim).filter(|model| !model.is_empty());
        let next = Apart {
          provider: provider.unwrap_or(&apart.provider).to_owned(),
          model: model.unwrap_or(&apart.model).to_owned(),
          ..apart.clone()
        };

        self.provider_ready(&next.provider).await?;

        if let Some((_, held)) = self.apart().get_mut(session) {
          *held = next.clone();
        }

        Ok(Session {
          id: session.clone(),
          agent,
          scope: Scope::Apart(next),
        })
      }
      Scope::Worker => Err(fault(
        FaultKind::Refused,
        format!("{session} is handed over on its change route, not switched"),
      )),
      Scope::Agent | Scope::Room(_) => Err(fault(
        FaultKind::Refused,
        format!("{session} goes out on the provider and model its agent's row names"),
      )),
    }
  }

  async fn push(&self, session: &SessionId, turn: i64, words: &str) -> Result<Aside, Fault> {
    let (agent, scope) = self.scope_held(session)?;

    self.admitted(&agent, &scope).await?;

    // The turn the words name must be the one the session carries now.
    let run = match self.going().get(session).map(Vec::as_slice) {
      Some([run]) if run.turn == turn => run.run.clone(),
      Some(_) => {
        return Err(fault(FaultKind::UnknownTurn, format!("no turn {turn} is going in {session}")));
      }
      None => {
        return Err(fault(FaultKind::UnknownTurn, format!("no turn {turn} is going in {session}")));
      }
    };

    // A job's turn that hears no words takes none.
    if self.deaf().contains(session) {
      return Ok(Aside::Refused);
    }
    let Some(mut offer) = aside::offer(&run, words) else {
      return Ok(Aside::Refused);
    };
    let said = |outcome: AsideOutcome| match outcome {
      AsideOutcome::Delivered => Aside::Delivered,
      AsideOutcome::Waiting => Aside::Waiting,
      AsideOutcome::Refused => Aside::Refused,
    };
    // The offer stands until a look takes it or the turn ends, which answers every offer it did not take.
    Ok((&mut offer.answer).await.map_or(Aside::Refused, said))
  }

  // The door's ending of every turn the send puts through a program's pane is kept for its end
  // (docs/api.md, 「sessions」, a turn's `ending`).
  async fn send_turn(&self, turn: Turn) -> Result<TurnEnd, Fault> {
    crate::door::liveness::keeping_endings(self.sent(turn)).await
  }

  async fn read_state(&self, session: &SessionId, model: Option<&str>) -> Result<State, Fault> {
    // A thrown-away session does not outlive the process that started it, but one carrying a picked-up turn
    // stands until that turn ends: its state answers by id meanwhile (docs/sessions.md, 「A restart hands the
    // session over」).
    let (agent, scope) = match self.scope_held(session) {
      Ok(held) => held,
      Err(fault) => match carried_scope_of(self.ground(), session) {
        Some((agent, scope)) => (agent, Scope::Apart(scope)),
        None => return Err(fault),
      },
    };

    self.admitted(&agent, &scope).await?;

    let ground = self.ground();
    let place = crate::lines::place::place_of(&ground.roots.data, &agent).to_string_lossy().into_owned();

    match &scope {
      // A room's session is readable from its start, before its first turn opens a generation: it stands
      // then on the provider and model its agent's row names, at generation 0.
      Scope::Room(room) => {
        let held = sessions::session(&ground.db, &agent, room_number(room)?).await.map_err(store_fault)?;
        let road = held.as_ref().and_then(|held| held.harness.as_ref()).map(|harness| &harness.road);
        let (provider, model) = match road.and_then(|road| road.split_once("::")) {
          Some((provider, model)) => (provider.to_owned(), model.to_owned()),
          None => match pinned(ground, &agent).await? {
            Some(pinned) => pinned
              .supervisor
              .road
              .as_ref()
              .map(|road| (road.provider.clone(), road.model.clone()))
              .unwrap_or_default(),
            None => Default::default(),
          },
        };

        let context = self.resident_context(&agent, &provider).or_else(|| self.left_of(session));

        Ok(State::standing(
          session.clone(),
          provider,
          model,
          place,
          u64::try_from(held.as_ref().map_or(0, |held| held.generation)).unwrap_or_default(),
          context,
        ))
      }
      Scope::Agent => {
        // A supervisor whose turns go through its resident's inbox holds no session id of its own: its session
        // stands from its start on its one road, at its resident's context.
        let held = crate::supervisors::named(&ground.db, &agent)
          .await
          .map_err(|why| store_fault(why.into()))?;
        let road = held.as_ref().and_then(road_of);

        if let Some((provider, model)) =
          road.clone().filter(|(provider, _)| crate::chat_stand_in::inbox_declared(provider))
        {
          let model = model.unwrap_or_default();
          let context = self.resident_context(&agent, &provider).or_else(|| self.left_of(session));

          return Ok(State::standing(session.clone(), provider, model, place, 1, context));
        }

        if agent_session(&ground.db, &agent).await.map_err(store_fault)?.is_none() {
          return Err(fault(FaultKind::Unknown, format!("{session} has not been opened")));
        }

        // Its session names its one road, never what another model once answered on.
        let (provider, model) = road
          .map(|(provider, model)| (provider, model.unwrap_or_default()))
          .unwrap_or_else(|| (CHAT_PROVIDER.to_owned(), String::new()));

        Ok(State::standing(session.clone(), provider, model, place, 1, self.left_of(session)))
      }
      Scope::Worker => {
        let Some(held) = self.latest_worker(&agent).await? else {
          return Err(fault(FaultKind::Unknown, format!("{session} has not been opened")));
        };
        let generation = records::generation(&ground.db, &agent, &held.model).await.map_err(store_fault)?;
        // The road the worker stands on: the provider it is pinned to, else the one its held session last
        // answered on, else Claude Code's own.
        let provider = match crate::queue::routes::worker_pin(&ground.db, &agent).await {
          Ok(Some((provider, _))) => provider,
          Ok(None) => {
            crate::worker_session::held::held(ground, &crate::worker_session::held::scope_of(&agent))
              .await
              .ok()
              .flatten()
              .map(|held| held.mode)
              .filter(|mode| !mode.is_empty())
              .unwrap_or_else(|| worker_session::PROVIDER.to_owned())
          }
          Err(why) => return Err(fault(FaultKind::Refused, why.0)),
        };
        let mut state = State::standing(
          session.clone(),
          provider,
          held.answered_model.clone().unwrap_or_else(|| held.model.clone()),
          place,
          generation,
          carried_of(held.context_tokens, held.context_ratio),
        );

        self.going_shown(session, &mut state);
        state.continues = match model {
          Some(model) => continues(ground, &agent, model).await.map_err(store_fault)?,
          None => None,
        };

        Ok(state)
      }
      Scope::Apart(apart) => {
        let mut state = State::standing(
          session.clone(),
          apart.provider.clone(),
          apart.model.clone(),
          apart.cwd.clone(),
          1,
          None,
        );

        self.going_shown(session, &mut state);
        Ok(state)
      }
      Scope::Conversation(conversation) => {
        let mut state = State::standing(
          session.clone(),
          conversation.provider.clone(),
          conversation.model.clone(),
          conversation.cwd.clone(),
          1,
          None,
        );

        self.going_shown(session, &mut state);
        state.continues = self.held_harness(session);
        Ok(state)
      }
    }
  }

  // A worker's and a thrown-away session's turns put their questions to their caller; every other scope's
  // leaves them standing for a person, and takes no answer.
  async fn answer(&self, session: &SessionId, question: &str, answer: Answer) -> Result<(), Fault> {
    let (agent, scope) = self.scope_held(session)?;

    self.admitted(&agent, &scope).await?;

    match scope {
      Scope::Worker | Scope::Apart(_) => self.questions.answer(session, question, answer),
      _ => Err(fault(
        FaultKind::Unknown,
        format!("{session} puts no question to its caller, so it stands on no question {question}"),
      )),
    }
  }

  // Every scope's turns keep what they write while they go, and a turn that has ended keeps its words
  // until the session's next turn begins.
  async fn output(&self, session: &SessionId, turn: Option<i64>) -> Result<Output, Fault> {
    // A thrown-away session carrying a turn picked up at the start stands until that turn ends, and its turn
    // is read by its id meanwhile (docs/sessions.md, 「A restart hands the session over」).
    // Once that turn's end is handed back its session is gone, and what it wrote still answers its id.
    let (agent, scope) = match self.scope_held(session) {
      Ok(held) => held,
      Err(fault) => match carried_scope_of(self.ground(), session) {
        Some((agent, scope)) => (agent, Scope::Apart(scope)),
        None => return self.outputs.read(session, turn).filter(|_| turn.is_some()).ok_or(fault),
      },
    };

    self.admitted(&agent, &scope).await?;

    self.outputs.read(session, turn).ok_or_else(|| match turn {
      Some(turn) => fault(FaultKind::UnknownTurn, format!("{session} has carried no turn {turn}")),
      None => fault(FaultKind::Unknown, format!("{session} has carried no turn to read")),
    })
  }

  async fn interrupt(&self, session: &SessionId, turn: i64) -> Result<(), Fault> {
    // A turn that lent its session to the receiver's handoff stands parked while it waits, and is cut there too.
    let mut runs = self.going().get(session).cloned().unwrap_or_default();

    runs.extend(self.parked().get(session).cloned().unwrap_or_default());

    let Some(running) = runs.into_iter().find(|running| running.turn == turn) else {
      return Err(fault(FaultKind::UnknownTurn, format!("no turn {turn} is going in {session}")));
    };
    let stops = crate::queue::stops::Stops::of(self.ground(), None);

    // Named before the stop is written, so a turn that ends while it is written still takes it back.
    self
      .interrupted
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .insert(running.run.clone());
    stops.cancel(&running.run).await.map_err(store_fault)?;
    Ok(())
  }

  async fn fold(&self, session: &SessionId) -> Result<(), Fault> {
    let (agent, scope) = self.scope_held(session)?;
    let row = self.admitted(&agent, &scope).await?;
    let ground = self.ground();

    if self.in_turn(session) {
      return Err(fault(FaultKind::Busy, format!("{session} is in a turn")));
    }

    match &scope {
      Scope::Room(room) => {
        let chat_id = room_number(room)?;
        let Some(pinned) = pinned(ground, &agent).await? else {
          return Err(fault(FaultKind::Unknown, format!("no agent is named {agent}")));
        };
        let (provider, model) =
          choice_of(ground, &pinned).await.map_err(|why| fault(FaultKind::Refused, why))?;
        let held = sessions::session(&ground.db, &agent, chat_id).await.map_err(store_fault)?;
        let Some(harness) = held.and_then(|held| held.harness) else {
          return Err(fault(FaultKind::Unknown, format!("{session} holds no harness session to fold")));
        };
        let settings = Settings::of(&ground.db, &agent)
          .await
          .map_err(|refused| fault(FaultKind::Refused, refused.error))?;
        let system = system_of(ground, &pinned, &settings, provider, &model)
          .await
          .map_err(|why| fault(FaultKind::Refused, why))?;
        let due = Due {
          provider,
          model,
          system,
          generation: generation_of(&settings),
          session: harness.id,
          placement: harness.placement,
          ratio: 1.0,
          why: Why::Limit,
        };

        hand_over(ground, &self.places.kept.reset, &pinned.speaker, chat_id, due).await;
        self.forget_left(session);
        Ok(())
      }
      Scope::Agent => self.fold_resident(&agent, session).await,
      Scope::Worker => {
        let (Some(row), Some(held)) = (row, self.latest_worker(&agent).await?) else {
          return Err(fault(FaultKind::Unknown, format!("{session} has not been opened")));
        };

        match fold(ground, &row, &held.model, &held.account, &held.session_id).await {
          Folded::Compacted => {
            self.forget_left(session);
            Ok(())
          }
          Folded::TooShort => Ok(()),
          Folded::Busy => Err(fault(FaultKind::Busy, format!("a job holds {agent}'s checkout"))),
          Folded::Failed => Err(fault(FaultKind::Refused, format!("{session} could not be folded"))),
        }
      }
      Scope::Apart(_) => Err(fault(
        FaultKind::Refused,
        format!("{session} is an apart session, whose turns each go out as a run of their own"),
      )),
      Scope::Conversation(_) => Err(fault(
        FaultKind::Refused,
        format!("{session} is a conversation, whose road compacts it inside its own turns"),
      )),
    }
  }

  async fn end(&self, session: &SessionId) -> Result<(), Fault> {
    let (agent, scope) = self.scope_held(session)?;

    self.admitted(&agent, &scope).await?;

    if self.in_turn(session) {
      return Err(fault(FaultKind::Busy, format!("{session} is in a turn")));
    }

    self.forget_left(session);

    let ground = self.ground();

    match &scope {
      Scope::Room(room) => {
        let chat_id = room_number(room)?;
        let held = sessions::session(&ground.db, &agent, chat_id).await.map_err(store_fault)?;

        if let Some(harness) = held.and_then(|held| held.harness) {
          let_go(ground, &agent, &[harness.id]).await;
          sessions::open_generation(&ground.db, &agent, chat_id).await.map_err(store_fault)?;
        }
      }
      Scope::Agent => {
        if let Some(held) = agent_session(&ground.db, &agent).await.map_err(store_fault)? {
          let_go(ground, &agent, &[held]).await;
          keep_agent_session(&ground.db, &agent, None).await.map_err(store_fault)?;
        }
      }
      Scope::Worker => {
        let place = crate::lines::place::Place::of(&ground.roots.data, &agent);

        place.release().await;

        for held in records::sessions(&ground.db, &agent).await.map_err(store_fault)? {
          records::forget(&ground.db, &agent, &held.model, Some(&held.account))
            .await
            .map_err(store_fault)?;
        }
      }
      Scope::Apart(apart) => {
        self.apart().remove(session);

        // A session in a place of its own ends it.
        if in_places(apart) {
          crate::apart_places::end(ground, &crate::apart_places::place_name(session)).await;
        }

        // A lease bound to the session ends with it (docs/role-slots.md, 「The line」).
        if let Err(why) = crate::order_run::dispatch::release_sessions(
          &ground.db,
          &ground.events,
          std::slice::from_ref(&session.0),
          "its role session ended",
        )
        .await
        {
          eprintln!("{session}: the slot lease bound to it was not given back: {why}");
        }
      }
      Scope::Conversation(_) => {
        let harness = self.conversations().remove(session).and_then(|(_, held)| held.harness);

        if let Some(harness) = harness {
          let_go(ground, &agent, &[harness]).await;
        }
      }
    }

    // A hold the session took for a turn that never started its job ends with the session (docs/api.md,
    // 「assignments and jobs」): a hold is never for a clock, so only an observed end lets it go. A room
    // session's harness may have named the agent itself rather than the session's id.
    let mut holders = vec![session.0.as_str()];

    if matches!(scope, Scope::Room(_)) && agent.as_str() != session.0.as_str() {
      holders.push(agent.as_str());
    }

    for holder in holders {
      if let Err(why) = crate::control::work::release_held(&ground.db, holder).await {
        eprintln!("{session}: the holds it took were not released: {}", why.error);
      }
    }

    self.outputs.forget(session);

    Ok(())
  }

  async fn list(&self) -> Result<Vec<Session>, Fault> {
    let db = &self.ground().db;
    let mut sessions = vec![];
    let rooms = all(db, "SELECT agent, chat_id FROM room_sessions ORDER BY agent, chat_id", vec![])
      .await
      .map_err(|why| store_fault(why.into()))?;

    for row in rooms {
      let agent: String = row.try_get("", "agent").map_err(|why| store_fault(why.into()))?;
      let chat_id: i64 = row.try_get("", "chat_id").map_err(|why| store_fault(why.into()))?;

      sessions.extend(listed(agent, Scope::Room(chat_id.to_string())));
    }

    let across = all(
      db,
      r#"SELECT "name" FROM "supervisors" WHERE "session_scope" = 'agent' AND "session_id" IS NOT NULL ORDER BY "name""#,
      vec![],
    )
    .await
    .map_err(|why| store_fault(why.into()))?;

    for row in across {
      let agent: String = row.try_get("", "name").map_err(|why| store_fault(why.into()))?;

      sessions.extend(listed(agent, Scope::Agent));
    }

    for (id, (agent, apart)) in self.apart().iter() {
      sessions.push(Session {
        id: id.clone(),
        agent: agent.clone(),
        scope: Scope::Apart(apart.clone()),
      });
    }

    for (id, (agent, held)) in self.conversations().iter() {
      sessions.push(Session {
        id: id.clone(),
        agent: agent.clone(),
        scope: Scope::Conversation(held.conversation.clone()),
      });
    }

    Ok(sessions)
  }
}

/// The refusal of a fold of a session across rooms whose program takes no command and folds itself, none where
/// the word reaches the program.
pub fn folds_itself(session: &SessionId, inbox: bool) -> Option<Fault> {
  inbox.then(|| {
    fault(
      FaultKind::Refused,
      format!("{session} is carried past its window by its program inside its own turns"),
    )
  })
}

// The road a supervisor's session across rooms stands on: its one road, with the model it names.
fn road_of(supervisor: &crate::supervisors::Supervisor) -> Option<(String, Option<String>)> {
  supervisor.road.as_ref().map(|road| (road.provider.clone(), Some(road.model.clone())))
}

// A turn across rooms as the chat line launches it: the room's block, the road and the base persona its
// caller handed in for where the agent's brain gives none, on the session named.
// The road the supervisor's session last answered on, where its row now names another: its provider and model as
// the row keeps its last answer. None where they are one, or the row keeps no last answer.
async fn answered_road(db: &sea_orm::DatabaseConnection, agent: &str) -> Option<crate::road::Road> {
  let held = crate::supervisors::named(db, agent).await.ok().flatten()?;
  let provider = held.last_mode.filter(|mode| !mode.is_empty())?;
  let model = held.last_model.filter(|model| !model.is_empty())?;
  let names = held.road.as_ref().map(|road| (road.provider.as_str(), road.model.as_str()));

  (names != Some((provider.as_str(), model.as_str()))).then_some(crate::road::Road {
    provider,
    model,
    account: None,
  })
}

// Resolves once a stop stands over the run, looked for at the run-stop policy's pace; never where none comes.
async fn stop_asked(ground: &Ground, run: &str) {
  let look = Duration::from_millis(policy::run_stop::RUN_STOP.interrupt_look_ms.max(500));

  loop {
    if crate::door::execution::cancelled(&ground.db, &[run]).await.unwrap_or(false) {
      return;
    }

    tokio::time::sleep(look).await;
  }
}

// A turn as a launch that names the model and folder where the caller chose them rather than the agent's
// own.
fn launch_on<'a>(
  agent: &'a str,
  room: &RoomBlock,
  context: &'a TurnContext,
  resume: Option<String>,
  model: Option<&'a str>,
  cwd: Option<&'a str>,
) -> ChatLaunch<'a> {
  ChatLaunch {
    agent,
    chat_id: room.chat_id,
    room: RoomContext {
      chat_id: room.chat_id,
      text: room.text.clone(),
      mark: room.mark.clone(),
    },
    via: context.via.as_deref(),
    resume,
    privileged: context.privileged,
    ping: false,
    base_persona: context.base_persona.as_deref(),
    model,
    cwd,
  }
}

/// The backend shared by the routes that answer the contract.
pub type Shared = Arc<HerdrSessions>;

// What a turn sent to a `<repository>@worker` address is answered: no session stands there.
fn no_worker_scope(session: &SessionId) -> Fault {
  fault(
    FaultKind::Unknown,
    format!("{session} is no session: {}", crate::control::session_ops::WORKER_RETIRED),
  )
}

// The worker's own session, as the contract names it.
fn worker_session_id(worker: &str) -> Result<SessionId, Fault> {
  id_of(worker, &Scope::Worker)
    .ok_or_else(|| fault(FaultKind::Unknown, format!("{worker} names no worker's session")))
}

// The run of a turn agency carries, as the ledger opens it: the name its log lines are written under, and the
// room it runs for and its title, each where the turn names one.
struct CarriedRun {
  unit: String,
  room: Option<i64>,
  title: Option<String>,
}

impl CarriedRun {
  // Opened in the ledger (docs/run-records.md, 「The run ledger」) with the round its context names and the
  // latest message it answers. A run the ledger would not take is said and never fails the turn.
  async fn open(
    self,
    db: &sea_orm::DatabaseConnection,
    agent: &str,
    context: &TurnContext,
    at: i64,
  ) -> Option<i64> {
    let opening = run_ledger::Opening {
      agent: agent.to_owned(),
      chat_id: self.room,
      work_unit: self.unit.clone(),
      depth: 0,
      cap_ms: None,
      started_at: at,
      started_by: None,
      search_id: None,
      round: context.round.clone(),
      message_id: context.answers.iter().max().copied(),
      title: self.title,
      carried: true,
    };

    match run_ledger::open(db, &opening).await {
      Ok(id) => Some(id),
      Err(why) => {
        eprintln!("{}: the turn's run could not be written to the ledger: {}", self.unit, why.detail);
        None
      }
    }
  }
}

// What a job's turn spent, each figure summed over the models that answered it; none where it reported none.
pub fn turn_spent(end: &TurnEnd) -> Option<run_ledger::Spent> {
  let spend = &end.reply.as_ref()?.spend;

  (!spend.is_empty()).then(|| {
    spend.iter().fold(run_ledger::Spent::default(), |sum, one| run_ledger::Spent {
      input_tokens: sum.input_tokens + one.input_tokens,
      output_tokens: sum.output_tokens + one.output_tokens,
      cache_read_tokens: sum.cache_read_tokens + one.cache_read_tokens,
      cache_write_tokens: sum.cache_write_tokens + one.cache_write_tokens,
      cost_micros: sum.cost_micros + crate::supply::reservations::micros(one.cost.unwrap_or_default()),
    })
  })
}

/// The run of a turn agency carries closed as its turn ended: how it ended, the tokens its context carried —
/// its answer's, else the context the turn left — and what it spent.
pub async fn close_turn_run(
  db: &sea_orm::DatabaseConnection,
  ledger: Option<i64>,
  end: &TurnEnd,
  spent: Option<run_ledger::Spent>,
  left: Option<Context>,
) {
  let Some(id) = ledger else {
    return;
  };
  let closing = run_ledger::Closing {
    finished_at: now_ms(),
    outcome: serde_json::to_value(end.outcome)
      .ok()
      .and_then(|outcome| outcome.as_str().map(str::to_owned))
      .unwrap_or_else(|| "failed".to_owned()),
    context_tokens: end
      .reply
      .as_ref()
      .and_then(|reply| reply.context_tokens)
      .or_else(|| left.and_then(|left| i64::try_from(left.carried).ok())),
    spent,
  };

  if let Err(run_ledger::NotClosed::Store(why)) = run_ledger::close(db, id, &closing).await {
    eprintln!("{}: the turn's run could not be closed in the ledger: {}", end.run, why.detail);
  }
}

// The cause the receiver's room line names for an owed replacement of the one session across rooms, by the reason
// it was owed for (docs/sessions.md, 「A restart hands the session over」, what the ask carries).
fn cause_of(why: &str) -> Cause {
  if why == HANDOFF.reasons.moved {
    Cause::Documents
  } else if why == HANDOFF.reasons.road {
    Cause::Road
  } else if why == HANDOFF.reasons.reset {
    Cause::Reset
  } else if why == HANDOFF.reasons.lost {
    Cause::Other
  } else {
    Cause::Road
  }
}

// Why a room's session hands over for the reset waiting for it: clean where the reset asks it so
// (docs/session-resets.md, 「What a reset is」).
async fn reset_why(db: &sea_orm::DatabaseConnection, agent: &str, chat_id: i64) -> Why {
  match sessions::waiting_reset(db, agent, Some(chat_id)).await {
    Ok(Some(reset)) if reset.clean => Why::CleanReset,
    _ => Why::Reset,
  }
}
