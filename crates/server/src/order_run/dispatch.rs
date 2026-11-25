//! The role slots (docs/role-slots.md): every run of a role asks a slot of it before its program launches,
//! waits in the role's line, is let in under the role's cap on the provider its order — or the road its
//! caller fixed — has quota on, is bound to the run it launched and given back with what it came to; and an
//! owner reserves slots of a role ahead. The labels and keys of a consumer's work arrive with each request;
//! nothing here reads its orders.

use std::sync::LazyLock;

use regex::Regex;
use sea_orm::{ConnectionTrait, DatabaseConnection, QueryResult, TransactionTrait};
use serde::Serialize;
use store::{all, one, run};

use crate::agent_roles::{self, AgentRole};
use crate::events::{Bus, Event};
use crate::order::Entry;
use crate::places::Ground;
use crate::road::{self, Ask, Row};

/// What a branch of a work order asks a slot as.
pub const COWORK: &str = "cowork";
/// What a job's run asks a slot as.
pub const JOB: &str = "job";

/// The shape a request's, an owner's and a reservation's key is held to.
pub static KEY: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9:._/@-]{0,499}$").expect("a pattern"));
static SOURCE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_-]{0,63}$").expect("a pattern"));

/// Why a slot step did not go through, in one English sentence.
pub type Fault = String;

fn faulted(why: impl std::fmt::Display) -> Fault {
  why.to_string()
}

/// The most slots a reservation holds.
pub const MOST_SEATS: i64 = 128;

/// A text the line keeps: trimmed, not empty, within its length and with no null in it.
pub fn text(value: &str, what: &str, most: usize) -> Result<String, Fault> {
  let clean = value.trim();

  if clean.is_empty() || clean.encode_utf16().count() > most || clean.contains('\0') {
    return Err(format!("Invalid {what}"));
  }

  Ok(clean.to_owned())
}

/// One slot request with its lease (docs/api.md, 「slots, reservations and places」).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
  pub sequence: i64,
  pub token: String,
  pub request_key: String,
  pub owner: String,
  pub source: String,
  pub role: String,
  /// The path its run will work at, where its caller names one.
  pub path: Option<String>,
  /// The provider, model and effort it is let in on; empty until it is, unless its caller fixed its road.
  pub provider: String,
  pub model: String,
  pub effort: Option<String>,
  /// The role's cap when it was let in.
  pub capacity: i64,
  pub state: String,
  pub enqueued_at: i64,
  pub leased_at: Option<i64>,
  pub child_id: Option<String>,
  pub ended_at: Option<i64>,
  pub outcome: Option<String>,
  pub reservation: Option<String>,
  pub yielding: bool,
  pub place_key: Option<String>,
  /// The session its run continues, where its caller named one.
  pub session: Option<String>,
  /// One line naming the work it is asked for, where its asker names one.
  pub title: Option<String>,
  /// The supervisor that asked for it, where its asker names one.
  pub asked_by: Option<String>,
  /// The account it belongs to: its asking supervisor's owner account, none for the deployment's own
  /// (docs/role-slots.md, 「Accounts」).
  pub account: Option<String>,
  /// The run its child is, where its binding names one.
  pub agent_name: Option<String>,
  /// The one agent of its role it asks to be let in on, where its asker named one.
  pub agent_wanted: Option<String>,
  /// The agent of its role it was let in on, its seat (docs/role-agents.md); none until it is let in.
  pub seat: Option<String>,
  /// Why it waits where no provider of its road has quota, or none.
  pub held_for: Option<String>,
  /// Who gave it back — the token's holder, its owner, or the line on evidence its run ended — or none while
  /// it stands.
  pub ended_by: Option<String>,
}

/// Who gave a request back (docs/role-slots.md, 「The line」).
pub const BY_HOLDER: &str = "holder";
pub const BY_OWNER: &str = "owner";
pub const BY_RUN_ENDED: &str = "run-ended";

/// The job a request is the run of, where its source is a job's: a job's slot is asked under the owner
/// `job-<id>` (docs/api.md, 「slots, reservations and places」).
pub fn job_of(request: &Request) -> Option<i64> {
  (request.source == JOB)
    .then(|| request.owner.strip_prefix(JOB_OWNER)?.parse().ok())
    .flatten()
}

/// The owner a job's slot is asked under, before the job's id.
pub const JOB_OWNER: &str = "job-";

impl Request {
  /// Whether its caller fixed the road it goes out on, which it then waits for and never leaves.
  pub fn fixed(&self) -> bool {
    !self.provider.is_empty() && self.state == "waiting"
  }
}

/// The event published as a slot request changes (docs/api.md, 「Events」): the request's token, its source and
/// its state, and its lease's fields as far as they are set.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotRequestChanged {
  pub token: String,
  pub source: String,
  pub state: String,
  pub role: String,
  pub owner: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub asked_by: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub title: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub provider: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub model: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub child_id: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub agent_name: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub outcome: Option<String>,
  pub enqueued_at: i64,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub leased_at: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ended_at: Option<i64>,
}

impl Event for SlotRequestChanged {
  const NAME: &'static str = "slot-request.changed";
}

impl SlotRequestChanged {
  /// The change one request is published as.
  pub fn of(request: &Request) -> Self {
    let some = |value: &Option<String>| value.clone().filter(|value| !value.is_empty());
    let kept = |value: &str| (!value.is_empty()).then(|| value.to_owned());

    Self {
      token: request.token.clone(),
      source: request.source.clone(),
      state: request.state.clone(),
      role: request.role.clone(),
      owner: request.owner.clone(),
      asked_by: some(&request.asked_by),
      title: some(&request.title),
      provider: kept(&request.provider),
      model: kept(&request.model),
      child_id: some(&request.child_id),
      agent_name: some(&request.agent_name),
      outcome: some(&request.outcome),
      enqueued_at: request.enqueued_at,
      leased_at: request.leased_at,
      ended_at: request.ended_at,
    }
  }
}

/// One request's change published on the bus (docs/api.md, 「Events」).
pub fn announce(events: &Bus, request: &Request) {
  events.publish(SlotRequestChanged::of(request));
}

fn optional_text(stored: String) -> Option<String> {
  (!stored.is_empty()).then_some(stored)
}

/// A request as its row holds it.
pub fn request_of(row: &QueryResult) -> Result<Request, Fault> {
  let get = |why: sea_orm::DbErr| why.to_string();

  Ok(Request {
    sequence: row.try_get("", "sequence").map_err(get)?,
    token: row.try_get("", "token").map_err(get)?,
    request_key: row.try_get("", "request_key").map_err(get)?,
    owner: row.try_get("", "owner").map_err(get)?,
    source: row.try_get("", "source").map_err(get)?,
    role: row.try_get("", "role").map_err(get)?,
    path: row.try_get("", "path").map_err(get)?,
    provider: row.try_get("", "provider").map_err(get)?,
    model: row.try_get("", "model").map_err(get)?,
    effort: optional_text(row.try_get("", "effort").map_err(get)?),
    capacity: row.try_get("", "capacity").map_err(get)?,
    state: row.try_get("", "state").map_err(get)?,
    enqueued_at: row.try_get("", "enqueued_at").map_err(get)?,
    leased_at: row.try_get("", "leased_at").map_err(get)?,
    child_id: row.try_get("", "child_id").map_err(get)?,
    ended_at: row.try_get("", "ended_at").map_err(get)?,
    outcome: row.try_get("", "outcome").map_err(get)?,
    reservation: row.try_get("", "reservation").map_err(get)?,
    yielding: row.try_get::<String>("", "class").map_err(get)? == "yielding",
    place_key: row.try_get("", "place_key").map_err(get)?,
    session: row.try_get("", "session").map_err(get)?,
    title: row.try_get("", "title").map_err(get)?,
    asked_by: row.try_get("", "asked_by").map_err(get)?,
    account: row.try_get("", "account").map_err(get)?,
    agent_name: row.try_get("", "agent").map_err(get)?,
    agent_wanted: row
      .try_get::<Option<String>>("", "agent_wanted")
      .map_err(get)?
      .filter(|name| !name.is_empty()),
    seat: row.try_get("", "seat").map_err(get)?,
    held_for: row.try_get("", "held_for").map_err(get)?,
    ended_by: row.try_get("", "ended_by").map_err(get)?,
  })
}

/// One owner's reservation of a role's slots.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reservation {
  pub token: String,
  pub reservation_key: String,
  pub state: String,
  pub owner: String,
  pub source: String,
  pub role: String,
  pub wanted: i64,
  pub created_at: i64,
}

/// A reservation as its row holds it.
pub fn reservation_of(row: &QueryResult) -> Result<Reservation, Fault> {
  let get = |why: sea_orm::DbErr| why.to_string();

  Ok(Reservation {
    token: row.try_get("", "token").map_err(get)?,
    reservation_key: row.try_get("", "reservation_key").map_err(get)?,
    state: row.try_get("", "state").map_err(get)?,
    owner: row.try_get("", "owner").map_err(get)?,
    source: row.try_get("", "source").map_err(get)?,
    role: row.try_get("", "role").map_err(get)?,
    wanted: row.try_get("", "wanted").map_err(get)?,
    created_at: row.try_get("", "created_at").map_err(get)?,
  })
}

/// What a slot is asked for: the role, the path its run works at, and the road its caller fixes where it
/// fixes one — a repository's pin, the asking supervisor's own, a session's named road.
#[derive(Clone, Debug, Default)]
pub struct Wanted<'a> {
  pub role: &'a str,
  pub path: Option<&'a str>,
  pub fixed: Option<Entry>,
  /// The one agent of the role it is to be let in on, where its asker names one.
  pub agent: Option<&'a str>,
}

/// One slot asked of a role's line: under a key the asker names again, by whom, for what, and where only one
/// run at a time may stand.
#[derive(Clone, Debug, Default)]
pub struct Asked<'a> {
  pub request_key: String,
  pub owner: String,
  pub wanted: Wanted<'a>,
  pub yielding: bool,
  pub token: Option<&'a str>,
  pub place_key: Option<&'a str>,
  pub session: Option<&'a str>,
  pub title: Option<&'a str>,
  pub asked_by: Option<&'a str>,
}

/// The road a caller names for its slot: the provider and model, at the effort and thinking the role's agent
/// on that road names, else its first agent on that provider.
pub async fn named_road<C: ConnectionTrait>(
  db: &C,
  role: &str,
  provider: &str,
  model: &str,
) -> Result<Entry, Fault> {
  let held = role_of(db, role).await?;
  let own = held.order.entry_of(provider, model);

  Ok(Entry {
    provider: provider.to_owned(),
    model: model.to_owned(),
    effort: own.and_then(|entry| entry.effort.clone()),
    thinking: own.and_then(|entry| entry.thinking.clone()),
    answer_tokens: own.and_then(|entry| entry.answer_tokens),
    advisor: own.and_then(|entry| entry.advisor.clone()),
  })
}

/// The provider a slot naming a model alone goes out on (docs/role-slots.md, 「Who takes a slot」): the first
/// entry of the role's order naming that model, else the role's active provider as the ask is made, else the
/// order's first.
pub async fn provider_for_model(
  ground: &crate::places::Ground,
  role: &str,
  model: &str,
) -> Result<String, Fault> {
  let held = role_of(&ground.db, role).await?;

  if let Some(entry) = held.order.entries.iter().find(|entry| entry.model == model) {
    return Ok(entry.provider.clone());
  }

  let ask = crate::road::Ask::of(crate::road::Row::Role(&held.name), &held.line, &held.order);
  let active = crate::road::resolve(ground, &ask)
    .await
    .ok()
    .and_then(|resolution| resolution.road().map(|road| road.provider.clone()));

  active
    .or_else(|| held.order.first().map(|entry| entry.provider.clone()))
    .ok_or_else(|| format!("The {role} role's order names no provider"))
}

/// The road a slot naming none is fixed to: for a role whose order is empty — the coder, as seeded — the one
/// road of the supervisor the ask names as its asker, as it stands now (docs/role-slots.md, 「A role that follows
/// its supervisor」). A slot of such a role no supervisor asks is refused, since it has no road. A role whose
/// order holds an entry is fixed to nothing and walks that order, whoever asks it.
pub async fn fixed_road<C: ConnectionTrait>(
  db: &C,
  role: &str,
  asked_by: Option<&str>,
) -> Result<Option<Entry>, Fault> {
  let held = role_of(db, role).await?;

  if !held.order.follows_supervisor() {
    return Ok(None);
  }

  let supervisor = match asked_by {
    Some(name) => crate::supervisors::named(db, name).await.map_err(faulted)?,
    None => None,
  };
  let Some(supervisor) = supervisor else {
    return Err(format!(
      "The {role} role's order is empty, so it goes out on the road of the supervisor that asks it, and no \
       supervisor asked this slot"
    ));
  };

  supervisor
    .road()
    .map(Some)
    .ok_or_else(|| format!("The supervisor {} names no provider to go out on", supervisor.name))
}

/// The request a token names.
pub async fn request<C: ConnectionTrait>(db: &C, token: &str) -> Result<Option<Request>, Fault> {
  one(db, r#"SELECT * FROM "role_slot_requests" WHERE "token" = $1"#, vec![token.into()])
    .await
    .map_err(faulted)?
    .as_ref()
    .map(request_of)
    .transpose()
}

/// The requests standing in the given states, in the order they came.
pub async fn requests<C: ConnectionTrait>(db: &C, states: &[&str]) -> Result<Vec<Request>, Fault> {
  let marks: Vec<String> = (1..=states.len()).map(|at| format!("${at}")).collect();
  let sql = format!(
    r#"SELECT * FROM "role_slot_requests" WHERE "state" IN ({}) ORDER BY "sequence""#,
    marks.join(",")
  );

  all(db, &sql, states.iter().map(|state| (*state).into()).collect())
    .await
    .map_err(faulted)?
    .iter()
    .map(request_of)
    .collect()
}

/// The role a slot is asked under, which the store must hold: an unknown role is an error, never a run on some
/// default (docs/role-slots.md, 「Who takes a slot」).
pub async fn role_of<C: ConnectionTrait>(db: &C, name: &str) -> Result<AgentRole, Fault> {
  agent_roles::named(db, name)
    .await
    .map_err(faulted)?
    .ok_or_else(|| agent_roles::unknown(name))
}

/// A slot asked of its role's line under the source its caller labels it with, or the one its token already
/// holds; `retain` keeps the token beside it in the same transaction, so no process dying between the two
/// leaves a request nobody owns.
pub async fn enqueue(
  db: &DatabaseConnection,
  events: &Bus,
  asked: &Asked<'_>,
  source: &str,
  retain: impl AsyncFnOnce(&sea_orm::DatabaseTransaction, &Request) -> Result<(), Fault>,
) -> Result<Request, Fault> {
  if !KEY.is_match(&asked.request_key) || !KEY.is_match(&asked.owner) {
    return Err("Invalid slot request identity".to_owned());
  }

  if !SOURCE.is_match(source) {
    return Err("Invalid slot request source".to_owned());
  }

  let role = role_of(db, asked.wanted.role).await?;
  let place_key = asked.place_key.map(|key| text(key, "slot place key", 500)).transpose()?;
  let path = asked.wanted.path.map(|path| text(path, "slot path", 4096)).transpose()?;

  if let Some(token) = asked.token {
    let held = request(db, token).await?.ok_or("The slot request is gone")?;

    if held.request_key != asked.request_key || held.owner != asked.owner || held.role != role.name {
      return Err("The slot request does not match its token".to_owned());
    }

    if held.state == "released" || held.state == "cancelled" {
      return Err("The slot request is already finished".to_owned());
    }

    return Ok(held);
  }

  let fixed = asked.wanted.fixed.as_ref();

  // An agent its caller names is one of the role's, switched on (docs/role-agents.md, 「Which agent a run takes」).
  if let Some(wanted) = asked.wanted.agent
    && !role.agents.iter().any(|agent| agent.enabled && agent.name == wanted)
  {
    return Err(format!("The {} role holds no agent {wanted} that is switched on", role.name));
  }

  // A road its caller names is one an agent of the role is on, or one an agent that names no road takes
  // (docs/role-agents.md, 「Which agent a run takes」).
  if let Some(named) = fixed {
    let roads: Vec<String> = role
      .agents
      .iter()
      .filter_map(|agent| agent.entry())
      .map(|entry| format!("{} {}", entry.provider, entry.model))
      .collect();

    let follows = role.agents.iter().any(|agent| agent.enabled && agent.entry().is_none());

    if !roads.is_empty()
      && !follows
      && !role.agents.iter().any(|agent| agent.on(&named.provider, &named.model))
    {
      return Err(format!(
        "No agent of the {} role is on {} {}; its agents are on: {}",
        role.name,
        named.provider,
        named.model,
        roads.join(", ")
      ));
    }
  }

  let account = account_of(db, asked.asked_by).await?;
  let written = db.begin().await.map_err(faulted)?;

  let opened = run(
    &written,
    r#"INSERT INTO "role_slot_requests" ("token", "request_key", "owner", "source", "role", "path", "provider",
         "model", "effort", "capacity", "state", "enqueued_at", "class", "place_key", "session", "title",
         "asked_by", "agent_wanted", "account")
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'waiting', $11, $12, $13, $14, $15, $16, $17, $18)
       ON CONFLICT ("request_key") DO NOTHING"#,
    vec![
      uuid::Uuid::new_v4().to_string().into(),
      asked.request_key.clone().into(),
      asked.owner.clone().into(),
      source.into(),
      role.name.clone().into(),
      path.into(),
      fixed.map(|entry| entry.provider.clone()).unwrap_or_default().into(),
      fixed.map(|entry| entry.model.clone()).unwrap_or_default().into(),
      fixed.and_then(|entry| entry.effort.clone()).unwrap_or_default().into(),
      role.cap_for(account.as_deref()).into(),
      crate::now_ms().into(),
      (if asked.yielding { "yielding" } else { "foreground" }).into(),
      place_key.clone().into(),
      asked.session.map(str::to_owned).into(),
      asked.title.map(str::to_owned).into(),
      asked.asked_by.map(str::to_owned).into(),
      asked.wanted.agent.map(str::to_owned).into(),
      account.clone().into(),
    ],
  )
  .await
  .map_err(faulted)?;

  let held = one(
    &written,
    r#"SELECT * FROM "role_slot_requests" WHERE "request_key" = $1"#,
    vec![asked.request_key.clone().into()],
  )
  .await
  .map_err(faulted)?
  .as_ref()
  .map(request_of)
  .transpose()?
  .ok_or("The slot request was not written")?;

  if held.owner != asked.owner
    || held.source != source
    || held.role != role.name
    || held.place_key != place_key
  {
    return Err(format!("The slot request's identity changed: {}", asked.request_key));
  }

  // A key names one request for good: asked again while it stands it is answered with it, and once it was
  // given back the key is spent (docs/role-slots.md, 「The line」).
  if opened == 0 && (held.state == "released" || held.state == "cancelled") {
    return Err(format!(
      "The slot request {} was given back; a new slot is asked under a new request key",
      asked.request_key
    ));
  }

  retain(&written, &held).await?;
  written.commit().await.map_err(faulted)?;

  // A request asked again under its key stands as it did, and is not a change.
  if opened == 1 {
    announce(events, &held);
  }

  Ok(held)
}

/// The road a request goes out on if it is let in now: the road its caller fixed, or the first provider of its
/// role's order with quota above the role's share; why it waits otherwise.
pub async fn road_of(
  ground: &Ground,
  held: &Request,
  role: &AgentRole,
) -> Result<Result<Entry, String>, Fault> {
  let fixed = (!held.provider.is_empty()).then(|| Entry {
    provider: held.provider.clone(),
    model: held.model.clone(),
    effort: held.effort.clone(),
    thinking: role
      .order
      .entry_of(&held.provider, &held.model)
      .and_then(|entry| entry.thinking.clone()),
    answer_tokens: role.order.entry_of(&held.provider, &held.model).and_then(|entry| entry.answer_tokens),
    advisor: role.order.entry_of(&held.provider, &held.model).and_then(|entry| entry.advisor.clone()),
  });
  let ask = Ask {
    fixed: fixed.as_ref(),
    ..Ask::of(Row::Role(&role.name), &role.line, &role.order)
  };
  let resolution = road::resolve(ground, &ask).await?;

  Ok(match resolution.taken() {
    Some(step) => Ok(step.entry.clone()),
    None => Err(resolution.sentence()),
  })
}

/// One agent a request may be let in on: its name, and the road the request goes out on there or why that
/// road cannot answer now.
#[derive(Clone, Debug)]
pub struct Seat {
  pub agent: String,
  pub road: Result<Entry, String>,
  /// Why the road of the supervisor the agent follows was passed over for the agent's own, where it was: that
  /// supervisor and its road, and the sentence the road answered (docs/role-agents.md, 「An agent that follows its
  /// asker」).
  pub passed_over: Option<String>,
}

/// The agents a request may be let in on (docs/role-agents.md, 「Which agent a run takes」), in the order it
/// looks for a free one, each with whether its road answers now. A request that names a road takes the agents
/// on that road, or the agents that name none where no agent of the role is on it; one that names none
/// takes every agent of the role that is switched on: an agent that follows its asker on the road of the
/// supervisor that asked the request, where one did, its own road standing where none did or that road cannot
/// answer now, and any other agent on its own road.
pub async fn seats_for(ground: &Ground, held: &Request, role: &AgentRole) -> Result<Vec<Seat>, Fault> {
  // A request that names an agent is let in on that agent alone (docs/role-agents.md, 「Which agent a run
  // takes」).
  // A request is let in only on its own account's agents (docs/role-slots.md, 「Accounts」).
  let on = role.agents.iter().filter(|agent| {
    agent.enabled
      && agent.owner_account == held.account
      && held.agent_wanted.as_deref().is_none_or(|wanted| wanted == agent.name)
  });
  let mut seats: Vec<Seat> = vec![];

  if held.provider.is_empty() {
    // An agent that follows its asker takes the request on its asking supervisor's road; one that names no road
    // takes none of a request no supervisor asked (docs/role-agents.md, 「An agent that follows its asker」).
    let followed = match (on.clone().any(|agent| role.follows(agent)), held.asked_by.as_deref()) {
      (true, Some(name)) => crate::supervisors::named(&ground.db, name)
        .await
        .map_err(faulted)?
        .as_ref()
        .and_then(|supervisor| supervisor.road()),
      _ => None,
    };
    // One road is asked of its provider once, whichever agents stand on it.
    let mut judged: Vec<(String, String, Result<(), String>)> = vec![];

    for agent in on {
      // The asker's road first where the agent follows it, then the agent's own, which stands where no
      // supervisor's road does or where that road cannot answer now.
      let asker = followed.clone().filter(|_| role.follows(agent));
      let own = agent.entry().filter(|own| {
        asker
          .as_ref()
          .is_none_or(|asker| (&asker.provider, &asker.model) != (&own.provider, &own.model))
      });
      let asker_road = asker.as_ref().map(|asker| format!("{}/{}", asker.provider, asker.model));
      let roads: Vec<Entry> = asker.into_iter().chain(own).collect();

      if roads.is_empty() {
        continue;
      }

      let mut refusals: Vec<String> = vec![];
      let mut taken: Option<Entry> = None;

      for road in roads {
        match answers_now(ground, held, role, &road, &mut judged).await? {
          Ok(()) => {
            taken = Some(road);
            break;
          }
          Err(why) => refusals.push(why),
        }
      }

      // The asker's road answered first where it was tried, so a refusal before a road taken is the asker's.
      let passed_over = match (&taken, &asker_road, refusals.first()) {
        // The road's sentence runs over several lines; the log keeps it on one.
        (Some(_), Some(asker_road), Some(why)) => Some(format!(
          "it follows {}, whose road {asker_road} cannot answer now: {}",
          held.asked_by.as_deref().unwrap_or_default(),
          why.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join(" ")
        )),
        _ => None,
      };

      seats.push(Seat {
        agent: agent.name.clone(),
        road: taken.ok_or_else(|| refusals.join("; on its own road, ")),
        passed_over,
      });
    }

    return Ok(seats);
  }

  let named: Vec<&crate::role_agents::RoleAgent> =
    on.clone().filter(|agent| agent.on(&held.provider, &held.model)).collect();
  let taken: Vec<&crate::role_agents::RoleAgent> = match named.is_empty() {
    false => named,
    true => on.filter(|agent| agent.entry().is_none()).collect(),
  };

  if taken.is_empty() {
    return Ok(seats);
  }

  let road = road_of(ground, held, role).await?;

  Ok(
    taken
      .into_iter()
      .map(|agent| Seat {
        agent: agent.name.clone(),
        road: road.clone(),
        passed_over: None,
      })
      .collect(),
  )
}

// Whether a road can take the request now, asked of its provider once however many agents stand on it: `judged`
// keeps each road's answer.
async fn answers_now(
  ground: &Ground,
  held: &Request,
  role: &AgentRole,
  road: &Entry,
  judged: &mut Vec<(String, String, Result<(), String>)>,
) -> Result<Result<(), String>, Fault> {
  if let Some((_, _, answers)) = judged
    .iter()
    .find(|(provider, model, _)| *provider == road.provider && *model == road.model)
  {
    return Ok(answers.clone());
  }

  let asked = Request {
    provider: road.provider.clone(),
    model: road.model.clone(),
    effort: road.effort.clone(),
    ..held.clone()
  };
  let answers = road_of(ground, &asked, role).await?.map(|_| ());

  judged.push((road.provider.clone(), road.model.clone(), answers.clone()));

  Ok(answers)
}

/// The request let in where its turn has come, or none where it waits on: its owner asking for it, let in once
/// no request whose owner is asking stands ahead of it in its role's line, the role's cap has room and its road
/// has quota. A request whose road has none keeps its place and says why; one let in carries the provider,
/// model and effort it goes out on (docs/role-slots.md, 「The line」).
pub async fn acquire(ground: &Ground, held: &Request) -> Result<Option<Request>, Fault> {
  if held.state == "released" || held.state == "cancelled" {
    return Ok(None);
  }

  if held.state == "active" {
    return Ok(Some(held.clone()));
  }

  let role = role_of(&ground.db, &held.role).await?;
  let seats = seats_for(ground, held, &role).await?;
  let passed_over: Vec<(String, String)> = seats
    .iter()
    .filter_map(|seat| seat.passed_over.clone().map(|why| (seat.agent.clone(), why)))
    .collect();
  let written = ground.db.begin().await.map_err(faulted)?;
  let (admitted, changed) = admission(&written, &held.token, &role, seats, crate::now_ms()).await?;

  written.commit().await.map_err(faulted)?;

  for request in &changed {
    announce(&ground.events, request);
  }

  if let Some(request) = &admitted {
    announce(&ground.events, request);

    // A request let in on an agent's own road in place of the road of the supervisor it follows says both: the
    // road it went out on, and why the supervisor's was passed over.
    if let Some((agent, why)) = passed_over.iter().find(|(agent, _)| request.seat.as_deref() == Some(agent))
    {
      eprintln!("{}", fallback_line(request, agent, why));
    }
  }

  Ok(admitted)
}

/// The line a request let in on an agent's own road in place of the road of the supervisor it follows is logged
/// with: the role, the request, the agent and its own road, and why the supervisor's road was passed over.
pub fn fallback_line(request: &Request, agent: &str, why: &str) -> String {
  format!(
    "{} slot {}: let in on {agent}'s own road {}/{} in place of its asker's; {why}",
    request.role, request.token, request.provider, request.model
  )
}

// Numbers a waiting request's ask in the line's one increasing order of asks, and answers the ask its owner
// made before this one, none where this is its first.
async fn ask<C: ConnectionTrait>(db: &C, token: &str) -> Result<Option<i64>, Fault> {
  let before =
    one(db, r#"SELECT "asked" FROM "role_slot_requests" WHERE "token" = $1"#, vec![token.into()])
      .await
      .map_err(faulted)?
      .map(|row| row.try_get::<Option<i64>>("", "asked"))
      .transpose()
      .map_err(faulted)?
      .flatten();

  run(
    db,
    r#"UPDATE "role_slot_requests" SET "asked" = nextval('role_slot_asks') WHERE "token" = $1"#,
    vec![token.into()],
  )
  .await
  .map_err(faulted)?;

  Ok(before)
}

async fn admission<C: ConnectionTrait>(
  db: &C,
  token: &str,
  role: &AgentRole,
  seats: Vec<Seat>,
  now: i64,
) -> Result<(Option<Request>, Vec<Request>), Fault> {
  // One role's asks are judged one at a time: the role's row is held until this transaction ends, so two asks
  // woken at once never both read the same agent as free (docs/role-slots.md, 「The line」).
  run(
    db,
    r#"SELECT 1 FROM "roles" WHERE "name" = $1 FOR UPDATE"#,
    vec![role.name.clone().into()],
  )
  .await
  .map_err(faulted)?;

  let Some(held) = request(db, token).await? else { return Ok((None, vec![])) };

  if held.state == "active" {
    return Ok((Some(held), vec![]));
  }

  // A reserved slot is let in by its reservation, never taken as a lease by whoever holds its token.
  if held.state != "waiting" || held.reservation.is_some() {
    return Ok((None, vec![]));
  }

  let since = ask(db, token).await?;
  let judged = judge(db, &held, role, &seats, since).await?;
  // Where every agent it may take is busy it says nothing; where the free ones' roads cannot answer it says why;
  // once a free agent's road answers it says nothing more.
  let held_for = match &judged {
    Judged::NoSeat(None) => None,
    Judged::NoSeat(Some(why)) => Some(Some(why.clone())),
    Judged::HeldBack | Judged::LetIn { .. } => Some(None),
  };

  if let Some(held_for) = held_for {
    run(
      db,
      r#"UPDATE "role_slot_requests" SET "held_for" = $2 WHERE "token" = $1"#,
      vec![token.into(), held_for.into()],
    )
    .await
    .map_err(faulted)?;
  }

  let Judged::LetIn {
    agent,
    entry,
    reserved,
    inherits,
  } = judged
  else {
    return Ok((None, vec![]));
  };
  let mut changed = vec![];

  if let Some(seat) = reserved
    && let Some(ended) = end_seat(db, &seat, "released", &format!("taken by {token}"), now).await?
  {
    changed.push(ended);
  }

  if let Some(first) = inherits
    && let Some(ended) = end_seat(db, &first, "cancelled", &format!("turn given to {token}"), now).await?
  {
    changed.push(ended);
  }

  let admitted = admit(db, token, &entry, &agent, role.cap_for(held.account.as_deref()), now).await?;

  Ok((admitted, changed))
}

// What the line makes of a waiting request's ask now (docs/role-slots.md, 「The line」), reading and writing
// nothing. `since` is the ask its owner made before this one, none where this is its first.
enum Judged {
  // No free agent it may take has a road that answers now: it keeps its place and stands in no one's way, with
  // why where a free agent's road cannot answer.
  NoSeat(Option<String>),
  // A free agent's road answers, but its place is taken, a request ahead stands in its way or the role's cap
  // is full.
  HeldBack,
  // Let in on the agent, on the road entry: in place of the slot its owner holds reserved in the role where one
  // stands, else of the first request ahead that is its own owner's reservation where that one stood in its way.
  LetIn {
    agent: String,
    entry: Entry,
    reserved: Option<String>,
    inherits: Option<String>,
  },
}

async fn judge<C: ConnectionTrait>(
  db: &C,
  held: &Request,
  role: &AgentRole,
  seats: &[Seat],
  since: Option<i64>,
) -> Result<Judged, Fault> {
  // The first agent it may take that is free and whose road answers now is its seat.
  let busy = crate::role_agents::busy(db, &held.role).await.map_err(faulted)?;
  let free: Vec<&Seat> = seats.iter().filter(|seat| !busy.contains(&seat.agent)).collect();
  let picked = free
    .iter()
    .find_map(|seat| seat.road.as_ref().ok().map(|entry| (seat.agent.clone(), entry.clone())));
  let Some((agent, entry)) = picked else {
    return Ok(Judged::NoSeat(free.first().and_then(|seat| seat.road.as_ref().err()).cloned()));
  };

  if place_taken(db, held.place_key.as_deref()).await? {
    return Ok(Judged::HeldBack);
  }

  // A slot its own owner holds reserved in this role is the request's place: the reservation's slot is given
  // up and the request let in in its stead, which moves nothing in the count.
  let reserved = one(
    db,
    r#"SELECT "token" FROM "role_slot_requests"
       WHERE "role" = $1 AND "owner" = $2 AND "state" = 'reserved' AND "reservation" IS NOT NULL
       ORDER BY "sequence" LIMIT 1"#,
    vec![held.role.clone().into(), held.owner.clone().into()],
  )
  .await
  .map_err(faulted)?;

  if let Some(reserved) = reserved {
    return Ok(Judged::LetIn {
      agent,
      entry,
      reserved: Some(reserved.try_get("", "token").map_err(faulted)?),
      inherits: None,
    });
  }

  if held.yielding && foreground_waits(db, &held.role, since).await? {
    return Ok(Judged::HeldBack);
  }

  // A request ahead stands in this one's way only where it could take the agent this one is let in on.
  let seat_of = role.agent(&agent);
  let takes = |provider: &str, model: &str, asked_by: Option<&str>, wanted: Option<&str>| {
    let Some(seat) = seat_of else { return true };

    // One that names an agent could take this agent only where it is the one it names.
    if let Some(wanted) = wanted {
      return wanted == seat.name;
    }

    match provider.is_empty() {
      true => seat.entry().is_some() || asked_by.is_some(),
      false => {
        seat.on(provider, model)
          || (seat.entry().is_none()
            && !role.agents.iter().any(|other| other.enabled && other.on(provider, model)))
      }
    }
  };
  let inherits =
    match in_the_way(db, &held.role, held.account.as_deref(), held.sequence, since, Some(&takes)).await? {
      None => None,
      Some((first, Some(_), owner)) if owner == held.owner => Some(first),
      Some(_) => return Ok(Judged::HeldBack),
    };

  if used(db, &held.role, held.account.as_deref()).await? >= role.cap_for(held.account.as_deref()) {
    return Ok(Judged::HeldBack);
  }

  Ok(Judged::LetIn {
    agent,
    entry,
    reserved: None,
    inherits,
  })
}

/// Whether a request asked so would wait at its first ask, as the line judges it now (docs/role-slots.md, 「The
/// line」): by the rule an ask is admitted by, for a request standing behind every one in the line that has not
/// asked before, with nothing written.
pub async fn first_ask_waits(ground: &Ground, asked: &Asked<'_>, source: &str) -> Result<bool, Fault> {
  let role = role_of(&ground.db, asked.wanted.role).await?;
  let account = account_of(&ground.db, asked.asked_by).await?;
  let unasked = unasked(asked, source, &role, account);
  let seats = seats_for(ground, &unasked, &role).await?;

  Ok(!matches!(judge(&ground.db, &unasked, &role, &seats, None).await?, Judged::LetIn { .. }))
}

// A request asked so as the line would hold it the moment it is written: waiting, behind every request in the
// line, with no token yet.
fn unasked(asked: &Asked<'_>, source: &str, role: &AgentRole, account: Option<String>) -> Request {
  let fixed = asked.wanted.fixed.as_ref();

  Request {
    sequence: i64::MAX,
    token: String::new(),
    request_key: asked.request_key.clone(),
    owner: asked.owner.clone(),
    source: source.to_owned(),
    role: role.name.clone(),
    path: asked.wanted.path.map(str::to_owned),
    provider: fixed.map(|entry| entry.provider.clone()).unwrap_or_default(),
    model: fixed.map(|entry| entry.model.clone()).unwrap_or_default(),
    effort: fixed.and_then(|entry| entry.effort.clone()),
    capacity: role.cap_for(account.as_deref()),
    state: "waiting".to_owned(),
    enqueued_at: crate::now_ms(),
    leased_at: None,
    child_id: None,
    ended_at: None,
    outcome: None,
    reservation: None,
    yielding: asked.yielding,
    place_key: asked.place_key.map(str::to_owned),
    session: asked.session.map(str::to_owned),
    title: asked.title.map(str::to_owned),
    asked_by: asked.asked_by.map(str::to_owned),
    account,
    agent_name: None,
    agent_wanted: asked.wanted.agent.map(str::to_owned),
    seat: None,
    held_for: None,
    ended_by: None,
  }
}

// The account a request belongs to: its asking supervisor's (docs/role-slots.md, 「Accounts」), none where no
// supervisor asked it or the one that did holds no account.
async fn account_of<C: ConnectionTrait>(db: &C, asked_by: Option<&str>) -> Result<Option<String>, Fault> {
  let Some(name) = asked_by else { return Ok(None) };

  Ok(
    crate::supervisors::named(db, name)
      .await
      .map_err(faulted)?
      .and_then(|supervisor| supervisor.owner_account)
      .map(|account| account.to_string()),
  )
}

// Whether an active request already stands in this place.
async fn place_taken<C: ConnectionTrait>(db: &C, place_key: Option<&str>) -> Result<bool, Fault> {
  let Some(place_key) = place_key else { return Ok(false) };

  Ok(
    one(
      db,
      r#"SELECT 1 AS "taken" FROM "role_slot_requests" WHERE "place_key" = $1 AND "state" = 'active' LIMIT 1"#,
      vec![place_key.into()],
    )
    .await
    .map_err(faulted)?
    .is_some(),
  )
}

// Whether a prior request of the role waits whose owner is asking, which a yielding request gives way to.
async fn foreground_waits<C: ConnectionTrait>(
  db: &C,
  role: &str,
  since: Option<i64>,
) -> Result<bool, Fault> {
  Ok(
    one(
      db,
      r#"SELECT 1 AS "waits" FROM "role_slot_requests"
         WHERE "role" = $1 AND "state" = 'waiting' AND "class" = 'foreground' AND "held_for" IS NULL
           AND ("reservation" IS NOT NULL OR $2::bigint IS NULL OR "asked" > $2::bigint) LIMIT 1"#,
      vec![role.into(), since.into()],
    )
    .await
    .map_err(faulted)?
    .is_some(),
  )
}

// Whether a waiting request could take the agent an asking request is let in on, judged by the provider and
// the model it names, each empty where it names none, and the supervisor that asked it.
type Takes<'a> = &'a (dyn Fn(&str, &str, Option<&str>, Option<&str>) -> bool + Sync);

// The first request of a role's line standing in the way of one asked for at `sequence`: one ahead of it that
// is a reservation's slot, or whose owner has asked since that request's own last ask `since` — every one
// ahead where it has not asked before. A request whose owner has not asked since, whose place key is taken
// or whose road has no quota is passed over and keeps its place; so is one that could not take the agent the
// asking request is let in on, as `takes` judges it by the road it names and the supervisor that asked it —
// a reservation's slot holds a seat by count, so it stands in the way whatever the agent.
pub(crate) async fn in_the_way<C: ConnectionTrait>(
  db: &C,
  role: &str,
  account: Option<&str>,
  sequence: i64,
  since: Option<i64>,
  takes: Option<Takes<'_>>,
) -> Result<Option<(String, Option<String>, String)>, Fault> {
  let rows = all(
    db,
    r#"SELECT "token", "reservation", "owner", "provider", "model", "asked_by", "agent_wanted"
       FROM "role_slot_requests" AS "waiting"
       WHERE "role" = $1 AND "state" = 'waiting' AND "class" = 'foreground' AND "held_for" IS NULL
         AND "sequence" < $2
         AND ("place_key" IS NULL OR NOT EXISTS (SELECT 1 FROM "role_slot_requests" AS "held"
           WHERE "held"."place_key" = "waiting"."place_key" AND "held"."state" = 'active'))
         AND ("reservation" IS NOT NULL OR $3::bigint IS NULL OR "asked" > $3::bigint)
         AND "account" IS NOT DISTINCT FROM $4
       ORDER BY "sequence""#,
    vec![role.into(), sequence.into(), since.into(), account.map(str::to_owned).into()],
  )
  .await
  .map_err(faulted)?;

  for row in rows {
    let reservation: Option<String> = row.try_get("", "reservation").map_err(faulted)?;
    let provider: Option<String> = row.try_get("", "provider").map_err(faulted)?;
    let model: Option<String> = row.try_get("", "model").map_err(faulted)?;
    let asked_by: Option<String> = row.try_get("", "asked_by").map_err(faulted)?;
    let wanted: Option<String> = row
      .try_get::<Option<String>>("", "agent_wanted")
      .map_err(faulted)?
      .filter(|name| !name.is_empty());
    let could = takes.is_none_or(|takes| {
      takes(
        provider.as_deref().unwrap_or_default(),
        model.as_deref().unwrap_or_default(),
        asked_by.as_deref(),
        wanted.as_deref(),
      )
    });

    if reservation.is_some() || could {
      return Ok(Some((
        row.try_get("", "token").map_err(faulted)?,
        reservation,
        row.try_get("", "owner").map_err(faulted)?,
      )));
    }
  }

  Ok(None)
}

/// How many slots of a role are taken: its runs let in at work and its reserved slots together, a job standing in
/// the machine's turn counted against no cap (docs/role-slots.md, 「The cap」).
pub async fn used<C: ConnectionTrait>(db: &C, role: &str, account: Option<&str>) -> Result<i64, Fault> {
  let row = one(
    db,
    &format!(
      r#"SELECT COUNT(*) AS "count" FROM "role_slot_requests"
         WHERE "role" = $1 AND "state" IN ('active', 'reserved') AND "account" IS NOT DISTINCT FROM $2
           AND {}"#,
      crate::watches::machine_turn::AT_WORK
    ),
    vec![role.into(), account.map(str::to_owned).into()],
  )
  .await
  .map_err(faulted)?;

  Ok(
    row
      .map(|row| row.try_get::<i64>("", "count"))
      .transpose()
      .map_err(faulted)?
      .unwrap_or(0),
  )
}

async fn admit<C: ConnectionTrait>(
  db: &C,
  token: &str,
  entry: &Entry,
  agent: &str,
  capacity: i64,
  now: i64,
) -> Result<Option<Request>, Fault> {
  let changed = run(
    db,
    r#"UPDATE "role_slot_requests" SET "state" = 'active', "leased_at" = $1, "provider" = $3, "model" = $4,
         "effort" = $5, "capacity" = $6, "held_for" = NULL, "seat" = $7
       WHERE "token" = $2 AND "state" = 'waiting'"#,
    vec![
      now.into(),
      token.into(),
      entry.provider.clone().into(),
      entry.model.clone().into(),
      entry.effort.clone().unwrap_or_default().into(),
      capacity.into(),
      agent.into(),
    ],
  )
  .await
  .map_err(faulted)?;

  if changed != 1 {
    return Ok(None);
  }

  request(db, token).await
}

async fn end_seat<C: ConnectionTrait>(
  db: &C,
  token: &str,
  state: &str,
  outcome: &str,
  now: i64,
) -> Result<Option<Request>, Fault> {
  let changed = run(
    db,
    r#"UPDATE "role_slot_requests" SET "state" = $1, "ended_at" = $2, "outcome" = $3, "ended_by" = $5
       WHERE "token" = $4 AND "state" IN ('waiting', 'reserved')"#,
    vec![
      state.into(),
      now.into(),
      text(outcome, "slot outcome", 500)?.into(),
      token.into(),
      // A seat the line ends is ended by its holder's own move: its reservation lowered, released or taken.
      BY_HOLDER.into(),
    ],
  )
  .await
  .map_err(faulted)?;

  if changed != 1 {
    return Ok(None);
  }

  request(db, token).await
}

/// The run a launch carries a slot's lease into, written once: a lease no longer active, or bound to another
/// run, refuses it. The run's name is kept where the caller names one.
pub async fn bind(
  db: &DatabaseConnection,
  events: &Bus,
  held: &Request,
  child_id: &str,
  agent_name: Option<&str>,
) -> Result<Request, Fault> {
  let clean = text(child_id, "slot child identity", 500)?;
  let written = db.begin().await.map_err(faulted)?;
  let now = request(&written, &held.token).await?.ok_or("Unknown slot lease")?;

  if now.state != "active" {
    return Err("Only an active slot lease may be bound".to_owned());
  }

  if now.child_id.as_ref().is_some_and(|bound| *bound != clean) {
    // A lease bound to the role session a start laid on it stays bound to it; the child a later binding names
    // is kept as the run it is.
    if now.session.is_some() && now.session == now.child_id {
      let naming = match agent_name {
        Some(_) => r#"UPDATE "role_slot_requests" SET "agent" = $1 WHERE "token" = $2"#,
        None => r#"UPDATE "role_slot_requests" SET "agent" = COALESCE("agent", $1) WHERE "token" = $2"#,
      };

      run(
        &written,
        naming,
        vec![
          agent_name.map(str::to_owned).unwrap_or_else(|| clean.clone()).into(),
          held.token.clone().into(),
        ],
      )
      .await
      .map_err(faulted)?;

      let bound = request(&written, &held.token).await?.ok_or("Unknown slot lease")?;

      written.commit().await.map_err(faulted)?;
      announce(events, &bound);

      return Ok(bound);
    }

    return Err("A slot lease's child cannot change".to_owned());
  }

  run(
    &written,
    r#"UPDATE "role_slot_requests" SET "child_id" = COALESCE("child_id", $1), "agent" = COALESCE($3, "agent")
       WHERE "token" = $2 AND "state" = 'active'"#,
    vec![
      clean.into(),
      held.token.clone().into(),
      agent_name.map(str::to_owned).into(),
    ],
  )
  .await
  .map_err(faulted)?;

  let bound = request(&written, &held.token).await?.ok_or("Unknown slot lease")?;

  written.commit().await.map_err(faulted)?;
  announce(events, &bound);

  Ok(bound)
}

/// A launch held to the lease it was let in on: the same owner and role, bound to the run it starts.
pub async fn require(
  db: &DatabaseConnection,
  events: &Bus,
  token: Option<&str>,
  owner: &str,
  role: &str,
  child_id: &str,
) -> Result<Request, Fault> {
  let token = token.ok_or("The launch holds no slot")?;
  let held = request(db, token).await?.filter(|held| held.state == "active");
  let held = held.ok_or("The launch holds no active slot")?;

  if held.owner != owner || held.role != role {
    return Err("The launch does not match its slot".to_owned());
  }

  bind(db, events, &held, child_id, None).await
}

/// One lease ended with what it came to — released, or cancelled where it never ran — and the reserved slots
/// of its role let in where room has come; whether it was still held.
pub async fn finish(
  db: &DatabaseConnection,
  events: &Bus,
  token: &str,
  outcome: &str,
  cancel: bool,
) -> Result<bool, Fault> {
  finish_by(db, events, token, outcome, cancel, BY_HOLDER).await
}

/// One lease ended as `finish` ends it, kept with who gave it back.
pub async fn finish_by(
  db: &DatabaseConnection,
  events: &Bus,
  token: &str,
  outcome: &str,
  cancel: bool,
  by: &str,
) -> Result<bool, Fault> {
  let clean = text(outcome, "slot outcome", 500)?;
  let now = crate::now_ms();
  let written = db.begin().await.map_err(faulted)?;
  let changed = run(
    &written,
    r#"UPDATE "role_slot_requests" SET "state" = $1, "ended_at" = $2, "outcome" = $3, "ended_by" = $5
       WHERE "token" = $4 AND "state" IN ('waiting', 'active', 'reserved')"#,
    vec![
      (if cancel { "cancelled" } else { "released" }).into(),
      now.into(),
      clean.into(),
      token.into(),
      by.into(),
    ],
  )
  .await
  .map_err(faulted)?;

  if changed != 1 {
    written.rollback().await.map_err(faulted)?;

    return Ok(false);
  }

  let ended = request(&written, token).await?;
  let promoted = match &ended {
    Some(held) => promote_role(&written, &held.role, now).await?,
    None => vec![],
  };

  written.commit().await.map_err(faulted)?;

  if let Some(ended) = &ended {
    announce(events, ended);
  }

  for request in &promoted {
    announce(events, request);
  }

  Ok(true)
}

/// Waiting slots a stopped order waited on given back, each cancelled with why.
pub async fn cancel_waiting(
  db: &DatabaseConnection,
  events: &Bus,
  requests: &[String],
  why: &str,
) -> Result<(), Fault> {
  for request in requests {
    finish(db, events, request, why, true).await?;
  }

  Ok(())
}

/// One role's reserved slots let in, in the line's order, while none stands in the way of the next and the
/// role's cap has room. The pass is an ask of every such slot waiting in the role, its standing reservation
/// being its owner asking.
pub async fn promote_role<C: ConnectionTrait>(db: &C, role: &str, now: i64) -> Result<Vec<Request>, Fault> {
  let Some(held_role) = agent_roles::named(db, role).await.map_err(faulted)? else {
    return Ok(vec![]);
  };
  let seats = all(
    db,
    r#"SELECT "token", "sequence", "asked", "account" FROM "role_slot_requests" AS "waiting"
       WHERE "role" = $1 AND "state" = 'waiting' AND "class" = 'foreground' AND "reservation" IS NOT NULL
         AND ("place_key" IS NULL OR NOT EXISTS (SELECT 1 FROM "role_slot_requests" AS "held"
           WHERE "held"."place_key" = "waiting"."place_key" AND "held"."state" = 'active'))
       ORDER BY "sequence""#,
    vec![role.into()],
  )
  .await
  .map_err(faulted)?;

  run(
    db,
    r#"UPDATE "role_slot_requests" SET "asked" = nextval('role_slot_asks')
       WHERE "role" = $1 AND "state" = 'waiting' AND "class" = 'foreground' AND "reservation" IS NOT NULL"#,
    vec![role.into()],
  )
  .await
  .map_err(faulted)?;

  let mut promoted = vec![];

  for seat in seats {
    let token: String = seat.try_get("", "token").map_err(faulted)?;
    let since: Option<i64> = seat.try_get("", "asked").map_err(faulted)?;
    let sequence: i64 = seat.try_get("", "sequence").map_err(faulted)?;
    let account: Option<String> = seat.try_get("", "account").map_err(faulted)?;
    let cap = held_role.cap_for(account.as_deref());

    // Each account's line and cap stand apart: one account's full cap holds back none of another's.
    if in_the_way(db, role, account.as_deref(), sequence, since, None).await?.is_some()
      || used(db, role, account.as_deref()).await? >= cap
    {
      continue;
    }

    let changed = run(
      db,
      r#"UPDATE "role_slot_requests" SET "state" = 'reserved', "leased_at" = $1, "capacity" = $3
         WHERE "token" = $2 AND "state" = 'waiting'"#,
      vec![now.into(), token.clone().into(), cap.into()],
    )
    .await
    .map_err(faulted)?;

    if changed == 1
      && let Some(request) = request(db, &token).await?
    {
      promoted.push(request);
    }
  }

  Ok(promoted)
}

/// The line's own pass (docs/role-slots.md, 「Reserving slots」): every role's reserved slots let in where its
/// cap has room.
pub async fn promote_reservations<C: ConnectionTrait>(db: &C, now: i64) -> Result<Vec<Request>, Fault> {
  let roles = all(
    db,
    r#"SELECT DISTINCT "role" FROM "role_slot_requests" WHERE "state" = 'waiting' AND "reservation" IS NOT NULL"#,
    vec![],
  )
  .await
  .map_err(faulted)?;
  let mut promoted = vec![];

  for row in roles {
    let role: String = row.try_get("", "role").map_err(faulted)?;

    promoted.extend(promote_role(db, &role, now).await?);
  }

  Ok(promoted)
}

/// What an owner's slots are held as: under a key the owner names again, by what brought them, for a role, so
/// many of them.
pub struct Seats<'a> {
  pub reservation_key: String,
  pub owner: String,
  pub source: &'a str,
  pub role: &'a str,
  pub count: i64,
}

/// Slots of a role held for an owner. Asked again with the same key, only its live slots move to the new
/// count — more join the line behind whoever stands in it, fewer give up the youngest first.
pub async fn reserve(
  db: &DatabaseConnection,
  events: &Bus,
  seats: &Seats<'_>,
) -> Result<Reservation, Fault> {
  if !KEY.is_match(&seats.reservation_key) || !KEY.is_match(&seats.owner) {
    return Err("Invalid slot reservation identity".to_owned());
  }

  if !(0..=MOST_SEATS).contains(&seats.count) {
    return Err(format!("A reservation holds a whole number of slots from 0 to {MOST_SEATS}"));
  }

  let role = role_of(db, seats.role).await?;
  let count = seats.count;
  let now = crate::now_ms();
  let written = db.begin().await.map_err(faulted)?;

  run(
    &written,
    r#"INSERT INTO "role_slot_reservations" ("token", "reservation_key", "owner", "source", "role", "provider",
         "model", "effort", "wanted", "state", "created_at")
       VALUES ($1, $2, $3, $4, $5, '', '', '', $6, 'held', $7) ON CONFLICT ("reservation_key") DO NOTHING"#,
    vec![
      uuid::Uuid::new_v4().to_string().into(),
      seats.reservation_key.clone().into(),
      seats.owner.clone().into(),
      seats.source.into(),
      role.name.clone().into(),
      count.into(),
      now.into(),
    ],
  )
  .await
  .map_err(faulted)?;

  let held = reservation_by_key(&written, &seats.reservation_key).await?;

  if held.owner != seats.owner || held.source != seats.source || held.role != role.name {
    return Err(format!("The slot reservation's identity changed: {}", seats.reservation_key));
  }

  run(
    &written,
    r#"UPDATE "role_slot_reservations" SET "state" = 'held', "wanted" = $1, "ended_at" = NULL, "outcome" = NULL,
         "created_at" = CASE WHEN "state" = 'held' THEN "created_at" ELSE $3 END
       WHERE "token" = $2"#,
    vec![count.into(), held.token.clone().into(), now.into()],
  )
  .await
  .map_err(faulted)?;

  let live = live_seats(&written, &held.token).await?;
  let made: i64 = one(
    &written,
    r#"SELECT COUNT(*) AS "count" FROM "role_slot_requests" WHERE "reservation" = $1"#,
    vec![held.token.clone().into()],
  )
  .await
  .map_err(faulted)?
  .map(|row| row.try_get::<i64>("", "count"))
  .transpose()
  .map_err(faulted)?
  .unwrap_or(0);
  let live_count = i64::try_from(live.len()).unwrap_or(i64::MAX);
  let mut made_keys = vec![];

  for seat in (made + 1)..=(made + count - live_count) {
    let key = format!("{}:seat-{seat}", held.reservation_key);

    run(
      &written,
      r#"INSERT INTO "role_slot_requests" ("token", "request_key", "owner", "source", "role", "provider", "model",
           "effort", "capacity", "state", "enqueued_at", "reservation")
         VALUES ($1, $2, $3, $4, $5, '', '', '', $6, 'waiting', $7, $8)"#,
      vec![
        uuid::Uuid::new_v4().to_string().into(),
        key.clone().into(),
        held.owner.clone().into(),
        held.source.clone().into(),
        held.role.clone().into(),
        role.max_concurrent.into(),
        now.into(),
        held.token.clone().into(),
      ],
    )
    .await
    .map_err(faulted)?;
    made_keys.push(key);
  }

  let mut ended = vec![];

  for seat in live.iter().skip(usize::try_from(count).unwrap_or(0)) {
    let state = if seat.state == "reserved" { "released" } else { "cancelled" };

    if let Some(request) = end_seat(&written, &seat.token, state, "reservation lowered", now).await? {
      ended.push(request);
    }
  }

  let promoted = promote_role(&written, &held.role, now).await?;
  let reserved = reservation_by_token(&written, &held.token).await?;

  written.commit().await.map_err(faulted)?;

  let promoted_tokens: std::collections::HashSet<&str> =
    promoted.iter().map(|request| request.token.as_str()).collect();

  for key in made_keys {
    if let Some(row) =
      one(db, r#"SELECT * FROM "role_slot_requests" WHERE "request_key" = $1"#, vec![key.into()])
        .await
        .map_err(faulted)?
    {
      let request = request_of(&row)?;

      if !promoted_tokens.contains(request.token.as_str()) {
        announce(events, &request);
      }
    }
  }

  for request in promoted.iter().chain(ended.iter()) {
    announce(events, request);
  }

  Ok(reserved)
}

async fn reservation_by_key<C: ConnectionTrait>(db: &C, key: &str) -> Result<Reservation, Fault> {
  one(
    db,
    r#"SELECT * FROM "role_slot_reservations" WHERE "reservation_key" = $1"#,
    vec![key.into()],
  )
  .await
  .map_err(faulted)?
  .as_ref()
  .map(reservation_of)
  .transpose()?
  .ok_or_else(|| "The slot reservation was not written".to_owned())
}

async fn reservation_by_token<C: ConnectionTrait>(db: &C, token: &str) -> Result<Reservation, Fault> {
  one(db, r#"SELECT * FROM "role_slot_reservations" WHERE "token" = $1"#, vec![token.into()])
    .await
    .map_err(faulted)?
    .as_ref()
    .map(reservation_of)
    .transpose()?
    .ok_or_else(|| "The slot reservation is gone".to_owned())
}

async fn live_seats<C: ConnectionTrait>(db: &C, token: &str) -> Result<Vec<Request>, Fault> {
  all(
    db,
    r#"SELECT * FROM "role_slot_requests" WHERE "reservation" = $1 AND "state" IN ('waiting', 'reserved')
       ORDER BY "sequence""#,
    vec![token.into()],
  )
  .await
  .map_err(faulted)?
  .iter()
  .map(request_of)
  .collect()
}

/// The live slots of one reservation, oldest first.
pub async fn seats(db: &DatabaseConnection, token: &str) -> Result<Vec<Request>, Fault> {
  live_seats(db, token).await
}

/// A reservation given back: its waiting slots leave the line and its reserved ones free; whether it was
/// still held.
pub async fn release_reservation(
  db: &DatabaseConnection,
  events: &Bus,
  token: &str,
  outcome: &str,
) -> Result<bool, Fault> {
  release_reservation_by(db, events, token, outcome, BY_HOLDER).await
}

/// A reservation given back as `release_reservation` gives it back, its slots kept with who gave them back.
pub async fn release_reservation_by(
  db: &DatabaseConnection,
  events: &Bus,
  token: &str,
  outcome: &str,
  by: &str,
) -> Result<bool, Fault> {
  let clean = text(outcome, "slot outcome", 500)?;
  let now = crate::now_ms();
  let written = db.begin().await.map_err(faulted)?;
  let changed = run(
    &written,
    r#"UPDATE "role_slot_reservations" SET "state" = 'released', "ended_at" = $1, "outcome" = $2
       WHERE "token" = $3 AND "state" = 'held'"#,
    vec![now.into(), clean.clone().into(), token.into()],
  )
  .await
  .map_err(faulted)?;

  if changed != 1 {
    written.rollback().await.map_err(faulted)?;

    return Ok(false);
  }

  let held = reservation_by_token(&written, token).await?;
  let mut ended = vec![];

  for seat in live_seats(&written, token).await? {
    let state = if seat.state == "reserved" { "released" } else { "cancelled" };

    if let Some(request) = end_seat(&written, &seat.token, state, &clean, now).await? {
      ended.push(request);
    }

    run(
      &written,
      r#"UPDATE "role_slot_requests" SET "ended_by" = $1 WHERE "token" = $2"#,
      vec![by.into(), seat.token.clone().into()],
    )
    .await
    .map_err(faulted)?;
  }

  let promoted = promote_role(&written, &held.role, now).await?;

  written.commit().await.map_err(faulted)?;

  for request in ended.iter().chain(promoted.iter()) {
    announce(events, request);
  }

  Ok(true)
}

/// The reservations an owner holds, or every one held where none is named, oldest first.
pub async fn held_reservations(
  db: &DatabaseConnection,
  owner: Option<&str>,
) -> Result<Vec<Reservation>, Fault> {
  let rows = match owner {
    Some(owner) => {
      all(
        db,
        r#"SELECT * FROM "role_slot_reservations" WHERE "owner" = $1 AND "state" = 'held' ORDER BY "created_at", "token""#,
        vec![owner.into()],
      )
      .await
    }
    None => {
      all(
        db,
        r#"SELECT * FROM "role_slot_reservations" WHERE "state" = 'held' ORDER BY "created_at", "token""#,
        vec![],
      )
      .await
    }
  };

  rows.map_err(faulted)?.iter().map(reservation_of).collect()
}

/// Every active lease bound to a launched unit, pane or execution, as its token, its child and whether the line
/// has seen that child standing; the leases bound to a role session, which end with their session, left out.
pub async fn bound_leases(db: &DatabaseConnection) -> Result<Vec<(String, String, bool)>, Fault> {
  let rows = all(
    db,
    r#"SELECT "r"."token", "r"."child_id", "r"."child_seen" FROM "role_slot_requests" "r"
       WHERE "r"."state" = 'active' AND "r"."child_id" IS NOT NULL
         AND "r"."child_id" IS DISTINCT FROM "r"."session"
       ORDER BY "r"."sequence""#,
    vec![],
  )
  .await
  .map_err(faulted)?;

  rows
    .iter()
    .map(|row| {
      Ok((
        row.try_get("", "token").map_err(faulted)?,
        row.try_get("", "child_id").map_err(faulted)?,
        row.try_get::<i64>("", "child_seen").map_err(faulted)? != 0,
      ))
    })
    .collect()
}

/// The bound leases read against what stands: a child standing is marked seen, and a child seen standing
/// before and standing no longer has its lease given back; a child never seen standing is left alone,
/// however long it takes to come up. How many were given back.
pub async fn release_ended(
  db: &DatabaseConnection,
  events: &Bus,
  leases: &[(String, String, bool)],
  standing: &std::collections::HashSet<String>,
) -> Result<i64, Fault> {
  let mut released = 0;

  for (token, child, seen) in leases {
    if standing.contains(child) {
      if !seen {
        run(
          db,
          r#"UPDATE "role_slot_requests" SET "child_seen" = 1 WHERE "token" = $1"#,
          vec![token.clone().into()],
        )
        .await
        .map_err(faulted)?;
      }

      continue;
    }

    if *seen
      && finish_by(db, events, token, "its launched child no longer stands", false, BY_RUN_ENDED).await?
    {
      released += 1;
    }
  }

  Ok(released)
}

/// Every bound lease whose child was seen standing and has ended released, asked of what runs on this
/// machine; nothing where that could not be asked, since "nothing stands" and "nobody could say" are kept
/// apart.
pub async fn reclaim_bound(
  db: &DatabaseConnection,
  events: &Bus,
  running: &crate::queue::running::Running,
) -> Result<i64, Fault> {
  let leases = bound_leases(db).await?;

  if leases.is_empty() {
    return Ok(0);
  }

  let children: Vec<String> = leases.iter().map(|(_, child, _)| child.clone()).collect();
  let Some(mut standing) = running.answered(&children).await.map_err(|why| why.detail)? else {
    return Ok(0);
  };

  standing.extend(kept_among(db, &children).await);

  release_ended(db, events, &leases, &standing).await
}

// The children that are a turn kept in flight under their name: a turn this process carries, or one the
// process before left open that this one adopted, stands while it is kept.
async fn kept_among(db: &DatabaseConnection, children: &[String]) -> Vec<String> {
  let mut kept = vec![];

  for child in children {
    if crate::turn_pickup::kept(db, child).await {
      kept.push(child.clone());
    }
  }

  kept
}

/// The leases bound to a run, by its name, marked seen standing: a turn the process before left open and this
/// one adopts stands as seen the moment it is adopted, so its end, or its program found gone, gives its lease
/// back (docs/role-slots.md, 「The line」). How many were marked.
pub async fn seen_standing(db: &DatabaseConnection, child: &str) -> Result<u64, Fault> {
  run(
    db,
    r#"UPDATE "role_slot_requests" SET "child_seen" = 1
       WHERE "state" = 'active' AND "child_id" = $1 AND "child_id" IS DISTINCT FROM "session""#,
    vec![child.into()],
  )
  .await
  .map_err(faulted)
}

/// An active lease bound to the role session a start laid on it opened, whether or not it was bound before, the
/// session kept beside it, so the lease ends with that session and no longer with the one it was bound to
/// (docs/role-slots.md, 「The line」). The run name a binding named stays only while the lease stays on the same
/// session; a lease moved onto another forgets it, and its child is read afresh.
pub async fn bind_session(
  db: &DatabaseConnection,
  events: &Bus,
  token: &str,
  session: &str,
) -> Result<(), Fault> {
  let changed = run(
    db,
    r#"UPDATE "role_slot_requests"
       SET "agent" = CASE WHEN "child_id" = $1 AND "session" = $1 THEN "agent" END,
           "child_seen" = CASE WHEN "child_id" = $1 AND "session" = $1 THEN "child_seen" ELSE 0 END,
           "child_id" = $1, "session" = $1
       WHERE "token" = $2 AND "state" = 'active'"#,
    vec![session.into(), token.into()],
  )
  .await
  .map_err(faulted)?;

  if changed == 1
    && let Some(request) = request(db, token).await?
  {
    announce(events, &request);
  }

  Ok(())
}

/// A bound lease marked as bound to the role session its child names, so it ends with that session.
pub async fn bound_to_session(db: &DatabaseConnection, token: &str) -> Result<(), Fault> {
  run(
    db,
    r#"UPDATE "role_slot_requests" SET "session" = "child_id" WHERE "token" = $1 AND "child_id" IS NOT NULL"#,
    vec![token.into()],
  )
  .await
  .map_err(faulted)?;

  Ok(())
}

/// Every active lease bound to one of these role sessions given back, as ended with its session; how many.
pub async fn release_sessions(
  db: &DatabaseConnection,
  events: &Bus,
  sessions: &[String],
  why: &str,
) -> Result<i64, Fault> {
  let mut released = 0;

  for session in sessions {
    let rows = all(
      db,
      r#"SELECT "token" FROM "role_slot_requests"
         WHERE "state" = 'active' AND "session" = $1 AND "child_id" = $1"#,
      vec![session.clone().into()],
    )
    .await
    .map_err(faulted)?;

    for row in rows {
      let token: String = row.try_get("", "token").map_err(faulted)?;

      if finish_by(db, events, &token, why, false, BY_RUN_ENDED).await? {
        released += 1;
      }
    }
  }

  Ok(released)
}

/// Every active lease bound to a role session given back: what a start of the process finds, since a role
/// session is held in memory from its start to its end and no session of the process before stands.
pub async fn release_every_session(db: &DatabaseConnection, events: &Bus) -> Result<i64, Fault> {
  let rows = all(
    db,
    r#"SELECT DISTINCT "session" FROM "role_slot_requests"
       WHERE "state" = 'active' AND "session" IS NOT NULL AND "child_id" = "session""#,
    vec![],
  )
  .await
  .map_err(faulted)?;
  let sessions: Vec<String> = rows
    .iter()
    .map(|row| row.try_get("", "session"))
    .collect::<Result<_, _>>()
    .map_err(faulted)?;

  release_sessions(db, events, &sessions, "its role session ended with the agency process that held it")
    .await
}

/// What an owner's release gave back (docs/role-slots.md, 「An owner's release」).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerReleased {
  pub cancelled: i64,
  pub released: i64,
  pub reservations: i64,
  /// The role sessions its leases were bound to, which the caller ends.
  pub sessions: Vec<String>,
}

/// Everything an owner holds given back as its owner's: each waiting or reserved request cancelled, each
/// active one released, each reservation released; the role sessions its active leases were bound to named
/// for the caller to end.
pub async fn release_owner(
  db: &DatabaseConnection,
  events: &Bus,
  owner: &str,
) -> Result<OwnerReleased, Fault> {
  release_owner_for(db, events, owner, "its owner gave back everything it held").await
}

/// Everything an owner holds given back as `release_owner` gives it back, each request and reservation kept
/// with the reason its owner gave, such as the end of the job whose run the owner is.
pub async fn release_owner_for(
  db: &DatabaseConnection,
  events: &Bus,
  owner: &str,
  why: &str,
) -> Result<OwnerReleased, Fault> {
  let mut done = OwnerReleased::default();

  for held in held_reservations(db, Some(owner)).await? {
    if release_reservation_by(db, events, &held.token, why, BY_OWNER).await? {
      done.reservations += 1;
    }
  }

  let rows = all(
    db,
    r#"SELECT * FROM "role_slot_requests"
       WHERE "owner" = $1 AND "state" IN ('waiting', 'reserved', 'active') ORDER BY "sequence""#,
    vec![owner.into()],
  )
  .await
  .map_err(faulted)?;

  for row in &rows {
    let held = request_of(row)?;
    let active = held.state == "active";

    if active
      && let Some(session) = held.session.clone().filter(|session| held.child_id.as_ref() == Some(session))
    {
      done.sessions.push(session);
    }

    if finish_by(db, events, &held.token, why, !active, BY_OWNER).await? {
      if active {
        done.released += 1;
      } else {
        done.cancelled += 1;
      }
    }
  }

  Ok(done)
}
