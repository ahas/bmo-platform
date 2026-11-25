//! A role's agents as the store keeps them (docs/role-agents.md): one row a seat of a role, with the one road
//! its runs go out on, read at every use so a change the owner makes holds from the next admission.

use sea_orm::{ConnectionTrait, QueryResult};
use serde::Serialize;
use store::{Fault, all, one, run};

use policy::roles::Sessions;

use crate::order::Entry;

/// One agent as its row holds it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleAgent {
  pub name: String,
  pub role: String,
  /// The login account whose seat it is, none for the deployment's own (docs/role-agents.md).
  pub owner_account: Option<String>,
  /// Its road: the provider and model its runs go out on, both or neither. An agent naming neither follows
  /// the supervisor that asks it.
  pub provider: Option<String>,
  pub model: Option<String>,
  pub effort: Option<String>,
  /// `on`, `off`, or none for the provider's own.
  pub thinking: Option<String>,
  pub answer_tokens: Option<i64>,
  /// The model its program may consult, none for none (docs/provider-order.md, 「The advisor」).
  pub advisor: Option<String>,
  pub enabled: bool,
  /// Whether its session's cache is kept warm between runs.
  pub keeps_cache: bool,
  /// Whether its runs keep their sessions (docs/agent-roles.md, 「A role's sessions」).
  #[serde(serialize_with = "sessions_word")]
  pub sessions: Sessions,
  /// Its compaction threshold, none where its role's stands.
  pub fold_ratio: Option<f64>,
  /// What it lays over its role's preset, each none where the role's own stands.
  pub prompt: Option<String>,
  pub bundles: Option<Vec<String>>,
  pub skills: Option<Vec<String>>,
  pub added_at: i64,
  /// Whether a run of it a supervisor asks goes out on that supervisor's road while its own road stands where
  /// none does, none where its role's setting stands (docs/role-agents.md, 「An agent that follows its asker」).
  pub follows_asker: Option<bool>,
}

// The sessions setting as the routes write it.
fn sessions_word<S: serde::Serializer>(sessions: &Sessions, serializer: S) -> Result<S::Ok, S::Error> {
  serializer.serialize_str(sessions.word())
}

// A list a row keeps as JSON text, none where the row keeps none or it does not read.
fn list(row: &QueryResult, name: &str) -> Result<Option<Vec<String>>, Fault> {
  Ok(
    row
      .try_get::<Option<String>>("", name)?
      .and_then(|kept| serde_json::from_str(&kept).ok()),
  )
}

impl RoleAgent {
  pub fn of(row: &QueryResult) -> Result<Self, Fault> {
    let text = |name: &str| -> Result<Option<String>, Fault> {
      Ok(row.try_get::<Option<String>>("", name)?.filter(|held| !held.trim().is_empty()))
    };

    Ok(Self {
      name: row.try_get("", "name")?,
      role: row.try_get("", "role")?,
      provider: text("provider")?,
      model: text("model")?,
      effort: text("effort")?,
      thinking: text("thinking")?,
      answer_tokens: row.try_get("", "answer_tokens")?,
      advisor: text("advisor")?,
      enabled: row.try_get::<i64>("", "enabled")? != 0,
      keeps_cache: row.try_get::<i64>("", "keeps_cache")? != 0,
      sessions: Sessions::of(&row.try_get::<String>("", "sessions")?).unwrap_or_default(),
      fold_ratio: row.try_get("", "fold_ratio")?,
      prompt: text("prompt")?,
      bundles: list(row, "bundles")?,
      skills: list(row, "skills")?,
      added_at: row.try_get("", "added_at")?,
      owner_account: row.try_get("", "owner_account")?,
      follows_asker: row.try_get::<Option<i64>>("", "follows_asker")?.map(|on| on != 0),
    })
  }

  /// The road its runs go out on, none where it follows the supervisor that asks it.
  pub fn entry(&self) -> Option<Entry> {
    Some(Entry {
      provider: self.provider.clone()?,
      model: self.model.clone()?,
      effort: self.effort.clone(),
      thinking: self.thinking.clone(),
      answer_tokens: self.answer_tokens,
      advisor: self.advisor.clone(),
    })
  }

  /// Whether a run of it a supervisor asks goes out on that supervisor's road (docs/role-agents.md, 「An agent
  /// that follows its asker」): an agent that names no road always does, and one that names a road does where its
  /// own setting says so, else where its role's, `role_follows`, does.
  pub fn follows(&self, role_follows: bool) -> bool {
    self.entry().is_none() || self.follows_asker.unwrap_or(role_follows)
  }

  /// Whether its road is this provider and model.
  pub fn on(&self, provider: &str, model: &str) -> bool {
    self.provider.as_deref() == Some(provider) && self.model.as_deref() == Some(model)
  }

  /// Whether its road runs the model an item's `model` label names (docs/repositories.md, 「An agent an item
  /// names」): the label is its model as the road names it, or its provider and model as `<provider>/<model>`.
  /// A model's own name may hold a `/`, so the whole label is matched as a model before it is split.
  pub fn runs(&self, named: &str) -> bool {
    let (Some(provider), Some(model)) = (self.provider.as_deref(), self.model.as_deref()) else {
      return false;
    };

    model == named || named.split_once('/') == Some((provider, model))
  }

  /// Whether it is a candidate for an item naming this model in this role, for a launch of this account: switched
  /// on, of the role and the account, and on a road that runs the model (docs/role-agents.md, 「Which agent a run
  /// takes」).
  pub fn candidate_for(&self, role: &str, model: &str, account: Option<&str>) -> bool {
    self.enabled && self.role == role && self.owner_account.as_deref() == account && self.runs(model)
  }
}

/// A role's agents with their roads, as a refusal for a model none of them runs names them: one an agent, its
/// road, and whether it is switched off.
pub fn roads_held(role: &str, agents: &[RoleAgent]) -> String {
  let lines: Vec<String> = agents
    .iter()
    .filter(|agent| agent.role == role)
    .map(|agent| {
      let road = match (&agent.provider, &agent.model) {
        (Some(provider), Some(model)) => format!("{provider}/{model}"),
        _ => "the asking supervisor's road".to_owned(),
      };
      let off = if agent.enabled { "" } else { ", switched off" };

      format!("- {}: {road}{off}", agent.name)
    })
    .collect();

  if lines.is_empty() {
    format!("(the {role} role holds no agent)")
  } else {
    lines.join("\n")
  }
}

/// The candidates of a role for an item naming a model, for a launch of this account, by name
/// (docs/role-agents.md, 「Which agent a run takes」).
pub async fn candidates<C: ConnectionTrait>(
  db: &C,
  role: &str,
  model: &str,
  account: Option<&str>,
) -> Result<Vec<RoleAgent>, Fault> {
  Ok(
    of_role(db, role)
      .await?
      .into_iter()
      .filter(|agent| agent.candidate_for(role, model, account))
      .collect(),
  )
}

/// Every agent the store holds, by role and then by name.
pub async fn every<C: ConnectionTrait>(db: &C) -> Result<Vec<RoleAgent>, Fault> {
  all(db, r#"SELECT * FROM "role_agents" ORDER BY "role", "name""#, vec![])
    .await?
    .iter()
    .map(RoleAgent::of)
    .collect()
}

/// A role's agents, by name: they stand in no order of priority, and a run naming no agent takes any that is free.
pub async fn of_role<C: ConnectionTrait>(db: &C, role: &str) -> Result<Vec<RoleAgent>, Fault> {
  all(
    db,
    r#"SELECT * FROM "role_agents" WHERE "role" = $1 ORDER BY "name""#,
    vec![role.into()],
  )
  .await?
  .iter()
  .map(RoleAgent::of)
  .collect()
}

/// The agent of this name, where the store holds one.
pub async fn named<C: ConnectionTrait>(db: &C, name: &str) -> Result<Option<RoleAgent>, Fault> {
  one(db, r#"SELECT * FROM "role_agents" WHERE "name" = $1"#, vec![name.into()])
    .await?
    .as_ref()
    .map(RoleAgent::of)
    .transpose()
}

/// The name an agent added with none is given: its role and the first number no agent of the role holds.
pub async fn next_name<C: ConnectionTrait>(db: &C, role: &str) -> Result<String, Fault> {
  let held: Vec<String> = every(db).await?.into_iter().map(|agent| agent.name).collect();

  Ok(
    (1..)
      .map(|number| format!("{role}-{number:02}"))
      .find(|name| !held.contains(name))
      .unwrap_or_else(|| format!("{role}-01")),
  )
}

// A list as a row keeps it, none where the agent sets none.
fn kept(list: &Option<Vec<String>>) -> Option<String> {
  list
    .as_ref()
    .map(|list| serde_json::to_string(list).unwrap_or_else(|_| "[]".to_owned()))
}

/// The agent written whole: added where the store holds none of its name, and every field replaced where it
/// holds one.
pub async fn write<C: ConnectionTrait>(db: &C, agent: &RoleAgent) -> Result<(), Fault> {
  run(
    db,
    r#"INSERT INTO "role_agents"
         ("name", "role", "provider", "model", "effort", "thinking", "answer_tokens", "enabled",
          "keeps_cache", "fold_ratio", "prompt", "bundles", "skills", "added_at", "sessions",
          "owner_account", "advisor", "follows_asker")
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
       ON CONFLICT ("name") DO UPDATE SET
         "role" = excluded."role", "provider" = excluded."provider", "model" = excluded."model",
         "effort" = excluded."effort", "thinking" = excluded."thinking",
         "answer_tokens" = excluded."answer_tokens", "enabled" = excluded."enabled",
         "keeps_cache" = excluded."keeps_cache",
         "fold_ratio" = excluded."fold_ratio", "prompt" = excluded."prompt", "bundles" = excluded."bundles",
         "skills" = excluded."skills", "sessions" = excluded."sessions",
         "owner_account" = excluded."owner_account", "advisor" = excluded."advisor",
         "follows_asker" = excluded."follows_asker""#,
    vec![
      agent.name.clone().into(),
      agent.role.clone().into(),
      agent.provider.clone().into(),
      agent.model.clone().into(),
      agent.effort.clone().into(),
      agent.thinking.clone().into(),
      agent.answer_tokens.into(),
      i64::from(agent.enabled).into(),
      i64::from(agent.keeps_cache).into(),
      agent.fold_ratio.into(),
      agent.prompt.clone().into(),
      kept(&agent.bundles).into(),
      kept(&agent.skills).into(),
      agent.added_at.into(),
      agent.sessions.word().into(),
      agent.owner_account.clone().into(),
      agent.advisor.clone().into(),
      agent.follows_asker.map(i64::from).into(),
    ],
  )
  .await?;

  Ok(())
}

/// A role's agents as a supervisor naming one for a job reads them (docs/repositories.md, 「An agent an item
/// names」): a line an agent, with its road, and whether it is switched off or busy now.
pub async fn listing<C: ConnectionTrait>(db: &C, role: &str) -> Result<String, Fault> {
  let busy = busy(db, role).await?;
  let lines: Vec<String> = of_role(db, role)
    .await?
    .iter()
    .map(|agent| {
      let road = match (&agent.provider, &agent.model) {
        (Some(provider), Some(model)) => {
          let effort = agent.effort.as_deref().map(|effort| format!(" at {effort}")).unwrap_or_default();

          format!("{provider}/{model}{effort}")
        }
        _ => "the asking supervisor's road".to_owned(),
      };
      let standing = if !agent.enabled {
        "switched off"
      } else if busy.contains(&agent.name) {
        "carries a run now"
      } else {
        "free"
      };

      format!("- {}: {road}, {standing}", agent.name)
    })
    .collect();

  Ok(if lines.is_empty() {
    format!("(the {role} role holds no agent)")
  } else {
    lines.join("\n")
  })
}

/// The agent removed; whether the store held it.
pub async fn remove<C: ConnectionTrait>(db: &C, name: &str) -> Result<bool, Fault> {
  Ok(run(db, r#"DELETE FROM "role_agents" WHERE "name" = $1"#, vec![name.into()]).await? > 0)
}

/// Every agent, of any role, a slot request stands let in on now.
pub async fn carrying_any<C: ConnectionTrait>(db: &C) -> Result<Vec<String>, Fault> {
  all(
    db,
    r#"SELECT "seat" FROM "role_slot_requests" WHERE "state" = 'active' AND "seat" IS NOT NULL"#,
    vec![],
  )
  .await?
  .iter()
  .map(|row| row.try_get::<String>("", "seat").map_err(Fault::from))
  .collect()
}

/// The agents busy now, by name (docs/role-agents.md, 「When an agent is busy」): the seats a slot request let in
/// on carries a run at work on, a job standing in the machine's turn leaving its seat free.
pub async fn busy<C: ConnectionTrait>(db: &C, role: &str) -> Result<Vec<String>, Fault> {
  all(
    db,
    &format!(
      r#"SELECT DISTINCT "seat" FROM "role_slot_requests"
         WHERE "role" = $1 AND "state" = 'active' AND "seat" IS NOT NULL AND {}"#,
      crate::watches::machine_turn::AT_WORK
    ),
    vec![role.into()],
  )
  .await?
  .iter()
  .map(|row| row.try_get::<String>("", "seat").map_err(Fault::from))
  .collect()
}

/// Every agent, of any role, busy now (docs/role-agents.md, 「When an agent is busy」).
pub async fn busy_any<C: ConnectionTrait>(db: &C) -> Result<Vec<String>, Fault> {
  all(
    db,
    &format!(
      r#"SELECT DISTINCT "seat" FROM "role_slot_requests"
         WHERE "state" = 'active' AND "seat" IS NOT NULL AND {}"#,
      crate::watches::machine_turn::AT_WORK
    ),
    vec![],
  )
  .await?
  .iter()
  .map(|row| row.try_get::<String>("", "seat").map_err(Fault::from))
  .collect()
}

/// The agents a slot request stands let in on now, by name: the seats that carry a run.
pub async fn carrying<C: ConnectionTrait>(db: &C, role: &str) -> Result<Vec<String>, Fault> {
  all(
    db,
    r#"SELECT "seat" FROM "role_slot_requests"
       WHERE "role" = $1 AND "state" = 'active' AND "seat" IS NOT NULL"#,
    vec![role.into()],
  )
  .await?
  .iter()
  .map(|row| row.try_get::<String>("", "seat").map_err(Fault::from))
  .collect()
}

/// The agent a run is let in on (docs/role-agents.md, 「What a run reads off its agent」): the seat of the newest
/// slot request that stands active at the path the run works at, of the role named where one is; and, for a
/// run that names no path, the seat of its role's only active request. None where neither tells one, and the
/// run then stands on its role's preset alone.
pub async fn seated_at<C: ConnectionTrait>(
  db: &C,
  role: Option<&str>,
  path: Option<&str>,
) -> Result<Option<RoleAgent>, Fault> {
  // A composition made as an agent outside any run of it reads that agent, where it is of the role named.
  if let Ok(agent) = COMPOSED_AS.try_with(Clone::clone) {
    return Ok(named(db, &agent).await?.filter(|agent| role.is_none_or(|role| agent.role == role)));
  }

  let path = path.map(|path| path.trim().trim_end_matches('/')).filter(|path| !path.is_empty());
  let seat = match (path, role) {
    (Some(path), Some(role)) => one(
      db,
      r#"SELECT "seat" FROM "role_slot_requests"
         WHERE rtrim("path", '/') = $1 AND "role" = $2 AND "state" = 'active' AND "seat" IS NOT NULL
         ORDER BY "sequence" DESC LIMIT 1"#,
      vec![path.into(), role.into()],
    )
    .await?
    .map(|row| row.try_get::<String>("", "seat"))
    .transpose()?,
    (Some(path), None) => one(
      db,
      r#"SELECT "seat" FROM "role_slot_requests"
         WHERE rtrim("path", '/') = $1 AND "state" = 'active' AND "seat" IS NOT NULL
         ORDER BY "sequence" DESC LIMIT 1"#,
      vec![path.into()],
    )
    .await?
    .map(|row| row.try_get::<String>("", "seat"))
    .transpose()?,
    (None, Some(role)) => match carrying(db, role).await?.as_slice() {
      [only] => Some(only.clone()),
      _ => None,
    },
    (None, None) => None,
  };

  match seat {
    Some(seat) => named(db, &seat).await,
    None => Ok(None),
  }
}

/// The advisor a run let in on the seat consults (docs/provider-order.md, 「The advisor」): the seat's own where it
/// names a road, and for a seat that follows its supervisor the advisor of the supervisor that asked the newest
/// slot request standing on it; none where that road's provider takes no advisor or names none.
pub async fn advisor_of<C: ConnectionTrait>(db: &C, seat: &RoleAgent) -> Result<Option<String>, Fault> {
  let road = match seat.entry() {
    Some(own) => Some(own),
    None => {
      let asker = one(
        db,
        r#"SELECT "asked_by" FROM "role_slot_requests"
           WHERE "seat" = $1 AND "state" = 'active' AND "asked_by" IS NOT NULL
           ORDER BY "sequence" DESC LIMIT 1"#,
        vec![seat.name.clone().into()],
      )
      .await?
      .map(|row| row.try_get::<String>("", "asked_by"))
      .transpose()?;

      match asker {
        Some(asker) => {
          crate::supervisors::named(db, &asker).await?.and_then(|supervisor| supervisor.road())
        }
        None => None,
      }
    }
  };

  Ok(road.and_then(|road| road.advisor_taken()))
}

/// The prompt a run of a role opens with: its agent's own where it lays one over its role's, else its role's;
/// none where neither carries one.
pub async fn prompt_at<C: ConnectionTrait>(
  db: &C,
  role: &str,
  path: Option<&str>,
) -> Result<Option<String>, Fault> {
  let own = seated_at(db, Some(role), path)
    .await?
    .and_then(|agent| agent.prompt)
    .filter(|prompt| !prompt.trim().is_empty());

  match own {
    Some(own) => Ok(Some(own)),
    None => crate::agent_roles::prompt_of(db, role).await,
  }
}

/// The bundles a run of a role is given: its agent's own where it lays them over its role's, else its role's.
pub async fn bundles_at<C: ConnectionTrait>(
  db: &C,
  role: &str,
  path: Option<&str>,
) -> Result<Vec<String>, Fault> {
  match seated_at(db, Some(role), path).await?.and_then(|agent| agent.bundles) {
    Some(own) => Ok(own),
    None => crate::agent_roles::bundles_of(db, role).await,
  }
}

tokio::task_local! {
  // The agent a composition is made as, where the work names it outright rather than by its folder.
  static COMPOSED_AS: String;
}

/// A piece of work done as an agent named outright — the warm ping of its kept session, which holds no slot
/// while it goes (docs/keepalive-schedule.md, 「An agent's kept session」), or a turn of a role's session, which
/// runs as the seat of the slot its session was started on (docs/role-agents.md, 「What a run reads off its
/// agent」): whatever it composes and wherever it is placed reads that agent as its seat.
pub async fn composed_as<F: Future>(agent: String, work: F) -> F::Output {
  COMPOSED_AS.scope(agent, work).await
}

/// The seat of the slot a role's session was started on, where the slot still stands active; none otherwise.
pub async fn session_seat<C: ConnectionTrait>(db: &C, session: &str) -> Result<Option<String>, Fault> {
  one(
    db,
    r#"SELECT "seat" FROM "role_slot_requests"
       WHERE "session" = $1 AND "state" = 'active' AND "seat" IS NOT NULL
       ORDER BY "sequence" DESC LIMIT 1"#,
    vec![session.into()],
  )
  .await?
  .map(|row| row.try_get::<String>("", "seat"))
  .transpose()
  .map_err(Into::into)
}

/// Whether a run works in a folder now: a slot request stands active at it.
pub async fn worked_in<C: ConnectionTrait>(db: &C, path: &str) -> Result<bool, Fault> {
  let path = path.trim().trim_end_matches('/');

  if path.is_empty() {
    return Ok(false);
  }

  Ok(
    one(
      db,
      r#"SELECT 1 AS "works" FROM "role_slot_requests"
         WHERE rtrim("path", '/') = $1 AND "state" = 'active' LIMIT 1"#,
      vec![path.into()],
    )
    .await?
    .is_some(),
  )
}

/// Whether a run of a role keeps its session (docs/agent-roles.md, 「A role's sessions」): the setting of the
/// agent it is let in on, by the folder it works in; a run whose agent cannot be told keeps none.
pub async fn sessions_at<C: ConnectionTrait>(
  db: &C,
  role: &str,
  path: Option<&str>,
) -> Result<Sessions, Fault> {
  Ok(seated_at(db, Some(role), path).await?.map_or(Sessions::Fresh, |agent| agent.sessions))
}
