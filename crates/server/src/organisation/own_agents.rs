//! A person's own agents (docs/role-agents.md, 「A person's own agents」): the set their organisation supplies — the
//! sign-up composition resolved for them, each seat on the road it names or on the road of its role's agent of no
//! account — and the person's own changes laid over it: agents of their own added, a road chosen, a supplied agent
//! switched off. Each change a person may make is a setting of the delegation chain, allowed or locked by their
//! organisation; a supplied agent is put back as it was supplied whatever the chain says.

use axum::http::StatusCode;
use policy::organisation::ORGANISATION;
use serde_json::{Value, json};

use super::composition::{self, Road};
use crate::agent_roles::{self, AgentRole};
use crate::chain::{self, Subject};
use crate::control::Refused;
use crate::places::Ground;
use crate::role_agents::{self, RoleAgent};

/// Whether a person may make one kind of change, and the link that said so where one did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Permission {
  pub allowed: bool,
  pub from: Option<String>,
}

impl Permission {
  pub fn view(&self) -> Value {
    json!({ "allowed": self.allowed, "from": self.from })
  }

  /// The refusal of a change it withholds, under `code`.
  pub fn required(&self, code: &str, what: &str) -> Result<(), Refused> {
    if self.allowed {
      return Ok(());
    }

    Err(
      Refused::new(StatusCode::FORBIDDEN, format!("The person's organisation does not let them {what}"))
        .with_code(code),
    )
  }
}

/// A flag of the chain resolved for the person, allowed where nothing says otherwise; a chain that does not answer
/// is refused, nothing guessed.
pub async fn permission(ground: &Ground, key: &str, person: i64) -> Result<Permission, Refused> {
  let answer = chain::resolve(&crate::apps::service(ground), key, Subject::Person(person))
    .await
    .map_err(|_| super::unchecked())?;

  Ok(Permission {
    allowed: chain::flag(answer.as_ref(), true),
    from: answer.as_ref().and_then(|answer| answer["from"].as_str()).map(str::to_owned),
  })
}

/// Whether the person may change the set of agents supplied to them.
pub async fn may_add(ground: &Ground, person: i64) -> Result<Permission, Refused> {
  permission(ground, &ORGANISATION.own_agents.add.key, person).await
}

/// Whether the person may choose their agents' roads.
pub async fn may_choose_road(ground: &Ground, person: i64) -> Result<Permission, Refused> {
  permission(ground, &ORGANISATION.own_agents.road.key, person).await
}

/// One seat of the set supplied to a person.
#[derive(Clone, Debug, PartialEq)]
pub struct Supplied {
  pub role: String,
  pub name: String,
  /// Its road: the composition's, else the role's agent of no account's; none where the seat follows its asker.
  pub road: Option<Road>,
  /// The link whose composition supplied it, none for the platform's own rule.
  pub from: Option<String>,
}

/// The road an agent stands on, none where it names none.
pub fn road_of(agent: &RoleAgent) -> Option<Road> {
  agent.provider.as_ref().map(|provider| Road {
    provider: Some(provider.clone()),
    model: agent.model.clone(),
    effort: agent.effort.clone(),
  })
}

/// What one role stands at for the person: whether it is off and who turned it off, and the seats supplied of it.
pub struct RoleSupply {
  pub role: AgentRole,
  pub off: bool,
  pub off_from: Option<String>,
  pub supplied: Vec<Supplied>,
  /// The role's agent of no account that is on, whose settings a seat made of the role carries.
  pub template: Option<RoleAgent>,
}

/// Every role as it stands for the person, with the seats supplied of it, as the composition resolved for them says.
pub async fn supply(ground: &Ground, person: i64) -> Result<Vec<RoleSupply>, Refused> {
  let answer = composition::resolved(ground, Subject::Person(person)).await?;
  let standing = composition::standing(answer.as_ref());
  let held = role_agents::every(&ground.db).await?;
  let mut out = Vec::new();

  for role in agent_roles::every(&ground.db).await? {
    let said = standing.get(&role.name);
    let off = said.is_some_and(|said| said.off);
    let template = held
      .iter()
      .find(|agent| agent.role == role.name && agent.owner_account.is_none() && agent.enabled)
      .cloned();
    let template_road = template.as_ref().and_then(road_of);
    let roads: Vec<(Option<Road>, Option<String>)> = match said.and_then(|said| said.agents.clone()) {
      _ if off => Vec::new(),
      Some(agents) => agents
        .into_iter()
        .map(|road| {
          let road = if road.provider.is_some() { Some(road) } else { template_road.clone() };

          (road, said.and_then(|said| said.from.clone()))
        })
        .collect(),
      None if template.is_some() => vec![(template_road.clone(), None)],
      None => Vec::new(),
    };
    let supplied = roads
      .into_iter()
      .enumerate()
      .map(|(index, (road, from))| Supplied {
        role: role.name.clone(),
        name: composition::seat_name(&role.name, person, index),
        road,
        from,
      })
      .collect();

    out.push(RoleSupply {
      off,
      off_from: said.and_then(|said| said.off_from.clone()),
      supplied,
      template,
      role,
    });
  }

  Ok(out)
}

/// The person's own agents, every role agent filed under their account.
pub async fn held(ground: &Ground, person: i64) -> Result<Vec<RoleAgent>, Refused> {
  let account = person.to_string();

  Ok(
    role_agents::every(&ground.db)
      .await?
      .into_iter()
      .filter(|agent| agent.owner_account.as_deref() == Some(account.as_str()))
      .collect(),
  )
}

/// Whether an agent stands as it was supplied: on, and on the road supplied.
pub fn as_supplied(agent: &RoleAgent, supplied: &Supplied) -> bool {
  agent.enabled && road_of(agent) == supplied.road
}

/// A seat made for the person: the template's settings where its role has one, on `road` where one is named.
pub async fn seat(
  ground: &Ground,
  role: &RoleSupply,
  name: String,
  person: i64,
  road: Option<&Road>,
) -> Result<RoleAgent, Refused> {
  let mut agent = role.template.clone().unwrap_or_else(|| composition::fresh(&role.role.name, None));

  agent.name = name;
  agent.owner_account = Some(person.to_string());
  agent.enabled = true;
  agent.added_at = crate::now_ms();

  laid(ground, &role.role, agent, road).await
}

/// An agent laid on `road` — none clearing its road so it follows its asker — judged against its role's line, the
/// effort, thinking, answer ceiling and advisor of another road dropped.
pub async fn laid(
  ground: &Ground,
  role: &AgentRole,
  agent: RoleAgent,
  road: Option<&Road>,
) -> Result<RoleAgent, Refused> {
  let body = match road {
    Some(road) => json!({
      "provider": road.provider,
      "model": road.model,
      "effort": road.effort,
      "thinking": null,
      "answerTokens": null,
      "advisor": null,
    }),
    None => json!({ "provider": null, "model": null, "advisor": null }),
  };

  crate::control::role_agents::judged(ground, role, agent, &body).await
}

/// The name of the person's next own agent of a role: the role and the person's mark, numbered past every seat
/// supplied and every name taken.
pub async fn next_name(ground: &Ground, role: &RoleSupply, person: i64) -> Result<String, Refused> {
  for index in role.supplied.len().. {
    let name = composition::seat_name(&role.role.name, person, index);

    if role_agents::named(&ground.db, &name).await?.is_none() {
      return Ok(name);
    }
  }

  unreachable!("an unbounded range yields a name no agent holds")
}

/// A road a person names, judged as the body gives it: a provider together with a model, or neither for none.
pub fn road_asked(body: &Value) -> Result<Option<Option<Road>>, Refused> {
  if body.get("provider").is_none() && body.get("model").is_none() {
    return Ok(None);
  }

  let text =
    |field: &str| body.get(field).and_then(Value::as_str).map(str::trim).filter(|text| !text.is_empty());

  match (text("provider"), text("model")) {
    (None, None) => Ok(Some(None)),
    (Some(provider), Some(model)) => Ok(Some(Some(Road {
      provider: Some(provider.to_owned()),
      model: Some(model.to_owned()),
      effort: text("effort").map(str::to_owned),
    }))),
    _ => Err(
      Refused::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "An agent's road names its provider together with its model, or neither.",
      )
      .with_field("provider"),
    ),
  }
}

/// A road the person may stand an agent on: one their supply carries now. A road naming none follows the asking
/// supervisor and is always open.
pub async fn carried(ground: &Ground, person: i64, road: Option<&Road>) -> Result<(), Refused> {
  let Some((Some(provider), Some(model))) =
    road.map(|road| (road.provider.as_deref(), road.model.as_deref()))
  else {
    return Ok(());
  };

  if crate::supply::carries(ground, person, provider, &[model.to_owned()]).await?.first() == Some(&true) {
    return Ok(());
  }

  Err(
    Refused::new(
      StatusCode::CONFLICT,
      format!("No supply of the person's carries {provider} {model} now"),
    )
    .with_code("road-not-supplied")
    .with_field("model"),
  )
}
