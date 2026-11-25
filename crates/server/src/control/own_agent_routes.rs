//! A person's own agents and their own instructions across the boundary (docs/api.md, 「own agents」;
//! docs/role-agents.md, 「A person's own agents」; docs/prompt-layers.md, 「The person's instructions」). A caller
//! naming a person acts on that person's account alone; the deployment's own caller acts on the account the path
//! names. Whether the person may make a change is the delegation chain's word for that person.

use std::collections::BTreeMap;

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use harness_providers::ARCHIVE_PROVIDERS;
use policy::organisation::ORGANISATION;
use policy::prompt_layers::PROMPT_LAYERS;
use serde_json::{Value, json};

use super::claude_accounts::Judging;
use super::role_agents::{queue_told, seats_changed, view};
use super::{Call, Refused, Route, answer};
use crate::access::Acting;
use crate::chain::{self, Subject};
use crate::organisation::own_agents::{self, RoleSupply, Supplied};
use crate::places::Ground;
use crate::role_agents::{self, RoleAgent};

// The account the path names, refused where a caller naming a person names another.
fn account_of(call: &Call) -> Result<i64, Refused> {
  let account = call
    .param("ownerAccount")
    .map(str::trim)
    .and_then(|account| account.parse::<i64>().ok())
    .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "an account is a user id"))?;

  match Acting::of(call)? {
    Acting::Person(person) if person != account => Err(
      Refused::new(StatusCode::FORBIDDEN, "The signed-in person acts on their own agents alone")
        .with_code("forbidden"),
    ),
    _ => Ok(account),
  }
}

fn not_theirs(name: &str) -> Refused {
  Refused::new(StatusCode::NOT_FOUND, format!("No agent of the person's is named {name}."))
    .with_code("not-found")
}

fn road_view(road: Option<&crate::organisation::composition::Road>) -> Value {
  road.map_or(
    Value::Null,
    |road| json!({ "provider": road.provider, "model": road.model, "effort": road.effort }),
  )
}

// One agent of the person's as the routes answer it: the agent where it stands, the seat supplied where it is one,
// and whether the person's own changes stand over that seat.
fn agent_view(
  agent: Option<&RoleAgent>,
  supplied: Option<&Supplied>,
  follows: bool,
  carrying: &[String],
) -> Value {
  let name = agent.map(|agent| agent.name.clone()).or_else(|| supplied.map(|seat| seat.name.clone()));

  json!({
    "name": name,
    "agent": agent.map(|agent| view(agent, follows, carrying)),
    "supplied": supplied.map(|seat| json!({ "road": road_view(seat.road.as_ref()), "from": seat.from })),
    "changed": match (agent, supplied) {
      (Some(agent), Some(seat)) => !own_agents::as_supplied(agent, seat),
      _ => false,
    },
  })
}

// Every role as it stands for the person, with their agents: the seats supplied first, in their order, then the
// agents of their own.
async fn roles_view(ground: &Ground, person: i64) -> Result<Vec<Value>, Refused> {
  let supply = own_agents::supply(ground, person).await?;
  let held = own_agents::held(ground, person).await?;
  let mut out = Vec::new();

  for role in &supply {
    let carrying = role_agents::carrying(&ground.db, &role.role.name).await?;
    let mut agents: Vec<Value> = role
      .supplied
      .iter()
      .map(|seat| {
        let agent = held.iter().find(|agent| agent.name == seat.name);

        agent_view(agent, Some(seat), role.role.follows_asker, &carrying)
      })
      .collect();

    agents.extend(
      held
        .iter()
        .filter(|agent| {
          agent.role == role.role.name && !role.supplied.iter().any(|seat| seat.name == agent.name)
        })
        .map(|agent| agent_view(Some(agent), None, role.role.follows_asker, &carrying)),
    );

    out.push(json!({
      "role": role.role.name,
      "label": role.role.label,
      "description": role.role.description,
      "line": role.role.line,
      "off": role.off,
      "offFrom": role.off_from,
      "most": ORGANISATION.composition.most_agents,
      "agents": agents,
    }));
  }

  Ok(out)
}

// The providers on now, each with the lines its program carries, for a road's chooser.
async fn providers_view(ground: &Ground) -> Result<Vec<Value>, Refused> {
  let mut out = Vec::new();

  for name in ARCHIVE_PROVIDERS {
    if !super::harness::provider_enabled(&ground.db, name).await? {
      continue;
    }

    let declared = super::harness::named(name)?.declared();

    out.push(json!({ "name": name, "label": declared.label, "lines": declared.lines }));
  }

  Ok(out)
}

async fn whole_view(ground: &Ground, person: i64) -> Result<Value, Refused> {
  let (add, road) = futures_util::try_join!(
    own_agents::may_add(ground, person),
    own_agents::may_choose_road(ground, person)
  )?;

  Ok(json!({
    "account": person,
    "add": add.view(),
    "road": road.view(),
    "roles": roles_view(ground, person).await?,
    "providers": providers_view(ground).await?,
  }))
}

// The role a person's agent stands in, as the person's supply says it.
async fn role_named(ground: &Ground, person: i64, role: &str) -> Result<RoleSupply, Refused> {
  own_agents::supply(ground, person)
    .await?
    .into_iter()
    .find(|held| held.role.name == role)
    .ok_or_else(|| {
      Refused::new(StatusCode::UNPROCESSABLE_ENTITY, crate::agent_roles::unknown(role))
        .with_code("unknown-role")
    })
}

/// `GET /own-agents/{ownerAccount}`: the agents supplied to the person and their own, every role as it stands for
/// them, whether they may change the set and choose roads, and the providers a road may name.
pub struct OwnAgentList;

impl Route for OwnAgentList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/own-agents/{ownerAccount}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let person = account_of(&call)?;

    Ok(answer(StatusCode::OK, whole_view(call.ground(), person).await?))
  }
}

crate::moved!(OwnAgentList);

/// `GET /own-agents/{ownerAccount}/models/{providerName}`: the models of a provider the person may stand an agent
/// on — a model another person owns left out — each with whether their supply carries it now.
pub struct OwnAgentModels;

impl Route for OwnAgentModels {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/own-agents/{ownerAccount}/models/{providerName}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let person = account_of(&call)?;
    let name = call.param("providerName").unwrap_or_default().to_owned();
    let provider = super::harness::named(&name)?;
    let offered = super::harness::offered(Judging::of(call.ground()), call.machine(), provider)
      .await
      .map_err(|why| Refused::new(StatusCode::SERVICE_UNAVAILABLE, why))?;
    let mut open = Vec::new();

    for model in &offered.models {
      let owner = crate::supply::owner_of(call.ground(), &name, &model.id).await?;

      if owner.is_none_or(|owner| owner == person) {
        open.push(model);
      }
    }

    let ids: Vec<String> = open.iter().map(|model| model.id.clone()).collect();
    let carried = crate::supply::carries(call.ground(), person, &name, &ids).await?;
    let models: Vec<Value> = open
      .iter()
      .zip(carried)
      .map(|(model, supplied)| {
        json!({ "id": model.id, "efforts": model.efforts, "inService": model.in_service, "supplied": supplied })
      })
      .collect();

    Ok(answer(StatusCode::OK, json!({ "provider": name, "models": models })))
  }
}

crate::moved!(OwnAgentModels);

/// `POST /own-agents/{ownerAccount}`: an agent of the person's own added to a role the organisation leaves on, on
/// the road named or on the road of the role's agent of no account.
pub struct OwnAgentCreate;

impl Route for OwnAgentCreate {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/own-agents/{ownerAccount}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let person = account_of(&call)?;
    let body = call.body().await?;
    let ground = call.ground();
    let asked_role = body.get("role").and_then(Value::as_str).map(str::trim).unwrap_or_default().to_owned();
    let role = role_named(ground, person, &asked_role).await?;

    own_agents::may_add(ground, person)
      .await?
      .required("own-agents-withheld", "add agents of their own")?;

    if role.off {
      return Err(
        Refused::new(StatusCode::FORBIDDEN, format!("The role {} is off for the person", role.role.name))
          .with_code("role-off"),
      );
    }

    let held = own_agents::held(ground, person).await?;

    if held.iter().filter(|agent| agent.role == role.role.name).count()
      >= ORGANISATION.composition.most_agents
    {
      return Err(
        Refused::new(
          StatusCode::CONFLICT,
          format!("A person holds at most {} agents of one role.", ORGANISATION.composition.most_agents),
        )
        .with_code("too-many-agents"),
      );
    }

    let road = match own_agents::road_asked(&body)? {
      Some(road) => {
        own_agents::may_choose_road(ground, person)
          .await?
          .required("road-choice-withheld", "choose a road")?;
        own_agents::carried(ground, person, road.as_ref()).await?;
        road
      }
      None => role.template.as_ref().and_then(own_agents::road_of),
    };
    let name = own_agents::next_name(ground, &role, person).await?;
    let agent = own_agents::seat(ground, &role, name, person, road.as_ref()).await?;

    role_agents::write(call.store(), &agent).await?;
    seats_changed(call.store(), call.events(), &agent.role).await?;
    queue_told(&call);

    Ok(answer(
      StatusCode::CREATED,
      agent_view(Some(&agent), None, role.role.follows_asker, &[]),
    ))
  }
}

crate::moved!(OwnAgentCreate);

// The person's agent the path names, with its role as the person's supply says it and its seat where it is one.
async fn theirs(
  ground: &Ground,
  name: &str,
  person: i64,
) -> Result<(RoleAgent, RoleSupply, Option<Supplied>), Refused> {
  let agent = role_agents::named(&ground.db, name)
    .await?
    .filter(|agent| agent.owner_account.as_deref() == Some(person.to_string().as_str()))
    .ok_or_else(|| not_theirs(name))?;
  let role = role_named(ground, person, &agent.role).await?;
  let seat = role.supplied.iter().find(|seat| seat.name == name).cloned();

  Ok((agent, role, seat))
}

/// `PATCH /own-agents/{ownerAccount}/{agentName}`: an agent of the person's laid on another road, or switched on
/// or off.
pub struct OwnAgentChange;

impl Route for OwnAgentChange {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/own-agents/{ownerAccount}/{agentName}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let person = account_of(&call)?;
    let body = call.body().await?;
    let ground = call.ground();
    let name = call.param("agentName").unwrap_or_default().to_owned();
    let (mut agent, role, seat) = theirs(ground, &name, person).await?;

    if let Some(road) = own_agents::road_asked(&body)? {
      own_agents::may_choose_road(ground, person)
        .await?
        .required("road-choice-withheld", "choose a road")?;
      own_agents::carried(ground, person, road.as_ref()).await?;
      agent = own_agents::laid(ground, &role.role, agent, road.as_ref()).await?;
    }

    if let Some(enabled) = body.get("enabled") {
      let enabled = enabled.as_bool().ok_or_else(|| {
        Refused::new(StatusCode::UNPROCESSABLE_ENTITY, "enabled: true or false.").with_field("enabled")
      })?;

      // A supplied agent switched off changes the set the organisation supplies; one switched back on returns to it.
      if !enabled && seat.is_some() {
        own_agents::may_add(ground, person)
          .await?
          .required("own-agents-withheld", "switch a supplied agent off")?;
      }

      agent.enabled = enabled;
    }

    role_agents::write(call.store(), &agent).await?;
    seats_changed(call.store(), call.events(), &agent.role).await?;
    queue_told(&call);

    let carrying = role_agents::carrying(call.store(), &agent.role).await?;

    Ok(answer(
      StatusCode::OK,
      agent_view(Some(&agent), seat.as_ref(), role.role.follows_asker, &carrying),
    ))
  }
}

crate::moved!(OwnAgentChange);

/// `POST /own-agents/{ownerAccount}/{agentName}/back`: a seat supplied to the person stands again as supplied — on,
/// on the road supplied — made where it no longer stands. Open whatever the chain withholds, since it only takes the
/// person's own change away.
pub struct OwnAgentBack;

impl Route for OwnAgentBack {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/own-agents/{ownerAccount}/{agentName}/back";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let person = account_of(&call)?;
    let name = call.param("agentName").unwrap_or_default().to_owned();
    let ground = call.ground();
    let supply = own_agents::supply(ground, person).await?;
    let (role, seat) = supply
      .iter()
      .find_map(|role| role.supplied.iter().find(|seat| seat.name == name).map(|seat| (role, seat.clone())))
      .ok_or_else(|| {
        Refused::new(StatusCode::NOT_FOUND, format!("No seat supplied to the person is named {name}."))
          .with_code("not-supplied")
      })?;
    let agent = match role_agents::named(call.store(), &name).await? {
      Some(agent) if agent.owner_account.as_deref() == Some(person.to_string().as_str()) => {
        let mut agent = own_agents::laid(ground, &role.role, agent, seat.road.as_ref()).await?;

        agent.enabled = true;
        agent
      }
      Some(_) => return Err(not_theirs(&name)),
      None => own_agents::seat(ground, role, name, person, seat.road.as_ref()).await?,
    };

    role_agents::write(call.store(), &agent).await?;
    seats_changed(call.store(), call.events(), &agent.role).await?;
    queue_told(&call);

    let carrying = role_agents::carrying(call.store(), &agent.role).await?;

    Ok(answer(
      StatusCode::OK,
      agent_view(Some(&agent), Some(&seat), role.role.follows_asker, &carrying),
    ))
  }
}

crate::moved!(OwnAgentBack);

/// `DELETE /own-agents/{ownerAccount}/{agentName}`: an agent of the person's own removed. A seat supplied to them is
/// switched off, never removed.
pub struct OwnAgentDelete;

impl Route for OwnAgentDelete {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/own-agents/{ownerAccount}/{agentName}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let person = account_of(&call)?;
    let name = call.param("agentName").unwrap_or_default().to_owned();
    let (agent, _, seat) = theirs(call.ground(), &name, person).await?;

    if seat.is_some() {
      return Err(
        Refused::new(
          StatusCode::CONFLICT,
          format!("{} is supplied by the organisation; switch it off instead.", agent.name),
        )
        .with_code("supplied-agent"),
      );
    }

    if role_agents::carrying(call.store(), &agent.role).await?.contains(&agent.name) {
      return Err(
        Refused::new(StatusCode::CONFLICT, format!("{} carries a run now.", agent.name))
          .with_code("agent-busy"),
      );
    }

    role_agents::remove(call.store(), &agent.name).await?;
    queue_told(&call);

    Ok(StatusCode::NO_CONTENT.into_response())
  }
}

crate::moved!(OwnAgentDelete);

// --- The person's instructions ---

// The person's agents an instruction may name: their supervisors and their role agents, each with its label.
async fn named_agents(ground: &Ground, person: i64) -> Result<Vec<Value>, Refused> {
  let mut out: Vec<Value> = crate::supervisors::every(&ground.db)
    .await?
    .into_iter()
    .filter(|supervisor| supervisor.owner_account == Some(person))
    .map(|supervisor| json!({ "name": supervisor.name, "label": supervisor.label, "kind": "supervisor" }))
    .collect();

  out.extend(
    own_agents::held(ground, person).await?.into_iter().map(
      |agent| json!({ "name": agent.name, "label": Value::Null, "role": agent.role, "kind": "agent" }),
    ),
  );

  Ok(out)
}

async fn instructions_view(ground: &Ground, person: i64) -> Result<Value, Refused> {
  let service = crate::apps::service(ground);
  let words = &PROMPT_LAYERS;
  let unchecked = |_| crate::organisation::unchecked();
  let (allowed, text, organisation) = futures_util::join!(
    own_agents::permission(ground, &words.personal.allowed.key, person),
    chain::resolve(&service, &words.personal.text.key, Subject::Person(person)),
    chain::resolve(&service, &words.organisation.kind.key, Subject::Person(person)),
  );
  let allowed = allowed?;
  let kept =
    crate::prompt_layers::personal_of(text.map_err(unchecked)?.as_ref().map(|answer| &answer["value"]));
  let agents = named_agents(ground, person).await?;
  let names: Vec<&str> = agents.iter().filter_map(|agent| agent["name"].as_str()).collect();
  let own: BTreeMap<&String, &String> =
    kept.agents.iter().filter(|(name, _)| names.contains(&name.as_str())).collect();
  let organisation: Vec<Value> = organisation
    .map_err(unchecked)?
    .as_ref()
    .and_then(|answer| answer["layers"].as_array().cloned())
    .into_iter()
    .flatten()
    .filter(|layer| layer["value"].as_str().is_some_and(|text| !text.trim().is_empty()))
    .map(|layer| json!({ "from": layer["link"], "text": layer["value"] }))
    .collect();

  Ok(json!({
    "allowed": allowed.view(),
    "all": kept.all,
    "agents": own,
    "named": agents,
    "organisation": organisation,
    "mostCharacters": ORGANISATION.own_agents.most_characters,
  }))
}

/// `GET /own-instructions/{ownerAccount}`: the person's own instructions, whether they may give any, the agents
/// they may give one to, and the organisation's, which stand before them.
pub struct OwnInstructionsRead;

impl Route for OwnInstructionsRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/own-instructions/{ownerAccount}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let person = account_of(&call)?;

    Ok(answer(StatusCode::OK, instructions_view(call.ground(), person).await?))
  }
}

crate::moved!(OwnInstructionsRead);

fn bad_instructions(error: impl Into<String>, field: &str) -> Refused {
  Refused::new(StatusCode::UNPROCESSABLE_ENTITY, error)
    .with_code("bad-instructions")
    .with_field(field)
}

// One instruction as the body gives it: a text of at most the policy's characters, an empty one none.
fn instruction(value: Option<&Value>, field: &str) -> Result<Option<String>, Refused> {
  match value {
    None | Some(Value::Null) => Ok(None),
    Some(Value::String(text)) if text.trim().is_empty() => Ok(None),
    Some(Value::String(text)) if text.chars().count() > ORGANISATION.own_agents.most_characters => {
      Err(bad_instructions(
        format!("An instruction holds at most {} characters.", ORGANISATION.own_agents.most_characters),
        field,
      ))
    }
    Some(Value::String(text)) => Ok(Some(text.trim().to_owned())),
    Some(_) => Err(bad_instructions("An instruction is a text, or null for none.", field)),
  }
}

/// `PUT /own-instructions/{ownerAccount}`: the person's own instructions replaced whole, `all` and `agents` by name,
/// written on their own link of the chain in their name.
pub struct OwnInstructionsWrite;

impl Route for OwnInstructionsWrite {
  const METHOD: Method = Method::PUT;
  const PATH: &'static str = "/own-instructions/{ownerAccount}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let person = account_of(&call)?;
    let body = call.body().await?;
    let words = &PROMPT_LAYERS;

    own_agents::permission(call.ground(), &words.personal.allowed.key, person)
      .await?
      .required("instructions-withheld", "give their agents instructions of their own")?;

    let all = instruction(body.get("all"), "all")?;
    let named = named_agents(call.ground(), person).await?;
    let mut agents = BTreeMap::new();

    match body.get("agents") {
      None | Some(Value::Null) => {}
      Some(Value::Object(given)) => {
        for (name, text) in given {
          if !named.iter().any(|agent| agent["name"] == name.as_str()) {
            return Err(
              Refused::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("{name} is no agent of the person's."),
              )
              .with_code("unknown-agent")
              .with_field("agents"),
            );
          }

          if let Some(text) = instruction(Some(text), "agents")? {
            agents.insert(name.clone(), text);
          }
        }
      }
      Some(_) => {
        return Err(bad_instructions("agents: an object from an agent's name to its text.", "agents"));
      }
    }

    let value = if all.is_none() && agents.is_empty() {
      Value::Null
    } else {
      json!({ "all": all, "agents": agents })
    };
    let service = crate::apps::service(call.ground());
    let written =
      chain::write_value(&service, &words.personal.text.key, &format!("person:{person}"), person, value)
        .await
        .map_err(|_| crate::organisation::unchecked())?;

    if !written.ok() {
      let status = StatusCode::from_u16(written.status).unwrap_or(StatusCode::BAD_GATEWAY);
      let error = written.body["error"]
        .as_str()
        .unwrap_or("The account service refused the instructions")
        .to_owned();
      let refused = Refused::new(status, error);

      return Err(match written.body["code"].as_str() {
        Some(code) => refused.with_code(code),
        None => refused,
      });
    }

    Ok(answer(StatusCode::OK, instructions_view(call.ground(), person).await?))
  }
}

crate::moved!(OwnInstructionsWrite);
