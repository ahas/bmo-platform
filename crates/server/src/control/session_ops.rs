#![allow(
  dead_code,
  unused_imports,
  unused_variables,
  clippy::needless_lifetimes,
  clippy::let_and_return,
  clippy::unnecessary_to_owned
)]
//! The session contract's ten operations as routes (docs/api.md, 「Sessions」): start, send a turn, push
//! words into it, read state, answer its question, read its output, interrupt, fold, end and list, each
//! answered by the backend the server stands its places on. A fault is answered with its outcome code and
//! an English sentence saying why.

use std::sync::Arc;
use std::time::Duration;

use agent_session::{
  Answer, Apart, Aside, Conversation, Fault, FaultKind, GateAsk, JobMark, RoomBlock, Scope, Session,
  SessionId, Sessions, Start, State, Turn, TurnContext, WorkParts,
};
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Call, Refused, Route, answer};
use crate::herdr_backend::Shared;
use crate::lines::Trees;
use crate::lines::runner;
use crate::lines::standing::session_ttl_ms;
use crate::lines::worker::worker_run;
use crate::places::{AgentRow, Ground};
use crate::turn_keys::{Claim, KeptEnd, Settled, Waiting};
use crate::worker_session::held as held_session;
use crate::worker_session::held::SessionRun;
use crate::worker_session::records;

fn backend(call: &Call) -> Result<Shared, Refused> {
  call.sessions().ok_or_else(|| {
    Refused::new(StatusCode::SERVICE_UNAVAILABLE, "agency stands no places to carry sessions on")
  })
}

// A fault as its route answers it: the status its kind stands for, its outcome code and why.
fn faulted(fault: Fault) -> Response {
  let status = match fault.kind {
    FaultKind::Unknown | FaultKind::UnknownTurn => StatusCode::NOT_FOUND,
    FaultKind::Busy => StatusCode::CONFLICT,
    FaultKind::Unreachable => StatusCode::SERVICE_UNAVAILABLE,
    FaultKind::Refused => StatusCode::UNPROCESSABLE_ENTITY,
  };

  answer(status, json!({ "code": fault.kind.code(), "error": fault.message }))
}

// A turn's context as a body names it, every part optional (docs/api.md, 「Sessions」).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContextRead {
  #[serde(default)]
  key: Option<String>,
  #[serde(default)]
  room: Option<RoomRead>,
  #[serde(default)]
  via: Option<String>,
  #[serde(default)]
  base_persona: Option<String>,
  #[serde(default)]
  privileged: bool,
  #[serde(default)]
  answers: Vec<i64>,
  #[serde(default)]
  began: Option<i64>,
  #[serde(default)]
  round: Option<String>,
  #[serde(default)]
  job: Option<i64>,
  #[serde(default)]
  item: Option<i64>,
  #[serde(default)]
  gate: Option<GateRead>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GateRead {
  #[serde(default)]
  system: Option<String>,
  #[serde(default)]
  thinking: Option<bool>,
  #[serde(default)]
  model: Option<String>,
  #[serde(default)]
  effort: Option<String>,
  #[serde(default)]
  answer_tokens: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoomRead {
  chat_id: i64,
  #[serde(default)]
  text: String,
  #[serde(default)]
  mark: String,
}

/// What a role's session's turn names of its own (docs/api.md, 「sessions」), every part optional: the run's
/// name and title, the room its lines stand under, its unit and user, which session it goes on, whether words
/// pushed into it reach it, and its variables. The road, the effort and the folder are the session's, and a
/// body naming a road field is refused (`work_asked`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkAsk {
  #[serde(default)]
  pub name: Option<String>,
  #[serde(default)]
  pub title: Option<String>,
  #[serde(default)]
  pub chat: Option<i64>,
  #[serde(default)]
  pub unit: Option<String>,
  #[serde(default)]
  pub user: Option<String>,
  #[serde(default)]
  pub resume: Option<agent_session::Resume>,
  #[serde(default, alias = "hears_words")]
  pub hears_words: Option<bool>,
  #[serde(default)]
  pub variables: Option<std::collections::BTreeMap<String, String>>,
}

/// The road fields a turn's body may not name: a turn's road is its session's (docs/api.md, 「sessions」).
pub const ROAD_FIELDS: [&str; 5] = ["provider", "model", "mapping", "workKey", "tier"];

/// The parts of its own a turn's body names, or why they do not read: a body naming a road field is refused.
pub fn work_asked(body: &Value) -> Result<WorkAsk, Refused> {
  if let Some(field) = ROAD_FIELDS.iter().find(|field| body.get(**field).is_some()) {
    return Err(bad(format!("a turn's road is its session's: a turn names no {field}")));
  }

  serde_json::from_value(body.clone()).map_err(|why| {
    Refused::new(StatusCode::BAD_REQUEST, format!("the turn's own parts do not read: {why}"))
  })
}

/// The work parts a role's session's turn goes out with: the session's provider, model, effort and folder, the
/// answer ceiling its role's entry for that provider names — the asking supervisor's where the role follows
/// it — and what the turn names of its own, a run name and a title given where it names none.
pub async fn work_of(
  db: &sea_orm::DatabaseConnection,
  agent: &str,
  apart: &Apart,
  asked: WorkAsk,
) -> WorkParts {
  let role = apart.role.clone().unwrap_or_else(|| crate::agent_roles::job_role().to_owned());
  let answer_tokens = match apart.provider.as_str() {
    "" => None,
    provider => crate::order::row_entry(db, &role, provider, apart.asked_by.as_deref())
      .await
      .ok()
      .and_then(|entry| entry.answer_tokens),
  };

  WorkParts {
    provider: apart.provider.clone(),
    model: apart.model.clone(),
    mapping: agent_session::Mapping {
      model: apart.model.clone(),
      effort: apart.effort.clone(),
      tier: None,
      answer_tokens: answer_tokens.and_then(|tokens| u64::try_from(tokens).ok()),
    },
    resume: asked.resume.unwrap_or(agent_session::Resume::Auto),
    cwd: apart.cwd.clone(),
    name: asked
      .name
      .filter(|name| !name.trim().is_empty())
      .unwrap_or_else(|| format!("{agent}-{}", crate::now_ms())),
    title: asked.title.filter(|title| !title.trim().is_empty()).unwrap_or_else(|| role.clone()),
    chat: asked.chat.unwrap_or_default(),
    unit: asked.unit,
    user: asked.user,
    work_key: role,
    // A run stands to be spoken to: its turn hears words pushed into it where its provider takes them,
    // and a caller that wants none says so (docs/agent-control.md, 「Words that arrive while a turn is
    // working」).
    hears_words: asked.hears_words.unwrap_or(true),
    variables: asked.variables.unwrap_or_default(),
  }
}

/// The turn's context a body names, or why it does not read as one. A role's session's work parts are no part
/// of it: they are the session's, laid on the turn as it is sent (`work_of`).
pub fn context_read(body: &Value) -> Result<TurnContext, Refused> {
  let read: ContextRead = serde_json::from_value(body.clone()).map_err(|why| {
    Refused::new(StatusCode::BAD_REQUEST, format!("the turn's context does not read: {why}"))
  })?;
  let work = None;
  let named =
    |value: Option<String>| value.map(|value| value.trim().to_owned()).filter(|value| !value.is_empty());

  // A key names its turn for a caller to re-attach by, so one that names nothing is refused (docs/api.md,
  // 「sessions」).
  if read.key.as_deref().is_some_and(|key| key.trim().is_empty()) {
    return Err(bad("key, when named, is a text that is not empty").with_field("key"));
  }

  Ok(TurnContext {
    key: named(read.key),
    room: read.room.map(|room| RoomBlock {
      chat_id: room.chat_id,
      text: room.text,
      mark: room.mark,
    }),
    via: named(read.via),
    base_persona: named(read.base_persona),
    privileged: read.privileged,
    answers: read.answers,
    began: read.began,
    round: named(read.round),
    job: read.job.map(|job| JobMark {
      job,
      item: read.item,
    }),
    work,
    gate: read.gate.map(|gate| GateAsk {
      system: named(gate.system),
      thinking: gate.thinking,
      model: named(gate.model),
      effort: named(gate.effort),
      answer_tokens: gate.answer_tokens,
    }),
  })
}

/// A scope as a start and a list answer it.
pub fn scope_value(scope: &Scope) -> Value {
  match scope {
    Scope::Agent => json!({ "kind": "agent" }),
    Scope::Room(room) => json!({ "kind": "room", "room": room }),
    Scope::Worker => json!({ "kind": "worker" }),
    Scope::Apart(apart) => json!({
      "kind": "role",
      "provider": apart.provider,
      "model": apart.model,
      "cwd": apart.cwd,
      "todo": apart.todo,
      "role": apart.role,
    }),
    Scope::Conversation(conversation) => json!({
      "kind": "conversation",
      "provider": conversation.provider,
      "model": conversation.model,
      "cwd": conversation.cwd,
    }),
  }
}

fn session_value(session: &Session) -> Value {
  json!({ "id": session.id.0, "agent": session.agent, "scope": scope_value(&session.scope) })
}

/// A session's state as its route answers it.
pub fn state_value(state: &State) -> Value {
  json!({
    "session": state.session.0,
    "provider": state.provider,
    "model": state.model,
    "place": state.place,
    "generation": state.generation,
    "context": state.context.map(|context| json!({ "carried": context.carried, "window": context.window })),
    "run": state.run,
    "went_out": state.went_out,
    "question": state.question,
    "continues": state.continues,
  })
}

fn bad(error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, error)
}

// The turn a path names, or why it names none.
fn turn_asked(call: &Call) -> Result<i64, Refused> {
  call
    .param("turnId")
    .and_then(|id| id.trim().parse::<i64>().ok())
    .ok_or_else(|| bad("turnId (a whole number) is required"))
}

fn text<'a>(body: &'a Value, field: &str) -> Result<&'a str, Refused> {
  body
    .get(field)
    .and_then(Value::as_str)
    .filter(|value| !value.is_empty())
    .ok_or_else(|| bad(format!("{field} (string) is required")))
}

/// The scope a start's body names, or why it does not read as one.
pub fn scope_read(body: &Value) -> Result<Scope, Refused> {
  let scope = body.get("scope").ok_or_else(|| bad("scope is required"))?;

  match scope.get("kind").and_then(Value::as_str) {
    Some("agent") => Ok(Scope::Agent),
    Some("worker") => Err(bad(WORKER_RETIRED)),
    Some("room") => match scope.get("room") {
      Some(Value::String(room)) if !room.is_empty() => Ok(Scope::Room(room.clone())),
      Some(Value::Number(room)) => Ok(Scope::Room(room.to_string())),
      _ => Err(bad("a room's scope names its room")),
    },
    Some("role" | "apart") => {
      let named = |field: &str| text(scope, field).map(|value| value.trim().to_owned());
      let todo = match scope.get("todo") {
        None | Some(Value::Null) => None,
        Some(todo) => Some(todo.as_i64().ok_or_else(|| bad("a thrown-away scope's todo is a number"))?),
      };
      let role = match scope.get("role") {
        None | Some(Value::Null) => None,
        Some(Value::String(role)) => Some(role.trim().to_owned()).filter(|role| !role.is_empty()),
        Some(_) => return Err(bad("a thrown-away scope's role is a string")),
      };

      let optional = |field: &str| {
        scope.get(field).and_then(Value::as_str).map(str::trim).unwrap_or_default().to_owned()
      };

      Ok(Scope::Apart(Apart {
        provider: optional("provider"),
        model: optional("model"),
        cwd: optional("cwd"),
        todo,
        role,
        ..Apart::default()
      }))
    }
    Some("conversation") => {
      let named = |field: &str| text(scope, field).map(|value| value.trim().to_owned());
      let provider = scope.get("provider").and_then(Value::as_str).map(str::trim).unwrap_or_default();

      Ok(Scope::Conversation(Conversation {
        provider: (if provider.is_empty() { CONVERSATION_PROVIDER } else { provider }).to_owned(),
        model: named("model")?,
        cwd: scope.get("cwd").and_then(Value::as_str).map(str::trim).unwrap_or_default().to_owned(),
      }))
    }
    _ => Err(bad("scope.kind is agent, room, role or conversation")),
  }
}

fn ok() -> Response {
  answer(StatusCode::OK, json!({ "ok": true }))
}

/// The word a session's scope goes by across the boundary.
fn scope_word(scope: &Scope) -> &'static str {
  match scope {
    Scope::Agent => "agent",
    Scope::Room(_) => "room",
    Scope::Worker => "worker",
    Scope::Apart(_) => "role",
    Scope::Conversation(_) => "conversation",
  }
}

fn room_id(scope: &Scope) -> Option<i64> {
  match scope {
    Scope::Room(room) => room.parse().ok(),
    _ => None,
  }
}

/// The scope a start's body names: the scope's word and a room's id, or the older object. A thrown-away
/// session handed a runner line's instructions and naming no folder is left to go out in that line's own.
pub fn scope_asked(body: &Value) -> Result<Scope, Refused> {
  if let Some(scope) = body.get("scope").and_then(Value::as_str) {
    return match scope {
      "agent" => Ok(Scope::Agent),
      "worker" => Err(bad(WORKER_RETIRED)),
      "room" => {
        let room = body
          .get("roomId")
          .and_then(super::whole)
          .ok_or_else(|| bad("a room's scope names its roomId"))?;
        Ok(Scope::Room(room.to_string()))
      }
      "apart" | "role" => {
        let options = body.get("options").cloned().unwrap_or(Value::Null);
        let field = |name: &str| {
          options
            .get(name)
            .or_else(|| body.get(name))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        };
        let runner = runner_asked(body)?;

        let cwd = field("cwd").unwrap_or_default().to_owned();

        // The road is the slot's, read as the start is answered; a start naming no slot names it whole.
        Ok(Scope::Apart(Apart {
          provider: field("provider").unwrap_or_default().to_owned(),
          model: field("model").unwrap_or_default().to_owned(),
          cwd,
          todo: number(&options, "todo").or_else(|| number(body, "todo")),
          role: field("role").map(str::to_owned),
          ..runner
        }))
      }
      "conversation" => {
        let options = body.get("options").cloned().unwrap_or(Value::Null);
        let field = |name: &str| {
          options
            .get(name)
            .or_else(|| body.get(name))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        };
        Ok(Scope::Conversation(Conversation {
          provider: field("provider").unwrap_or(CONVERSATION_PROVIDER).to_owned(),
          model: field("model").ok_or_else(|| bad("a conversation's scope names its model"))?.to_owned(),
          cwd: field("cwd").map(str::to_owned).unwrap_or_default(),
        }))
      }
      other => Err(bad(format!("scope is agent, room, role or conversation, not {other}"))),
    };
  }

  scope_read(body)
}

/// Why a start naming the `worker` scope is refused (docs/api.md, 「sessions」).
pub const WORKER_RETIRED: &str = "There is no worker scope: a run in a repository is a role's run at its checkout, started with the role scope \
   and the slot it was let in on";

/// The fault a route answers for a session of the retired worker scope.
fn retired(id: &str) -> Fault {
  Fault::new(FaultKind::Unknown, format!("{id} is no session: {WORKER_RETIRED}"))
}

/// The provider a conversation's turns go out on where its start names none.
const CONVERSATION_PROVIDER: &str = "claude-code";

/// The labels keys agency writes into a record itself, which a start's labels may not name.
const OWN_LABELS: [&str; 6] = ["line", "checkout", "archive", "role", "place", "reads"];

// What a thrown-away session's start names of a runner line whose instructions its caller hands in whole:
// the line, the instructions, the labels its record carries and the capabilities that record keeps; and the
// program's own session its turns continue, with the words a turn goes out with where that one cannot be
// resumed. Instructions with no line, a line that is no runner's, labels that are no object or name a key
// agency writes itself, capabilities that are no list of names, a session or words that are no text, and words
// with no session to stand in for are refused.
fn runner_asked(body: &Value) -> Result<Apart, Refused> {
  let line = match body.get("line") {
    None | Some(Value::Null) => None,
    Some(Value::String(line)) if runner::is_runner_line(line.trim()) => Some(line.trim().to_owned()),
    Some(other) => {
      return Err(bad(format!(
        "line names a runner line, one of {}, not {other}",
        runner::LINES.join(", ")
      )));
    }
  };
  let system_prompt = match body.get("systemPrompt") {
    None | Some(Value::Null) => None,
    Some(Value::String(text)) if !text.trim().is_empty() => Some(text.clone()),
    Some(_) => return Err(bad("systemPrompt is the line's instructions, a text that is not empty")),
  };

  if system_prompt.is_some() && line.is_none() {
    return Err(bad("a start handing systemPrompt names the line it goes out on"));
  }

  let labels = match body.get("labels") {
    None | Some(Value::Null) => None,
    Some(Value::Object(labels)) => {
      if let Some(own) = labels.keys().find(|key| OWN_LABELS.contains(&key.as_str())) {
        return Err(bad(format!("labels may not name {own}, which agency writes itself")));
      }

      Some(Value::Object(labels.clone()).to_string())
    }
    Some(_) => return Err(bad("labels is an object")),
  };
  let tools_allowed = match body.get("toolsAllowed") {
    None | Some(Value::Null) => None,
    Some(Value::Array(names)) => Some(
      names
        .iter()
        .map(|name| name.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| bad("toolsAllowed is a list of capability names"))?,
    ),
    Some(_) => return Err(bad("toolsAllowed is a list of capability names")),
  };

  // The session the caller names for its turns to continue (docs/sessions.md, 「A role's session」).
  let named = |field: &str| match body.get(field) {
    None | Some(Value::Null) => Ok(None),
    Some(Value::String(text)) if !text.trim().is_empty() => Ok(Some(text.clone())),
    Some(_) => Err(bad(format!("{field} is a text that is not empty"))),
  };
  let continues = named("continues")?;
  let continues_else = named("continuesElse")?;

  if continues_else.is_some() && continues.is_none() {
    return Err(bad("continuesElse stands in for the session continues names, and the start names none"));
  }

  // The demands of the caller's own a registration whose engine the arbiter stands runs on: one granted to
  // stand on, or the run's demand each turn's own is asked within (docs/own-llm.md, 「An engine the arbiter
  // stands」).
  let demand = |field: &str| match body.get(field) {
    None | Some(Value::Null) => Ok(None),
    Some(value) => value
      .as_i64()
      .filter(|id| *id > 0)
      .map(Some)
      .ok_or_else(|| bad(format!("{field} is the id of a demand the arbiter's master granted"))),
  };

  Ok(Apart {
    line,
    system_prompt,
    labels,
    tools_allowed,
    demand: demand("demandId")?,
    within: demand("within")?,
    card_rank: card_rank(body)?,
    card_pool: card_pool(body)?,
    resume: continues,
    resume_else: continues_else,
    ..Apart::default()
  })
}

/// The rank a body names for the demands of its work (docs/api.md, 「sessions」).
pub fn card_rank(body: &Value) -> Result<Option<u32>, Refused> {
  match body.get("cardRank") {
    None | Some(Value::Null) => Ok(None),
    Some(rank) => rank
      .as_u64()
      .and_then(|rank| u32::try_from(rank).ok())
      .map(Some)
      .ok_or_else(|| bad("cardRank is a whole number of zero or more")),
  }
}

/// The pool a body names for the demands of its work, its key and its limit (docs/api.md, 「sessions」).
pub fn card_pool(body: &Value) -> Result<Option<(String, u32)>, Refused> {
  match body.get("cardPool") {
    None | Some(Value::Null) => Ok(None),
    Some(pool) => {
      let key = pool.get("key").and_then(Value::as_str).map(str::trim).filter(|key| !key.is_empty());
      let cards = pool.get("cards").and_then(Value::as_u64).and_then(|cards| u32::try_from(cards).ok());
      match (key, cards.filter(|cards| *cards > 0)) {
        (Some(key), Some(cards)) => Ok(Some((key.to_owned(), cards))),
        _ => Err(bad("cardPool names a key and a limit of one card or more")),
      }
    }
  }
}

fn number(body: &Value, field: &str) -> Option<i64> {
  body
    .get(field)
    .and_then(Value::as_i64)
    .or_else(|| body.get(field).and_then(super::whole))
}

fn agent_named<'a>(body: &'a Value) -> Result<&'a str, Refused> {
  body
    .get("agentName")
    .or_else(|| body.get("agent"))
    .and_then(Value::as_str)
    .filter(|agent| !agent.is_empty())
    .ok_or_else(|| bad("agentName (a string) is required"))
}

/// A session as the boundary answers it: the state its backend holds and what its start named.
fn member(session: &Session, state: Option<&State>, body: &Value) -> Value {
  let scope = &session.scope;
  json!({
    "id": session.id.0,
    "agentName": session.agent,
    "scope": scope_word(scope),
    "roomId": room_id(scope),
    "provider": state.map(|state| state.provider.clone()).unwrap_or_default(),
    "model": state.map(|state| state.model.clone()).unwrap_or_default(),
    "place": state.map(|state| state.place.clone()),
    "generation": state.map(|state| state.generation).unwrap_or(0),
    "holder": Value::Null,
    "prefixChangeReason": Value::Null,
    "options": body.get("options").cloned().unwrap_or_else(|| json!({})),
    "context": state
      .and_then(|state| state.context)
      .map(|context| json!({ "carried": context.carried, "window": context.window })),
  })
}

// The member with the share of its model's window its session is folded at, its supervisor's or its role's
// compaction threshold read afresh (docs/sessions.md, 「Session compaction」), the row it runs for, and whether it
// was opened without the handoff it was owed.
async fn folded(db: &sea_orm::DatabaseConnection, mut member: Value) -> Result<Value, Refused> {
  let provider = member["provider"].as_str().unwrap_or_default().to_owned();
  let agent = member["agentName"].as_str().unwrap_or_default().to_owned();
  let supervisor = crate::supervisors::named(db, &agent).await?;
  let role = match &supervisor {
    Some(_) => None,
    None => crate::agent_roles::named(db, &agent).await?,
  };
  let ratio = supervisor
    .as_ref()
    .map(|held| held.fold_ratio)
    .or(role.as_ref().map(|held| held.fold_ratio));

  member["foldRatio"] = json!(ratio);
  member["supervisor"] = json!(supervisor.map(|held| held.name));
  member["role"] = json!(role.map(|held| held.name));
  let conversational = matches!(member["scope"].as_str(), Some("agent" | "room"));

  // A room's or an agent's session on a program that carries its own turns compacts itself at that share.
  member["foldsItself"] = json!(conversational && crate::chat_stand_in::inbox_declared(&provider));
  // A room's or an agent's session opened without the handoff it was owed says so until a turn has answered on it
  // (docs/sessions.md, 「A restart hands the session over」).
  member["handoffLost"] = json!(
    conversational
      && crate::supervisor_turn::replacement::lost(db, member["id"].as_str().unwrap_or_default()).await?
  );

  Ok(member)
}

/// A session started for an agent at a scope, or the one kept for that scope.
pub struct SessionStart;

impl Route for SessionStart {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/sessions";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let mut scope = scope_asked(&body)?;

    if let Scope::Apart(apart) = &mut scope {
      on_slot(call.store(), &body, apart).await?;
    }

    // A role's run goes under its role's name where its start names no other.
    let agent = match (&scope, agent_named(&body)) {
      (_, Ok(agent)) => agent.to_owned(),
      (
        Scope::Apart(Apart {
          role: Some(role), ..
        }),
        Err(_),
      ) => role.clone(),
      (_, Err(refused)) => return Err(refused),
    };

    if let Scope::Apart(Apart {
      role: Some(role), ..
    }) = &scope
      && !crate::agent_roles::holds(call.store(), role).await?
    {
      return Err(
        Refused::new(StatusCode::UNPROCESSABLE_ENTITY, crate::agent_roles::unknown(role))
          .with_code("unknown-role"),
      );
    }

    let mut start = Start { agent, scope };

    if let Scope::Conversation(conversation) = &mut start.scope
      && conversation.cwd.trim().is_empty()
    {
      conversation.cwd = call.ground().source.to_string_lossy().into_owned();
    }

    if let Scope::Apart(apart) = &start.scope
      && let Some(why) = line_not_carried(call.ground(), apart).await?
    {
      return Err(Refused::new(StatusCode::SERVICE_UNAVAILABLE, why).with_code("line-not-carried"));
    }

    let backend = backend(&call)?;

    match backend.start(start).await {
      Ok(session) => {
        // The slot the start names is bound to the session it opened, where it is bound to nothing yet, so
        // the lease ends with the session (docs/role-slots.md, 「The line」).
        if let Some(token) = slot_named(&body) {
          crate::order_run::dispatch::bind_session(call.store(), call.events(), &token, &session.id.0)
            .await
            .map_err(Refused::failed)?;
        }

        let state = backend.read_state(&session.id, None).await.ok();
        let answered = folded(call.store(), member(&session, state.as_ref(), &body)).await?;

        Ok(answer(StatusCode::CREATED, answered))
      }
      Err(fault) => Ok(faulted(fault)),
    }
  }
}

crate::moved!(SessionStart);

// The token of the slot a role's start names: at the top of the body, among its options, or in its scope.
fn slot_named(body: &Value) -> Option<String> {
  [body.get("slot"), body.pointer("/options/slot"), body.pointer("/scope/slot")]
    .into_iter()
    .flatten()
    .find_map(Value::as_str)
    .map(str::trim)
    .filter(|token| !token.is_empty())
    .map(str::to_owned)
}

fn contradicts(why: String) -> Refused {
  Refused::new(StatusCode::UNPROCESSABLE_ENTITY, why).with_code("contradicts-slot")
}

/// A role's start laid on the slot its caller was let in on (docs/api.md, 「sessions」): the slot's role, provider,
/// model and effort, and its path where the start names no folder. The slot must be active; a role, provider or
/// model the start names must be the slot's own. A start naming no slot is refused.
pub async fn on_slot(
  db: &sea_orm::DatabaseConnection,
  body: &Value,
  apart: &mut Apart,
) -> Result<(), Refused> {
  let Some(token) = slot_named(body) else {
    return Err(bad("a role's run names the slot it was let in on"));
  };
  let held = crate::order_run::dispatch::request(db, &token)
    .await
    .map_err(Refused::failed)?
    .filter(|held| held.state == "active")
    .ok_or_else(|| {
      Refused::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!("No active slot request holds the token {token}"),
      )
      .with_code("slot-not-active")
    })?;

  if let Some(role) = &apart.role
    && *role != held.role
  {
    return Err(contradicts(format!("The slot is the {} role's, not {role}'s", held.role)));
  }

  for (field, named, own) in
    [("provider", &apart.provider, &held.provider), ("model", &apart.model, &held.model)]
  {
    if !named.is_empty() && named != own {
      return Err(contradicts(format!("The slot was let in on the {field} {own}, not {named}")));
    }
  }

  // The turns stand on the demand the slot's lease holds, so a run holds one card however many turns it takes.
  if apart.demand.is_none()
    && let Some((demand, _)) = crate::cards::of_lease(db, &token).await.map_err(Refused::from)?
  {
    apart.demand = Some(demand);
  }

  // Named by no caller, its rank is that of the supervisor that asked its slot.
  if apart.card_rank.is_none()
    && let Some(supervisor) = held.asked_by.as_deref()
  {
    apart.card_rank = crate::cards::supervisor_rank(db, supervisor).await;
  }

  apart.role = Some(held.role.clone());
  apart.asked_by.clone_from(&held.asked_by);
  apart.provider.clone_from(&held.provider);
  apart.model.clone_from(&held.model);
  apart.effort.clone_from(&held.effort);

  // A slot naming no path leaves the folder to the start, which puts it in the line's own.
  if apart.cwd.is_empty()
    && let Some(path) = &held.path
  {
    apart.cwd.clone_from(path);
  }

  Ok(())
}

/// Why a thrown-away session on a runner line cannot go out on the provider its start names, in one sentence:
/// a provider this build does not carry, one the owner switched off, or one whose program does not carry the
/// line; none where it can, and for a session on the worker line, whose provider the backend judges.
pub async fn line_not_carried(
  ground: &crate::places::Ground,
  apart: &Apart,
) -> Result<Option<String>, Refused> {
  let Some(line) = apart.line.as_deref() else {
    return Ok(None);
  };
  let provider = apart.provider.as_str();
  let Some(declared) = harness_providers::provider(provider) else {
    return Ok(Some(format!(
      "{provider} is no provider this build carries, so the {line} line cannot go out on it"
    )));
  };

  if !crate::control::harness::provider_enabled(&ground.db, provider)
    .await
    .map_err(|why| Refused::new(StatusCode::INTERNAL_SERVER_ERROR, why.error))?
  {
    return Ok(Some(format!("{provider} is switched off, so the {line} line cannot go out on it")));
  }

  Ok((!declared.declared().carries(line)).then(|| {
    format!(
      "{provider}'s program does not carry the {line} line: its tools are not the run's MCP servers alone"
    )
  }))
}

/// Work a route answers with, carried to its end on a task of its own: a caller that lets go of the
/// request, or goes down while it waits, drops only its wait, never the work (docs/api.md, 「sessions」).
/// A panic in the work is the route's as it would have been.
pub async fn carried<T: Send + 'static>(work: impl Future<Output = T> + Send + 'static) -> T {
  match tokio::spawn(work).await {
    Ok(done) => done,
    Err(failed) => std::panic::resume_unwind(failed.into_panic()),
  }
}

/// One turn sent into a session and carried until it ends, whether or not its sender still waits. A send
/// naming a key attaches to the turn standing under it, or is answered with its end as kept; one cut by the
/// server's drain is answered so its caller sends again under the same key once the server stands
/// (docs/api.md, 「sessions」).
pub struct SessionTurn;

impl Route for SessionTurn {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/sessions/{sessionId}/turns";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let session = call.param("sessionId").unwrap_or_default().to_owned();
    let mut turn = Turn {
      session: SessionId(session.clone()),
      prompt: text(&body, "prompt")?.to_owned(),
      context: context_read(&body)?,
    };
    let asked = work_asked(&body)?;
    let sessions = backend(&call)?;
    let ground = call.shared_ground();
    let prompt = text(&body, "prompt")?.to_owned();
    let context = body.get("context").cloned().unwrap_or_else(|| json!({}));
    let key = turn.context.key.clone();

    // A repeat of a keyed send is answered by the turn standing under its key, or its end as kept, before
    // anything of the session is asked: the session of a turn picked up at the start ends with that turn.
    let fresh = match &key {
      Some(key) => match ground.turn_keys.claim(&ground.db, &session, key).await {
        Claim::Kept(end) => return Ok(answer(StatusCode::CREATED, end.answer(&prompt, context))),
        Claim::Standing(waiting) => return Ok(waited(&ground, Some(key), waiting, &prompt, context).await),
        Claim::New(waiting) => Some(waiting),
      },
      None => None,
    };

    // A role's session lays its own road on the turn; there is no worker scope to send one to.
    let scoped = match sessions.held_scope(&turn.session).await {
      Ok((_, Scope::Worker)) => Err(retired(&session)),
      Ok((agent, Scope::Apart(apart))) if turn.context.gate.is_none() => {
        turn.context.work = Some(work_of(call.store(), &agent, &apart, asked).await);
        Ok(())
      }
      _ => Ok(()),
    };

    if let Err(fault) = scoped {
      if let Some(key) = &key {
        ground.turn_keys.settle(&session, key, Settled::Refused(fault.clone()));
      }

      return Ok(faulted(fault));
    }

    // The turn is carried on a task of its own, so a sender that goes away, or the server's drain, drops only
    // the wait; a keyed turn's end reaches every caller waiting on its key.
    let sent = {
      let ground = ground.clone();
      let (session, key) = (session.clone(), key.clone());

      tokio::spawn(async move {
        let unsettled = key.map(|key| Unsettled {
          ground: ground.clone(),
          session: session.clone(),
          key,
          fault: None,
        });
        let sent = sessions.send_turn(turn).await;

        if let (Some(mut unsettled), Err(fault)) = (unsettled, &sent) {
          unsettled.fault = Some(fault.clone());
        }

        sent
      })
    };

    Ok(match fresh {
      Some(waiting) => waited(&ground, key.as_deref(), waiting, &prompt, context).await,
      None => {
        tokio::select! {
          ended = sent => match ended {
            Ok(Ok(end)) => {
              answer(StatusCode::CREATED, KeptEnd::of(&session, "", &end).answer(&prompt, context))
            }
            Ok(Err(fault)) => faulted(fault),
            Err(failed) => std::panic::resume_unwind(failed.into_panic()),
          },
          () = ground.drain.begun() => stopping(None),
        }
      }
    })
  }
}

// A keyed send's key while its turn is carried: the turn's own end settles it as it is handed back, and what
// is left unsettled once the send is over — a refusal before the turn began, or the send's task failing — is
// settled with that, so no caller waits on the key for ever.
struct Unsettled {
  ground: Arc<Ground>,
  session: String,
  key: String,
  fault: Option<Fault>,
}

impl Drop for Unsettled {
  fn drop(&mut self) {
    let fault = self.fault.take().unwrap_or_else(|| Fault {
      kind: FaultKind::Refused,
      message: format!("{}: the turn's send failed before it ended", self.session),
    });

    // A turn whose end was handed back holds its key no longer, so this settles nothing there.
    self.ground.turn_keys.settle(&self.session, &self.key, Settled::Refused(fault));
  }
}

// A keyed turn waited on until it settles, answered as its send is; or the server's drain, answered so the
// caller sends again under the key.
async fn waited(
  ground: &Ground,
  key: Option<&str>,
  waiting: Waiting,
  prompt: &str,
  context: Value,
) -> Response {
  tokio::select! {
    settled = waiting.settled() => match settled {
      Some(Settled::Ended(end)) => answer(StatusCode::CREATED, end.answer(prompt, context)),
      Some(Settled::Refused(fault)) => faulted(fault),
      // Let go of with no end only as the process goes: the turn stays kept in flight for the next server.
      None => stopping(key),
    },
    () = ground.drain.begun() => stopping(key),
  }
}

// A send's wait cut by the server's drain: the turn goes on, and the caller sends again under its key.
fn stopping(key: Option<&str>) -> Response {
  let retry = crate::drain::retry_after();
  let mut answered = answer(
    StatusCode::SERVICE_UNAVAILABLE,
    json!({
      "code": crate::drain::STOPPING,
      "error": "agency is stopping; the turn goes on, and its send asked again with the same key attaches to it",
      "key": key,
    }),
  );

  if let Ok(value) = axum::http::HeaderValue::from_str(&retry.to_string()) {
    answered.headers_mut().insert(axum::http::header::RETRY_AFTER, value);
  }

  answered
}

crate::moved!(SessionTurn);

/// Words pushed into the turn a session is working, answered with what became of them.
pub struct SessionPush;

impl Route for SessionPush {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/sessions/{sessionId}/turns/{turnId}/inputs";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let session = SessionId(call.param("sessionId").unwrap_or_default().to_owned());
    let turn = turn_asked(&call)?;
    let words = text(&body, "words")?.to_owned();

    Ok(match backend(&call)?.push(&session, turn, &words).await {
      Ok(said) => answer(StatusCode::CREATED, json!({ "words": words, "said": said_word(said) })),
      Err(fault) => faulted(fault),
    })
  }
}

crate::moved!(SessionPush);

fn said_word(said: Aside) -> &'static str {
  match said {
    Aside::Delivered => "delivered",
    Aside::Waiting => "waiting",
    Aside::Refused => "refused",
  }
}

/// A session's state, by its id; a worker's names what its next turn on the model named would continue.
pub struct SessionState;

impl Route for SessionState {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/sessions/{sessionId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let id = call.param("sessionId").unwrap_or_default().to_owned();
    let session = SessionId(id.clone());
    let model = call.query("model").filter(|model| !model.trim().is_empty());
    let backend = backend(&call)?;
    let (agent, scope) = match backend.held_scope(&session).await {
      Ok((_, Scope::Worker)) => return Ok(faulted(retired(&id))),
      Ok(held) => held,
      // A thrown-away session carrying a picked-up turn stands until that turn ends, and its state is read
      // by id meanwhile (docs/sessions.md, 「A restart hands the session over」).
      Err(fault) => match backend.carried_scope(&session) {
        Some((agent, apart)) => (agent, Scope::Apart(apart)),
        None => return Ok(faulted(fault)),
      },
    };

    Ok(match backend.read_state(&session, model.as_deref()).await {
      Ok(state) => {
        let started = agent_session::Session {
          id: SessionId(id.clone()),
          agent,
          scope,
        };
        let (holder, reason) = holder_of(call.store(), &id).await?;

        let read = folded(call.store(), member(&started, Some(&state), &json!({}))).await?;

        answer(StatusCode::OK, with_holder(read, holder, reason))
      }
      Err(fault) => faulted(fault),
    })
  }
}

crate::moved!(SessionState);

/// The block a session's answer is writing right now, as the model gateway counts it: read for a supervisor's
/// session by the program session it holds, 204 wherever none is to be read.
pub struct SessionStreaming;

impl Route for SessionStreaming {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/sessions/{sessionId}/streaming";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let id = call.param("sessionId").unwrap_or_default().to_owned();
    let none = || StatusCode::NO_CONTENT.into_response();
    let Ok((agent, _)) = backend(&call)?.held_scope(&SessionId(id)).await else {
      return Ok(none());
    };
    let Some(program) =
      crate::supervisors::named(call.store(), &agent).await?.and_then(|held| held.session_id)
    else {
      return Ok(none());
    };

    Ok(match crate::streaming::open_block(call.machine(), &program).await {
      Some(block) => answer(StatusCode::OK, block),
      None => none(),
    })
  }
}

crate::moved!(SessionStreaming);

// Where a session's holder and the reason its prefix last changed are kept, by the session's id.
const HOLDER: &str = "session_holder:";

/// The holder a session's row names, and why its prefix last changed.
pub(crate) async fn holder_of(
  db: &impl sea_orm::ConnectionTrait,
  session: &str,
) -> Result<(Option<String>, Option<String>), Refused> {
  let Some(text) = crate::queue::settings::get(db, &format!("{HOLDER}{session}")).await? else {
    return Ok((None, None));
  };
  let read: Value = serde_json::from_str(&text).unwrap_or(Value::Null);

  Ok((
    read.get("holder").and_then(Value::as_str).map(str::to_owned),
    read.get("prefixChangeReason").and_then(Value::as_str).map(str::to_owned),
  ))
}

// The holder kept as the body gives it: a name written, a name cleared where null, and the field left
// alone where the body does not name it.
async fn keep_holder(
  db: &impl sea_orm::ConnectionTrait,
  session: &str,
  body: &Value,
) -> Result<(Option<String>, Option<String>), Refused> {
  let (mut holder, mut reason) = holder_of(db, session).await?;

  if let Some(given) = body.get("holder") {
    holder = given.as_str().map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned);
  }

  if let Some(given) = body.get("prefixChangeReason") {
    reason = given.as_str().map(str::trim).filter(|why| !why.is_empty()).map(str::to_owned);
  }

  let key = format!("{HOLDER}{session}");

  if holder.is_none() && reason.is_none() {
    crate::queue::settings::forget(db, &key).await?;
  } else {
    let kept = json!({ "holder": holder, "prefixChangeReason": reason });

    crate::queue::settings::set(db, &key, &kept.to_string()).await?;
  }

  Ok((holder, reason))
}

/// A session moved: a conversation's or a role's session's provider and model switched where the body names
/// them, or a role's kept session handed over to the holder the body names, the session kept for its role at
/// its path on its provider and model closed with its record, and the holder kept for the session's state.
pub struct SessionHandover;

impl Route for SessionHandover {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/sessions/{sessionId}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let id = call.param("sessionId").unwrap_or_default().to_owned();
    let body = call.body().await?;
    let session = SessionId(id.clone());
    let backend = backend(&call)?;
    let (agent, scope) = match backend.held_scope(&session).await {
      Ok(held) => held,
      Err(fault) => return Ok(faulted(fault)),
    };

    // A conversation's and a role's session's provider and model move on the change route; every other
    // scope's stands on its row, and the worker scope is no session's (docs/api.md, 「sessions」).
    if matches!(scope, Scope::Worker) {
      return Ok(faulted(retired(&id)));
    }

    let options = body.get("options").cloned().unwrap_or(Value::Null);
    let given = |name: &str| {
      options
        .get(name)
        .or_else(|| body.get(name))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
    };
    let (provider, model) = (given("provider"), given("model"));

    // A role's kept session handed over: the session kept for its role at its path on its provider and model
    // closed with its record, and the holder the body names kept for the session's state.
    if let Scope::Apart(apart) = &scope
      && provider.is_none()
      && model.is_none()
      && body.get("holder").is_some()
    {
      // The job whose turn last answered on it writes its handoff before the session goes (docs/sessions.md,
      // 「The handoff a job's session leaves」).
      super::work::handed_over(&call.shared_ground(), backend.as_ref(), &id).await;

      if let Some(role) = &apart.role {
        crate::apart_sessions::hand_over_key(call.store(), role, &apart.cwd, &apart.provider, &apart.model)
          .await
          .map_err(Refused::failed)?;
      }

      let (holder, reason) = keep_holder(call.store(), &id, &body).await?;
      let state = backend.read_state(&session, None).await.ok();
      let started = Session {
        id: session.clone(),
        agent,
        scope: scope.clone(),
      };
      let read = folded(call.store(), member(&started, state.as_ref(), &body)).await?;

      return Ok(answer(StatusCode::OK, with_holder(read, holder, reason)));
    }

    if provider.is_none() && model.is_none() {
      return Err(Refused::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!("{id} is switched by naming the provider or the model it moves to"),
      ));
    }

    Ok(match backend.switch(&session, provider.as_deref(), model.as_deref()).await {
      Ok(moved) => {
        let state = backend.read_state(&moved.id, None).await.ok();

        answer(StatusCode::OK, folded(call.store(), member(&moved, state.as_ref(), &body)).await?)
      }
      Err(fault) => faulted(fault),
    })
  }
}

// A session's member with the holder its row keeps.
fn with_holder(member: Value, holder: Option<String>, reason: Option<String>) -> Value {
  let mut member = member;

  member["holder"] = json!(holder);
  member["prefixChangeReason"] = json!(reason);

  member
}

crate::moved!(SessionHandover);

/// The answer a body gives the question a session's turn stands on, or why it does not read as one.
pub fn answer_read(body: &Value) -> Result<Answer, Refused> {
  let given = body.get("answer").ok_or_else(|| bad("answer is required"))?;

  serde_json::from_value(given.clone())
    .map_err(|why| bad(format!("answer.kind is allow, leave or refuse, with words for a refusal: {why}")))
}

/// The caller's answer to the question a session's turn stands on.
pub struct SessionAnswer;

impl Route for SessionAnswer {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/sessions/{sessionId}/answers";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let session = SessionId(call.param("sessionId").unwrap_or_default().to_owned());
    let question = body
      .get("questionId")
      .or_else(|| body.get("question"))
      .and_then(Value::as_str)
      .ok_or_else(|| bad("questionId (a string) is required"))?
      .to_owned();
    let given = match body.get("answer") {
      Some(Value::String(word)) => match word.as_str() {
        "allow" => Answer::Allow,
        "leave" => Answer::Leave,
        "refuse" => Answer::Refuse { words: None },
        other => return Err(bad(format!("an answer is allow, leave or refuse, not {other}"))),
      },
      Some(_) => answer_read(&body)?,
      None => return Err(bad("answer is required")),
    };

    Ok(match backend(&call)?.answer(&session, &question, given).await {
      Ok(()) => answer(
        StatusCode::CREATED,
        json!({ "questionId": question, "answer": body.get("answer").cloned().unwrap_or(Value::Null) }),
      ),
      Err(fault) => faulted(fault),
    })
  }
}

crate::moved!(SessionAnswer);

/// What a session's turn now going, or its last one, has written so far.
pub struct SessionOutput;

impl Route for SessionOutput {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/sessions/{sessionId}/turns/{turnId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let session = SessionId(call.param("sessionId").unwrap_or_default().to_owned());
    let turn = turn_asked(&call)?;

    Ok(match backend(&call)?.output(&session, Some(turn)).await {
      Ok(output) => answer(
        StatusCode::OK,
        json!({
          "id": output.turn,
          "sessionId": session.0,
          "prompt": "",
          "context": {},
          "state": if output.running { "running" } else { "ended" },
          "delivery": Value::Null,
          "outcome": Value::Null,
          "ending": Value::Null,
          "reason": Value::Null,
          "answer": Value::Null,
          "answers": [],
          "text": [output.text],
          "waiting": output.waiting,
        }),
      ),
      Err(fault) => faulted(fault),
    })
  }
}

crate::moved!(SessionOutput);

/// Whether a turn has ended, asked with `ended` every `look` until it has or `bound` has passed.
pub async fn ended_within<Look, Looking>(mut ended: Look, bound: Duration, look: Duration) -> bool
where
  Look: FnMut() -> Looking,
  Looking: Future<Output = bool>,
{
  let began = tokio::time::Instant::now();

  loop {
    if ended().await {
      return true;
    }

    let waited = began.elapsed();

    if waited >= bound {
      return false;
    }

    tokio::time::sleep(look.min(bound - waited)).await;
  }
}

/// The interrupt's answer (docs/api.md, 「sessions」): a turn that ended answered stopped, one still going
/// answered `202` running, with what it waits for, since the stop stands over it and its end comes as
/// `turn.ended`.
pub fn interrupted(
  session: &SessionId,
  turn: i64,
  ended: bool,
  waiting: Option<&agent_session::Waiting>,
) -> (StatusCode, Value) {
  let (status, state, outcome, waiting) = if ended {
    (StatusCode::OK, "interrupted", Value::from("stopped"), Value::Null)
  } else {
    (
      StatusCode::ACCEPTED,
      "running",
      Value::Null,
      waiting.and_then(|waiting| serde_json::to_value(waiting).ok()).unwrap_or(Value::Null),
    )
  };

  (
    status,
    json!({
      "id": turn,
      "sessionId": session.0,
      "prompt": "",
      "context": {},
      "state": state,
      "delivery": Value::Null,
      "outcome": outcome,
      "ending": outcome,
      "reason": Value::Null,
      "answer": Value::Null,
      "answers": [],
      "text": [],
      "waiting": waiting,
    }),
  )
}

/// The turn a session is in cut short: a stop stood over it and the turn waited on, up to the interrupt's bound,
/// so the answer says whether it ended.
pub struct SessionInterrupt;

impl Route for SessionInterrupt {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/sessions/{sessionId}/turns/{turnId}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let asked = call.body().await?;
    let session = SessionId(call.param("sessionId").unwrap_or_default().to_owned());
    let turn = turn_asked(&call)?;

    if asked.get("state").and_then(Value::as_str).is_some_and(|state| state != "interrupted") {
      return Err(bad("a turn is interrupted, not moved to another state"));
    }

    let backend = backend(&call)?;

    if let Err(fault) = backend.interrupt(&session, turn).await {
      return Ok(faulted(fault));
    }

    let stop = &policy::run_stop::RUN_STOP;
    let read = || backend.output(&session, Some(turn));
    let ended = ended_within(
      || async { read().await.is_ok_and(|output| !output.running) },
      Duration::from_millis(stop.interrupt_wait_ms),
      Duration::from_millis(stop.interrupt_look_ms),
    )
    .await;
    // A turn that ended between the last look and the bound is read once more, so the answer is as of now.
    let now = if ended { None } else { read().await.ok() };
    let ended = ended || now.as_ref().is_some_and(|output| !output.running);
    let waiting = now.as_ref().and_then(|output| output.waiting.as_ref());
    let (status, body) = interrupted(&session, turn, ended, waiting);

    Ok(answer(status, body))
  }
}

crate::moved!(SessionInterrupt);

/// A session carried past its window as its provider declares.
pub struct SessionFold;

impl Route for SessionFold {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/sessions/{sessionId}/folds";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let session = SessionId(call.param("sessionId").unwrap_or_default().to_owned());
    let backend = backend(&call)?;

    if let Ok((_, Scope::Worker)) = backend.held_scope(&session).await {
      return Ok(faulted(retired(&session.0)));
    }

    Ok(match backend.fold(&session).await {
      Ok(()) => answer(StatusCode::CREATED, json!({ "sessionId": session.0, "at": crate::now_ms() })),
      Err(fault) => faulted(fault),
    })
  }
}

crate::moved!(SessionFold);

/// A session ended and its program taken down.
pub struct SessionEnd;

impl Route for SessionEnd {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/sessions/{sessionId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let session = SessionId(call.param("sessionId").unwrap_or_default().to_owned());
    let backend = backend(&call)?;

    if let Ok((_, Scope::Worker)) = backend.held_scope(&session).await {
      return Ok(faulted(retired(&session.0)));
    }

    Ok(match backend.end(&session).await {
      Ok(()) => StatusCode::NO_CONTENT.into_response(),
      Err(fault) => faulted(fault),
    })
  }
}

crate::moved!(SessionEnd);
