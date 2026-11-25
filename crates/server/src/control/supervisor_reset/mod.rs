//! A supervisor started afresh in the ranges a caller names (docs/supervisors.md, 「Starting it afresh」): what
//! it said, what it remembers, or both. Agency takes its own part — its rows in the store, the records its
//! harnesses keep of the sessions it ran, and the program standing in its place — asks the brain to take
//! what it remembers, and tells the units holding the rest once it is done. Which rooms it answers, alone
//! or with others, the caller names.

pub mod cut;
pub mod files;
mod record;
pub mod rows;

use std::path::PathBuf;
use std::sync::Arc;

use axum::http::{Method, StatusCode};
use axum::response::Response;
use harness_machine::supervisor_prompt::Roots;
use harness_providers::Machine;
use policy::control_words::CONTROL_WORDS;
use policy::fill_marks;
use serde_json::{Value, json};
use store::one;

use self::rows::{Counted, Range, Scope};
use super::claude_accounts::login_homes;
use super::sessions::asked_by;
use super::supervisor_settings::roots;
use super::{Call, Refused, Route, answer};
use crate::now_ms;
use crate::places::follow::RoomTitle;
use crate::places::{Ground, Places};
use crate::supervisor_turn::going::GoingRuns;
use crate::supervisor_turn::overtaken::ResetHold;
use crate::supervisor_turn::reset::SupervisorReset as Reset;

/// What a reset was asked for: the supervisor, the ranges, and the rooms the caller says it answers alone
/// and those it shares.
pub struct Asked {
  pub agent: String,
  pub ranges: Vec<Range>,
  pub alone: Vec<i64>,
  pub shared: Vec<i64>,
}

fn switched(value: Option<&Value>) -> bool {
  match value {
    Some(Value::Bool(on)) => *on,
    Some(Value::String(text)) => matches!(text.as_str(), "1" | "true" | "on"),
    Some(Value::Number(number)) => number.as_i64() == Some(1),
    _ => false,
  }
}

/// The rooms a body or a query names: a list of whole numbers, or one text of them joined by commas.
pub fn rooms_named(value: Option<&Value>) -> Vec<i64> {
  match value {
    Some(Value::Array(rooms)) => rooms.iter().filter_map(super::whole).collect(),
    Some(Value::String(text)) => text.split(',').filter_map(|room| room.trim().parse().ok()).collect(),
    _ => vec![],
  }
}

impl Asked {
  /// What a body or a query asks, or the refusal of one naming no supervisor or no range.
  pub fn read(
    agent: Option<&Value>,
    conversation: Option<&Value>,
    person: Option<&Value>,
    rooms: (Option<&Value>, Option<&Value>),
  ) -> Result<Self, Refused> {
    let words = &CONTROL_WORDS.supervisor_reset;
    let agent = agent.and_then(Value::as_str).map(str::trim).unwrap_or_default().to_owned();

    if agent.is_empty() {
      return Err(Refused::new(StatusCode::BAD_REQUEST, words.no_agent.as_str()));
    }

    let ranges: Vec<Range> = [(Range::Conversation, conversation), (Range::Person, person)]
      .into_iter()
      .filter(|(_, value)| switched(*value))
      .map(|(range, _)| range)
      .collect();

    if ranges.is_empty() {
      return Err(Refused::new(StatusCode::BAD_REQUEST, words.no_range.as_str()));
    }

    let alone = rooms_named(rooms.0);
    let shared = rooms_named(rooms.1).into_iter().filter(|room| !alone.contains(room)).collect();

    Ok(Self {
      agent,
      ranges,
      alone,
      shared,
    })
  }

  fn takes(&self, range: Range) -> bool {
    self.ranges.contains(&range)
  }
}

/// The supervisor as its row says, refused where it is none.
struct Supervisor {
  label: String,
  // Its folder under the brain, where it keeps one.
  folder: Option<String>,
}

async fn supervisor(db: &sea_orm::DatabaseConnection, agent: &str) -> Result<Supervisor, Refused> {
  let words = &CONTROL_WORDS.supervisor_reset;
  let row = one(db, r#"SELECT * FROM "supervisors" WHERE "name" = $1"#, vec![agent.into()]).await?;
  let Some(row) = row else {
    return Err(Refused::new(
      StatusCode::NOT_FOUND,
      fill_marks(&words.not_supervisor, &[("agent", agent)]),
    ));
  };
  let folder = row
    .try_get::<Option<String>>("", "brain_folder")?
    .map(|folder| folder.trim_end_matches('/').to_owned());

  // A supervisor's folder is its own; the brain root holds every memory, no one supervisor's.
  if folder.as_deref() == Some("") {
    return Err(Refused::new(
      StatusCode::BAD_REQUEST,
      fill_marks(&words.brain_root, &[("agent", agent)]),
    ));
  }

  Ok(Supervisor {
    label: row.try_get::<Option<String>>("", "label")?.unwrap_or_else(|| agent.to_owned()),
    folder,
  })
}

/// What the front stands on that a reset reads, taken out of the call before anything is awaited.
pub struct Standing {
  ground: Arc<Ground>,
  machine: Machine,
  roots: Roots,
  places: Option<Places>,
  hold: ResetHold,
  going: Arc<GoingRuns>,
}

impl Standing {
  pub fn of(call: &Call) -> Self {
    let places = call.places().cloned();

    Self {
      ground: call.shared_ground(),
      machine: call.machine().clone(),
      roots: roots(call),
      hold: places.as_ref().map(|places| places.kept.reset.clone()).unwrap_or_default(),
      going: call.going(),
      places,
    }
  }

  fn brain_root(&self) -> PathBuf {
    self.ground.config.brain_root(&self.ground.roots.data)
  }
}

/// The folders the harnesses ran in whose sessions the ranges asked take: the supervisor's home for the
/// conversation, and its brain folder, for the person range.
fn harness_folders(standing: &Standing, asked: &Asked, supervisor: &Supervisor) -> Vec<PathBuf> {
  let mut folders = vec![];

  if asked.takes(Range::Conversation) {
    folders.push(harness_machine::supervisor_prompt::folder(&standing.roots, &asked.agent));
  }

  if asked.takes(Range::Person)
    && let Some(folder) = supervisor.folder.as_deref()
  {
    folders.push(standing.brain_root().join(folder));
  }

  folders
}

/// The records the harnesses keep of the sessions the ranges asked take, `logins` the homes of the logins
/// agency holds.
fn harness_records(
  standing: &Standing,
  asked: &Asked,
  supervisor: &Supervisor,
  logins: &[PathBuf],
) -> Vec<PathBuf> {
  harness_folders(standing, asked, supervisor)
    .iter()
    .flat_map(|folder| files::harness_records(&standing.machine, logins, folder))
    .collect()
}

/// Each room as a caller names it: its number and the title its owner answers.
async fn rooms(standing: &Standing, chat_ids: &[i64]) -> Vec<Value> {
  let mut shown = vec![];

  for chat_id in chat_ids {
    let title = standing.ground.hooks.ask::<RoomTitle>(*chat_id).await;

    shown.push(json!({ "chatId": chat_id, "title": title }));
  }

  shown
}

fn counted(rows: &[Counted]) -> Value {
  rows
    .iter()
    .map(|one| json!({ "table": one.table, "range": one.range.name(), "rows": one.rows }))
    .collect()
}

async fn view(
  standing: &Standing,
  asked: &Asked,
  supervisor: &Supervisor,
  rows: &[Counted],
  files: &[PathBuf],
  brain: &Value,
) -> Value {
  json!({
    "agent": asked.agent,
    "label": supervisor.label,
    "ranges": asked.ranges.iter().map(|range| range.name()).collect::<Vec<_>>(),
    "alone": rooms(standing, &asked.alone).await,
    "shared": rooms(standing, &asked.shared).await,
    "rows": counted(rows),
    "files": files.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
    "brain": brain,
  })
}

fn scope(asked: &Asked) -> Scope {
  Scope {
    agent: asked.agent.clone(),
    alone: asked.alone.clone(),
  }
}

/// A reset of the ranges asked, answered with what it took.
pub struct SupervisorReset;

impl Route for SupervisorReset {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/supervisors/{supervisorName}/resets";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let agent = call.param("supervisorName").map(|name| Value::String(name.to_owned()));
    let asked = Asked::read(
      agent.as_ref(),
      body.get("conversation"),
      body.get("person"),
      (body.get("alone"), body.get("shared")),
    )?;
    let standing = Standing::of(&call);

    if body.get("dryRun").and_then(Value::as_bool) == Some(true) {
      let db = &standing.ground.db;
      let supervisor = supervisor(db, &asked.agent).await?;
      let rows = rows::count(db, &rows::picks(&scope(&asked), &asked.ranges)).await?;
      let logins = login_homes(db, &standing.machine).await?;
      let files = harness_records(&standing, &asked, &supervisor, &logins);
      let brain = if asked.takes(Range::Person) {
        let preview =
          standing.ground.peers.brain.reset(&asked.agent, true).await.map_err(Refused::failed)?;

        json!({ "took": preview.took, "couldNot": preview.could_not })
      } else {
        Value::Null
      };

      return Ok(answer(
        StatusCode::OK,
        view(&standing, &asked, &supervisor, &rows, &files, &brain).await,
      ));
    }

    let by = asked_by(&body);
    let shown = to_its_end(async move { carry_out(&standing, &asked, &by).await }).await?;

    Ok(answer(StatusCode::OK, shown))
  }
}

crate::moved!(SupervisorReset);

/// Work carried out apart from the request that asked it, so a caller that stops waiting loses only the
/// answer: a request's own future is dropped when its connection closes, which would leave a reset or a
/// removal cut wherever it stood.
pub async fn to_its_end<T: Send + 'static>(
  work: impl Future<Output = Result<T, Refused>> + Send + 'static,
) -> Result<T, Refused> {
  tokio::spawn(work).await.map_err(Refused::failed)?
}

/// A reset of the conversation range, and of the person range where the brain is to go with it, as a
/// supervisor's removal starts with, answered with what it took: every room it answers read as one it shares,
/// so each keeps its records and only the supervisor's own rows go. What it reads of the call is taken
/// before it is awaited.
pub fn reset_whole(
  call: &Call,
  agent: &str,
  rooms: Vec<i64>,
  by: &str,
  forget_brain: bool,
) -> impl Future<Output = Result<Value, Refused>> + Send + use<> {
  let standing = Standing::of(call);
  let ranges = if forget_brain {
    vec![Range::Conversation, Range::Person]
  } else {
    vec![Range::Conversation]
  };
  let asked = Asked {
    agent: agent.to_owned(),
    ranges,
    alone: vec![],
    shared: rooms,
  };
  let by = by.to_owned();

  async move { carry_out(&standing, &asked, &by).await }
}

/// A reset of the ranges asked, carried out, and what it took.
async fn carry_out(standing: &Standing, asked: &Asked, by: &str) -> Result<Value, Refused> {
  let ground = &standing.ground;
  let db = &ground.db;
  let supervisor = supervisor(db, &asked.agent).await?;
  let picks = rows::picks(&scope(asked), &asked.ranges);
  let logins = login_homes(db, &standing.machine).await?;
  let mut files = harness_records(standing, asked, &supervisor, &logins);
  let mut kept: Vec<String> = vec![];
  let cut_off = || cut::cut_off(db, &ground.events, &standing.hold, &standing.going, &asked.agent);

  // Every turn of the supervisor begun until now is overtaken and its runs cancelled before anything is
  // taken, so nothing it takes is written again behind it.
  cut_off().await?;

  // The room sessions the conversation range ends, and the runs its log lines went under, read before their
  // rows go: the rooms' owner takes the records written under those runs by name.
  let ended = if asked.takes(Range::Conversation) {
    record::sessions(db, &asked.agent).await?
  } else {
    vec![]
  };
  let runs = if asked.takes(Range::Conversation) {
    rows::runs_of(db, &asked.agent).await?
  } else {
    vec![]
  };
  let rows = rows::delete(db, &picks).await?;

  // The conversation range ends the sessions it takes, and the holds they took go with them (docs/api.md,
  // 「assignments and jobs」).
  if asked.takes(Range::Conversation) {
    let mut holders = vec![asked.agent.clone()];

    holders.extend(ended.iter().map(|chat_id| format!("{}@{chat_id}", asked.agent)));

    for holder in holders {
      if let Err(why) = crate::control::work::release_held(db, &holder).await {
        eprintln!("{}: the holds the reset ended were not released: {}", asked.agent, why.error);
      }
    }
  }

  let at = now_ms() / 1000;

  // What it remembers is the brain's to take: everything it holds under the supervisor's scope, so nothing
  // said before comes back.
  let brain = if asked.takes(Range::Person) {
    let took = ground.peers.brain.reset(&asked.agent, false).await.map_err(Refused::failed)?;

    kept.extend(took.could_not.iter().map(|said| format!("brain: {said}")));

    json!({ "took": took.took, "couldNot": took.could_not })
  } else {
    Value::Null
  };

  // The program standing in its place goes with the conversation, whatever session it stands on, since
  // every one is the conversation's (docs/supervisors.md, 「The program on an ended session」), and before
  // the records it writes to as it ends, which are read again then, so a session opened while the reset ran
  // goes with the rest. A turn holding it is one the reset overtook, and takes it down itself as it ends.
  if asked.takes(Range::Conversation)
    && let Some(places) = &standing.places
  {
    crate::supervisor_turn::run::let_go_any(&places.ground, &asked.agent).await;
  }

  // Its place's records hold every prompt, answer and title of the sessions its program stood on, each of
  // them the conversation's. A program an overtaken turn still holds writes on, and that turn takes its
  // session's lines out.
  let place = crate::lines::place::place_of(&ground.roots.data, &asked.agent);

  if asked.takes(Range::Conversation)
    && let Err(error) = crate::door::place_run::forget_records(&place, None)
  {
    kept.push(format!("{}: {error}", place.display()));
  }

  for path in harness_records(standing, asked, &supervisor, &logins) {
    if !files.contains(&path) {
      files.push(path);
    }
  }

  // A record an overtaken turn removed behind its own program, before or while the reset reached it, is
  // gone as asked.
  for path in &files {
    if let Err(error) = files::remove(path)
      && path.exists()
    {
      kept.push(format!("{}: {error}", path.display()));
    }
  }

  // The prompts the harnesses kept in their histories hold the words the records did.
  for folder in harness_folders(standing, asked, &supervisor) {
    kept.extend(files::forget_prompts(&standing.machine, &logins, &folder));
  }

  record::write(
    db,
    &record::Taken {
      agent: &asked.agent,
      by,
      at,
      ended: &ended,
    },
  )
  .await?;

  // Cut off again now the store holds nothing of what it took: a turn begun while it ran may have read it.
  if let Err(said) = cut_off().await {
    kept.push(format!("reset point: {}", said.error));
  }

  // The units holding the rest of it take their own rows.
  ground.events.publish(Reset {
    agent: asked.agent.clone(),
    ranges: asked.ranges.iter().map(|range| range.name().to_owned()).collect(),
    alone: asked.alone.clone(),
    shared: asked.shared.clone(),
    runs,
    by: by.to_owned(),
    at,
  });

  let mut shown = view(standing, asked, &supervisor, &rows, &files, &brain).await;

  shown["failures"] = json!(kept);

  Ok(shown)
}

/// The moment an agent's last reset cut it off at, which a consumer holds a turn it carries to: a turn begun
/// at or before it is overtaken.
pub struct ResetPoint;

impl Route for ResetPoint {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/reset-point";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let Some(agent) = call
      .query("agent")
      .map(|agent| agent.trim().to_owned())
      .filter(|agent| !agent.is_empty())
    else {
      return Err(Refused::new(StatusCode::BAD_REQUEST, "agent is required"));
    };
    let point = crate::supervisor_turn::overtaken::point(call.store(), &agent).await;

    Ok(answer(StatusCode::OK, json!({ "point": point })))
  }
}
