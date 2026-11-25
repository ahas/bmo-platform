//! The agents an account is given as it is made ready (docs/role-agents.md, 「The agents an account is given」):
//! the chain's `signup.composition`, a JSON object agency judges and writes on the chain itself, resolved for the
//! person and laid over the platform's own rule — one agent for every role that has an agent of no account, on
//! that agent's road.
//!
//! A composition names roles under `roles`, each with `off`, whether the people under the link may not use the
//! role, and `agents`, the roads of the agents a person is given of it, each `provider`, `model` and `effort`. A
//! role named nowhere stands as the platform's rule gives it.

use std::collections::BTreeMap;

use axum::http::StatusCode;
use policy::organisation::ORGANISATION;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::agent_roles::{self, AgentRole};
use crate::chain::{self, Subject};
use crate::control::Refused;
use crate::places::Ground;
use crate::role_agents::{self, RoleAgent};

/// One agent's road in a composition: a provider and a model together, or neither for the road of the role's own
/// agent of no account.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Road {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub provider: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub effort: Option<String>,
}

/// What a composition says of one role.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RoleSaid {
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub off: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub agents: Option<Vec<Road>>,
}

/// A link's composition.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Composition {
  #[serde(default)]
  pub roles: BTreeMap<String, RoleSaid>,
}

/// What stands for one role for a subject.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standing {
  /// Whether a link read says the role is off.
  pub off: bool,
  /// The roads of the agents given of it, as the nearest link naming them says; none for the platform's rule.
  pub agents: Option<Vec<Road>>,
  /// The link that said the agents, where one did.
  pub from: Option<String>,
  /// The outermost link that turned the role off, where one did.
  pub off_from: Option<String>,
}

fn key() -> &'static str {
  &ORGANISATION.composition.key
}

fn unprocessable(error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::UNPROCESSABLE_ENTITY, error).with_code("bad-composition")
}

/// A composition a caller wrote, judged: its shape, every role one agency holds, at most the policy's agents a
/// role, and each road judged against its role's line as an agent's road is.
pub async fn judge(ground: &Ground, value: &Value) -> Result<Composition, Refused> {
  let composition: Composition = serde_json::from_value(value.clone()).map_err(|why| {
    unprocessable(format!("A composition is {{ roles: {{ <role>: {{ off, agents }} }} }}: {why}"))
  })?;

  for (name, said) in &composition.roles {
    let Some(role) = agent_roles::named(&ground.db, name).await? else {
      return Err(unprocessable(agent_roles::unknown(name)));
    };
    let agents = said.agents.as_deref().unwrap_or_default();

    if agents.len() > ORGANISATION.composition.most_agents {
      return Err(unprocessable(format!(
        "{name}: at most {} agents of one role are given to a person.",
        ORGANISATION.composition.most_agents
      )));
    }

    for road in agents {
      if road.provider.is_some() != road.model.is_some() {
        return Err(unprocessable(format!(
          "{name}: an agent's road names its provider together with its model, or neither."
        )));
      }

      if road.provider.is_some() {
        crate::control::role_agents::judged(ground, &role, fresh(name, None), &road_body(road)).await?;
      }
    }
  }

  Ok(composition)
}

/// What stands for each role named anywhere on the subject's chain: off where any link still read says so, the
/// agents of the nearest link naming them.
pub fn standing(answer: Option<&Value>) -> BTreeMap<String, Standing> {
  let mut out: BTreeMap<String, Standing> = BTreeMap::new();

  for layer in answer.and_then(|answer| answer["layers"].as_array()).into_iter().flatten() {
    let Ok(composition) = serde_json::from_value::<Composition>(layer["value"].clone()) else {
      continue;
    };
    let link = layer["link"].as_str().unwrap_or_default();

    for (role, said) in composition.roles {
      let held = out.entry(role).or_insert(Standing {
        off: false,
        agents: None,
        from: None,
        off_from: None,
      });

      if said.off && !held.off {
        held.off_from = Some(link.to_owned());
      }

      held.off |= said.off;
      if let Some(agents) = said.agents {
        held.agents = Some(agents);
        held.from = Some(link.to_owned());
      }
    }
  }

  out
}

/// What the chain resolves for a subject, read afresh.
pub async fn resolved(ground: &Ground, subject: Subject) -> Result<Option<Value>, Refused> {
  chain::resolve(&crate::apps::service(ground), key(), subject)
    .await
    .map_err(|_| super::unchecked())
}

/// Whether the role stands off for the person; where the account service does not answer, it stands on, as it
/// does where nothing is said.
pub async fn role_off(ground: &Ground, person: i64, role: &str) -> bool {
  match resolved(ground, Subject::Person(person)).await {
    Ok(answer) => standing(answer.as_ref()).get(role).is_some_and(|held| held.off),
    Err(_) => false,
  }
}

/// An agent of no road, as an added agent starts.
pub(crate) fn fresh(role: &str, name: Option<String>) -> RoleAgent {
  RoleAgent {
    name: name.unwrap_or_default(),
    role: role.to_owned(),
    provider: None,
    model: None,
    effort: None,
    thinking: None,
    answer_tokens: None,
    advisor: None,
    enabled: true,
    keeps_cache: false,
    sessions: policy::roles::Sessions::default(),
    fold_ratio: None,
    prompt: None,
    bundles: None,
    skills: None,
    added_at: crate::now_ms(),
    owner_account: None,
    follows_asker: None,
  }
}

pub(crate) fn road_body(road: &Road) -> Value {
  json!({ "provider": road.provider, "model": road.model, "effort": road.effort })
}

/// The name of a person's `index`th agent of a role: the role and the person's mark, numbered from the second,
/// at most 32 characters as a seat's name is.
pub(crate) fn seat_name(role: &str, user: i64, index: usize) -> String {
  let name = if index == 0 {
    format!("{role}-u{user}")
  } else {
    format!("{role}-u{user}-{}", index + 1)
  };

  name.chars().take(32).collect()
}

/// The person's agents made as the composition resolved for them says: for each role the person holds no agent
/// of and that does not stand off, the agents it names, else one on the road of the role's agent of no account
/// that is on, each carrying that agent's settings. An agent whose road its role's line refuses is said to the
/// log and left out. Answers the agents made.
pub async fn compose(ground: &Ground, user: i64) -> Result<Vec<RoleAgent>, Refused> {
  let answer = resolved(ground, Subject::Person(user)).await?;
  let standing = standing(answer.as_ref());
  let account = user.to_string();
  let held = role_agents::every(&ground.db).await?;
  let mut made = Vec::new();

  for role in agent_roles::every(&ground.db).await? {
    if held
      .iter()
      .any(|agent| agent.role == role.name && agent.owner_account.as_deref() == Some(&account))
    {
      continue;
    }

    let said = standing.get(&role.name);

    if said.is_some_and(|said| said.off) {
      continue;
    }

    let template = held
      .iter()
      .find(|agent| agent.role == role.name && agent.owner_account.is_none() && agent.enabled)
      .cloned();
    let roads: Vec<Option<Road>> = match said.and_then(|said| said.agents.clone()) {
      Some(agents) => agents.into_iter().map(|road| road.provider.is_some().then_some(road)).collect(),
      None if template.is_some() => vec![None],
      None => Vec::new(),
    };

    for (index, road) in roads.into_iter().enumerate() {
      let name = seat_name(&role.name, user, index);

      if role_agents::named(&ground.db, &name).await?.is_some() {
        continue;
      }

      match seat(ground, &role, template.clone(), name.clone(), &account, road.as_ref()).await {
        Ok(agent) => {
          role_agents::write(&ground.db, &agent).await?;
          made.push(agent);
        }
        Err(refused) => eprintln!("account {user}: the agent {name} was not made: {}", refused.error),
      }
    }
  }

  Ok(made)
}

// One agent of the person's: the template's settings where the role has one, on the road given or the template's.
async fn seat(
  ground: &Ground,
  role: &AgentRole,
  template: Option<RoleAgent>,
  name: String,
  account: &str,
  road: Option<&Road>,
) -> Result<RoleAgent, Refused> {
  let mut agent = template.unwrap_or_else(|| fresh(&role.name, None));

  agent.name = name;
  agent.owner_account = Some(account.to_owned());
  agent.enabled = true;
  agent.added_at = crate::now_ms();

  match road {
    Some(road) => crate::control::role_agents::judged(ground, role, agent, &road_body(road)).await,
    None => Ok(agent),
  }
}
