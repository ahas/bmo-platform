//! The agency API's bodies and members as Rust types.
//!
//! Every route of `docs/openapi.json` names one of these in its request or answer; the document is
//! generated from them.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One of the values the API accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum PromptDocumentKind {
  #[serde(rename = "base")]
  Base,
  #[serde(rename = "rulebook")]
  Rulebook,
  #[serde(rename = "persona")]
  Persona,
  #[serde(rename = "agent")]
  Agent,
  #[serde(rename = "system")]
  System,
}

/// One of the values the API accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum SessionScope {
  #[serde(rename = "room")]
  Room,
  #[serde(rename = "agent")]
  Agent,
  #[serde(rename = "role")]
  Role,
  #[serde(rename = "conversation")]
  Conversation,
}

/// One of the values the API accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum TurnState {
  #[serde(rename = "running")]
  Running,
  #[serde(rename = "ended")]
  Ended,
  #[serde(rename = "interrupted")]
  Interrupted,
}

/// One of the values the API accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum TurnInputSaid {
  #[serde(rename = "delivered")]
  Delivered,
  #[serde(rename = "waiting")]
  Waiting,
  #[serde(rename = "refused")]
  Refused,
}

/// One of the values the API accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum RunState {
  #[serde(rename = "open")]
  Open,
  #[serde(rename = "launched")]
  Launched,
  #[serde(rename = "stopping")]
  Stopping,
  #[serde(rename = "closed")]
  Closed,
}

/// One of the values the API accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum OvertimeRequestVerdict {
  #[serde(rename = "granted")]
  Granted,
  #[serde(rename = "refused")]
  Refused,
  #[serde(rename = "None")]
  None,
}

/// One of the values the API accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum SeatRequestState {
  #[serde(rename = "waiting")]
  Waiting,
  #[serde(rename = "reserved")]
  Reserved,
  #[serde(rename = "active")]
  Active,
  #[serde(rename = "released")]
  Released,
  #[serde(rename = "cancelled")]
  Cancelled,
}

/// One of the values the API accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ReservationState {
  #[serde(rename = "held")]
  Held,
  #[serde(rename = "released")]
  Released,
}

/// One of the values the API accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum PlaceState {
  #[serde(rename = "taken")]
  Taken,
  #[serde(rename = "prepared")]
  Prepared,
  #[serde(rename = "remembered")]
  Remembered,
  #[serde(rename = "retired")]
  Retired,
}

/// One of the values the API accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum PinCheckKind {
  #[serde(rename = "auto")]
  Auto,
  #[serde(rename = "provider")]
  Provider,
  #[serde(rename = "model")]
  Model,
}

/// One of the values the API accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AssignmentState {
  #[serde(rename = "held")]
  Held,
  #[serde(rename = "running")]
  Running,
  #[serde(rename = "released")]
  Released,
}

/// Where a job stands: launched, its work's turn going, its report under review, finished, or cut short.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum JobState {
  #[serde(rename = "starting")]
  Starting,
  #[serde(rename = "running")]
  Running,
  #[serde(rename = "reviewing")]
  Reviewing,
  #[serde(rename = "ended")]
  Ended,
  #[serde(rename = "stopped")]
  Stopped,
}

/// The state a change takes a job to: the two that end it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum JobEnd {
  #[serde(rename = "ended")]
  Ended,
  #[serde(rename = "stopped")]
  Stopped,
}

/// What chose a job's provider and model: its item's `model` label, its repository's pin or its role's
/// agents in their order (`order`). `forced` stands on a job row that holds it, and no job is given it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum JobChoice {
  #[serde(rename = "item")]
  Item,
  #[serde(rename = "forced")]
  Forced,
  #[serde(rename = "pin")]
  Pin,
  #[serde(rename = "order")]
  Order,
}

/// Error:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename = "Error")]
pub struct ApiError {
  pub code: String,
  pub error: String,
}

/// Health:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Health {
  pub ready: bool,
  pub reason: Option<String>,
  pub version: String,
}

/// Quiet: the work the server holds that the next server could not pick up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Quiet {
  /// Whether the server holds no such work.
  pub quiet: bool,
  pub work: Vec<QuietWork>,
}

/// QuietWork: a turn the server carries that no program's record names yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuietWork {
  pub run: String,
  pub session: String,
  pub agent: String,
  pub turn_id: i64,
  /// epoch milliseconds
  pub since: i64,
}

/// Event:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Event {
  pub id: i64,
  pub kind: String,
  /// epoch milliseconds
  pub at: i64,
  pub data: Value,
}

/// EventPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventPage {
  pub items: Vec<Event>,
  pub more: bool,
  /// The notice that events after the number asked are no longer kept, or that it is a number the service
  /// never gave; none otherwise. The reader reads afresh the state it follows.
  #[serde(default)]
  pub missed: Option<EventsMissed>,
}

/// EventsMissed: what the stream's `events.missed` notice and a page's `missed` carry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventsMissed {
  /// the number the reader named
  pub after: i64,
  /// the oldest number kept, none where none is
  pub kept_from: Option<i64>,
}

/// Agent:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
  pub name: String,
  pub label: String,
  pub role: String,
  pub gates: Value,
  pub reasoning: Value,
  pub voice: Option<String>,
  /// epoch milliseconds
  pub reset_at: Option<i64>,
  /// the id of the login account that owns it, none for the administrator's
  pub owner_account: Option<i64>,
}

/// PromptDocument:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptDocument {
  pub name: String,
  pub kind: PromptDocumentKind,
  pub agent_name: Option<String>,
  pub version: String,
  pub text: String,
}

/// DocumentMove:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMove {
  pub paths: Vec<Value>,
  /// epoch milliseconds
  pub at: i64,
  pub agent_name: Option<String>,
  pub room_ids: Vec<Value>,
}

/// Session:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Session {
  pub id: String,
  pub agent_name: String,
  pub scope: SessionScope,
  pub room_id: Option<i64>,
  pub provider: String,
  pub model: String,
  pub place: Option<String>,
  pub generation: i64,
  pub holder: Option<String>,
  pub prefix_change_reason: Option<String>,
  pub options: Value,
  /// The share of its model's window the session may fill before it is folded once the turn that crossed it is
  /// over: the compaction threshold of its supervisor or its role, read afresh with each read.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub fold_ratio: Option<f64>,
  /// The context its last turn left, `carried` tokens against the model's `window`, or none.
  #[serde(default)]
  pub context: Option<Value>,
  /// Whether the session's program compacts itself at its fold ratio and takes no fold call.
  #[serde(default)]
  pub folds_itself: bool,
  /// The supervisor the session runs for, or none.
  #[serde(default)]
  pub supervisor: Option<String>,
  /// The role the session runs for, or none.
  #[serde(default)]
  pub role: Option<String>,
  /// Whether a supervisor's or a room's session was opened without the handoff it was owed, until a turn begun
  /// after that has answered on it.
  #[serde(default)]
  pub handoff_lost: bool,
}

/// Turn:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
  pub id: i64,
  pub session_id: String,
  pub prompt: String,
  pub context: Value,
  pub state: TurnState,
  pub delivery: Option<String>,
  pub outcome: Option<String>,
  /// The word the turn's wait ended on, typed: for a turn put into a program's pane `answered`, `silent`,
  /// `errored`, `vanished`, `program-lost`, `silent-peer`, `blocked`, `untrusted`, `unsubmitted`,
  /// `overran` or `stopped`; for any other its outcome's word; none while the turn runs.
  #[serde(default)]
  pub ending: Option<String>,
  /// Why a turn with no answer has none; none where it answered or still runs.
  #[serde(default)]
  pub reason: Option<String>,
  pub answer: Option<String>,
  /// The answer of each turn of its program the round spanned, in order, which `answer` was built from;
  /// empty while the turn runs and where its road reads no such turns.
  #[serde(default)]
  pub answers: Vec<SpannedAnswer>,
  /// The program's own session the turn answered on, where its road reads one; a later start names it as
  /// `continues`.
  #[serde(default)]
  pub harness_session: Option<String>,
  pub text: Vec<Value>,
  /// What the running turn waits for before its program is put to work; none while nothing holds it back
  /// and once it has ended.
  #[serde(default)]
  pub waiting: Option<TurnWaiting>,
}

/// SpannedAnswer: one turn's answer among those a round spanned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SpannedAnswer {
  /// That turn's last response.
  pub text: String,
  /// What opened that turn.
  pub opened: SpannedAnswerOpened,
}

/// What opened a turn of a round: `handed` for words handed over, the round's own prompt or words pushed
/// into it; `program` for a turn the program opened on its own, a background task's notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum SpannedAnswerOpened {
  #[serde(rename = "handed")]
  Handed,
  #[serde(rename = "program")]
  Program,
}

/// TurnWaiting: what a running turn waits for, typed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TurnWaiting {
  /// What it waits for: `card`, a card the arbiter's master has not yet granted, `node-program`, a program on
  /// a node that has stood past its notice without taking the turn, or `node-link`, a person's node whose link is
  /// not heard.
  #[serde(rename = "for")]
  pub kind: TurnWaitKind,
  /// The service whose grant it waits on, or the node whose program it waits on.
  pub service: String,
  /// Why it waits, as it stands now.
  pub reason: String,
  /// epoch milliseconds: when the wait began
  pub since: i64,
}

/// One of the values the API accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum TurnWaitKind {
  #[serde(rename = "card")]
  Card,
  #[serde(rename = "node-program")]
  NodeProgram,
  #[serde(rename = "node-link")]
  NodeLink,
}

/// TurnInput:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TurnInput {
  pub words: String,
  pub said: TurnInputSaid,
}

/// Answer:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename = "Answer")]
pub struct TurnAnswer {
  pub question_id: String,
  pub answer: Value,
}

/// Fold:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Fold {
  pub session_id: String,
  /// epoch milliseconds
  pub at: i64,
}

/// TurnContextAsk:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TurnContextAsk {
  pub session_id: String,
  pub text: String,
}

/// TurnContext:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TurnContext {
  pub context: Option<String>,
}

/// StreamingBlock: the block a session's answer is writing right now, as the model gateway counts it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StreamingBlock {
  /// Its place in the answer.
  pub index: u64,
  /// `thinking`, `text` or `tool`.
  pub kind: String,
  /// The bytes its deltas have written so far.
  pub bytes: u64,
}

/// Run:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Run {
  pub id: i64,
  pub name: String,
  pub agent_name: String,
  pub unit_name: String,
  /// What the run was asked to do: a role session's turn's title, or the title the body opening it named.
  #[serde(default)]
  pub title: Option<String>,
  pub worker_name: Option<String>,
  pub job_id: Option<i64>,
  pub state: RunState,
  /// epoch milliseconds
  pub started_at: i64,
  /// epoch milliseconds
  pub ended_at: Option<i64>,
  pub outcome: Option<String>,
  /// epoch milliseconds
  pub cap_ends_at: Option<i64>,
  /// epoch milliseconds
  pub kept_until: Option<i64>,
  pub context_tokens: Option<i64>,
  /// What the run's turn spent, the four figures of `usage` together; none on an open run or where its road
  /// reported no figures.
  pub spent_tokens: Option<i64>,
  /// What the run's turn spent, apart.
  #[serde(default)]
  pub usage: Option<RunUsage>,
  pub counts: Value,
  /// The room the run ran for.
  #[serde(default)]
  pub room_id: Option<i64>,
  /// The round of the room the run belongs to, as the turn named it.
  #[serde(default)]
  pub round: Option<String>,
  /// The message the run's answer went out as once attached, the message the turn answered until then.
  #[serde(default)]
  pub message_id: Option<i64>,
}

/// RunUsage: what a chat turn or a job's turn spent, each figure summed over the models that answered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunUsage {
  pub input_tokens: i64,
  pub output_tokens: i64,
  pub cache_read_tokens: i64,
  pub cache_write_tokens: i64,
}

/// LogLine:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
  pub id: i64,
  pub run_id: i64,
  pub job_id: Option<i64>,
  pub agent_name: String,
  pub kind: String,
  pub text: String,
  /// epoch milliseconds
  pub at: i64,
  pub fields: Value,
  /// The metadata the writer kept beside the line, the JSON object itself or null where it kept none.
  pub metadata: Value,
  /// The room of the ledger's run the line was written under, where the ledger holds it.
  #[serde(default)]
  pub room_id: Option<i64>,
  /// That run's round.
  #[serde(default)]
  pub round: Option<String>,
  /// That run's message.
  #[serde(default)]
  pub message_id: Option<i64>,
}

/// Execution:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Execution {
  pub id: i64,
  pub run_id: i64,
  pub command: String,
  pub state: String,
  pub cancelled: bool,
}

/// OvertimeRequest:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OvertimeRequest {
  pub id: i64,
  pub run_id: i64,
  pub minutes: i64,
  pub why: String,
  pub verdict: Option<OvertimeRequestVerdict>,
  /// epoch milliseconds
  pub closed_at: Option<i64>,
}

/// StopMark:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StopMark {
  pub job_id: i64,
  pub reason: String,
  /// epoch milliseconds
  pub at: i64,
}

/// Pool:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Pool {
  pub buckets: Vec<Value>,
  pub workers: Vec<Value>,
  /// epoch milliseconds
  pub read_at: i64,
}

/// SeatRequest: a seat asked of the pool: `waiting` in its bucket's line, `reserved` under a reservation,
/// `active` once let in, and `released` or `cancelled` once given back; the provider, model, effort and tier
/// its work key resolved to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SeatRequest {
  pub token: String,
  pub source: String,
  pub owner: String,
  pub role: String,
  pub provider: String,
  pub model: String,
  /// None where the mapping it resolved to names no effort.
  pub effort: Option<String>,
  pub tier: String,
  pub work_key: Option<String>,
  pub state: SeatRequestState,
  pub child_id: Option<String>,
  pub outcome: Option<String>,
  /// The session a worker's own turn continues, or none where it opens a fresh one.
  pub session: Option<String>,
  /// One line naming the work the seat is asked for, where its asker named one.
  pub title: Option<String>,
  /// The agent that asked for the seat, where its asker named one.
  pub asked_by: Option<String>,
  /// The agent the seat's child runs on, where its binding named one.
  pub agent_name: Option<String>,
}

/// Reservation:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Reservation {
  pub token: String,
  pub owner: String,
  pub role: String,
  pub tier: String,
  pub count: i64,
  pub state: ReservationState,
  pub outcome: Option<String>,
}

/// Place:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Place {
  pub id: String,
  pub path: String,
  pub node_name: Option<String>,
  pub slot: Option<i64>,
  pub role: Option<String>,
  pub state: PlaceState,
  /// any JSON value, or null
  pub last_write: Option<Value>,
}

/// ProviderQuota: what a provider has left as an order reads it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderQuota {
  /// Whether anything of it is left.
  pub has: bool,
  /// A fraction from 0 to 1, or null where the provider reports none.
  pub left: Option<f64>,
  /// The name of the window that counted, or null.
  pub window: Option<String>,
  /// What its balance reader last read, for a provider that keeps a balance, or null.
  pub balance: Option<f64>,
  /// The currency the balance is in, or null.
  pub currency: Option<String>,
  /// When the balance was seen, in epoch seconds, or null.
  pub balance_seen_at: Option<i64>,
  /// The request allowance read with the balance, or null where the provider states none.
  pub requests: Option<Requests>,
  /// When the counting window resets, in epoch seconds, or null.
  pub resets_at: Option<i64>,
}

/// Provider: one provider agency carries, with its switch, what it declares and what it has left.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
  pub name: String,
  /// The kebab-case word a screen words the provider by: its name.
  #[serde(default)]
  pub code: String,
  /// The English name a log keeps.
  #[serde(default)]
  pub label: String,
  pub enabled: bool,
  /// Whether its models are the registry's registrations alone.
  pub registered: bool,
  /// Whether its models are served by agency's model gateway.
  pub gateway: bool,
  /// Whether its engine is the owner's own, as the provider declares it: what a session on it reads stays with
  /// the owner. Neither `registered` nor `gateway` stands in for it.
  pub owners_engine: bool,
  /// Whether a road of it may name an advisor its program consults.
  #[serde(default)]
  pub takes_advisor: bool,
  /// The family words such a road may name its advisor by beside a model of its own; empty where it takes none.
  #[serde(default)]
  pub advisor_words: Vec<String>,
  /// The launch lines its program carries.
  pub lines: Vec<String>,
  pub quota: ProviderQuota,
  /// Why it cannot answer now, the English sentence, or null where it can.
  pub reason: Option<String>,
  /// The kebab-case code of why it cannot answer now, or null where it can.
  pub reason_code: Option<String>,
  /// The compaction threshold a row whose order begins on it is given where its add names none.
  pub fold_ratio_default: f64,
  /// The file under the data root's `env/` its own key is read from, or null for a provider that reads none. Its
  /// content is never reported.
  #[serde(default)]
  pub key_file: Option<String>,
}

/// Model: one registration of the registry's provider and its health.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Model {
  pub key: String,
  /// The registration's own name, which an order's entry and a repository's pin name.
  pub name: String,
  pub provider: String,
  pub model: String,
  /// The arbiter service its engine is, where it names one: its instances are those the arbiter's master
  /// reports standing, and it holds no address of its own.
  #[serde(default)]
  pub card_service: Option<String>,
  /// Whether the card service's instances keep role files.
  #[serde(default)]
  pub role_files: bool,
  /// Every supervisor, role and repository whose order or pin names it.
  #[serde(default)]
  pub used_by: Vec<String>,
  pub healthy: Option<bool>,
  /// When the engine behind it last answered a health probe, in unix seconds, where it ever has.
  pub last_answered_at: Option<i64>,
  /// Its window; null for a model that is no registration.
  pub context_tokens: Option<i64>,
  /// Its answer ceiling; null for a model that is no registration.
  pub answer_tokens: Option<i64>,
  /// Its thinking default: what a run on it does where its order's entry leaves thinking to the provider;
  /// null for a model that is no registration.
  pub thinking: Option<bool>,
  /// The languages a run on it writes a result for a person in, each by its English name; empty for every
  /// language.
  #[serde(default)]
  pub languages: Vec<String>,
  /// The user id the registration belongs to; null for the platform's.
  #[serde(default)]
  pub account: Option<i64>,
}

/// ModelCatalogueEntry:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogueEntry {
  pub name: String,
  pub label: String,
  pub file: String,
  pub bytes: i64,
  pub sha256: String,
  pub source: Option<String>,
  pub engine: Option<String>,
  pub registration: Option<String>,
  pub nodes: Vec<String>,
  pub thinking: Option<bool>,
  pub temperature: Option<f64>,
  pub top_p: Option<f64>,
  pub presence_penalty: Option<f64>,
  pub max_tokens: Option<i64>,
  pub note: Option<String>,
  pub serving: bool,
}

/// ModelCatalogueUse:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogueUse {
  pub id: i64,
  pub entry: String,
  pub engine: String,
  pub chosen_by: String,
  pub started_at: i64,
  pub ended_at: Option<i64>,
  pub state: String,
  pub step: Option<String>,
  pub reason: Option<String>,
  pub log_tail: Vec<String>,
  pub log: Option<String>,
}

/// ModelCatalogueView:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogueView {
  pub entries: Vec<ModelCatalogueEntry>,
  pub uses: Vec<ModelCatalogueUse>,
}

/// ModelCatalogueChange: the action and the fields a change of the catalogue names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogueChange {
  pub action: String,
  pub name: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub label: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub file: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub bytes: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sha256: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub source: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub engine: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub registration: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub note: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub thinking: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub temperature: Option<f64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub top_p: Option<f64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub presence_penalty: Option<f64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub max_tokens: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub by: Option<String>,
}

/// PinCheckAsk:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PinCheckAsk {
  pub value: String,
}

/// PinCheck:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PinCheck {
  pub kind: PinCheckKind,
  pub provider: Option<String>,
  pub model: Option<String>,
}

/// Tier:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Tier {
  pub name: String,
  pub coworker: bool,
  pub mapping: Value,
}

/// Fallback:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Fallback {
  #[serde(default)]
  pub routes: Vec<Value>,
  /// The agent serving the chat now and what it has used; null on a person's call.
  #[serde(default)]
  pub active: Option<FallbackActive>,
  /// The Claude accounts the caller reads, as the road walks them: a person's own alone, and the platform's
  /// on the administrator's and the deployment's own calls.
  #[serde(default)]
  pub accounts: Vec<FallbackAccount>,
  /// Each person's supply as the roads it serves, on the administrator's and the deployment's own calls alone.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub personal_supply: Option<Vec<PersonSupply>>,
}

/// A person's supply as the platform reads it: the person and the roads their own credentials serve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonSupply {
  pub user: i64,
  /// `claude-code` where they hold a Claude account that is on, and the gateway roads of their keys that are on.
  pub roads: Vec<String>,
}

/// One of a person's own keys for a gateway road (docs/supply.md, 「What a person brings」).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalSupplyKey {
  pub id: i64,
  /// `codex`, `deepseek` or `openrouter`.
  pub road: String,
  /// The secret's name in the person's space of the secret store; its value is never reported.
  pub secret: String,
  /// Its place among the person's keys of the road, from 1.
  pub place: i64,
  pub enabled: bool,
  /// epoch milliseconds
  pub refused_at: Option<i64>,
  /// epoch milliseconds
  pub added_at: i64,
}

/// The personal supply a caller reads: a person's own keys, or for the platform each person's roads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalSupplyPage {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub items: Option<Vec<PersonalSupplyKey>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub people: Option<Vec<PersonSupply>>,
}

/// A key of the signed-in person's added for a gateway road.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalSupplyAsk {
  pub road: String,
  /// One plain file name in the person's space of the secret store.
  pub secret: String,
}

/// A change of one of the person's keys: its place, its switch, or both.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalSupplyChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub place: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub enabled: Option<bool>,
}

/// One item of an organisation unit's own supply: a key for a gateway road, or an Anthropic API key standing as a
/// Claude account of the unit's, `claude-code` its road.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrganisationSupplyItem {
  /// `key-<n>` for a gateway key, `account-<id>` for a Claude account.
  pub id: String,
  pub road: String,
  /// The secret's name in the unit's space of the secret store; its value is never reported.
  pub secret: String,
  /// The Claude account's name, for an account.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
  pub enabled: bool,
  /// epoch milliseconds: when its provider refused it, or the account was found waiting.
  pub refused_at: Option<i64>,
}

/// An organisation unit's own supply as its heads read it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrganisationSupplyPage {
  pub unit: i64,
  /// Whether organisation supply stands open for the unit on the delegation chain.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub open: Option<bool>,
  pub items: Vec<OrganisationSupplyItem>,
}

/// A key of an organisation unit's added: a gateway road and a secret of the unit's space, or `claude-code` for an
/// Anthropic API key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrganisationSupplyAsk {
  pub road: String,
  /// One plain file name in the unit's space of the secret store.
  pub secret: String,
}

/// One of an organisation unit's keys or accounts turned on or off.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrganisationSupplyChange {
  pub enabled: bool,
}

/// The supply sources the signed-in person's delegation chain leaves open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplySources {
  /// Their own login or key.
  pub personal: bool,
  /// The platform's supply.
  pub platform: bool,
  /// Their organisation's own supply.
  pub organisation: bool,
  /// Whether a unit on their path holds supply of its own that is on.
  pub organisation_supply: bool,
  /// Whether their own login or key is all that can carry their runs.
  pub personal_only: bool,
  /// Whether they hold a login, a key or a local AI of their own that is on.
  pub own_supply: bool,
  /// Whether they may add a local AI of their own now: their chain allows it and they hold a PC of their own.
  #[serde(default)]
  pub local_ai: bool,
}

/// One agent's road in a sign-up composition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompositionRoad {
  /// None, with no model, for the road of the role's own agent of no account.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub provider: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub effort: Option<String>,
}

/// What stands of one role in a sign-up composition for a subject.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompositionRole {
  pub role: String,
  /// Whether the role is off for the people under the subject.
  pub off: bool,
  /// The roads of the agents a person is given of it, none for the platform's rule.
  pub agents: Option<Vec<CompositionRoad>>,
  /// The link that said the agents, where one did.
  pub from: Option<String>,
}

/// A sign-up composition as it stands for a unit or a person, with the subject's own link where the caller reads
/// it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignupComposition {
  /// `unit:<id>` or `person:<user>`.
  pub subject: String,
  pub roles: Vec<CompositionRole>,
  /// The subject's own link as the delegation chain answers it to the caller: its `value`, `delegation`,
  /// `lockedBy` and `writable`.
  pub own: Option<Value>,
}

/// A link's sign-up composition written: `{ roles: { <role>: { off, agents } } }`, or null to follow above.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignupCompositionAsk {
  pub value: Option<Value>,
}

/// The agents made for a person by their composition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComposedAgents {
  pub items: Vec<RoleAgent>,
}

/// What a person's off-boarding turned off.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OffBoarded {
  /// How many of their Claude accounts were turned off.
  pub accounts: i64,
  /// How many of their keys were turned off.
  pub keys: i64,
  /// How many of their local AIs were turned off.
  pub local_ais: i64,
}

/// What removing an owner or an organisation unit for good would take now, by kind, every kind present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemovalCounts {
  /// How many of each kind the removal would take.
  pub counts: BTreeMap<String, i64>,
}

/// What removing an owner or an organisation unit for good took, by kind, every kind present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemovalDone {
  /// How many of each kind the removal took.
  pub removed: BTreeMap<String, i64>,
}

/// The source of a road a supply test asks of: the person's own for a person's call, the platform's otherwise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplyTestAsk {
  /// One of `claude-code`, `codex`, `deepseek`, `openrouter` and `registered`.
  pub road: String,
  /// The registration to test on `registered`, by its name or its key: the platform's for a platform's test, a
  /// local AI of the person's own for a person's.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<String>,
  /// A local AI of the person's not saved yet, tried in place of `model` on `registered`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub local: Option<LocalModelTried>,
}

/// A local AI tried before it is saved: its address, the model it serves and its key's secret name, where it asks one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelTried {
  /// A plain http address ending in `/chat/completions`, as the person's PC reaches it.
  pub url: String,
  pub model_name: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub key_name: Option<String>,
}

/// One local AI of a person's own (docs/supply.md, 「A person's local AI」).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalModel {
  /// The name the person gave it.
  pub name: String,
  /// Its registration's key on the registry's provider, `registered::<registration>`.
  pub model: String,
  /// The address as the person's PC reaches it.
  pub url: String,
  /// The model the server serves.
  pub model_name: String,
  /// The name of the person's secret its key is, or null.
  pub key_name: Option<String>,
  pub context_tokens: i64,
  pub enabled: bool,
  /// epoch seconds
  pub added_at: i64,
}

/// A person's own local AIs, and whether their chain lets them add one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalModels {
  pub allowed: bool,
  pub items: Vec<PersonalModel>,
}

/// A local AI of the person's own made or changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalModelAsk {
  /// A plain http address ending in `/chat/completions`, as the person's PC reaches it.
  pub url: String,
  pub model_name: String,
  /// One plain name in the person's own space of the secret store, or null for no key.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub key_name: Option<String>,
  /// The window a run on it is told, within the registry's bounds; the policy's where none is named.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub context_tokens: Option<i64>,
}

/// One of the person's local AIs turned on or off.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalModelChange {
  pub enabled: bool,
}

/// Whether a source of a road answers now, and where it does not, why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplyTested {
  pub road: String,
  /// Whether the source answers now.
  pub ok: bool,
  /// Why it does not, the kebab-case code — `off`, `no-source`, `login-needed`, `not-ready`, `refused`,
  /// `failed`, `withheld`, `no-pc` or `unreachable` — or null where it does.
  pub reason_code: Option<String>,
  /// Why it does not, the English sentence, or null where it does.
  pub reason: Option<String>,
}

/// The agent serving the chat now: the provider its road answers on, the login it walks to where the
/// provider holds logins, and that slot's two headline windows and balance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FallbackActive {
  pub provider: String,
  pub account: Option<String>,
  /// The login's name, or the provider's label where it holds none.
  pub account_name: String,
  /// How many logins are on, where the provider holds logins; 0 otherwise.
  pub accounts: i64,
  pub session: Option<UsageWindow>,
  pub week: Option<UsageWindow>,
  /// What the slot has left, where its provider or its endpoint account offers a balance.
  pub balance: Option<Balance>,
}

/// One Claude account as the fallback's view shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FallbackAccount {
  pub id: String,
  pub name: String,
  pub place: i64,
  pub enabled: bool,
  /// One of `directory`, `token`, `endpoint` or `api-key`.
  pub credential_kind: String,
  pub credential_path: String,
  pub endpoint_url: Option<String>,
  /// An endpoint account's key file or an `api-key` account's secret. Its content is never reported.
  pub key_file: Option<String>,
  pub models: Option<EndpointModels>,
  pub base_prompt: Option<String>,
  /// The person it belongs to, or null for the platform's.
  #[serde(default)]
  pub user_id: Option<i64>,
  /// The organisation unit it belongs to in place of a person, where one does.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub unit_id: Option<i64>,
  /// The node a person's login stands on, or null.
  #[serde(default)]
  pub node: Option<String>,
  /// Whether the account goes out for another person's run on a platform source.
  #[serde(default)]
  pub lendable: bool,
  /// One of `none` (no login held on this machine), `standing`, `ending` or `gone`.
  pub login: String,
  /// epoch seconds
  pub login_ends_at: Option<i64>,
  /// Whether its runs here go out on its token for the work nodes now, its login here having ended, waiting
  /// for a person or near its end (docs/cache-fallback.md, 「A directory account carries on by its token」).
  #[serde(default)]
  pub on_token: bool,
  pub chosen: bool,
  pub serving: bool,
  /// epoch seconds
  pub rest_until: Option<i64>,
  pub session: Option<UsageWindow>,
  pub week: Option<UsageWindow>,
  pub balance: Option<Balance>,
}

/// What a paid account has left, as its balance reader last read it (docs/usage-windows.md, 「Balances」).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
  pub amount: f64,
  pub currency: String,
  /// epoch seconds
  pub seen_at: i64,
  /// The request allowance read with it, or null where the provider states none.
  pub requests: Option<Requests>,
}

/// A request allowance as a provider states it (docs/usage-windows.md, 「Balances」): how many requests of the
/// day are used, the day's ceiling and how many are left.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Requests {
  pub used: i64,
  pub limit: i64,
  pub left: i64,
  /// What the allowance covers, a kebab-case word a screen words it by, or null where it covers every request.
  pub scope: Option<String>,
}

/// ItemFilingAsk: an item a supervisor files for the turn it carries now; the agent is the one its harness
/// plugin was started for, and the item's room and whether the owner asked it are agency's to read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ItemFilingAsk {
  pub agent_name: String,
  pub title: String,
  pub brief: String,
  pub queue_name: String,
  /// The repository the work is in, written as the item's `assigneeName`; none for host work.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub repository: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub priority: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub assignee_name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub requester_name: Option<String>,
  #[serde(default)]
  pub labels: std::collections::BTreeMap<String, String>,
  /// The stages the item's job runs in, in order; written as its work order.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub steps: Vec<String>,
  /// The mark the steps check answered with, once the steps were checked against it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub steps_check: Option<String>,
}

/// FileItem: an item to file as the supervisor asks it, everything but the agent its plugin was started for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileItem {
  /// The item's name in the queue: a short line in the asker's language.
  pub title: String,
  /// The run's whole instruction, in English.
  pub brief: String,
  /// The queue it is filed in.
  pub queue_name: String,
  /// The repository the work is in, by its internal name, so only that repository's worker takes it;
  /// left out for host work, which is in no repository. One change that reaches two or more repositories is one
  /// item: its repository the one it runs in, an umbrella where it has no home of its own, and its label
  /// `repositories` naming every repository it reaches, comma-separated.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub repository: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub priority: Option<i64>,
  /// The worker it is to go to, where it names one; the repository's name where `repository` names one.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub assignee_name: Option<String>,
  /// Who asked for it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub requester_name: Option<String>,
  /// Labels of the filer's own, each a string.
  #[serde(default)]
  pub labels: std::collections::BTreeMap<String, String>,
  /// The stages the item's job runs in, in order, each examined before the next (docs/job-review.md): each one
  /// the question its agent is asked at that stage, in your words. The job's worker cannot change them.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub steps: Vec<String>,
  /// Left out the first time: the filing is then refused `steps-unchecked` with the skills and tools of the
  /// agent the item runs on and what to check the steps against. Check them, and file again with the steps as
  /// they then stand and this set to the mark that refusal named.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub steps_check: Option<String>,
}

/// ItemFiled: the item as the job service raised it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ItemFiled {
  pub id: i64,
  /// `open` where the job service's unattended labels opened it, `draft` otherwise.
  pub state: String,
  pub scope: Option<String>,
  /// The item's labels, each a string.
  pub labels: Value,
  /// The job service's sentence where the work order the steps make was refused; null where it was saved or no
  /// step was named.
  #[serde(default)]
  pub work_order_refused: Option<String>,
}

/// JobReviewAsk: a supervisor's move on a job's report under review, for the turn it carries now; the agent is the
/// one its harness plugin was started for (docs/job-review.md, 「What the orderer may do」).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobReviewAsk {
  pub agent_name: String,
  pub job_id: i64,
  /// `ask`, `reject`, `pass` or `escalate`.
  pub action: String,
  /// The words the action hands on; required for every action but `pass`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub text: Option<String>,
  /// For `reject`: the stage, counted from one, the work goes back to; one already reached. None sends back the
  /// stage under way.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub stage: Option<i64>,
}

/// ReviewJob: a move on a job's report under review as the supervisor asks it, everything but the agent its
/// plugin was started for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReviewJob {
  /// The job under review, as its review instruction names it.
  pub job_id: i64,
  /// `ask` puts a question to the worker on the check due; `reject` sends the work back with why and what is to
  /// be done; `pass` passes the check due; `escalate` puts the work to the owner, refused until the work was sent
  /// back as often as the review's ceiling.
  pub action: String,
  /// The question, the reason and the rework, what the check found, or what the owner is asked and why.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub text: Option<String>,
  /// For `reject` only: the stage, counted from one, the work goes back to — one already passed whose work the
  /// report shows falls short, or the stage under way; left out, the stage under way is sent back. Its step is
  /// handed over again with `text` as what is to be done, and the stages after it follow again.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub stage: Option<i64>,
}

/// JobStopAsk: a job a supervisor stops for the turn it carries now, by its id or by its item; the agent is the
/// one its harness plugin was started for, and whether the owner asked for the stop is agency's to read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobStopAsk {
  pub agent_name: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub job_id: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub item_id: Option<i64>,
  pub reason: String,
}

/// StopJob: a job to stop as the supervisor asks it, everything but the agent its plugin was started for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StopJob {
  /// The job, where the stop names it by its id.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub job_id: Option<i64>,
  /// The item whose starting or running job is stopped, where the stop names it by its item.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub item_id: Option<i64>,
  /// Why, in a short line: the job's report.
  pub reason: String,
}

/// WorkKeyAssignment: one work key's tier, the tiers it may be assigned, the owner's choices from before the
/// tiers kept for review, and what it resolves to on each provider of the registry's archive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkKeyAssignment {
  pub key: String,
  /// The key in kebab case, the word a screen words it by.
  pub code: String,
  /// The English name its profile gives, none for a key no profile declares.
  pub label: Option<String>,
  pub tier: String,
  pub choices: Vec<String>,
  /// Each `provider`, `previous` and `accepted`.
  pub conflicts: Vec<Value>,
  /// Each `provider`, `selection`, `error` (one short clause), `detail` (the whole of why) and `errorCode`.
  pub effective: Vec<Value>,
}

/// WorkKeyAssignmentPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkKeyAssignmentPage {
  pub items: Vec<WorkKeyAssignment>,
  pub more: bool,
}

/// WorkKey:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkKey {
  pub key: String,
  pub provider: String,
  pub model: String,
  pub thinking: Option<bool>,
  pub effort: Option<String>,
  pub answer_tokens: Option<i64>,
  pub cap_ms: i64,
  pub run_mode: String,
}

/// RoadBlock:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoadBlock {
  pub id: i64,
  pub selection: String,
  /// epoch milliseconds
  pub until: i64,
  pub reason: String,
}

/// LaunchRoom:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRoom {
  pub held: bool,
  pub reasons: Vec<Value>,
  pub job_count: i64,
  pub browser_job_count: i64,
}

/// An endpoint's model ids by the family word Claude Code picks a model by:
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EndpointModels {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub opus: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sonnet: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub haiku: Option<String>,
}

/// One usage window of a slot as the usage-window store holds it (docs/usage-windows.md).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
  pub name: String,
  /// The login the slot is kept under, none on a provider that holds no logins.
  pub account: Option<String>,
  /// The provider the window was read of.
  pub mode: String,
  /// The share of the limit used, 0 to 1.
  pub utilization: f64,
  /// epoch seconds
  pub resets_at: Option<i64>,
  /// epoch seconds: when the reading it came in was seen.
  pub seen_at: i64,
  pub duration_minutes: Option<i64>,
  pub limit_name: Option<String>,
}

/// Account:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Account {
  pub id: String,
  pub provider: String,
  /// The user it belongs to, as `userId` names it.
  pub user_name: Option<i64>,
  pub held: bool,
  pub home: Option<String>,
  /// The usage a report named, on the report's answer; null on the list.
  pub window: Value,
  /// The account's session window, none where it holds none: an endpoint account never does.
  #[serde(default)]
  pub session: Option<UsageWindow>,
  /// The account's week window, none where it holds none.
  #[serde(default)]
  pub week: Option<UsageWindow>,
  /// What an endpoint account has left, where its endpoint's provider offers a balance.
  #[serde(default)]
  pub balance: Option<Balance>,
  /// epoch milliseconds
  pub rest_until: Option<i64>,
  /// One of `directory`, `token`, `endpoint` or `api-key`.
  pub credential_kind: Option<String>,
  /// An endpoint account's base URL.
  pub endpoint_url: Option<String>,
  /// An endpoint account's key file or an `api-key` account's secret. Its content is never reported.
  pub key_file: Option<String>,
  /// An endpoint account's model ids by family word.
  pub models: Option<EndpointModels>,
  /// The base prompt a worker's run on it opens with.
  pub base_prompt: Option<String>,
  /// The person it belongs to, or null for the platform's.
  #[serde(default)]
  pub user_id: Option<i64>,
  /// The node a person's login stands on, or null.
  #[serde(default)]
  pub node: Option<String>,
  /// Whether the account goes out for another person's run on a platform source.
  #[serde(default)]
  pub lendable: bool,
}

/// Rest: the deployment's rest as it stands, or as a write or an end of it left it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Rest {
  pub resting: bool,
  /// epoch seconds
  pub until: Option<i64>,
  pub kind: Option<String>,
  /// The login a written rest stands under, null for the service's outage.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub account_id: Option<String>,
  /// Whether a write wrote its rest, false where one already standing there was left as it stood.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub written: Option<bool>,
  /// Whether a write's rest was the one that silenced the deployment.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub silenced: Option<bool>,
  /// Whether the deployment rested before an end by hand.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub ended: Option<bool>,
}

/// SupervisorRoad: a supervisor's one road, and whether a turn would go out on it now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorRoad {
  pub supervisor: String,
  /// its road's provider, null where its row names no road
  #[serde(default)]
  pub provider: Option<String>,
  #[serde(default)]
  pub model: Option<String>,
  #[serde(default)]
  pub effort: Option<String>,
  /// `on`, `off`, or null for the provider's own
  #[serde(default)]
  pub thinking: Option<String>,
  /// whether a turn would go out on its road now
  pub answers: bool,
  /// the sentence a turn would fail with where its road cannot take it, null where it can
  #[serde(default)]
  pub reason: Option<String>,
}

/// Usage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
  pub groups: Vec<UsageGroup>,
  /// epoch milliseconds
  pub after: i64,
  /// epoch milliseconds
  pub before: i64,
}

/// UsageGroup: what the jobs, or a round's turns, under one key of a usage query spent (docs/api.md,
/// 「accounts, rest and usage」).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsageGroup {
  /// The value the query groups by; empty where the job holds none.
  pub key: String,
  /// What the group's jobs' models spent.
  pub tokens: i64,
  /// How many jobs are behind the figure; 0 for a round.
  pub jobs: i64,
  /// How many turns a round holds; absent outside `round`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub turns: Option<i64>,
  /// epoch milliseconds
  pub first_at: i64,
  /// epoch milliseconds
  pub last_at: i64,
  /// The input tokens summed over the jobs whose turns reported all four kinds; null where none did.
  pub input_tokens: Option<i64>,
  /// The output tokens summed over the jobs whose turns reported all four kinds; null where none did.
  pub output_tokens: Option<i64>,
  /// The cache-read tokens summed over the jobs whose turns reported all four kinds; null where none did.
  pub cache_read_tokens: Option<i64>,
  /// The cache-write tokens summed over the jobs whose turns reported all four kinds; null where none did.
  pub cache_write_tokens: Option<i64>,
  /// How many jobs spent tokens without reporting all four kinds.
  pub unsplit_jobs: i64,
  /// What those jobs spent.
  pub unsplit_tokens: i64,
  /// The tokens the roles' sessions on the key carry; null where no session is matched to the key.
  pub context_tokens: Option<i64>,
  /// What the group's jobs or runs cost in US dollars; null where none reported a cost.
  #[serde(default)]
  pub cost_usd: Option<f64>,
}

/// Settings:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
  /// The supervisor whose brain a role's run reads and writes, or null.
  pub research_supervisor: Option<String>,
  pub downgrade_return: Value,
  /// The console secret, of the kind `card`, the DeepSeek top-up reads its card from, or null for none.
  pub deepseek_card_secret: Option<String>,
  /// The console secret, of the kind `account-login`, the DeepSeek top-up signs in with, or null for none.
  pub deepseek_login_secret: Option<String>,
  /// The minutes between two flushes of the deploy queue in force: the owner's, else the policy's.
  pub deploy_flush_minutes: i64,
}

/// SupervisorSettings: a supervisor as its settings page shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorSettings {
  pub agent: String,
  pub label: String,
  pub icon: Option<String>,
  /// Whether the deployment seeds it.
  pub builtin: bool,
  pub turn_shape: String,
  /// What its order names first.
  pub provider: Option<String>,
  pub model: Option<String>,
  /// The registration its order names at the registry's provider, or null.
  pub registration: Option<String>,
  /// Every offered registration, each `name` and `label`.
  pub registrations: Vec<Value>,
  /// The model files behind its registration.
  pub entries: Vec<Value>,
  pub serving: Option<String>,
  pub model_use: Option<Value>,
  /// `referenceOn`, `brainOn`, `toolsAllowed`, `persona`, `generation`, `updatedAt` and `updatedBy`.
  pub settings: Value,
  /// Every generation setting with its bounds.
  pub generation_settings: Vec<Value>,
  pub thinking: Option<bool>,
  pub persona_file: String,
  pub base_prompt: Option<String>,
  pub base_prompts: Vec<Value>,
  /// The system prompt its next turn sends, `characters`, `hash` and `text`, where its turn shape is `pieces`.
  pub system: Option<Value>,
  /// Its room sessions.
  pub sessions: Vec<Value>,
}

/// SupervisorReset: what a reset of a supervisor took, or would take on a dry run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorReset {
  pub agent: String,
  pub label: String,
  pub ranges: Vec<String>,
  /// The rooms it answers alone, each `chatId` and `title`.
  pub alone: Vec<Value>,
  /// The rooms it shares with other supervisors, each `chatId` and `title`.
  pub shared: Vec<Value>,
  /// What it took of each table and range, each `table`, `range` and `rows`.
  pub rows: Vec<Value>,
  /// The harness records it took.
  pub files: Vec<String>,
  /// The brain's answer for the person range, or null.
  pub brain: Value,
}

/// Worker:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Worker {
  pub name: String,
  pub label: String,
  pub cwd: String,
  pub icon: Option<String>,
  pub builtin: bool,
  pub node_name: Option<String>,
  pub role: Option<String>,
  /// working while a job of its is starting or running, or idle
  pub state: String,
  /// The job it is working on, where it is working.
  #[serde(default)]
  pub job_id: Option<i64>,
  /// That job's item.
  #[serde(default)]
  pub item_id: Option<i64>,
  pub sampling: Value,
  pub session_id: Option<String>,
  pub run_count: i64,
  /// epoch milliseconds
  pub last_run_at: Option<i64>,
  /// The documents its session's prefix carries, in order: a relative path from its checkout, an absolute
  /// one where it stands.
  #[serde(default)]
  pub prefix_documents: Vec<String>,
}

/// WorkerDefinition:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerDefinition {
  pub trailer: String,
  pub handoff: Option<String>,
}

/// Workbench:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Workbench {
  pub root: String,
  pub scratches: Vec<Value>,
}

/// PrefixCheckAsk:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrefixCheckAsk {
  pub worker_name: String,
  pub files: Vec<Value>,
}

/// PrefixCheck:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrefixCheck {
  pub touched: Vec<Value>,
}

/// Hook:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Hook {
  pub point: String,
  pub url: String,
  pub token: Option<String>,
}

/// PublishedEvent:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublishedEvent {
  pub kind: String,
  pub data: Value,
}

/// RoomHolding:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoomHolding {
  pub room_id: i64,
  pub holdings: Vec<Value>,
}

/// HookPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HookPage {
  pub items: Vec<Hook>,
  pub more: bool,
}

/// AgentPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentPage {
  pub items: Vec<Agent>,
  pub more: bool,
}

/// AgentRoleBundle: a bundle a role may give, by its name and the label a screen writes it by.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRoleBundle {
  pub name: String,
  pub label: String,
}

/// RoleSessions: whether a role's runs go on in the session an earlier run left while its cache is warm,
/// `keep`, or each open a session of its own, `fresh`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RoleSessions {
  #[default]
  Keep,
  Fresh,
}

/// AgentRole: one role an agent may hold, as the store keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRole {
  pub name: String,
  pub label: String,
  pub bundles: Vec<AgentRoleBundle>,
  /// The most of its agents that run a turn at once; null where it is unbounded.
  pub max_concurrent: Option<i64>,
  /// The prompt its agents are given before their own; null where it carries none.
  pub prompt: Option<String>,
  /// The tier its work key `role:<name>` is assigned; null where none is.
  pub tier: Option<String>,
  /// Whether its runs keep their sessions.
  pub sessions: RoleSessions,
  /// How many agent rows hold it.
  pub agents: i64,
  /// Whether a kind's own is it, which keeps it from being removed.
  pub kind_default: bool,
}

/// AgentRoleList: every role an agent may hold, and every bundle a role may give.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRoleList {
  pub roles: Vec<AgentRole>,
  pub bundles: Vec<AgentRoleBundle>,
}

/// AgentRoleAsk: the body that adds a role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRoleAsk {
  /// kebab-case, held by no role and no co-worker role
  pub name: String,
  pub label: String,
  /// bundle names the policy declares
  pub bundles: Vec<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub max_concurrent: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub prompt: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tier: Option<String>,
  /// `keep` where none is named
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sessions: Option<RoleSessions>,
}

// A field given, even as null, read as given; a field left out stays none.
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
  given: D,
) -> Result<Option<Option<T>>, D::Error> {
  Option::<T>::deserialize(given).map(Some)
}

/// AgentRoleChange: a change to a role; a field left out keeps what it holds, and a bound, a prompt or a
/// tier given as null is cleared.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRoleChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub label: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub bundles: Option<Vec<String>>,
  #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
  pub max_concurrent: Option<Option<i64>>,
  #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
  pub prompt: Option<Option<String>>,
  #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
  pub tier: Option<Option<String>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sessions: Option<RoleSessions>,
}

/// PoolLimit: how many children of one co-worker tier may be alive on one provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PoolLimit {
  pub provider: String,
  pub tier: String,
  /// from 0, which closes the tier on the provider
  pub capacity: i64,
}

/// PromptDocumentPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptDocumentPage {
  pub items: Vec<PromptDocument>,
  pub more: bool,
}

/// ChatDocument: one of the supervisor's own documents, as it stands now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatDocument {
  pub name: String,
  pub kind: String,
  /// `stored` where the owner's text stands, `file` where the deployed file does.
  pub source: String,
  /// When the stored text was last saved, in seconds; null on a document standing as its file.
  pub version: Option<i64>,
  pub text: String,
}

/// ChatDocumentPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatDocumentPage {
  pub items: Vec<ChatDocument>,
  pub more: bool,
}

/// ChatDocumentChange:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatDocumentChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub action: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub text: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub by: Option<String>,
}

/// Rulebook:
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Rulebook {
  pub name: String,
  pub text: String,
}

/// Rulebooks:
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Rulebooks {
  pub rulebooks: Vec<Rulebook>,
}

/// SessionPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionPage {
  pub items: Vec<Session>,
  pub more: bool,
}

/// RunPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunPage {
  pub items: Vec<Run>,
  pub more: bool,
}

/// LogLinePage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogLinePage {
  pub items: Vec<LogLine>,
  pub more: bool,
}

/// One agent's log across its runs: its lines, whether another page waits towards the way it was read,
/// and, for a page opened at a moment, whether older lines stand before it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentLogPage {
  pub items: Vec<LogLine>,
  pub has_more: bool,
  #[serde(default)]
  pub has_older: bool,
}

/// ExecutionPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPage {
  pub items: Vec<Execution>,
  pub more: bool,
}

/// BackgroundWork: one task a supervisor's program still runs in its own background after the turn that launched it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundWork {
  pub task: String,
  pub kind: String,
  pub description: String,
  pub command: Option<String>,
  pub room_id: Option<i64>,
  pub agent_name: Option<String>,
  pub launched_at: i64,
  pub status: String,
}

/// BackgroundWorkPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundWorkPage {
  pub items: Vec<BackgroundWork>,
  pub more: bool,
}

/// OvertimeRequestPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OvertimeRequestPage {
  pub items: Vec<OvertimeRequest>,
  pub more: bool,
}

/// SeatRequestPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SeatRequestPage {
  pub items: Vec<SeatRequest>,
  pub more: bool,
}

/// ReservationPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReservationPage {
  pub items: Vec<Reservation>,
  pub more: bool,
}

/// PlacePage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlacePage {
  pub items: Vec<Place>,
  pub more: bool,
}

/// ProviderPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPage {
  pub items: Vec<Provider>,
  pub more: bool,
}

/// ModelPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelPage {
  pub items: Vec<Model>,
  pub more: bool,
}

/// TierPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TierPage {
  pub items: Vec<Tier>,
  pub more: bool,
}

/// Where a fold ratio comes from: the table's row, the provider's own declared share where the table holds none, or the general share where the provider declares none either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum FoldRatioSource {
  #[serde(rename = "set")]
  Set,
  #[serde(rename = "provider")]
  Provider,
  #[serde(rename = "default")]
  Default,
}

/// A provider's fold ratio at a tier: the share of a model's window a session may fill before it is folded once the turn that crossed it is over.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoldRatio {
  pub provider: String,
  pub tier: String,
  pub ratio: f64,
  pub source: FoldRatioSource,
}

/// FoldRatioPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoldRatioPage {
  pub items: Vec<FoldRatio>,
  pub more: bool,
}

/// The body that sets a provider's fold ratio at a tier: a share above 0 and at most 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoldRatioWrite {
  pub ratio: f64,
}

/// RoadBlockPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoadBlockPage {
  pub items: Vec<RoadBlock>,
  pub more: bool,
}

/// AccountPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountPage {
  pub items: Vec<Account>,
  pub more: bool,
  /// Each person's supply as the roads it serves, on the administrator's and the deployment's own calls alone.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub personal_supply: Option<Vec<PersonSupply>>,
}

/// WorkerPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerPage {
  pub items: Vec<Worker>,
  pub more: bool,
}

/// Assignment: A worker held for one item for the harness that asked. held until POST /jobs uses it; running while its job runs; released when the harness lets it go or the job ends. No clock ends a hold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
  pub id: i64,
  pub item_id: i64,
  pub holder_name: String,
  pub worker_name: String,
  /// The repository the checkout `workerName` names is of, none for a checkout no repository holds any longer.
  #[serde(default)]
  pub repository_name: Option<String>,
  /// The role the item's job runs as.
  #[serde(default)]
  pub role_name: Option<String>,
  pub state: AssignmentState,
  /// epoch milliseconds
  pub created_at: i64,
  /// epoch milliseconds
  pub changed_at: i64,
}

/// AssignmentPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssignmentPage {
  pub items: Vec<Assignment>,
  pub more: bool,
}

/// JobItem: The item's content as the harness read it from the job service; the agency reads nothing there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobItem {
  pub title: String,
  pub brief: String,
  pub work_order: Option<Value>,
  pub notes: Vec<Value>,
  /// the caller's own metadata
  pub labels: BTreeMap<String, String>,
  /// The room the item was filed from, where it names one: the room a job's end is told in.
  #[serde(default)]
  pub scope: Option<String>,
  /// The repository the item goes to, where it names one: an assignment naming no assignee takes a checkout
  /// of this repository alone.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub assignee_name: Option<String>,
  /// The item's small-model instruction, where it carries one.
  #[serde(default)]
  pub instruction: Option<JobInstruction>,
}

/// JobInstruction: the whole of a small-model instruction: the checkout the change is made in, the
/// canonical documents it answers to, the sentences it must make or keep true and the commands whose
/// passing says it is done; every one present and none empty.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobInstruction {
  pub repository: String,
  pub documents: Vec<String>,
  pub sentences: Vec<String>,
  pub checks: Vec<String>,
}

/// InstructionText: the one line a work-order step is written as, composed from an instruction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstructionText {
  pub text: String,
}

/// Job: itemId and leaseId are the harness's references, kept as given: the harness renews the lease, and reports and closes the item in the job service from report and outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Job {
  pub id: i64,
  pub item_id: i64,
  pub lease_id: i64,
  pub holder_name: String,
  pub assignment_id: i64,
  pub worker_name: String,
  /// The repository the checkout `workerName` names is of, none for a checkout no repository holds any longer.
  #[serde(default)]
  pub repository_name: Option<String>,
  /// The role the job runs as.
  #[serde(default)]
  pub role_name: Option<String>,
  /// The agent that does the job's work, the one its item's `agent` label named; none for a job launched before
  /// items named one. Its log's lines stand under this name.
  #[serde(default)]
  pub agent_name: Option<String>,
  pub session_id: Option<String>,
  pub run_id: Option<i64>,
  pub run_name: Option<String>,
  pub state: JobState,
  pub outcome: Option<String>,
  pub report: Option<String>,
  /// What the job's turn spent, the four figures of `usage` together; 0 until it ends or where its road
  /// reported no figures.
  pub token_count: i64,
  /// What the job's turn spent, apart; none until it ends or where its road reported no figures.
  #[serde(default)]
  pub usage: Option<RunUsage>,
  /// The small-model instruction the job carries, where it carries one.
  #[serde(default)]
  pub instruction: Option<JobInstruction>,
  /// Every road the job's turns went out on, in order, each with why the road before it was left.
  #[serde(default)]
  pub roads: Vec<JobRoad>,
  /// Why the job waits, where no provider of its order can answer now; none whenever a turn is out.
  #[serde(default)]
  pub held_for: Option<String>,
  /// What chose its provider and model; none for a job launched before it was kept.
  #[serde(default)]
  pub chosen_by: Option<JobChoice>,
  /// Where its review stands (docs/job-review.md).
  pub review: JobReview,
  /// epoch milliseconds
  pub started_at: i64,
  /// epoch milliseconds
  pub ended_at: Option<i64>,
}

/// JobReview: where a job's review stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobReview {
  /// The steps the job runs in order, empty for a job of one stage.
  pub stages: Vec<String>,
  /// The stage under way counted from 1, or none for the final report.
  pub stage: Option<i64>,
  pub round: i64,
  /// The check due now, kept while a question of it is out; none while a work turn runs.
  pub check: Option<String>,
  pub rejections: i64,
  /// The owner's question while the work waits on it.
  pub question: Option<i64>,
  /// Whether the job gave its checkout up while that question waited.
  pub parked: bool,
}

/// JobReviewed: where a job stands after its orderer's move, as `POST /job-reviews` answers it: nothing of its
/// report, its steps or its spend, which the review's instruction told or a read of the job answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobReviewed {
  pub id: i64,
  pub item_id: i64,
  pub state: JobState,
  /// Where its review stands now.
  pub review: ReviewPosition,
}

/// ReviewPosition: where a job's review stands, without its steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPosition {
  /// The stage under way counted from 1, or none for the final report.
  pub stage: Option<i64>,
  /// How many stages the job runs in, 1 for a job of one stage.
  pub stages: i64,
  pub round: i64,
  /// The check due now, kept while a question of it is out; none while a work turn runs.
  pub check: Option<String>,
  pub rejections: i64,
  /// The owner's question while the work waits on it.
  pub question: Option<i64>,
}

/// JobRoad: one road a job's turn went out on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobRoad {
  pub provider: String,
  pub model: String,
  /// The login it went out on, where its provider holds logins.
  pub account: Option<String>,
  /// epoch milliseconds
  pub at: i64,
  /// Why the road before it was left; none on the first.
  pub reason: Option<String>,
}

/// JobPage:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobPage {
  pub items: Vec<Job>,
  pub more: bool,
}

/// JobPrompt:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobPrompt {
  pub job_id: i64,
  pub text: String,
  pub english: Option<String>,
}

/// SessionWork:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionWork {
  pub session_id: String,
  pub item_id: Option<i64>,
  pub results: Vec<Value>,
}

/// SessionLoads:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionLoads {
  pub session_id: String,
  pub documents: Vec<Value>,
}

/// The body that asks for a worker to be assigned to an item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssignWorker {
  pub item_id: i64,
  pub holder_name: String,
  pub item: JobItem,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub assignee_name: Option<String>,
  /// The role the item's job runs as, a role the store holds; `worker` where it names none.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub role_name: Option<String>,
}

/// The body that asks for a checkout of another repository lent to a running job, which names the job by the run
/// it works in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BorrowAsk {
  pub run_name: String,
  pub repository_name: String,
  /// wait for a checkout to come free rather than be refused for none
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub wait: bool,
}

/// A checkout lent to a job: the checkout's name, where it stands, and the assignment held in the job's name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Borrowing {
  pub worker_name: String,
  pub cwd: String,
  pub assignment_id: i64,
}

/// The body that releases an assignment the harness will not run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseWorker {
  pub assignment_id: i64,
}

/// The body that releases an assignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssignmentChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub state: Option<AssignmentState>,
}

/// The body that hands an item and its lease over to be run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunJob {
  pub assignment_id: i64,
  pub item_id: i64,
  pub lease_id: i64,
  pub holder_name: String,
  pub item: JobItem,
  /// The job's small-model instruction, over any its item carries.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub instruction: Option<JobInstruction>,
}

/// The body that ends or stops a job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub state: Option<JobEnd>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub outcome: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub report: Option<String>,
}

/// The body that records the item a session serves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionWorkChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub item_id: Option<i64>,
}

/// The body that registers a hook's address at a point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HookRegistration {
  pub url: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub token: Option<String>,
}

/// The body that changes an agent's gates, reasoning, role, voice or own model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub gates: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reasoning: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub role: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub voice: Option<Value>,
  /// the agent's own model
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<AgentModel>,
}

/// An agent's own model: `fixed` on a provider and one model of it, or on a registration by its name
/// alone; `route` on the supervisor key's road, a supervisor's alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentModel {
  pub method: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub provider: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub registration: Option<String>,
}

/// The body that registers an agent: its key, label, kind, role and owner, and the fields its kind
/// carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentAsk {
  pub name: String,
  pub label: String,
  pub kind: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub role: Option<String>,
  /// the id of the login account that owns it, none for the administrator's
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub owner_account: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub icon: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub thinking: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub effort: Option<String>,
  /// the `chat` kind's pinned provider
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub provider: Option<String>,
  /// the `chat` kind's pinned model
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<String>,
  /// the worker a `chat` agent's items go to
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub worker: Option<String>,
  /// a `worker`'s checkout
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cwd: Option<String>,
  /// the node a `worker` stands on
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub node_name: Option<String>,
}

/// What became of a registered agent's brain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BrainBirth {
  Made,
  Kept,
  None,
  Failed,
}

/// An agent registered, and what became of its brain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentCreated {
  pub agent: Agent,
  pub brain: BrainBirth,
}

/// The body that creates, saves, removes or chooses a prompt document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptDocumentChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub action: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub label: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub text: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub agent: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub prompt: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub by: Option<String>,
}

/// The body that words which session documents moved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMoveAsk {
  pub paths: Vec<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub room_ids: Option<Vec<i64>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub rooms: Option<Vec<i64>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub now: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub agent: Option<String>,
}

/// What a session's start options name: the provider, the model and the folder a throwaway or a
/// conversation's turns go out on, and the item and role a co-work branch's place labels its run with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionOptions {
  pub provider: Option<String>,
  pub model: Option<String>,
  pub cwd: Option<String>,
  pub todo: Option<i64>,
  pub role: Option<String>,
}

/// The body that starts a session: the agent, the scope, the room and the options, with the older
/// top-level names read where the options name none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionStart {
  /// The token of the slot request a role's run was let in on: the run takes its role, provider, model, effort
  /// and, where the start names no folder, its path.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub slot: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub agent_name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub agent: Option<String>,
  pub scope: Value,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub room_id: Option<i64>,
  /// The runner line a thrown-away session goes out on with the instructions its caller hands in whole.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub line: Option<String>,
  /// The line's instructions, handed in whole: after the platform's harness opening on the search and browse
  /// lines, and as the whole system prompt on the plain line; needs `line`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub system_prompt: Option<String>,
  /// The program's own session the session's turns continue, the `harnessSession` an earlier turn answered on.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub continues: Option<String>,
  /// The words a turn goes out with where the session `continues` names cannot be resumed; needs `continues`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub continues_else: Option<String>,
  /// What the session's record carries under each key of its labels, for the placed tools that read them.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub labels: Option<serde_json::Map<String, Value>>,
  /// The capabilities the session's record keeps, of those its line and its agent's role give.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tools_allowed: Option<Vec<String>>,
  /// A granted demand of the caller's own on the arbiter's master, which each turn of a thrown-away session
  /// on a registration naming a card service stands on, with no other asked.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub demand_id: Option<i64>,
  /// The caller's granted demand each turn's own demand is asked within.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub within: Option<i64>,
  /// The rank a demand this work asks stands at, a whole number of zero or more.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub card_rank: Option<u32>,
  /// The pool a demand this work asks stands in.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub card_pool: Option<CardPool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub options: Option<SessionOptions>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub provider: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cwd: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub todo: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub role: Option<String>,
}

/// What a session's change names besides its model and holder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionChangeOptions {
  pub provider: Option<String>,
  pub model: Option<String>,
  pub cwd: Option<String>,
  pub session: Option<String>,
}

/// The body that moves a conversation's or a throwaway session's provider and model, or hands a worker's
/// session over.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub provider: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub options: Option<SessionChangeOptions>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub holder: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub prefix_change_reason: Option<String>,
}

/// A room's block as a turn's body names it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoomBlockAsk {
  pub chat_id: i64,
  pub text: Option<String>,
  pub mark: Option<String>,
}

/// What a role's session's turn names of its own; its road, effort and folder are the session's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkPartsAsk {
  pub resume: Option<Value>,
  pub name: Option<String>,
  pub title: Option<String>,
  pub chat: Option<i64>,
  pub unit: Option<String>,
  pub user: Option<String>,
  pub hears_words: Option<bool>,
  pub variables: Option<Value>,
}

/// The body that sends a turn: its prompt, the context around it, and what a role's session's turn names of its
/// own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TurnSend {
  pub prompt: String,
  /// The caller's key for the turn: a repeat of the send with it attaches to the turn standing under it, or
  /// is answered with its end once it has ended.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub key: Option<String>,
  /// The context as the answer echoes it; the parts below are what the turn reads.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub context: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub room: Option<RoomBlockAsk>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub via: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub base_persona: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub privileged: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub answers: Option<Vec<i64>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub began: Option<i64>,
  /// The round of the room the turn belongs to, which the turn's run keeps.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub round: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub job: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub item: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub resume: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub title: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub chat: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub unit: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub user: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub hears_words: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub variables: Option<Value>,
}

/// The body that interrupts a turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TurnInterrupt {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub state: Option<String>,
}

/// The body that pushes words into a working turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TurnWords {
  pub words: String,
}

/// One of the values the API accepts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum WordsSaid {
  #[serde(rename = "delivered")]
  Delivered,
  #[serde(rename = "waiting")]
  Waiting,
  #[serde(rename = "kept")]
  Kept,
  #[default]
  #[serde(rename = "refused")]
  Refused,
  #[serde(rename = "started")]
  Started,
}

/// One file said with the words, as its bytes stand base64-encoded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WordFile {
  pub name: String,
  #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
  pub kind: Option<String>,
  pub data: String,
}

/// The body that says words to a run's session, or to a repository's worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WordsAsk {
  #[serde(default, alias = "text")]
  pub words: String,
  /// Who says them: `owner`, or the supervisor that asked for the run.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub by: Option<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub files: Vec<WordFile>,
  /// The checkout of the repository the words are said to, where one is named.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub worker: Option<String>,
}

/// What became of words said to a run or a worker.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WordsSent {
  pub said: WordsSaid,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub session_id: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub run: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub turn_id: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub job_id: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reason: Option<String>,
}

/// The body that answers a question a turn stands on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnswerAsk {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub question_id: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub question: Option<String>,
  pub answer: Value,
}

/// The body a fold takes, which reads nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoldAsk {}

/// The body that opens a run in the ledger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunOpen {
  pub agent: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub chat_id: Option<i64>,
  pub unit: String,
  pub depth: u32,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cap_ms: Option<i64>,
  pub started_at: i64,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub started_by: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub search_id: Option<Value>,
  /// What the run was asked to do.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub title: Option<String>,
}

/// The body that closes a run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunClose {
  /// The message the run's answer went out as; named alone, it is attached and the closing left as it
  /// stands.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub message_id: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub finished_at: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub outcome: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub context_tokens: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub state: Option<String>,
  /// The older name for `finishedAt`, read where that is absent.
  #[serde(default, rename = "finished_at", skip_serializing_if = "Option::is_none")]
  pub finished_at_legacy: Option<i64>,
  /// The older name for `contextTokens`, read where that is absent.
  #[serde(default, rename = "context_tokens", skip_serializing_if = "Option::is_none")]
  pub context_tokens_legacy: Option<i64>,
}

/// The body that writes a note into a run's log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogLineAsk {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub agent: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub agent_name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub run: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub run_id: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub chat: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub chat_id: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub job: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub job_id: Option<i64>,
  pub text: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub kind: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub at: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub source: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub state: Option<String>,
}

/// A run's runner as a launch names it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProgramAsk {
  pub program: String,
  pub args: Vec<String>,
  pub cwd: String,
  pub environment: std::collections::BTreeMap<String, String>,
}

/// The body that launches a run's runner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionLaunch {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub run_id: Option<i64>,
  pub title: String,
  pub agent: String,
  pub command: ProgramAsk,
}

/// The body that cancels an execution or takes the cancel back, which reads nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionChange {}

/// The body that answers an overtime request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OvertimeAnswer {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub ask: Option<i64>,
  pub verdict: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub minutes: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub why: Option<String>,
}

/// The body that marks a job to stop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StopMarkAsk {
  pub reason: String,
}

/// What a seat request or a reservation asks of the pool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PoolWanted {
  pub role: String,
  pub selection: String,
  pub tier: Option<String>,
  pub provider: Option<String>,
  pub work_key: Option<String>,
}

/// The body that asks for a co-worker seat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SeatRequestAsk {
  pub request_key: String,
  pub owner: String,
  pub role: String,
  pub selection: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tier: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub provider: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub work_key: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub generation: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub yielding: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub token: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub place_key: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub source: Option<String>,
  /// One line naming the work the seat is asked for.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub title: Option<String>,
  /// The agent asking for the seat.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub asked_by: Option<String>,
}

/// The body that moves a seat request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SeatRequestChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub state: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub action: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub child_id: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub outcome: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cancel: Option<bool>,
  /// On an admission ask: a granted demand of the caller's own the seat stands on, with none of its own
  /// asked.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub demand_id: Option<i64>,
  /// On an admission ask: the asking run's granted demand the seat's own is asked within.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub within: Option<i64>,
  /// The rank a demand this work asks stands at, a whole number of zero or more.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub card_rank: Option<u32>,
  /// The pool a demand this work asks stands in.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub card_pool: Option<CardPool>,
  /// On a binding: the agent the seat's child runs on.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub agent_name: Option<String>,
}

/// The body that reserves seats for an owner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReservationAsk {
  pub reservation_key: String,
  pub owner: String,
  pub source: String,
  pub role: String,
  pub selection: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tier: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub provider: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub work_key: Option<String>,
  pub count: i64,
}

/// The body that releases a reservation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReservationChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub outcome: Option<String>,
}

/// The body that takes a branch place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlaceAsk {
  pub cwd: String,
  pub selection: String,
  pub role: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tier: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub held: Option<Vec<Value>>,
}

/// The body that prepares, remembers or retires a place, or records its last write.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlaceChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub state: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub source: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cwd: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub slot: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub role: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub mode: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub effort: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub upstream: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub used: Option<Vec<i64>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub held: Option<Vec<Value>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub provider: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub last_write: Option<Value>,
}

/// The body that throws a provider's switch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub enabled: Option<bool>,
}

/// One address of a registration set, its key named by a file in the platform's space of the secret store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelAddressAsk {
  /// An http or https address ending in `/chat/completions`.
  pub url: String,
  /// One plain file name under the data root's `env/`, or null or absent for no key. Agency keeps the name alone.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub key_file: Option<String>,
}

/// The body that saves or removes a registration, or sets or removes one of its addresses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub action: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub label: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub context_tokens: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub answer_tokens: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub thinking: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub walked: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub keeps_research: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub node: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub url: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub key: Option<String>,
  /// Whether the instance behind the address keeps role files, or on a registration naming a card service
  /// whether its instances do; left as it stands where none is named.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub role_files: Option<bool>,
  /// The arbiter service the registration's engine is, or `none`; naming one takes its addresses away.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub card_service: Option<String>,
  /// The languages a run on the registration writes a result for a person in, each by its English name; an
  /// empty list or null names none, which means every language. Left as they stand where none is named.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub languages: Option<Vec<String>>,
  /// The user id the registration belongs to, or null to give it back to the platform; the owner's alone to
  /// set. Left as it stands where none is named.
  #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
  pub account: Option<Option<i64>>,
}

/// One member of the gateway's members file; its key file is never reported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GatewayMember {
  pub name: String,
  /// The agent provider it belongs to; null for a member the gateway serves alone.
  pub road: Option<String>,
  /// What a screen shows in place of the name; null where the file names none.
  pub label: Option<String>,
  /// The LiteLLM provider it goes out on.
  pub provider: String,
  /// The user id it belongs to; null for the platform's.
  pub account: Option<i64>,
}

/// The gateway's members, in the members file's order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GatewayMemberPage {
  pub items: Vec<GatewayMember>,
}

/// A change of a gateway member's owner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GatewayMemberChange {
  /// The user id the member belongs to, or null to give it back to the platform.
  pub account: Option<i64>,
}

/// The body that assigns a tier to a work key or sets a tier's mapping on a provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TierChange {
  pub action: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tier: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub key: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub accept_conflicts: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub provider: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub thinking: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub effort: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub answer_tokens: Option<i64>,
}

/// The body that changes the fallback screen's routes, providers and accounts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FallbackChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub action: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub order: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub provider: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub enabled: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub account: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub kind: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub path: Option<String>,
  /// An endpoint account's base URL.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub endpoint: Option<String>,
  /// An endpoint account's key file, or the secret an `api-key` account's key sits in: the person's own
  /// `users/<user>/<name>` or a plain name, or the platform's plain name.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub key_file: Option<String>,
  /// An endpoint account's model ids by family word.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub models: Option<EndpointModels>,
  /// The base prompt the account names, `none` or null to let it go.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub base_prompt: Option<String>,
  /// Whether the account goes out for another person's run on a platform source; the administrator's alone.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub lendable: Option<bool>,
  /// `update-account`: the organisation unit a platform account is given to, null to give it back; the owner's
  /// alone.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub unit_id: Option<Option<i64>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub key: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub mode: Option<String>,
  /// The registration a work key runs on at the registry's provider in place of its tier's cell, `auto`
  /// or null to read the cell again.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub registration: Option<String>,
}

/// The body that blocks a selection's road.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoadBlockAsk {
  pub selection: String,
  pub until: i64,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reason: Option<String>,
}

/// The body that tells the launcher what holds launches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LaunchHeldAsk {
  pub held: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reasons: Option<Vec<String>>,
}

/// The body that reports a login's session usage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsage {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub usage: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub window: Option<Value>,
}

/// The body that writes a rest down.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestAsk {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub clear_passed: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub until: Option<f64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub kind: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub account: Option<String>,
}

/// The body that changes agency's own switches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SettingsAsk {
  /// A supervisor the store holds, or null for none.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub research_supervisor: Option<Value>,
  /// A console secret's name, one plain file name, or null for none.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub deepseek_card_secret: Option<Value>,
  /// A console secret's name, one plain file name, or null for none.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub deepseek_login_secret: Option<Value>,
  /// The minutes between two flushes of the deploy queue, a positive whole number, or null for the policy's.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub deploy_flush_minutes: Option<Value>,
}

/// A supervisor's sleep setting (docs/sleep.md, 「Its setting」): whether it sleeps and its own hour, none for the
/// policy's, beside the policy's hour, the day it last slept for and the moment it last tried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorSleep {
  pub on: bool,
  pub hour: Option<u32>,
  pub default_hour: u32,
  pub slept_on: Option<String>,
  pub tried_at: Option<i64>,
}

/// The body that sets a supervisor's sleep: whether it sleeps, and its own hour from 0 to 23, or null for the
/// policy's.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorSleepChange {
  pub on: bool,
  #[serde(default)]
  pub hour: Option<u32>,
}

/// The body that changes a supervisor's settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorSettingsChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub by: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reference_on: Option<bool>,
  /// Whether the supervisor's brain reaches its turns.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub brain_on: Option<bool>,
  /// The capabilities its sessions keep, a list, or null for everything they are given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tools_allowed: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub persona: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub registration: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub generation: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub thinking: Option<bool>,
}

/// The body that asks for a supervisor reset.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorResetAsk {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub conversation: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub person: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub alone: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub shared: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub dry_run: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub by: Option<String>,
}

/// The body that adds a worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerAdd {
  pub name: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub label: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cwd: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub icon: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub role: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub node_name: Option<String>,
  /// The documents its session's prefix carries, in order.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub prefix_documents: Option<Vec<String>>,
}

/// The body that changes a worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sampling: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub node_name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub node: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub label: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cwd: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub icon: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub role: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub stand: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub bench: Option<bool>,
  /// The documents its session's prefix carries, in order, the list whole; an empty list clears it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub prefix_documents: Option<Vec<String>>,
}

/// The body that keeps a session's loads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionLoadsAsk {
  pub documents: Vec<Value>,
}

/// GithubDeliveryTaken: a GitHub delivery taken, to be acted on after the answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GithubDeliveryTaken {
  pub taken: bool,
}

/// TranslationRunAsk: one text to translate on the `translate` road: what it is for (`document`, `worker` or
/// `record`), the language to render it in and the text.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TranslationRunAsk {
  pub purpose: String,
  pub language: String,
  pub text: String,
  /// The caller's word on this one part, carried into the run's input as a note not to be translated.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub note: Option<String>,
  /// The words of this one part the translation renders as the caller says, set before the text as the
  /// translation model's reference translations.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub terms: Vec<TranslationTerm>,
}

/// TranslationTerm: a word of a part to translate and how the translation writes it: `source` as the text
/// writes it, and `target`, the source itself where none is named.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TranslationTerm {
  pub source: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub target: Option<String>,
}

/// TranslationRun: a translation as it came back, whole, with the road it went out on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TranslationRun {
  pub text: String,
  pub provider: String,
  pub model: String,
}

/// One gate kind as the gate declares it: its key, the part of a turn it stands in, its step there and
/// the condition it runs under where it runs in some turns only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GateKind {
  pub key: String,
  pub part: String,
  pub step: i64,
  pub condition: Option<String>,
}

/// A page of the gate kinds the gate declared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GateKindPage {
  pub items: Vec<GateKind>,
  pub more: bool,
}

/// One gate of the gate's declaration, as the gate publishes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredGate {
  pub gate_name: String,
  /// `pre`, `post` or `apart`.
  pub part: String,
  pub step: i64,
  pub condition: Option<String>,
  pub switchable: bool,
  pub laya_probe: bool,
  pub default_laya_mode: Option<String>,
  pub answer_tokens: Option<i64>,
}

/// The gate's declaration of its kinds, written over the one standing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GateKindDeclaration {
  pub gates: Vec<DeclaredGate>,
}

/// A pool the demands of a work stand in: its key, and the most cards they use of one service at once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CardPool {
  pub key: String,
  pub cards: u32,
}

/// Supervisor: one supervisor as its row holds it, on its one road.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Supervisor {
  pub name: String,
  pub label: Option<String>,
  pub owner_account: Option<i64>,
  /// its call names as one line, joined by a comma and a space; null for none
  pub aliases: Option<String>,
  pub icon: Option<String>,
  pub session_scope: String,
  pub turn_shape: String,
  pub brain_folder: Option<String>,
  /// the repository its TODO items go to
  pub repository: Option<String>,
  /// its one road's provider, every turn of it going out there; null where its row names no road
  #[serde(default)]
  pub provider: Option<String>,
  #[serde(default)]
  pub model: Option<String>,
  /// a level of the effort scale; null for the provider's own
  #[serde(default)]
  pub effort: Option<String>,
  /// `on`, `off`, or null for the provider's own
  #[serde(default)]
  pub thinking: Option<String>,
  /// on the registry's provider, the answer ceiling; null for the registration's own
  #[serde(default)]
  pub answer_tokens: Option<i64>,
  /// the model its program may consult, a model of the provider or a family word; null for none
  #[serde(default)]
  pub advisor: Option<String>,
  /// for each provider, whether its prefix is kept warm there
  pub warming: BTreeMap<String, bool>,
  /// its compaction threshold
  pub fold_ratio: f64,
  /// the rank of the demands of the runs for it, where it holds one
  #[serde(default)]
  pub card_rank: Option<u32>,
  pub gates: Value,
  pub voice: Option<String>,
  pub sampling: Option<Value>,
  /// Whether the deployment seeds it: its removal is refused 409 `seeded`.
  #[serde(default)]
  pub seeded: bool,
  /// The moment in epoch milliseconds its last reset cut its turns off at, none where it was never reset.
  #[serde(default)]
  pub reset_point: Option<i64>,
}

/// SupervisorRoom: one room a supervisor's turns went into, with its latest run there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorRoom {
  pub room_id: i64,
  /// How many of its runs the ledger holds there.
  pub runs: i64,
  pub last_run_id: i64,
  pub last_run_unit: String,
  /// epoch milliseconds
  pub last_started_at: i64,
  /// epoch milliseconds
  pub last_finished_at: Option<i64>,
}

/// SupervisorRoomList: the rooms a supervisor's turns went into, the latest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorRoomList {
  pub items: Vec<SupervisorRoom>,
  pub more: bool,
}

/// SupervisorResetPoint: a supervisor's reset point alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorResetPoint {
  pub supervisor: String,
  /// epoch milliseconds, or null where it was never reset
  pub reset_point: Option<i64>,
}

/// SupervisorList: every supervisor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorList {
  pub items: Vec<Supervisor>,
  pub more: bool,
}

/// SupervisorAsk: the body that registers a supervisor on its one road.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorAsk {
  pub name: String,
  pub label: String,
  /// Its road's provider and model, both named.
  pub provider: String,
  pub model: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub effort: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub thinking: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub answer_tokens: Option<i64>,
  /// the model its program may consult, on a provider that takes an advisor
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub advisor: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub repository: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub owner_account: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub fold_ratio: Option<f64>,
}

/// SupervisorRegistered: a supervisor registered, and what became of its brain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorRegistered {
  pub supervisor: Supervisor,
  pub brain: BrainBirth,
}

/// SupervisorChange: the fields a change of a supervisor names.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub label: Option<String>,
  /// The whole list of its call names, at most ten, each 1 to 40 characters with no comma; null or empty for none.
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub aliases: Option<Option<Vec<String>>>,
  /// Its road's provider; a changed provider or model drops the road's other fields not named.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub provider: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<String>,
  /// null for the provider's own
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub effort: Option<Option<String>>,
  /// `on`, `off`, or null for the provider's own
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub thinking: Option<Option<String>>,
  /// null for the registration's own
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub answer_tokens: Option<Option<i64>>,
  /// the model its program may consult, a model of the provider or a family word; null for none
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub advisor: Option<Option<String>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub warming: Option<BTreeMap<String, bool>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub fold_ratio: Option<f64>,
  /// the rank of the demands of the runs for it; null for none
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub card_rank: Option<Option<u32>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub repository: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub gates: Option<BTreeMap<String, bool>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sampling: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub voice: Option<String>,
}

/// DeleteSupervisorQuery: what a supervisor's removal is asked with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSupervisorQuery {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub force: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub forget_brain: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub rooms: Option<String>,
}

/// RoleBundle: one bundle a role may give, with its kebab-case code and its label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleBundle {
  pub name: String,
  pub code: String,
  pub label: String,
}

/// Role: one role as its row holds it, and what stands of it now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Role {
  pub name: String,
  pub code: String,
  pub label: String,
  pub prompt: Option<String>,
  pub description: Option<String>,
  pub bundles: Vec<RoleBundle>,
  /// the skills its runs carry, or null where it names none and its runs carry the whole plugin the release
  /// places for it
  #[serde(default)]
  pub skills: Option<Vec<String>>,
  pub line: String,
  /// its agents, in the order a run looks for a free one
  #[serde(default)]
  pub agents: Vec<RoleAgent>,
  /// the most of its runs that go at once: how many of its agents are switched on
  pub max_concurrent: i64,
  /// Whether a run of it a supervisor asks goes out on that supervisor's road: its switched-on agents name no
  /// road, or each follows its asker; a screen shows its road as the supervisor's.
  #[serde(default)]
  pub follows_supervisor: bool,
  /// whether its agents follow the supervisor that asks a run of them, each one's own road standing where no
  /// supervisor's does or that road cannot answer now; an agent's own setting stands over it
  #[serde(default)]
  pub follows_asker: bool,
  /// the text a run of it is given beside its prompt where its program stands on a provider, by provider
  #[serde(default)]
  pub provider_prompts: std::collections::BTreeMap<String, String>,
  pub running: i64,
  pub waiting: i64,
  /// the provider its next run goes out on, or null where none of its agents' roads can answer or none
  /// names a road
  pub provider: Option<String>,
  pub waiting_for: Option<String>,
}

/// RoleList: every role, every bundle a role may give, every skill a role or an agent may name and every line
/// a role may name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleList {
  pub roles: Vec<Role>,
  pub bundles: Vec<RoleBundle>,
  /// the name of every skill a plugin of the release carries
  #[serde(default)]
  pub skills: Vec<String>,
  pub lines: Vec<String>,
}

/// RoleAsk: the body that adds a role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleAsk {
  pub name: String,
  pub label: String,
  pub bundles: Vec<String>,
  pub line: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub prompt: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub description: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub skills: Option<Vec<String>>,
  /// the text a run of it is given beside its prompt where its program stands on a provider, by provider
  #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
  pub provider_prompts: std::collections::BTreeMap<String, String>,
  /// whether its agents follow the supervisor that asks a run of them; false where it names none
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub follows_asker: Option<bool>,
}

/// RoleChange: the fields a change of a role names; a prompt, a description or the skills given as null clears
/// it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub label: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub bundles: Option<Vec<String>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub line: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub prompt: Option<Option<String>>,
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub description: Option<Option<String>>,
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub skills: Option<Option<Vec<String>>>,
  /// each provider's prompt set, or taken away where it is given as null; a provider left out stands as it is
  #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
  pub provider_prompts: std::collections::BTreeMap<String, Option<String>>,
  /// whether its agents follow the supervisor that asks a run of them
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub follows_asker: Option<bool>,
}

/// RoleAgent: one agent of a role as its row holds it (docs/role-agents.md): one seat of the role, on the one
/// road its runs go out on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleAgent {
  pub name: String,
  pub role: String,
  /// its road's provider, or null where it follows the supervisor that asks it
  pub provider: Option<String>,
  pub model: Option<String>,
  pub effort: Option<String>,
  /// `on`, `off`, or null for the provider's own
  pub thinking: Option<String>,
  pub answer_tokens: Option<i64>,
  /// the model its program may consult, or null for none
  #[serde(default)]
  pub advisor: Option<String>,
  /// an agent switched off takes no run
  pub enabled: bool,
  /// whether the session its last answered run kept is pinged before its cache cools
  pub keeps_cache: bool,
  /// whether its runs keep their sessions
  pub sessions: RoleSessions,
  /// its compaction threshold, or null where the ratio its provider declares stands
  pub fold_ratio: Option<f64>,
  /// what it lays over its role's preset, each null where the role's own stands
  pub prompt: Option<String>,
  pub bundles: Option<Vec<String>>,
  pub skills: Option<Vec<String>>,
  /// epoch milliseconds
  pub added_at: i64,
  /// whether it follows its asker, its own road standing where no supervisor's does or that road cannot answer
  /// now, or null where its role's setting stands
  #[serde(default)]
  pub follows_asker: Option<bool>,
  /// whether a run of it a supervisor asks goes out on that supervisor's road: it names no road, or it follows
  /// its asker by its own setting or its role's
  #[serde(default)]
  pub follows_supervisor: bool,
  /// whether a run stands let in on it now
  #[serde(default)]
  pub carries: bool,
}

/// RoleAgentPage: the agents, by role and name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleAgentPage {
  pub items: Vec<RoleAgent>,
}

/// RoleAgentAsk: the body that adds an agent to a role; every field but the role is optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleAgentAsk {
  pub role: String,
  /// kebab-case; the role and a number where none is named
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
  #[serde(flatten)]
  pub fields: RoleAgentChange,
}

/// RoleAgentChange: the fields a change of an agent names; a field given as null is cleared, which for an
/// override gives the role's own back.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleAgentChange {
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub provider: Option<Option<String>>,
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub model: Option<Option<String>>,
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub effort: Option<Option<String>>,
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub thinking: Option<Option<String>>,
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub answer_tokens: Option<Option<i64>>,
  /// the model its program may consult, a model of the provider or a family word; null for none
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub advisor: Option<Option<String>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub enabled: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub keeps_cache: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sessions: Option<RoleSessions>,
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub fold_ratio: Option<Option<f64>>,
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub prompt: Option<Option<String>>,
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub bundles: Option<Option<Vec<String>>>,
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub skills: Option<Option<Vec<String>>>,
  /// whether it follows its asker, its own road standing where no supervisor's does or that road cannot answer
  /// now; null gives its role's setting back
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub follows_asker: Option<Option<bool>>,
}

/// RoleSession: one session a role's runs opened, and whether the next run of its key would resume it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleSession {
  pub id: i64,
  pub session_id: String,
  pub path: String,
  pub provider: String,
  pub model: String,
  pub effort: Option<String>,
  pub account: Option<String>,
  /// epoch milliseconds
  pub opened_at: i64,
  /// epoch milliseconds
  pub last_answered_at: Option<i64>,
  pub runs: i64,
  pub context: RoleSessionContext,
  pub valid: bool,
}

/// RoleSessionContext: what a session's last answered turn carried.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleSessionContext {
  /// The tokens it carried, or null where none was read.
  pub tokens: Option<i64>,
  /// The share of the window they filled, from 0 to 1, or null.
  pub share: Option<f64>,
}

/// RoleSessionPage: the sessions a role's runs opened, newest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleSessionPage {
  pub items: Vec<RoleSession>,
  pub more: bool,
}

/// RoleSessionsQuery: the page of a role's sessions asked for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleSessionsQuery {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub before: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
}

/// RepositoryPin: a repository's pinned provider and model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryPin {
  pub provider: String,
  pub model: String,
}

/// CheckoutJob: the job holding a checkout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutJob {
  pub job_id: Option<i64>,
  pub item_id: Option<i64>,
  /// the role the job runs as
  pub role: Option<String>,
}

/// RepositoryCheckout: one checkout of a repository.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryCheckout {
  pub name: String,
  pub path: String,
  /// The work node it stands on, or null for this machine.
  pub node_name: Option<String>,
  /// `base`, `support` or `node`
  pub kind: String,
  /// The job holding it, or null where it is free.
  pub job: Option<CheckoutJob>,
}

/// SamplingSetting: one setting a sampling may hold, with its bounds; a switch has none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SamplingSetting {
  pub name: String,
  /// Whether it takes on or off rather than a number.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub switch: Option<bool>,
  /// Whether the chat template reads it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub template: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub min: Option<f64>,
  /// the most it takes, or null where it is held below the registration's window
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub max: Option<f64>,
  /// Whether the least value itself is refused.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub above_min: Option<bool>,
  /// Whether only whole numbers are taken.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub whole: Option<bool>,
}

/// NodeCheckoutAsk: one checkout on a work node a repository's change names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeCheckoutAsk {
  pub node_name: String,
  /// written from the node's home or absolute; null for the default beside the base's folder
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub path: Option<String>,
}

/// WorkNode: one node the node service keeps, with whether it takes jobs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkNode {
  pub name: String,
  pub label: String,
  /// `ssh`, or `jake` for a node reached through a hub
  pub kind: String,
  /// the services the node service reserves it for, or null
  pub role: Option<String>,
  pub state: String,
  /// epoch seconds
  pub checked_at: Option<i64>,
  pub takes_jobs: bool,
  /// why it takes no jobs, or null where it takes them
  pub jobs_refusal: Option<String>,
}

/// WorkNodeList: the nodes the node service keeps, in its list's order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkNodeList {
  pub items: Vec<WorkNode>,
  pub more: bool,
}

/// PersonNode: the node an account's runs stand on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonNode {
  pub owner_account: String,
  pub node: String,
  /// Whether the administrator declared the node shared for the account.
  pub shared: bool,
  /// epoch milliseconds
  pub set_at: i64,
  /// Whether the node's link is up now: connected and heard from in the last twenty seconds.
  #[serde(default)]
  pub linked: bool,
}

/// PersonNodeList: every account's node, by account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonNodeList {
  pub items: Vec<PersonNode>,
  pub more: bool,
}

/// One entry of a personal app, by its full name, as the tool host holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalAppEntry {
  pub name: String,
  #[serde(default)]
  pub title: Option<String>,
  pub description: String,
}

/// What a personal app's manifest asks for beyond its own folders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalAppPermissions {
  pub network: Vec<String>,
  pub secrets: Vec<String>,
}

/// PersonalApp: one of an account's personal apps: the version in place, when it was put, its entries, its
/// permissions, and the versions its PC keeps older than the one in place, newest first, null where the PC was not
/// reached.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalApp {
  pub name: String,
  /// `mcp` for an MCP server, `app` for every other.
  pub kind: String,
  pub version: String,
  pub put_at: i64,
  pub entries: Vec<PersonalAppEntry>,
  pub permissions: PersonalAppPermissions,
  pub kept: Option<Vec<String>>,
}

/// PersonalAppList: an account's personal apps, and whether its PC was reached for the versions it keeps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalAppList {
  pub items: Vec<PersonalApp>,
  pub reached: bool,
}

/// PersonalAppBack: the version a personal app is sent back to, the newest kept older than the current where none
/// is named.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalAppBack {
  #[serde(default)]
  pub version: Option<String>,
}

/// PersonalAppDone: what a put, a going back or a removal of a personal app came to: the version now in place,
/// none after a removal, the entries the account's sessions reach, and what the tool host and the build said.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalAppDone {
  pub name: String,
  pub version: Option<String>,
  pub entries: Vec<PersonalAppEntry>,
  pub said: Vec<String>,
}

/// ScreenSession: a short-lived tool session a console reads and calls a person's personal screens under: the token
/// the tool host issued, and when it lapses, in epoch milliseconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScreenSession {
  pub token: String,
  pub expires_at: u64,
}

/// CatalogueVersion: one version of a shared app: when it was shared, its source hash, what it asks for beyond its
/// own folders, and its review report's state and text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueVersion {
  pub version: String,
  pub shared_at: i64,
  pub source_hash: String,
  pub permissions: PersonalAppPermissions,
  pub review: CatalogueReview,
}

/// CatalogueReview: a version's review report: `pending`, `written` with its text, or `failed` with why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueReview {
  pub state: String,
  #[serde(default)]
  pub report: Option<String>,
  #[serde(default)]
  pub at: Option<i64>,
}

/// CatalogueApproval: whether a head must approve the newest version before anybody installs it, and whether one did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueApproval {
  pub required: bool,
  pub approved: bool,
}

/// CatalogueCopy: the reader's copy of a shared app: the version it stands on, and how it came (`self` or `unit`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueCopy {
  pub version: String,
  pub how: String,
}

/// CatalogueMay: what the reader may do with a shared app; `recall` and `turnOn` name the links they may do it at,
/// `unit:<id>` for a unit they head and `platform` for the administrator.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueMay {
  pub install: bool,
  pub update: bool,
  pub uninstall: bool,
  pub clone: bool,
  pub withdraw: bool,
  pub approve: bool,
  pub promote: bool,
  /// Whether the reader sets a locale's text: the maker and the owner.
  #[serde(default)]
  pub text: bool,
  /// Whether the reader asks for the other languages to be filled: the maker, the owner and a head of the maker's
  /// home unit.
  #[serde(default)]
  pub translate: bool,
  pub recall: Vec<String>,
  pub turn_on: Vec<String>,
}

/// CatalogueServer: how a shared MCP server is reached, read off its newest version's `mcp.toml`: `local`, its
/// `command`; or `remote`, its `url`, its `auth` (`none`, `bearer` or `header`), the `header` it is sent in and the
/// name of the `secret` each receiver holds, never its value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueServer {
  pub transport: String,
  #[serde(default)]
  pub url: Option<String>,
  #[serde(default)]
  pub auth: Option<String>,
  #[serde(default)]
  pub header: Option<String>,
  #[serde(default)]
  pub secret: Option<String>,
  #[serde(default)]
  pub command: Option<Vec<String>>,
}

/// CatalogueApp: a shared app as its reader sees it in their company's catalogue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueApp {
  pub id: String,
  /// `app`, `skill` or `mcp`.
  pub kind: String,
  pub key: String,
  pub name: String,
  /// The maker's language's title and description.
  pub title: String,
  pub description: String,
  /// The maker's language, where the item names one.
  #[serde(default)]
  pub language: Option<String>,
  /// The text by locale, each locale it is written in.
  #[serde(default)]
  pub titles: BTreeMap<String, String>,
  #[serde(default)]
  pub descriptions: BTreeMap<String, String>,
  /// The locales whose text a translation wrote.
  #[serde(default)]
  pub machine: Vec<String>,
  /// How an MCP server is reached; none for any other kind.
  #[serde(default)]
  pub server: Option<CatalogueServer>,
  pub maker: String,
  pub owner: String,
  pub targets: Vec<Value>,
  pub version: String,
  pub versions: Vec<CatalogueVersion>,
  pub permissions: PersonalAppPermissions,
  pub review: CatalogueReview,
  pub approval: CatalogueApproval,
  pub installs: i64,
  #[serde(default)]
  pub withdrawn: Option<i64>,
  #[serde(default)]
  pub promotion: Option<Value>,
  #[serde(default)]
  pub installed: Option<CatalogueCopy>,
  pub update_waiting: bool,
  pub permissions_grown: bool,
  pub may: CatalogueMay,
}

/// CatalogueNotice: what the reader is told of: an app shared with them since they last looked (`shared`), a copy
/// waiting for them to confirm what it asks for (`update`), or a version waiting for their approval (`approval`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueNotice {
  pub kind: String,
  pub app: String,
  pub at: i64,
}

/// CatalogueList: the reader's company's catalogue as they see it: the targets they may share with, the apps, their
/// notices, and when they last looked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueList {
  pub company: String,
  pub ceiling: Vec<String>,
  pub apps: Vec<CatalogueApp>,
  pub notices: Vec<CatalogueNotice>,
  #[serde(default)]
  pub seen_at: Option<i64>,
}

/// CatalogueSource: a version's source, checked against the hash recorded as it was shared, every file by its path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueSource {
  pub version: String,
  pub source_hash: String,
  pub files: BTreeMap<String, String>,
}

/// CatalogueShare: one of the reader's personal apps shared by its name, with whom: each target `{"team": true}`,
/// `{"heads": true}`, `{"company": true}`, `{"person": <user>}`, `{"group": <id>}` or `{"unit": <id>}`; none shares
/// with the team and the heads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueShare {
  pub name: String,
  #[serde(default)]
  pub targets: Vec<Value>,
  /// `app` where left out, for an app or an MCP server of the PC's 'my apps'; `skill` for an own skill at home.
  #[serde(default)]
  pub kind: Option<String>,
  /// The language its plain text is written in, one of `ko`, `en`, `ja` and `zh-CN`; `en` where left out.
  #[serde(default)]
  pub language: Option<String>,
}

/// CatalogueText: one locale's title and description set on a shared item, or the locale cleared where both are
/// null.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueText {
  pub locale: String,
  #[serde(default)]
  pub title: Option<String>,
  #[serde(default)]
  pub description: Option<String>,
}

/// PersonalSkill: one of an account's skills at home: its front matter's text, its version, whether it is the
/// person's own or an installed copy, the catalogue id a copy is of, and the holders it is attached to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalSkill {
  pub name: String,
  pub description: String,
  #[serde(default)]
  pub title: Option<String>,
  #[serde(default)]
  pub summary: Option<String>,
  #[serde(default)]
  pub titles: BTreeMap<String, String>,
  #[serde(default)]
  pub summaries: BTreeMap<String, String>,
  #[serde(default)]
  pub language: Option<String>,
  pub version: String,
  pub origin: String,
  #[serde(default)]
  pub app: Option<String>,
  #[serde(default)]
  pub attached_to: Vec<String>,
}

/// PersonalSkillList: an account's skills at home.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalSkillList {
  pub skills: Vec<PersonalSkill>,
}

/// PersonalSkillFiles: one skill's files, by path, with its version and origin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalSkillFiles {
  pub name: String,
  pub version: String,
  pub origin: String,
  pub files: BTreeMap<String, String>,
}

/// PersonalSkillSave: an own skill's files, by path, `SKILL.md` among them, saved whole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalSkillSave {
  pub files: BTreeMap<String, String>,
}

/// PersonalSkillRemoved: the skill removed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonalSkillRemoved {
  pub name: String,
}

/// AttachmentTakes: whether a holder's line takes attached skills, and attached MCP servers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentTakes {
  pub skills: bool,
  pub mcp: bool,
}

/// AttachmentHolder: one of an account's holders — a `supervisor` or a role's `agent` — its line, what that line
/// takes, and the skills and MCP servers attached to it, by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentHolder {
  pub kind: String,
  pub name: String,
  #[serde(default)]
  pub label: Option<String>,
  #[serde(default)]
  pub role: Option<String>,
  pub line: String,
  pub takes: AttachmentTakes,
  pub skills: Vec<String>,
  pub mcp: Vec<String>,
}

/// AttachableItem: a skill or an MCP server of an account's that may be attached: its name, its title and titles
/// where it has them, and whether it is the person's own or an installed copy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttachableItem {
  pub name: String,
  #[serde(default)]
  pub title: Option<String>,
  #[serde(default)]
  pub titles: BTreeMap<String, String>,
  pub origin: String,
}

/// AttachmentList: an account's holders and what is attached to each, and the skills and MCP servers that may be.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentList {
  pub holders: Vec<AttachmentHolder>,
  pub skills: Vec<AttachableItem>,
  pub mcp: Vec<AttachableItem>,
}

/// AttachmentSet: one holder's attached skills and MCP servers, by name, set whole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentSet {
  #[serde(default)]
  pub skills: Vec<String>,
  #[serde(default)]
  pub mcp: Vec<String>,
}

/// CatalogueUnitSwitch: a shared app turned on or off for a unit, locked for those under it where `lock` says so.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueUnitSwitch {
  pub on: bool,
  #[serde(default)]
  pub lock: bool,
}

/// CatalogueRecall: the unit a head recalls a shared app at; none for the administrator's recall at the platform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueRecall {
  #[serde(default)]
  pub unit: Option<i64>,
}

/// CatalogueApprove: the version a head approves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueApprove {
  pub version: String,
}

/// CataloguePromote: the request to make a shared app a platform app, with a note for the administrator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CataloguePromote {
  #[serde(default)]
  pub note: String,
}

/// CataloguePromotion: one request to make a shared app a platform app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CataloguePromotion {
  pub company: String,
  pub id: String,
  pub title: String,
  pub maker: String,
  pub by: String,
  pub at: i64,
  pub note: String,
}

/// CataloguePromotions: every company's requests to make a shared app a platform app, newest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CataloguePromotions {
  pub items: Vec<CataloguePromotion>,
}

/// CatalogueSeen: when the reader last looked at what was shared with them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueSeen {
  pub at: i64,
}

/// A Claude account's sign-in started from the console: a directory account there by its id, or a new account by
/// its name, at most one of the two; naming neither starts a new account agency names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeLoginStart {
  #[serde(default)]
  pub account: Option<String>,
  #[serde(default)]
  pub name: Option<String>,
}

/// A sign-in under way: its identifier, the address the owner approves it at, and the name of the account it signs
/// in, the one agency gave a new login whose start named none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeLoginStarted {
  pub login: String,
  pub url: String,
  pub name: String,
}

/// The code the sign-in page showed, handed to the sign-in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeLoginCode {
  pub code: String,
}

/// The body that sets an account's node: a hub node the node service keeps, and whether it is shared with other
/// accounts, false where left out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonNodeAsk {
  pub node: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub shared: Option<bool>,
}

/// NodeAccess: whether a caller reaches a node, as the node service's tool asks it before it runs a command there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeAccess {
  pub node: String,
  /// The accounts whose record names the node; none for a node that is no person's.
  pub owners: Vec<String>,
  pub agent: String,
  /// The account the caller acts for; null for the deployment's own.
  pub account: Option<String>,
  /// Whether the caller holds the administrator's grant, which reaches every node.
  pub administrator: bool,
  pub reaches: bool,
  /// The node path of the caller's account's workplace root where its record names the node shared; null otherwise.
  pub root: Option<String>,
}

/// SupportCount: how many support checkouts a repository has beside its base: a whole number from 0 to 8, or
/// `auto` for as many as its launches grow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SupportCount {
  Count(i64),
  Grown(SupportAuto),
}

/// SupportAuto: the support count that grows a checkout at a launch that finds every one held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SupportAuto {
  Auto,
}

impl SupportCount {
  /// The count a repository takes where none is named.
  pub const AUTO: Self = Self::Grown(SupportAuto::Auto);

  /// The count as the store keeps it: the number, or none for `auto`.
  pub fn stored(self) -> Option<i64> {
    match self {
      Self::Count(count) => Some(count),
      Self::Grown(_) => None,
    }
  }

  /// The count the store's column holds: none is `auto`.
  pub fn of_stored(count: Option<i64>) -> Self {
    count.map_or(Self::AUTO, Self::Count)
  }
}

/// Repository: one repository as its row holds it, with its checkouts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
  pub name: String,
  pub label: Option<String>,
  pub icon: Option<String>,
  pub owner_account: Option<i64>,
  /// The organisation unit it belongs to in place of an account, whose people's agents alone work on it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub owner_unit: Option<i64>,
  pub path: String,
  pub node_name: Option<String>,
  pub support_count: SupportCount,
  pub prefix_documents: Vec<String>,
  pub preamble: Option<String>,
  pub rules: Option<String>,
  pub dev_command: Option<String>,
  pub pin: Option<RepositoryPin>,
  /// a folder holding other repositories' checkouts below it, whose own tree its jobs never write
  #[serde(default)]
  pub umbrella: bool,
  pub downgrade_heal: bool,
  /// each setting it holds by name: a number or a switch; a setting left out is the engine's own default
  pub sampling: Option<BTreeMap<String, Value>>,
  /// every setting its sampling may hold, with its bounds
  pub sampling_settings: Vec<SamplingSetting>,
  pub checkouts: Vec<RepositoryCheckout>,
}

/// RepositoryList: every repository.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryList {
  pub items: Vec<Repository>,
  pub more: bool,
}

/// RepositoryAsk: the body that adds a repository, or the fields a change names.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryAsk {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub label: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub owner_account: Option<i64>,
  /// The organisation unit an added repository is filed under in place of an account, by a head of the unit.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub owner_unit: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub path: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub node_name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub support_count: Option<SupportCount>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub prefix_documents: Option<Vec<String>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub preamble: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub rules: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub dev_command: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
  pub pin: Option<Option<RepositoryPin>>,
  /// an umbrella, which takes no support checkouts and no node checkouts
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub umbrella: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub downgrade_heal: Option<bool>,
  /// each setting by name, a number, a switch, or null to leave it to the engine
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sampling: Option<BTreeMap<String, Value>>,
  /// the checkouts on the work nodes, the list whole; a change only
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub node_checkouts: Option<Vec<NodeCheckoutAsk>>,
  /// ready the repository's bench; a change only
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub bench: Option<bool>,
}

/// SlotRequest: one slot request of a role with its lease.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SlotRequest {
  pub sequence: i64,
  pub token: String,
  pub request_key: String,
  pub owner: String,
  pub source: String,
  pub role: String,
  pub path: Option<String>,
  /// the road it is let in on; empty until it is, unless its caller fixed it
  pub provider: String,
  pub model: String,
  pub effort: Option<String>,
  /// the role's cap when it was let in
  pub capacity: i64,
  /// `waiting`, `reserved`, `active`, `released` or `cancelled`
  pub state: String,
  /// epoch milliseconds
  pub enqueued_at: i64,
  pub leased_at: Option<i64>,
  pub child_id: Option<String>,
  pub ended_at: Option<i64>,
  pub outcome: Option<String>,
  pub reservation: Option<String>,
  pub yielding: bool,
  pub place_key: Option<String>,
  pub session: Option<String>,
  pub title: Option<String>,
  pub asked_by: Option<String>,
  pub agent_name: Option<String>,
  /// why it waits where its provider has no quota, or null
  pub held_for: Option<String>,
  /// who gave it back — `holder`, `owner` or `run-ended` — or null while it stands
  #[serde(default)]
  pub ended_by: Option<String>,
}

/// SlotAdmission: what an admission asked of a slot request answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SlotAdmission {
  /// the request, active, where it is let in; null otherwise
  pub request: Option<SlotRequest>,
  /// the card demand a slot on a registration the arbiter stands asks, or null
  pub demand: Option<Value>,
  /// `waiting`, `reserved`, `active`, `released` or `cancelled`: the request now
  pub state: String,
  /// who gave it back — `holder`, `owner` or `run-ended` — or null while it stands
  pub ended_by: Option<String>,
  /// the words it was given back with, or null while it stands
  pub outcome: Option<String>,
}

/// SlotFinished: what giving a slot request back answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SlotFinished {
  /// whether it was still held
  pub held: bool,
  pub stood_down: bool,
  pub note: Option<String>,
}

/// SlotRequestMoved: what a move of a slot request answers: the request bound, the request given back, or an
/// admission. Each is read in that order, the first whose required members the answer holds: a bound request
/// carries its token, a given-back one `held`, and an admission neither.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SlotRequestMoved {
  Bound(SlotRequest),
  Finished(SlotFinished),
  Admission(SlotAdmission),
}

/// OwnerRelease: what an owner's release gave back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnerRelease {
  /// its requests that waited or stood reserved, cancelled
  pub cancelled: i64,
  /// its requests that were let in, released
  pub released: i64,
  /// its reservations, released
  pub reservations: i64,
  /// the role sessions its leases were bound to, ended
  pub sessions_ended: Vec<String>,
}

/// OwnerReleaseQuery: whose holds an owner's release gives back.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnerReleaseQuery {
  pub owner: String,
}

/// SlotRequestAsk: the body that asks a slot of a role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SlotRequestAsk {
  pub request_key: String,
  pub owner: String,
  /// The role the slot is asked of; an ask naming none is refused.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub role: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub source: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub path: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub provider: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub place_key: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub yielding: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub token: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub session: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub title: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub asked_by: Option<String>,
}

/// SlotRequestMove: a slot request asked for its admission, bound to its run, or given back.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SlotRequestMove {
  /// `bound` or `finished`; none asks for admission
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub state: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub child_id: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub agent_name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub outcome: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cancel: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub demand_id: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub within: Option<i64>,
  /// The rank a demand this work asks stands at, a whole number of zero or more.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub card_rank: Option<u32>,
  /// The pool a demand this work asks stands in.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub card_pool: Option<CardPool>,
}

/// SlotRequestPage: the open slot requests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SlotRequestPage {
  pub items: Vec<SlotRequest>,
  pub more: bool,
}

/// SlotRequestsQuery: the open slot requests narrowed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SlotRequestsQuery {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub role: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub owner: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub waiting: Option<bool>,
}

/// SlotRun: one run let in on a role's slot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SlotRun {
  pub token: String,
  pub role: String,
  pub path: Option<String>,
  pub provider: String,
  pub model: String,
  pub source: String,
  /// epoch milliseconds
  pub since: Option<i64>,
  /// the agent of the role it was let in on, or null where none is named
  #[serde(default)]
  pub agent: Option<String>,
  /// the run a binding named since the slot's last start; else, for a slot bound to a role session, the run
  /// name of that session's running turn, null while none runs; else the child it is bound to, null until bound
  pub run_name: Option<String>,
  /// the role session it is bound to, or null
  pub session: Option<String>,
  /// the job whose run it is, where its source is `job`
  pub job_id: Option<i64>,
  pub title: Option<String>,
}

/// RoleSlots: one role's slots: its cap, its runs let in, reserved and waiting, and each run let in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleSlots {
  pub role: String,
  pub max_concurrent: i64,
  pub active: i64,
  pub reserved: i64,
  pub waiting: i64,
  pub runs: Vec<SlotRun>,
}

/// Slots: every role's slots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Slots {
  pub items: Vec<RoleSlots>,
}

/// ProviderModel: one model an order's entry may name on a provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModel {
  pub id: String,
  /// the levels it takes; null in the list stands for the provider's own default
  pub efforts: Vec<Option<String>>,
  /// whether it costs nothing, where the catalogue prices it
  pub free: Option<bool>,
  /// whether it is served rather than superseded, where the catalogue tells them apart
  pub in_service: Option<bool>,
  /// its window, for a gateway road's member or a registration
  pub context_tokens: Option<i64>,
  /// its answer ceiling, for a gateway road's member or a registration
  pub answer_tokens: Option<i64>,
  /// its thinking default, for a gateway road's member or a registration
  pub thinking: Option<bool>,
}

/// ProviderModels: the models an order's entry may name on a provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModels {
  pub provider: String,
  pub models: Vec<ProviderModel>,
  /// the moment of a stale read in epoch seconds, or null
  pub stale_read_at: Option<f64>,
}

/// RelayAsk: one BMO hands another person's BMO a line, or opens a chat into that person's room.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RelayAsk {
  /// the sending BMO's name
  pub from: String,
  /// the receiving BMO's name
  pub to: String,
  pub text: String,
  /// the room the line is said in
  pub room: i64,
  /// the receiving person's account id, where the sender names one
  pub person: Option<i64>,
}

/// Relay: one line handed to another person's BMO, and what came of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Relay {
  pub id: i64,
  pub from: String,
  pub to: String,
  pub room: i64,
  /// `relay`, a line handed over, or `chat`, a chat opened
  pub kind: String,
  pub line: String,
  /// `delivered`, `refused` or `failed`
  pub state: String,
  pub answer: Option<String>,
  pub reason: Option<String>,
  /// epoch milliseconds
  pub created_at: i64,
}

/// RelayPage: the relays one BMO sent or received, the latest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RelayPage {
  pub items: Vec<Relay>,
  pub more: bool,
}

/// Whether a person may make one kind of change over the agents supplied to them, and the link that said so.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnPermission {
  pub allowed: bool,
  /// `platform` or `unit:<id>`, none where the default stands.
  pub from: Option<String>,
}

/// One agent of a person's: the agent where it stands, the seat supplied where it is one, and whether the person's
/// own changes stand over that seat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnAgent {
  pub name: String,
  /// The agent's row as the role agents' routes answer it; none for a seat supplied that no longer stands.
  pub agent: Option<RoleAgent>,
  /// The seat supplied: its `road` (`provider`, `model`, `effort`, or null to follow the asker) and `from`, the link
  /// whose composition supplied it, null for the platform's own rule. None for an agent of the person's own.
  pub supplied: Option<Value>,
  /// Whether the agent stands otherwise than supplied: on another road, or switched off.
  pub changed: bool,
}

/// One role as it stands for a person, with their agents of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnRole {
  pub role: String,
  pub label: String,
  pub description: Option<String>,
  pub line: String,
  pub off: bool,
  /// The link that turned the role off, where one did.
  pub off_from: Option<String>,
  /// The most agents of the role a person holds.
  pub most: i64,
  pub agents: Vec<OwnAgent>,
}

/// A provider a person's road may name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnProvider {
  pub name: String,
  pub label: String,
  pub lines: Vec<String>,
}

/// The agents supplied to a person and their own changes over them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnAgents {
  pub account: i64,
  /// Whether they may add agents of their own and switch a supplied one off.
  pub add: OwnPermission,
  /// Whether they may choose their agents' roads.
  pub road: OwnPermission,
  pub roles: Vec<OwnRole>,
  pub providers: Vec<OwnProvider>,
}

/// An agent of a person's own asked for: its role, and its road where they name one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnAgentAsk {
  pub role: String,
  #[serde(default)]
  pub provider: Option<String>,
  #[serde(default)]
  pub model: Option<String>,
  #[serde(default)]
  pub effort: Option<String>,
}

/// A change of a person's agent: its road, a provider with a model or both null to follow the asker, or its switch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnAgentChange {
  #[serde(default)]
  pub provider: Option<String>,
  #[serde(default)]
  pub model: Option<String>,
  #[serde(default)]
  pub effort: Option<String>,
  #[serde(default)]
  pub enabled: Option<bool>,
}

/// A model a person may stand an agent on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnModel {
  pub id: String,
  /// The levels it takes, null in the list for the provider's own default.
  pub efforts: Vec<Option<String>>,
  pub in_service: Option<bool>,
  /// Whether the person's supply carries it now.
  pub supplied: bool,
}

/// The models of a provider a person may stand an agent on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnModels {
  pub provider: String,
  pub models: Vec<OwnModel>,
}

/// A person's own instructions, whether they may give any, the agents they may give one to, and the organisation's,
/// which stand before them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnInstructions {
  pub allowed: OwnPermission,
  /// The text every agent of theirs is given.
  pub all: Option<String>,
  /// One agent's own text, by its name.
  pub agents: BTreeMap<String, String>,
  /// The agents an instruction may name, each `name`, `label`, `kind` (`supervisor` or `agent`) and `role`.
  pub named: Vec<Value>,
  /// The organisation's instructions, outermost first, each `from`, its link, and `text`.
  pub organisation: Vec<Value>,
  pub most_characters: i64,
}

/// A person's own instructions written whole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnInstructionsAsk {
  #[serde(default)]
  pub all: Option<String>,
  #[serde(default)]
  pub agents: Option<BTreeMap<String, String>>,
}
