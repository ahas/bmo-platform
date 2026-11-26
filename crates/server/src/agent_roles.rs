//! The roles as the store keeps them (docs/agent-roles.md, 「What a role row holds」): every field of a role's
//! row is read off it at every use, so a change the owner makes holds from the next read with no restart.

use policy::roles::{ROLES, Sessions};
use sea_orm::{ConnectionTrait, QueryResult};
use store::{Fault, all, one};

use crate::order::{Entry, Order};
use crate::role_agents::{self, RoleAgent};

/// One role as its row holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentRole {
  pub name: String,
  pub label: String,
  /// The bundles it gives, each one the policy declares.
  pub bundles: Vec<String>,
  /// The skills its runs carry on the worker line, where it names them; none leaves its runs the whole plugin
  /// the release places for it.
  pub skills: Option<Vec<String>>,
  /// The most of its runs that go at once.
  pub max_concurrent: i64,
  /// The prompt its runs open with after the harness opening, where it carries one.
  pub prompt: Option<String>,
  /// What it is for, in one sentence a screen shows.
  pub description: Option<String>,
  /// Whether its runs keep their sessions (docs/agent-roles.md, 「A role's sessions」).
  pub sessions: Sessions,
  /// The launch line its runs go out on.
  pub line: String,
  /// Its provider order.
  pub order: Order,
  /// Whether its agents follow the supervisor that asks a run of them, each agent's own setting standing over
  /// it (docs/role-agents.md, 「An agent that follows its asker」).
  pub follows_asker: bool,
  /// The share of its model's window a run's session may fill before it is folded.
  pub fold_ratio: f64,
  pub position: i64,
  /// Its agents, in the order a run looks for a free one (docs/role-agents.md).
  pub agents: Vec<RoleAgent>,
  /// The agent each road of its order is taken from, one a road in the order's own order: the first agent
  /// standing on that road as a run looks for a free one.
  pub road_agents: Vec<String>,
}

impl AgentRole {
  /// The most of an account's runs of this role that go at once: the agents it may take — its own, and the
  /// seats of `units`, the units of its path — that are switched on (docs/role-slots.md, 「Accounts」).
  /// `account` is the account id as the slot's line holds it, in digits.
  pub fn cap_for(&self, account: Option<&str>, units: &[i64]) -> i64 {
    self
      .agents
      .iter()
      .filter(|agent| agent.enabled && agent.may_take(account, units))
      .count() as i64
  }

  /// Whether its switched-on agents all name no road, so a run of it follows the supervisor that asks it
  /// (docs/role-agents.md, 「Which agent a run takes」). False for a role with no agent switched on, whose runs
  /// wait until the owner switches one on (docs/role-slots.md, 「The cap」).
  pub fn names_no_road(&self) -> bool {
    self.order.entries.is_empty() && self.agents.iter().any(|agent| agent.enabled)
  }

  /// Whether a run let in on `agent` that a supervisor asks goes out on that supervisor's road.
  pub fn follows(&self, agent: &RoleAgent) -> bool {
    agent.follows(self.follows_asker)
  }

  /// Whether a run of it a supervisor asks goes out on that supervisor's road: every agent of it switched on
  /// follows its asker, and one at least is switched on.
  pub fn runs_follow(&self) -> bool {
    let mut on = self.agents.iter().filter(|agent| agent.enabled).peekable();

    on.peek().is_some() && on.all(|agent| self.follows(agent))
  }
}

impl AgentRole {
  pub fn of(row: &QueryResult) -> Result<Self, Fault> {
    let bundles: String = row.try_get("", "bundles")?;
    let skills: Option<String> = row.try_get("", "skills")?;
    let order: String = row.try_get("", "provider_order")?;

    Ok(Self {
      name: row.try_get("", "name")?,
      label: row.try_get("", "label")?,
      bundles: serde_json::from_str(&bundles).map_err(|error| Fault::new(error.to_string()))?,
      skills: skills
        .map(|held| serde_json::from_str(&held).map_err(|error| Fault::new(error.to_string())))
        .transpose()?,
      max_concurrent: row.try_get("", "max_concurrent")?,
      prompt: row.try_get("", "prompt")?,
      description: row.try_get("", "description")?,
      sessions: Sessions::of(&row.try_get::<String>("", "sessions")?).unwrap_or_default(),
      line: row.try_get("", "line")?,
      order: Order::of_row(&order)
        .forcing(row.try_get("", "active_provider")?, row.try_get("", "active_model")?),
      fold_ratio: row.try_get("", "fold_ratio")?,
      follows_asker: row.try_get::<i64>("", "follows_asker")? != 0,
      position: row.try_get("", "position")?,
      agents: vec![],
      road_agents: vec![],
    })
  }

  /// The role with its agents: how many of its runs go at once is the number of them switched on, and the
  /// roads its runs go out on are theirs, each once, in the order a run looks for a free agent — the agents
  /// that carry no run first, `carrying` naming the ones that do, so a run reading the role's roads finds
  /// the first free agent's. A role whose agents name no road follows the supervisor that asks it.
  pub fn seated(self, agents: Vec<RoleAgent>, carrying: &[String]) -> Self {
    let on: Vec<&RoleAgent> = agents.iter().filter(|agent| agent.enabled).collect();
    let (free, busy): (Vec<&RoleAgent>, Vec<&RoleAgent>) =
      on.iter().copied().partition(|agent| !carrying.contains(&agent.name));
    let mut entries: Vec<Entry> = vec![];
    let mut road_agents: Vec<String> = vec![];

    for (agent, entry) in free.iter().chain(busy.iter()).filter_map(|agent| Some((agent, agent.entry()?))) {
      if !entries.iter().any(|held| held.provider == entry.provider && held.model == entry.model) {
        entries.push(entry);
        road_agents.push(agent.name.clone());
      }
    }

    Self {
      max_concurrent: on.len() as i64,
      order: Order {
        entries,
        forced: None,
      },
      agents,
      road_agents,
      ..self
    }
  }

  /// The agent a run that holds no seat of the role goes out as on a road its order took: the one the road was
  /// taken from, the first free agent standing on it, else the first busy one; none for a road no agent of it
  /// stands on.
  pub fn agent_of_road(&self, provider: &str, model: &str) -> Option<&RoleAgent> {
    let at = self
      .order
      .entries
      .iter()
      .position(|entry| entry.provider == provider && entry.model == model)?;

    self.road_agents.get(at).and_then(|name| self.agent(name))
  }

  /// The agent of this name among the role's, where it holds one.
  pub fn agent(&self, name: &str) -> Option<&RoleAgent> {
    self.agents.iter().find(|agent| agent.name == name)
  }
}

/// Every role the store holds, in the order a screen lists them.
pub async fn every<C: ConnectionTrait>(db: &C) -> Result<Vec<AgentRole>, Fault> {
  let mut agents = role_agents::every(db).await?;
  let carrying = role_agents::carrying_any(db).await?;
  let roles: Vec<AgentRole> = all(db, r#"SELECT * FROM "roles" ORDER BY "position", "name""#, vec![])
    .await?
    .iter()
    .map(AgentRole::of)
    .collect::<Result<_, _>>()?;

  Ok(
    roles
      .into_iter()
      .map(|role| {
        let (own, rest): (Vec<RoleAgent>, Vec<RoleAgent>) =
          std::mem::take(&mut agents).into_iter().partition(|agent| agent.role == role.name);

        agents = rest;
        role.seated(own, &carrying)
      })
      .collect(),
  )
}

/// The role of this name, where the store holds one.
pub async fn named<C: ConnectionTrait>(db: &C, name: &str) -> Result<Option<AgentRole>, Fault> {
  let Some(role) = one(db, r#"SELECT * FROM "roles" WHERE "name" = $1"#, vec![name.into()])
    .await?
    .as_ref()
    .map(AgentRole::of)
    .transpose()?
  else {
    return Ok(None);
  };

  let carrying = role_agents::carrying(db, name).await?;

  Ok(Some(role.seated(role_agents::of_role(db, name).await?, &carrying)))
}

/// Whether the store holds a role of this name.
pub async fn holds<C: ConnectionTrait>(db: &C, name: &str) -> Result<bool, Fault> {
  Ok(named(db, name).await?.is_some())
}

/// Every role's name, in the order a screen lists them, as a refusal lists them.
pub async fn names<C: ConnectionTrait>(db: &C) -> Result<Vec<String>, Fault> {
  Ok(every(db).await?.into_iter().map(|role| role.name).collect())
}

/// The bundles a role gives; none for a role the store does not hold.
pub async fn bundles_of<C: ConnectionTrait>(db: &C, name: &str) -> Result<Vec<String>, Fault> {
  Ok(named(db, name).await?.map(|role| role.bundles).unwrap_or_default())
}

/// The prompt a role's runs open with; none for a role carrying none or one the store does not hold.
pub async fn prompt_of<C: ConnectionTrait>(db: &C, name: &str) -> Result<Option<String>, Fault> {
  Ok(
    named(db, name)
      .await?
      .and_then(|role| role.prompt)
      .filter(|prompt| !prompt.trim().is_empty()),
  )
}

/// The text a run of the role is given beside its prompt where its program stands on `provider`
/// (docs/agent-roles.md, 「A role's provider prompts」); none where the role holds none for that provider.
pub async fn provider_prompt<C: ConnectionTrait>(
  db: &C,
  role: &str,
  provider: &str,
) -> Result<Option<String>, Fault> {
  Ok(
    one(
      db,
      r#"SELECT "prompt" FROM "role_provider_prompts" WHERE "role" = $1 AND "provider" = $2"#,
      vec![role.into(), provider.into()],
    )
    .await?
    .map(|row| row.try_get::<String>("", "prompt"))
    .transpose()?
    .filter(|prompt| !prompt.trim().is_empty()),
  )
}

/// Every provider prompt a role holds, by its provider.
pub async fn provider_prompts<C: ConnectionTrait>(
  db: &C,
  role: &str,
) -> Result<std::collections::BTreeMap<String, String>, Fault> {
  all(
    db,
    r#"SELECT "provider", "prompt" FROM "role_provider_prompts" WHERE "role" = $1 ORDER BY "provider""#,
    vec![role.into()],
  )
  .await?
  .iter()
  .map(|row| Ok((row.try_get("", "provider")?, row.try_get("", "prompt")?)))
  .collect()
}

/// The sentence an ask naming a role the store does not hold is refused with.
pub fn unknown(name: &str) -> String {
  format!("No role named {name} is held; a run names one of the store's roles.")
}

/// The role a job runs as where its item names none.
pub fn job_role() -> &'static str {
  &ROLES.job
}
