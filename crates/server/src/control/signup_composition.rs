//! The agents an account is given as it is made ready, as the routes keep them (docs/role-agents.md, 「The agents
//! an account is given」; docs/api.md, 「roles」): the agents made for a person by the composition the chain
//! resolves for them, the composition read for a unit or a person, and a link's composition judged and written on
//! the chain in the caller's name.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use serde_json::{Value, json};

use super::role_agents::view;
use super::{Call, Refused, Route, answer};
use crate::access::Acting;
use crate::chain::{self, Subject};
use crate::organisation::composition::{self, Standing};

fn forbidden(error: &str) -> Refused {
  Refused::new(StatusCode::FORBIDDEN, error).with_code("forbidden")
}

/// `POST /composed-agents/{ownerAccount}`: the person's agents made as their composition says, the deployment's
/// own call alone; answers the agents made, none where the person holds one of every role already.
pub struct ComposedAgents;

impl Route for ComposedAgents {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/composed-agents/{ownerAccount}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    if call.header("owner-key").is_some_and(|key| !key.trim().is_empty()) {
      return Err(forbidden("An account's agents are made by the deployment's own call alone"));
    }

    let Some(user) = call.param("ownerAccount").and_then(|account| account.trim().parse::<i64>().ok())
    else {
      return Err(Refused::new(StatusCode::NOT_FOUND, "No account of that number"));
    };
    let ground = call.shared_ground();
    let made = composition::compose(&ground, user).await?;
    let mut roles: Vec<&str> = made.iter().map(|agent| agent.role.as_str()).collect();

    roles.dedup();
    for role in roles {
      for request in &crate::order_run::dispatch::promote_role(&ground.db, role, crate::now_ms())
        .await
        .map_err(Refused::failed)?
      {
        crate::order_run::dispatch::announce(call.events(), request);
      }
    }

    super::role_agents::queue_told(&call);

    let items: Vec<Value> = made.iter().map(|agent| view(agent, false, &[])).collect();

    Ok(answer(StatusCode::OK, json!({ "items": items })))
  }
}

crate::moved!(ComposedAgents);

fn standing_view(role: &str, held: Option<&Standing>) -> Value {
  json!({
    "role": role,
    "off": held.is_some_and(|held| held.off),
    "agents": held.and_then(|held| held.agents.clone()),
    "from": held.and_then(|held| held.from.clone()),
  })
}

/// `GET /signup-composition?unit=|person=`: what stands for the subject, each role with whether it is off, the
/// agents given of it and the link that said them; and, for a caller the console names, the subject's own link as
/// the chain answers it to them, its value and whether they may change it.
pub struct SignupCompositionRead;

impl Route for SignupCompositionRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/signup-composition";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let whole = |value: Option<String>| value.and_then(|value| value.trim().parse::<i64>().ok());
    let (unit, person) = (whole(call.query("unit")), whole(call.query("person")));
    let (subject, link) = match (unit, person) {
      (Some(unit), None) => (Subject::Unit(unit), format!("unit:{unit}")),
      (None, Some(person)) => (Subject::Person(person), format!("person:{person}")),
      _ => return Err(Refused::new(StatusCode::BAD_REQUEST, "Name a unit or a person, one of them")),
    };
    let caller = call.header("owner-key").and_then(|key| key.trim().parse::<i64>().ok());
    let ground = call.shared_ground();
    let answer_read = composition::resolved(&ground, subject).await?;
    let standing = composition::standing(answer_read.as_ref());
    let roles: Vec<Value> = crate::agent_roles::every(&ground.db)
      .await?
      .iter()
      .map(|role| standing_view(&role.name, standing.get(&role.name)))
      .collect();
    let own = match caller {
      Some(caller) => {
        let read = chain::entry(
          &crate::apps::service(&ground),
          &policy::organisation::ORGANISATION.composition.key,
          &link,
          caller,
        )
        .await
        .map_err(|_| crate::organisation::unchecked())?;

        read.ok().then_some(read.body)
      }
      None => None,
    };

    Ok(answer(StatusCode::OK, json!({ "subject": link, "roles": roles, "own": own })))
  }
}

crate::moved!(SignupCompositionRead);

/// `PUT /signup-composition/{link}`: a link's composition judged and written on the chain in the name of the
/// person the console names, null to follow the link above; the chain answers who may write it and whether a
/// lock above withholds it, and its answer is this route's.
pub struct SignupCompositionWrite;

impl Route for SignupCompositionWrite {
  const METHOD: Method = Method::PUT;
  const PATH: &'static str = "/signup-composition/{link}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let link = call.param("link").unwrap_or_default().to_owned();
    let Some(caller) = call.header("owner-key").and_then(|key| key.trim().parse::<i64>().ok()) else {
      return Err(forbidden("A composition is written in the name of a signed-in person"));
    };
    let _ = Acting::of(&call)?;
    let body = call.body().await?;
    let ground = call.shared_ground();
    let value = match body.get("value") {
      None | Some(Value::Null) => Value::Null,
      Some(value) => {
        serde_json::to_value(composition::judge(&ground, value).await?).map_err(Refused::failed)?
      }
    };
    let written = chain::write_value(
      &crate::apps::service(&ground),
      &policy::organisation::ORGANISATION.composition.key,
      &link,
      caller,
      value,
    )
    .await
    .map_err(|_| crate::organisation::unchecked())?;
    let status = StatusCode::from_u16(written.status).unwrap_or(StatusCode::BAD_GATEWAY);

    Ok(answer(status, written.body))
  }
}

crate::moved!(SignupCompositionWrite);
