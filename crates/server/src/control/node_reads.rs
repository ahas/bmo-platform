#![allow(
  dead_code,
  unused_imports,
  unused_variables,
  clippy::needless_lifetimes,
  clippy::let_and_return,
  clippy::unnecessary_to_owned
)]
//! The nodes as a job's run reads them across the boundary (docs/api.md, 「Workers」): the node service's
//! reads handed on as they stand — the nodes, and how SSH reaches one — and where the copy of a checkout on
//! a node is kept on this machine, the shadow the node's link keeps of it.

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use bmo_chat::{ErrorCode, WireError};
use serde_json::json;

use super::{Call, Refused, Route, answer};
use crate::peers::PeerFault;
use crate::watches::links::checkout_shadow;

// A node service that could not answer, refused with its words.
fn unanswered(fault: PeerFault) -> Refused {
  Refused::new(StatusCode::BAD_GATEWAY, fault.to_string()).with_code("upstream-failed")
}

fn name_of(call: &Call) -> Result<String, Refused> {
  call
    .param("name")
    .filter(|name| !name.is_empty())
    .map(str::to_owned)
    .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "a node's name is required"))
}

/// Why a node takes no job, or none where it takes them (docs/work-nodes.md, 「Which nodes take jobs」).
fn jobs_refusal(node: &crate::peers::node::NodeRead) -> Option<String> {
  crate::verbs::work_node::work_refusal(&node.name, node.kind.over_ssh(), node.role.as_deref())
}

/// Why a repository may not stand on, or keep a checkout on, the node named, or none where it may: the node
/// service keeps it and it takes jobs.
pub async fn placing_refusal(
  ground: &crate::places::Ground,
  node: &str,
) -> Result<Option<String>, Refused> {
  let nodes = ground.peers.node.node_reads().await.map_err(unanswered)?;

  Ok(match nodes.iter().find(|held| held.name == node) {
    None => Some(format!("The node service keeps no node named {node}.")),
    Some(held) => jobs_refusal(held),
  })
}

/// `GET /nodes`: the nodes in the node list's order, as the node service reads them, each with whether it
/// takes jobs.
pub struct ListNodes;

impl Route for ListNodes {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/nodes";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let nodes = call.ground().peers.node.node_reads().await.map_err(unanswered)?;
    let items: Vec<serde_json::Value> = nodes
      .iter()
      .map(|node| {
        let refusal = jobs_refusal(node);

        json!({
          "name": node.name,
          "label": node.label,
          "kind": node.kind.word(),
          "role": node.role,
          "state": node.state,
          "checkedAt": node.checked_at,
          "takesJobs": refusal.is_none(),
          "jobsRefusal": refusal,
        })
      })
      .collect();

    Ok(answer(StatusCode::OK, json!({ "items": items, "more": false })))
  }
}

crate::moved!(ListNodes);

/// How SSH reaches a node, or nothing held where it has no SSH reach.
pub struct NodeReach;

impl Route for NodeReach {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/nodes/{name}/reach";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let name = name_of(&call)?;

    Ok(match call.ground().peers.node.ssh_reach(&name).await.map_err(unanswered)? {
      Some(reach) => answer(StatusCode::OK, json!(reach)),
      None => {
        WireError::new(ErrorCode::NotFound, format!("{name} has no SSH reach the node service holds"))
          .into_response()
      }
    })
  }
}

/// Where the copy of a checkout on a node is kept on this machine.
pub struct NodeCopy;

impl Route for NodeCopy {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/nodes/{name}/copy";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let name = name_of(&call)?;
    let checkout = call
      .query("checkout")
      .filter(|checkout| !checkout.is_empty())
      .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "checkout is required"))?;
    let path = checkout_shadow(&call.ground().roots.data, &name, &checkout);

    Ok(answer(StatusCode::OK, json!({ "path": path.to_string_lossy() })))
  }
}
