//! An account's node across the boundary (docs/api.md, 「person-nodes」): the record of the node an account's runs
//! stand on, read with whether its link is up, set by the service that binds the account to its machine, and taken
//! away.

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::{Call, Refused, Route, answer};
use crate::peers::PeerFault;
use crate::person_nodes::{self, PersonNode};
use crate::watches::links::link_heard;

fn account_of(call: &Call) -> Result<String, Refused> {
  call
    .param("ownerAccount")
    .map(str::trim)
    .filter(|account| !account.is_empty())
    .map(str::to_owned)
    .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "an account is required"))
}

fn unanswered(fault: PeerFault) -> Refused {
  Refused::new(StatusCode::BAD_GATEWAY, fault.to_string()).with_code("upstream-failed")
}

/// A record as the boundary reads it: with whether its node's link is up now, as a run of the account judges it.
fn read(call: &Call, record: &PersonNode) -> Value {
  let mut read = json!(record);
  read["linked"] = json!(link_heard(&call.ground().roots.data, &record.node));
  read
}

/// `GET /person-nodes`: every account's node, by account.
pub struct ListPersonNodes;

impl Route for ListPersonNodes {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/person-nodes";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let items: Vec<Value> = person_nodes::every(call.store())
      .await?
      .iter()
      .map(|record| read(&call, record))
      .collect();

    Ok(answer(StatusCode::OK, json!({ "items": items, "more": false })))
  }
}

crate::moved!(ListPersonNodes);

/// `PUT /person-nodes/{ownerAccount}`: an account's node set, a hub node the node service keeps, shared with other
/// accounts where the body says so and theirs are shared too.
pub struct SetPersonNode;

impl Route for SetPersonNode {
  const METHOD: Method = Method::PUT;
  const PATH: &'static str = "/person-nodes/{ownerAccount}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let account = account_of(&call)?;
    let body = call.body().await?;
    let node = body
      .get("node")
      .and_then(Value::as_str)
      .map(str::trim)
      .filter(|node| !node.is_empty())
      .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "node (string) is required"))?
      .to_owned();
    let shared = match body.get("shared") {
      None | Some(Value::Null) => false,
      Some(shared) => shared
        .as_bool()
        .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "shared is a boolean").with_field("shared"))?,
    };
    let own = match body.get("own") {
      None | Some(Value::Null) => false,
      Some(own) => own
        .as_bool()
        .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "own is a boolean").with_field("own"))?,
    };

    if let Some(why) = person_nodes::refusal(call.ground().peers.node.as_ref(), &node)
      .await
      .map_err(unanswered)?
    {
      return Err(Refused::new(StatusCode::UNPROCESSABLE_ENTITY, why).with_code("not-a-hub-node"));
    }

    match person_nodes::set(call.store(), &account, &node, shared, own).await? {
      Ok(set) => Ok(answer(StatusCode::OK, read(&call, &set))),
      Err(taken) => Err(Refused::new(StatusCode::CONFLICT, taken.to_string()).with_code("node-taken")),
    }
  }
}

crate::moved!(SetPersonNode);

/// `DELETE /person-nodes/{ownerAccount}`: an account's node taken away.
pub struct ClearPersonNode;

impl Route for ClearPersonNode {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/person-nodes/{ownerAccount}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let account = account_of(&call)?;
    let cleared = match given(&call, "node") {
      Some(node) => person_nodes::clear_one(call.store(), &account, &node).await?,
      None => person_nodes::clear(call.store(), &account).await?,
    };

    if !cleared {
      return Err(
        Refused::new(StatusCode::NOT_FOUND, format!("account {account} has no node"))
          .with_code("not-found"),
      );
    }

    Ok(StatusCode::NO_CONTENT.into_response())
  }
}

crate::moved!(ClearPersonNode);

fn given(call: &Call, name: &str) -> Option<String> {
  call.query(name).map(|value| value.trim().to_owned()).filter(|value| !value.is_empty())
}

/// `GET /nodes/{nodeName}/access`: whose node it is, and whether a caller named as `agent` reaches it. The
/// `agent` may stand out: the node service's owners' read wants the owners alone and names no tool session.
pub struct NodeAccess;

impl Route for NodeAccess {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/nodes/{nodeName}/access";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let node = call
      .param("nodeName")
      .map(str::trim)
      .filter(|node| !node.is_empty())
      .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "a node is required"))?
      .to_owned();
    let agent = given(&call, "agent");
    let role = given(&call, "role");
    let checkout = given(&call, "checkout");
    let caller = person_nodes::Caller {
      agent: agent.as_deref().unwrap_or_default(),
      role: role.as_deref(),
      checkout: checkout.as_deref(),
    };
    let access = person_nodes::access(call.store(), &call.ground().roots.data, &node, caller).await?;

    Ok(answer(StatusCode::OK, json!(access)))
  }
}

crate::moved!(NodeAccess);
