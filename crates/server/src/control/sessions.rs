//! Every session an agent holds now, and an ask to clear one (docs/session-resets.md). Each kind of agent
//! that holds sessions answers which it holds and how a reset of one is carried out; the ask is kept as a
//! row and carried out by the session's own holder. A worker's is carried out here once the ask is written,
//! where no job of it runs, and by its next job otherwise; a supervisor's has the program standing on its
//! room's session taken down here, and the session itself opened afresh by the room's next turn.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use agent_session::Sessions as _;
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use sea_orm::{ConnectionTrait, DatabaseTransaction, QueryResult, TransactionTrait};
use serde::Serialize;
use serde_json::{Value, json};

use policy::control_words::CONTROL_WORDS;
use policy::fill_marks;
use policy::roles::ROLES;
use store::{all, one, run};

use super::{Call, Refused, Route, answer, canonical_agent, whole};
use crate::conversational::Scope;
use crate::hooks::Hooks;
use crate::now_ms;
use crate::places::Ground;
use crate::places::follow::RoomTitle;
use crate::queue::{holds, workers};

// The word a writer outside this process says once it has written an ask that clears at once.
pub const TELL: &str = "session-resets";

/// Which workers have a job running now, or none where nobody could say: then every worker counts as
/// running, and a reset of it waits.
pub struct Running(pub Option<Vec<String>>);

impl Running {
  /// What reads what is running, taken off the call before anything is awaited, so no route holds the
  /// call across a wait.
  pub fn reader(call: &Call) -> Option<Arc<Ground>> {
    Some(call.shared_ground())
  }

  /// What is running, read; nothing where it could not be read.
  pub async fn read(reader: Option<Arc<Ground>>) -> Self {
    match reader {
      Some(ground) => Self(running_workers(&ground).await),
      None => Self(None),
    }
  }

  /// Read for asks alone, where no answer shows whether a session runs.
  pub fn unread() -> Self {
    Self(None)
  }

  /// Whether a worker's job is running, or nobody could say.
  pub fn holds(&self, worker: &str) -> bool {
    self.0.as_ref().is_none_or(|running| running.iter().any(|one| one == worker))
  }
}

/// Which workers have a job running now: each a job is at work in, in any stage or round of it. A worker on a
/// node is read as running, so a reset of it waits for its next job; none where the workers could not be read.
pub async fn running_workers(ground: &Ground) -> Option<Vec<String>> {
  let workers = workers::workers(&ground.db, &ground.source).await.ok()?;
  let at_work = holds::checkouts_at_work(&ground.db).await.ok()?;

  Some(
    workers
      .into_iter()
      .filter(|worker| worker.node.is_some() || at_work.contains(&worker.name))
      .map(|worker| worker.name)
      .collect(),
  )
}

/// One session as a holder holds it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
  agent: String,
  // The room it is one room's, or none.
  chat_id: Option<i64>,
  // The holder's kind, which says what a reset of it clears.
  kind: &'static str,
  // What tells this session apart: the room's title, as the room's owner answers it, and the generation,
  // the models it runs on.
  title: Option<String>,
  generation: Option<i64>,
  models: Vec<Value>,
  // Whether a turn or a job of it is known to be running now, which a reset waits for.
  running: bool,
}

/// How a holder carries out a reset it was asked.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Clearing {
  /// By the session's next turn or job, which settles the ask.
  Waits,
  /// Here once the ask is written, as far as nothing of it runs, and the rest by its next turn or job.
  AtOnce,
}

type Holding<'a, T> = Pin<Box<dyn Future<Output = Result<T, Refused>> + Send + 'a>>;

/// A kind of agent that holds sessions.
trait SessionHolder: Sync {
  /// Every session of this kind there is now.
  fn sessions<'a>(
    &'a self,
    db: &'a DatabaseTransaction,
    running: &'a Running,
  ) -> Holding<'a, Vec<AgentSession>>;

  /// A reset asked of one of its sessions, set going.
  fn clear<'a>(&'a self, db: &'a DatabaseTransaction, session: &'a AgentSession) -> Holding<'a, Clearing>;

  /// Its waiting asks carried out as far as they can be here, with nothing of theirs running; a holder
  /// whose sessions are cleared by their next turn alone has nothing to do.
  fn carry_out<'a>(&'a self, _places: &'a crate::places::Places, _running: &'a Running) -> Holding<'a, ()> {
    Box::pin(async { Ok(()) })
  }
}

/// A worker: its recorded sessions, one for each model and login it ran on.
struct Worker;

impl SessionHolder for Worker {
  fn sessions<'a>(
    &'a self,
    db: &'a DatabaseTransaction,
    running: &'a Running,
  ) -> Holding<'a, Vec<AgentSession>> {
    Box::pin(async move {
      let mut by_worker: Vec<(String, Vec<String>)> = vec![];

      for checkout in crate::queue::workers::workers_in(db).await? {
        let mut models: Vec<String> = vec![];

        for held in crate::worker_session::records::sessions(db, &checkout).await? {
          if !models.contains(&held.model) {
            models.push(held.model);
          }
        }

        if !models.is_empty() {
          by_worker.push((checkout, models));
        }
      }

      Ok(
        by_worker
          .into_iter()
          .map(|(worker, models)| AgentSession {
            running: running.holds(&worker),
            agent: worker,
            chat_id: None,
            kind: "worker",
            title: None,
            generation: None,
            models: models.into_iter().map(Value::String).collect(),
          })
          .collect(),
      )
    })
  }

  // Whether a job of it runs is read again as the ask is carried out.
  fn clear<'a>(
    &'a self,
    _db: &'a DatabaseTransaction,
    _session: &'a AgentSession,
  ) -> Holding<'a, Clearing> {
    Box::pin(async { Ok(Clearing::AtOnce) })
  }

  // Every waiting ask of a worker that runs nothing carried out whole; the rest wait for its next job.
  fn carry_out<'a>(&'a self, places: &'a crate::places::Places, running: &'a Running) -> Holding<'a, ()> {
    Box::pin(async move {
      let waiting = all(
        &places.ground.db,
        r#"SELECT DISTINCT "agent" FROM "session_resets"
           WHERE "chat_id" IS NULL AND "done_at" IS NULL
             AND "agent" NOT IN (SELECT "name" FROM "supervisors")
           ORDER BY "agent""#,
        vec![],
      )
      .await?;

      for row in waiting {
        let Ok(worker) = row.try_get::<String>("", "agent") else { continue };

        if running.holds(&worker) {
          continue;
        }

        if let Err(refusal) =
          crate::job_run::claude::reset::carry_out_worker_reset(&places.ground, &worker).await
        {
          eprintln!("agency: the reset of {worker}'s sessions was not carried out: {}", refusal.detail);
        }
      }

      Ok(())
    })
  }
}

/// A supervisor's session in one room, cleared by the next turn there (docs/supervisors.md, 「Clearing
/// it」).
struct Supervisor;

impl SessionHolder for Supervisor {
  fn sessions<'a>(
    &'a self,
    db: &'a DatabaseTransaction,
    _running: &'a Running,
  ) -> Holding<'a, Vec<AgentSession>> {
    Box::pin(async move {
      let by_room =
        crate::conversational::agents_where(db, |settings| settings.scope == Scope::Room).await?;
      let mut sessions = vec![];

      for name in by_room {
        let held = crate::supervisors::named(db, &name).await?;
        let models: Vec<Value> = held
          .iter()
          .filter_map(|held| held.road.as_ref().map(|road| Value::String(road.model.clone())))
          .collect();
        let rooms = all(
          db,
          "SELECT chat_id, generation FROM room_sessions WHERE agent = $1 ORDER BY chat_id",
          vec![name.clone().into()],
        )
        .await?;

        for room in rooms {
          let chat_id: i64 = room.try_get("", "chat_id")?;

          sessions.push(AgentSession {
            agent: name.clone(),
            chat_id: Some(chat_id),
            kind: "room",
            title: None,
            generation: Some(room.try_get("", "generation")?),
            models: models.clone(),
            running: false,
          });
        }
      }

      Ok(sessions)
    })
  }

  fn clear<'a>(
    &'a self,
    _db: &'a DatabaseTransaction,
    _session: &'a AgentSession,
  ) -> Holding<'a, Clearing> {
    Box::pin(async { Ok(Clearing::AtOnce) })
  }

  // A waiting ask is carried out by the session's handover, which the session carrier makes once the asks are
  // written (`carry_out_waiting`), and the program goes with it (docs/supervisors.md, 「Clearing it」); nothing is
  // taken down ahead of it, since the handoff turn goes on in that very session.
  fn carry_out<'a>(&'a self, _places: &'a crate::places::Places, _running: &'a Running) -> Holding<'a, ()> {
    Box::pin(async { Ok(()) })
  }
}

/// The supervisor: the one session every room's rounds go on in, when its row gives it the agent scope.
struct Chat;

impl SessionHolder for Chat {
  fn sessions<'a>(
    &'a self,
    db: &'a DatabaseTransaction,
    _running: &'a Running,
  ) -> Holding<'a, Vec<AgentSession>> {
    Box::pin(async move {
      let names = all(
        db,
        r#"SELECT "name" FROM "supervisors" WHERE "session_scope" = 'agent' ORDER BY "name""#,
        vec![],
      )
      .await?;
      let models = Vec::new();

      Ok(
        names
          .iter()
          .filter_map(|row| row.try_get::<String>("", "name").ok())
          .map(|agent| AgentSession {
            agent,
            chat_id: None,
            kind: "chat",
            title: None,
            generation: None,
            models: models.clone(),
            running: false,
          })
          .collect(),
      )
    })
  }

  fn clear<'a>(
    &'a self,
    _db: &'a DatabaseTransaction,
    _session: &'a AgentSession,
  ) -> Holding<'a, Clearing> {
    Box::pin(async { Ok(Clearing::Waits) })
  }
}

// Every holder, in the order they are asked.
const HOLDERS: [&dyn SessionHolder; 3] = [&Worker, &Supervisor, &Chat];

/// An ask to clear a session, waiting or carried out.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionReset {
  id: i64,
  agent: String,
  chat_id: Option<i64>,
  by: String,
  at: i64,
  reason: String,
  pub done_at: Option<i64>,
}

impl SessionReset {
  fn of(row: &QueryResult) -> Result<Self, sea_orm::DbErr> {
    Ok(Self {
      id: row.try_get("", "id")?,
      agent: row.try_get("", "agent")?,
      chat_id: row.try_get("", "chat_id")?,
      by: row.try_get("", "asked_by")?,
      at: row.try_get("", "asked_at")?,
      reason: row.try_get("", "reason")?,
      done_at: row.try_get("", "done_at")?,
    })
  }
}

/// The ask waiting for a session, or none.
pub async fn waiting<C: ConnectionTrait>(
  db: &C,
  agent: &str,
  chat_id: Option<i64>,
) -> Result<Option<SessionReset>, Refused> {
  let row = one(
    db,
    "SELECT * FROM session_resets WHERE agent = $1 AND chat_id IS NOT DISTINCT FROM $2 AND done_at IS NULL
     ORDER BY id LIMIT 1",
    vec![agent.into(), chat_id.into()],
  )
  .await?;

  Ok(row.as_ref().map(SessionReset::of).transpose()?)
}

/// The last ask carried out for a session, or none.
pub async fn last_done<C: ConnectionTrait>(
  db: &C,
  agent: &str,
  chat_id: Option<i64>,
) -> Result<Option<SessionReset>, Refused> {
  let row = one(
    db,
    "SELECT * FROM session_resets WHERE agent = $1 AND chat_id IS NOT DISTINCT FROM $2 AND done_at IS NOT NULL
     ORDER BY done_at DESC, id DESC LIMIT 1",
    vec![agent.into(), chat_id.into()],
  )
  .await?;

  Ok(row.as_ref().map(SessionReset::of).transpose()?)
}

/// Every supervisor's row, by name.
pub async fn awake_agents<C: ConnectionTrait>(db: &C) -> Result<Vec<QueryResult>, Refused> {
  Ok(all(db, r#"SELECT *, 'chat' AS "kind" FROM "supervisors" ORDER BY "name""#, vec![]).await?)
}

/// A supervisor's row by any name it answers to.
pub async fn agent_row<C: ConnectionTrait>(db: &C, name: &str) -> Result<Option<QueryResult>, Refused> {
  Ok(
    one(
      db,
      r#"SELECT *, 'chat' AS "kind" FROM "supervisors" WHERE "name" = $1"#,
      vec![canonical_agent(db, name).await?.into()],
    )
    .await?,
  )
}

/// A session as a caller shows it: the agent's label and icon, its room's title, the ask waiting for it and
/// when it was last cleared.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionView {
  #[serde(flatten)]
  session: AgentSession,
  label: String,
  icon: Option<String>,
  waiting: Option<SessionReset>,
  last_done_at: Option<i64>,
}

async fn view_of<C: ConnectionTrait>(
  db: &C,
  hooks: &Hooks,
  mut session: AgentSession,
) -> Result<SessionView, Refused> {
  let row = agent_row(db, &session.agent).await?;

  if let Some(chat_id) = session.chat_id {
    session.title = hooks.ask::<RoomTitle>(chat_id).await;
  }

  let label: Option<String> = row.as_ref().map(|row| row.try_get("", "label")).transpose()?.flatten();
  let kind: Option<String> = row.as_ref().map(|row| row.try_get("", "kind")).transpose()?.flatten();
  let own: Option<String> = row.as_ref().map(|row| row.try_get("", "icon")).transpose()?.flatten();
  let icon = kind.as_deref().and(ROLES.icon("supervisor")).map(str::to_owned).or(own);

  Ok(SessionView {
    label: label.unwrap_or_else(|| session.agent.clone()),
    icon,
    waiting: waiting(db, &session.agent, session.chat_id).await?,
    last_done_at: last_done(db, &session.agent, session.chat_id).await?.and_then(|done| done.done_at),
    session,
  })
}

/// An ask for one session a holder holds, kept under who asked and why unless one already waits for
/// it, and set going by its holder; a clean ask makes the one waiting clean (docs/session-resets.md, 「What a
/// reset is」). Answers how it is carried out, or none where it already waited.
async fn ask(
  db: &DatabaseTransaction,
  holder: &dyn SessionHolder,
  session: &AgentSession,
  by: &str,
  reason: &str,
  clean: bool,
) -> Result<Option<Clearing>, Refused> {
  if waiting(db, &session.agent, session.chat_id).await?.is_some() {
    if clean {
      run(
        db,
        "UPDATE session_resets SET clean = true WHERE agent = $1 AND chat_id IS NOT DISTINCT FROM $2 AND done_at IS NULL",
        vec![session.agent.clone().into(), session.chat_id.into()],
      )
      .await?;
    }

    return Ok(None);
  }

  run(
    db,
    "INSERT INTO session_resets (agent, chat_id, asked_by, asked_at, reason, clean) VALUES ($1, $2, $3, $4, $5, $6)",
    vec![
      session.agent.clone().into(),
      session.chat_id.into(),
      by.into(),
      (now_ms() / 1000).into(),
      reason.into(),
      clean.into(),
    ],
  )
  .await?;

  Ok(Some(holder.clear(db, session).await?))
}

/// Every waiting ask carried out as far as its holder carries one out here; the rest wait for their
/// session's next turn or job. A list of what runs that cannot be read counts every worker as running. What
/// it stands on is taken off the call before anything is awaited, so no route holds the call across a wait.
pub fn carry_out_waiting(call: &Call) -> impl Future<Output = ()> + Send + use<> {
  let places = call.places().cloned();
  let sessions = call.sessions();
  let reader = Running::reader(call);

  async move {
    let Some(places) = places else {
      return;
    };

    carry_out_on(&places, Running::read(reader).await).await;

    // A room's session a waiting ask clears is handed over by the session carrier (docs/session-resets.md, 「Who
    // holds a session」).
    if let Some(sessions) = sessions {
      sessions.replace_reset_rooms().await;
    }
  }
}

async fn carry_out_on(places: &crate::places::Places, running: Running) {
  for holder in HOLDERS {
    if let Err(refused) = holder.carry_out(places, &running).await {
      eprintln!("agency: the waiting session resets were not carried out: {}", refused.error);
    }
  }
}

/// An ask to clear every session one agent holds now; an agent holding none has nothing to clear.
/// Answers whether one of them clears at once, to be carried out once the asks are written.
pub async fn ask_agent_resets(
  db: &DatabaseTransaction,
  agent: &str,
  by: &str,
  reason: &str,
) -> Result<bool, Refused> {
  let running = Running::unread();
  let mut tell = false;

  for holder in HOLDERS {
    for session in holder.sessions(db, &running).await?.into_iter().filter(|one| one.agent == agent) {
      tell |= ask(db, holder, &session, by, reason, false).await? == Some(Clearing::AtOnce);
    }
  }

  Ok(tell)
}

/// Every session there is now, in the agent list's order.
pub struct Sessions;

impl Route for Sessions {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/sessions";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let running = Running::read(Running::reader(&call)).await;
    let db = call.store().begin().await?;
    let order: HashMap<String, usize> = awake_agents(&db)
      .await?
      .iter()
      .enumerate()
      .map(|(at, row)| row.try_get::<String>("", "name").map(|name| (name, at)))
      .collect::<Result<_, _>>()?;
    let mut views = vec![];

    for holder in HOLDERS {
      for session in holder.sessions(&db, &running).await? {
        views.push(view_of(&db, &call.ground().hooks, session).await?);
      }
    }

    let mut kept = HashMap::new();

    for view in &views {
      let id = match view.session.chat_id {
        Some(room) => format!("{}@{}", view.session.agent, room),
        None => view.session.agent.clone(),
      };

      kept.insert(id.clone(), super::session_ops::holder_of(&db, &id).await?);
    }

    db.commit().await?;

    let place = |view: &SessionView| order.get(&view.session.agent).copied().unwrap_or(usize::MAX);

    views.sort_by(|a, b| match place(a).cmp(&place(b)) {
      Ordering::Equal => a.session.chat_id.unwrap_or(0).cmp(&b.session.chat_id.unwrap_or(0)),
      unequal => unequal,
    });

    let agent = call.query("agentName").unwrap_or_default();
    let scope = call.query("scope").unwrap_or_default();
    let room = call.query("roomId").and_then(|room| room.parse::<i64>().ok());
    let mut items: Vec<Value> = views
      .iter()
      .filter(|view| agent.is_empty() || view.session.agent == agent)
      .filter(|view| {
        let word = if view.session.chat_id.is_some() { "room" } else { "agent" };
        scope.is_empty() || scope == word
      })
      .filter(|view| room.is_none_or(|room| view.session.chat_id == Some(room)))
      .map(|view| {
        let session = &view.session;
        let id = match session.chat_id {
          Some(room) => format!("{}@{}", session.agent, room),
          None => session.agent.clone(),
        };
        let model = session.models.first().cloned().unwrap_or(Value::Null);
        let (holder, reason) = kept.get(&id).cloned().unwrap_or_default();
        json!({
          "id": id,
          "agentName": session.agent,
          "scope": if session.chat_id.is_some() { "room" } else { "agent" },
          "roomId": session.chat_id,
          "provider": model.get("provider").cloned().unwrap_or(Value::Null),
          "model": model.get("model").cloned().unwrap_or(Value::Null),
          "place": Value::Null,
          "generation": session.generation.unwrap_or(0),
          "holder": holder,
          "prefixChangeReason": reason,
          "options": {},
        })
      })
      .collect();

    // The thrown-away sessions and conversations the backend holds, which no holder keeps.
    if let Some(backend) = call.sessions()
      && let Ok(held) = backend.list().await
    {
      for session in held {
        let (kind, provider, model) = match &session.scope {
          agent_session::Scope::Apart(apart) => ("role", &apart.provider, &apart.model),
          agent_session::Scope::Conversation(conversation) => {
            ("conversation", &conversation.provider, &conversation.model)
          }
          _ => continue,
        };

        if !agent.is_empty() && session.agent != agent {
          continue;
        }

        if !scope.is_empty() && scope != kind {
          continue;
        }

        if room.is_some() {
          continue;
        }

        items.push(json!({
          "id": session.id.0,
          "agentName": session.agent,
          "scope": kind,
          "roomId": Value::Null,
          "provider": provider,
          "model": model,
          "place": Value::Null,
          "generation": 1,
          "holder": Value::Null,
          "prefixChangeReason": Value::Null,
          "options": {},
        }));
      }
    }

    Ok(answer(StatusCode::OK, json!({ "items": items, "more": false })))
  }
}

crate::moved!(Sessions);

/// The session a body names: an agent, and a room or none.
pub struct Asked {
  pub agent: String,
  pub chat_id: Option<i64>,
  /// A room named by a number no room is, kept as written for the refusal.
  pub no_room: Option<String>,
}

impl Asked {
  /// The session a body names, or none where it names none.
  pub fn read(body: &Value) -> Option<Self> {
    let agent = body.as_object()?.get("agent")?.as_str()?.to_owned();

    match body.get("chatId") {
      None | Some(Value::Null) => Some(Self {
        agent,
        chat_id: None,
        no_room: None,
      }),
      Some(room @ Value::Number(_)) => Some(match whole(room) {
        Some(chat_id) => Self {
          agent,
          chat_id: Some(chat_id),
          no_room: None,
        },
        None => Self {
          agent,
          chat_id: None,
          no_room: Some(room.to_string()),
        },
      }),
      Some(_) => None,
    }
  }

  fn nothing_held(&self) -> Response {
    let room = self.no_room.clone().or_else(|| self.chat_id.map(|id| id.to_string()));
    let words = &CONTROL_WORDS.sessions;
    let error = match room {
      Some(room) => fill_marks(&words.none_in_room, &[("agent", &self.agent), ("room", &room)]),
      None => fill_marks(&words.none, &[("agent", &self.agent)]),
    };

    answer(StatusCode::BAD_REQUEST, json!({ "error": error, "field": "session" }))
  }
}

/// Whether a body asks a clean reset: its `clean` true, and not where it names none.
pub fn clean_of(body: &Value) -> bool {
  body.get("clean").and_then(Value::as_bool).unwrap_or(false)
}

/// Who a body says asked, or the console where it names nobody.
pub fn asked_by(body: &Value) -> String {
  body
    .get("by")
    .and_then(Value::as_str)
    .map(str::trim)
    .filter(|by| !by.is_empty())
    .unwrap_or("console")
    .to_owned()
}

/// An ask to clear one session, answered with the session as it stands after.
pub struct SessionResetAsk;

impl Route for SessionResetAsk {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/session-reset";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let Some(asked) = Asked::read(&body) else {
      return Err(Refused::new(StatusCode::BAD_REQUEST, CONTROL_WORDS.sessions.asked_shape.as_str()));
    };

    if asked.no_room.is_some() {
      return Ok(asked.nothing_held());
    }

    let running = Running::read(Running::reader(&call)).await;
    let db = call.store();
    let written = db.begin().await?;
    let mut found = None;

    for holder in HOLDERS {
      let held = holder.sessions(&written, &running).await?;

      if let Some(session) = held
        .into_iter()
        .find(|one| one.agent == asked.agent && (one.chat_id == asked.chat_id || one.chat_id.is_none()))
      {
        let clearing = ask(
          &written,
          holder,
          &session,
          &asked_by(&body),
          &CONTROL_WORDS.sessions.asked,
          clean_of(&body),
        )
        .await?;

        found = Some((session, clearing));
        break;
      }
    }

    written.commit().await?;

    let Some((session, clearing)) = found else {
      return Ok(asked.nothing_held());
    };

    if clearing == Some(Clearing::AtOnce) {
      carry_out_waiting(&call).await;
    }

    Ok(Json(view_of(db, &call.ground().hooks, session).await?).into_response())
  }
}

crate::moved!(SessionResetAsk);
