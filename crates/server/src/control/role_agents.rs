//! A role's agents as the owner lists, adds, changes and removes them (docs/role-agents.md, 「Changing the
//! agents」; docs/api.md, 「role agents」): each a seat of a role with the one road its runs go out on. A field
//! is judged before anything is written.

use std::sync::LazyLock;

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use policy::control_words::CONTROL_WORDS;
use policy::roles::ROLES;
use regex::Regex;
use serde_json::{Value, json};

use super::claude_accounts::Judging;
use super::{Call, Refused, Route, answer, whole};
use crate::access::{self, Acting};
use crate::agent_roles::{self, AgentRole};
use crate::now_ms;
use crate::order::{self, Empty};
use crate::order_run::dispatch;
use crate::places::Ground;
use crate::role_agents::{self, RoleAgent};

static NAME: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(&ROLES.name_pattern).expect("the policy's role name pattern reads"));

fn unprocessable(error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::UNPROCESSABLE_ENTITY, error)
}

fn unknown(name: &str) -> Refused {
  Refused::new(StatusCode::NOT_FOUND, format!("No agent is named {name}."))
}

/// An agent as the routes answer it: every field of its row, whether a run a supervisor asks of it goes out on that
/// supervisor's road, its role following where `role_follows` says so, and whether it carries a slot request now.
pub fn view(agent: &RoleAgent, role_follows: bool, carrying: &[String]) -> Value {
  let mut shown = serde_json::to_value(agent).unwrap_or(Value::Null);

  shown["followsSupervisor"] = json!(agent.follows(role_follows));
  shown["carries"] = json!(carrying.contains(&agent.name));

  shown
}

// A field that is text or null: none where the body clears it or names nothing.
fn text(value: &Value, field: &str) -> Result<Option<String>, Refused> {
  match value {
    Value::Null => Ok(None),
    Value::String(held) if held.trim().is_empty() => Ok(None),
    Value::String(held) => Ok(Some(held.trim().to_owned())),
    _ => Err(unprocessable(format!("{field}: text, or null for none."))),
  }
}

// A field that is a list of names or null.
fn names(value: &Value, field: &str) -> Result<Option<Vec<String>>, Refused> {
  match value {
    Value::Null => Ok(None),
    Value::Array(listed) => listed
      .iter()
      .map(|one| one.as_str().map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned))
      .collect::<Option<Vec<String>>>()
      .map(Some)
      .ok_or_else(|| unprocessable(format!("{field}: a list of names, or null for none."))),
    _ => Err(unprocessable(format!("{field}: a list of names, or null for none."))),
  }
}

/// The body's fields laid over an agent, each judged: its road against its role's line.
pub(crate) async fn judged(
  ground: &Ground,
  role: &AgentRole,
  held: RoleAgent,
  body: &Value,
) -> Result<RoleAgent, Refused> {
  let held_road = (held.provider.clone(), held.model.clone());
  let mut agent = held;

  // The account whose seat it is, a login account's id written as digits, or none for the deployment's own.
  if let Some(account) = body.get("ownerAccount") {
    agent.owner_account = match account {
      Value::Number(number) => Some(number.to_string()),
      other => text(other, "ownerAccount")?,
    };
  }

  if let Some(provider) = body.get("provider") {
    agent.provider = text(provider, "provider")?;
  }

  if let Some(model) = body.get("model") {
    agent.model = text(model, "model")?;
  }

  if let Some(effort) = body.get("effort") {
    agent.effort = text(effort, "effort")?;
  }

  if let Some(thinking) = body.get("thinking") {
    agent.thinking = text(thinking, "thinking")?;
  }

  // An advisor was the old model's: a road moved to another provider or model and naming none keeps none.
  if let Some(advisor) = body.get("advisor") {
    agent.advisor = text(advisor, "advisor")?;
  } else if (agent.provider.clone(), agent.model.clone()) != held_road {
    agent.advisor = None;
  }

  if let Some(tokens) = body.get("answerTokens") {
    agent.answer_tokens = match tokens {
      Value::Null => None,
      tokens => Some(
        whole(tokens)
          .filter(|tokens| *tokens > 0)
          .ok_or_else(|| unprocessable("answerTokens: a whole number above zero, or null for none."))?,
      ),
    };
  }

  if let Some(enabled) = body.get("enabled") {
    agent.enabled = enabled.as_bool().ok_or_else(|| unprocessable("enabled: true or false."))?;
  }

  if let Some(sessions) = body.get("sessions") {
    agent.sessions = sessions
      .as_str()
      .and_then(policy::roles::Sessions::of)
      .ok_or_else(|| unprocessable(CONTROL_WORDS.agent_roles.sessions.as_str()))?;
  }

  if let Some(keeps) = body.get("keepsCache") {
    agent.keeps_cache = keeps.as_bool().ok_or_else(|| unprocessable("keepsCache: true or false."))?;
  }

  // Whether a run a supervisor asks of it goes out on that supervisor's road, its own road standing where none
  // does; null gives its role's setting back (docs/role-agents.md, 「An agent that follows its asker」).
  if let Some(follows) = body.get("followsAsker") {
    agent.follows_asker = match follows {
      Value::Null => None,
      Value::Bool(on) => Some(*on),
      _ => return Err(unprocessable("followsAsker: true or false, or null for its role's.")),
    };
  }

  if let Some(ratio) = body.get("foldRatio") {
    agent.fold_ratio = match ratio {
      Value::Null => None,
      ratio => Some(
        ratio
          .as_f64()
          .filter(|ratio| *ratio > 0.0 && *ratio <= 1.0)
          .ok_or_else(|| unprocessable("foldRatio: a share above 0 and at most 1, or null for none."))?,
      ),
    };
  }

  if let Some(prompt) = body.get("prompt") {
    agent.prompt = text(prompt, "prompt")?;
  }

  if let Some(bundles) = body.get("bundles") {
    agent.bundles = names(bundles, "bundles")?;

    if let Some(unknown) = agent
      .bundles
      .iter()
      .flatten()
      .find(|named| !ROLES.bundles.iter().any(|bundle| &bundle.name == *named))
    {
      return Err(unprocessable(format!("bundles: {unknown} is no bundle the policy declares.")));
    }
  }

  if let Some(skills) = body.get("skills") {
    agent.skills = names(skills, "skills")?;

    let carried = crate::lines::agent_plugin::carried(&crate::lines::Trees::of(ground));

    if let Some(unknown) = agent.skills.iter().flatten().find(|named| !carried.contains(*named)) {
      return Err(unprocessable(format!(
        "skills: {unknown} is no skill a plugin of this release carries."
      )));
    }
  }

  // Its road is a provider and a model together, or neither; a road named is judged as an order's entry is.
  match (&agent.provider, &agent.model) {
    (None, None) => {
      agent.effort = None;
      agent.thinking = None;
      agent.answer_tokens = None;

      if agent.advisor.is_some() {
        return Err(unprocessable(
          "advisor: an agent following its supervisor consults the supervisor's advisor; name a road first.",
        ));
      }
    }
    (Some(provider), Some(model)) => {
      let entry = json!([{
        "provider": provider,
        "model": model,
        "effort": agent.effort,
        "thinking": agent.thinking,
        "answerTokens": agent.answer_tokens,
        "advisor": agent.advisor,
      }]);

      order::judge(Judging::of(ground), &ground.machine(), &role.line, &entry, Empty::Refused)
        .await
        .map_err(|refusal| {
          let error = if refusal.field == order::ADVISOR {
            format!("advisor: {}", refusal.error)
          } else {
            refusal.error
          };

          unprocessable(error)
        })?;
    }
    _ => {
      return Err(unprocessable("An agent's road names its provider together with its model, or neither."));
    }
  }

  Ok(agent)
}

// The account an agent a member adds or changes is filed under (docs/api.md, 「Who a caller acts for」): their own,
// a body naming another refused. The platform's call keeps the owner the body gives.
fn filed(acting: Acting, agent: &mut RoleAgent) -> Result<(), Refused> {
  if acting == Acting::Platform {
    return Ok(());
  }

  let named = agent.owner_account.as_deref().map(access::seat_owner);
  let owner = acting.files_under(named, &format!("the role agent {}", agent.name))?;

  agent.owner_account = owner.map(|owner| owner.to_string());

  Ok(())
}

/// A role whose seats changed lets in at once the reserved slots a seat more frees.
pub(crate) async fn seats_changed(
  db: &sea_orm::DatabaseConnection,
  events: &crate::events::Bus,
  role: &str,
) -> Result<(), Refused> {
  let promoted = dispatch::promote_role(db, role, now_ms()).await.map_err(Refused::failed)?;

  for request in &promoted {
    dispatch::announce(events, request);
  }

  Ok(())
}

/// `GET /role-agents`: every agent, by role and name, narrowed to one role's by `role`.
pub struct RoleAgentList;

impl Route for RoleAgentList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/role-agents";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let role = call.query("role");
    let carrying = role_agents::carrying_any(call.store()).await?;
    let following: Vec<String> = agent_roles::every(call.store())
      .await?
      .into_iter()
      .filter(|held| held.follows_asker)
      .map(|held| held.name)
      .collect();
    let items: Vec<Value> = role_agents::every(call.store())
      .await?
      .iter()
      .filter(|agent| role.as_deref().is_none_or(|role| agent.role == role))
      .map(|agent| view(agent, following.contains(&agent.role), &carrying))
      .collect();

    Ok(answer(StatusCode::OK, json!({ "items": items })))
  }
}

crate::moved!(RoleAgentList);

/// `POST /role-agents`: an agent added to a role.
pub struct RoleAgentCreate;

impl Route for RoleAgentCreate {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/role-agents";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let db = call.store();
    let role = body.get("role").and_then(Value::as_str).map(str::trim).unwrap_or_default().to_owned();
    let Some(held) = agent_roles::named(db, &role).await? else {
      return Err(unprocessable(agent_roles::unknown(&role)).with_code("unknown-role"));
    };
    let name = match body.get("name").map(|name| text(name, "name")).transpose()?.flatten() {
      Some(name) => name,
      None => role_agents::next_name(db, &role).await?,
    };

    if !NAME.is_match(&name) {
      return Err(unprocessable("name: lower-case letters, digits and hyphens, as a role's name is."));
    }

    if role_agents::named(db, &name).await?.is_some() {
      return Err(Refused::new(StatusCode::CONFLICT, format!("An agent is already named {name}.")));
    }

    let fresh = RoleAgent {
      name,
      role,
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
      added_at: now_ms(),
      owner_account: None,
      follows_asker: None,
    };
    let mut agent = judged(call.ground(), &held, fresh, &body).await?;
    let acting = Acting::of(&call)?;

    // A role the person's composition turns off takes no agent of theirs (docs/role-agents.md, 「The agents an
    // account is given」).
    if let Acting::Person(person) = acting
      && crate::organisation::composition::role_off(call.ground(), person, &agent.role).await
    {
      return Err(
        Refused::new(
          StatusCode::FORBIDDEN,
          format!("The role {} is off for the signed-in person", agent.role),
        )
        .with_code("role-off"),
      );
    }

    filed(acting, &mut agent)?;
    role_agents::write(db, &agent).await?;
    seats_changed(db, call.events(), &agent.role).await?;
    queue_told(&call);

    Ok(answer(StatusCode::CREATED, view(&agent, held.follows_asker, &[])))
  }
}

crate::moved!(RoleAgentCreate);

/// `PATCH /role-agents/{agentName}`: any field of an agent changed.
pub struct RoleAgentChange;

impl Route for RoleAgentChange {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/role-agents/{agentName}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let name = call.param("agentName").unwrap_or_default().to_owned();
    let db = call.store();
    let held = role_agents::named(db, &name).await?.ok_or_else(|| unknown(&name))?;
    let role = agent_roles::named(db, &held.role)
      .await?
      .ok_or_else(|| unprocessable(agent_roles::unknown(&held.role)))?;
    let mut agent = judged(call.ground(), &role, held, &body).await?;

    filed(Acting::of(&call)?, &mut agent)?;
    role_agents::write(db, &agent).await?;
    seats_changed(db, call.events(), &agent.role).await?;
    queue_told(&call);

    let carrying = role_agents::carrying(db, &agent.role).await?;

    Ok(answer(StatusCode::OK, view(&agent, role.follows_asker, &carrying)))
  }
}

crate::moved!(RoleAgentChange);

/// `DELETE /role-agents/{agentName}`: an agent that carries no run, removed.
pub struct RoleAgentDelete;

impl Route for RoleAgentDelete {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/role-agents/{agentName}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let name = call.param("agentName").unwrap_or_default().to_owned();
    let db = call.store();
    let held = role_agents::named(db, &name).await?.ok_or_else(|| unknown(&name))?;

    if role_agents::carrying(db, &held.role).await?.contains(&name) {
      return Err(Refused::new(StatusCode::CONFLICT, format!("{name} carries a run now.")));
    }

    role_agents::remove(db, &name).await?;
    queue_told(&call);

    Ok(StatusCode::NO_CONTENT.into_response())
  }
}

crate::moved!(RoleAgentDelete);

/// The queue told, apart from the request, of what an agent's or a role's change leaves no launch to take, and the
/// label values published (docs/agent-control.md, 「A label agency does not hold is refused as it is set」).
pub fn queue_told(call: &Call) {
  let ground = call.shared_ground();

  tokio::spawn(async move { crate::watches::queue_idle::agents_changed(&ground.db, ground.vars()).await });
}
