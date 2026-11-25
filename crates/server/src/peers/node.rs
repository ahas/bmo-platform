//! The node service (docs/api.md, 「Calls agency makes」) through the client it publishes: what each place on
//! this machine holds, its standing and the pressures it tells, whether a node's link stands, the nodes it
//! keeps and the SSH login to each.

use std::sync::Arc;

use bmo_node_client::events::{Change, EventsQuery};
use bmo_node_client::nodes::{Node, NodeKind as PublishedKind, NodesQuery, Target};
use bmo_node_client::{CallError, NodeClient};
use futures_util::StreamExt;
use futures_util::stream::BoxStream;
use serde::{Deserialize, Serialize};

use super::{Asking, PeerAddress, PeerFault, Variables};

pub use bmo_node_client::machines::{
  Alert, KernelReading, Largest, Machine, MachineKind, Pressure, PressureLine, ScopeUse, Standing,
};

/// Where the deployment names the node service.
pub const ADDRESS: Variables = Variables {
  url: "AGENCY_NODE_URL",
  token: "AGENCY_NODE_TOKEN",
};

const PEER: &str = "node service";

// The name the service gives its own machine.
const THIS_MACHINE: &str = "local";

/// The kind of event a machine's standing and its pressures are told as.
const MACHINE_CHANGED: &str = "machine.changed";

/// How a node is reached: over SSH, or through a hub its agent attaches to, which offers no shell.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
  Ssh,
  Jake,
}

impl NodeKind {
  /// The word agency writes it as.
  pub fn word(self) -> &'static str {
    match self {
      Self::Ssh => "ssh",
      Self::Jake => "jake",
    }
  }

  /// Whether a shell, and so a job, reaches the node over SSH.
  pub fn over_ssh(self) -> bool {
    self == Self::Ssh
  }
}

/// One node as agency lists it, with the label it is shown under, how it is reached, its state and what it
/// says of it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct NodeRow {
  pub node: String,
  #[serde(default)]
  pub label: Option<String>,
  pub kind: NodeKind,
  pub state: String,
  #[serde(default)]
  pub detail: Option<String>,
}

/// One node as the node service's reads answer it, handed on as it stands.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeRead {
  pub name: String,
  pub label: String,
  pub kind: NodeKind,
  #[serde(default)]
  pub role: Option<String>,
  pub state: String,
  #[serde(default)]
  pub checked_at: Option<i64>,
}

/// How SSH reaches an ssh node, as the node service's reads answer it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SshReach {
  pub host: String,
  pub port: u16,
  pub user: String,
  #[serde(default)]
  pub key_path: Option<String>,
  #[serde(default)]
  pub shell: Option<String>,
}

/// How a hub node is reached, as the node service's reads answer it: the hub's control address, the file its
/// control token is kept in, and the name the hub knows the node's agent by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HubReach {
  pub address: String,
  pub token_file: String,
  pub agent: String,
}

/// The machines the node service tells changed, as they come; a stream that drops ends.
pub type Pressures = BoxStream<'static, Result<Machine, PeerFault>>;

fn kind_of(kind: PublishedKind) -> NodeKind {
  match kind {
    PublishedKind::Ssh => NodeKind::Ssh,
    PublishedKind::Hub => NodeKind::Jake,
  }
}

fn row_of(node: Node) -> NodeRow {
  NodeRow {
    node: node.name,
    label: Some(node.label),
    kind: kind_of(node.kind),
    state: node.state.name().to_owned(),
    detail: node.detail,
  }
}

fn read_of(node: Node) -> NodeRead {
  NodeRead {
    name: node.name,
    label: node.label,
    kind: kind_of(node.kind),
    role: node.role,
    state: node.state.name().to_owned(),
    checked_at: node.checked_at,
  }
}

fn fault(why: CallError) -> PeerFault {
  match why {
    CallError::Answered { status, error } => PeerFault::Failed {
      peer: PEER,
      status,
      code: error.code.as_str().to_owned(),
      error: error.error,
    },
    CallError::Unreadable { status, body } => PeerFault::Malformed {
      peer: PEER,
      detail: format!("{status}: {body}"),
    },
    CallError::Transport(why) => PeerFault::Unreachable {
      peer: PEER,
      detail: why.to_string(),
    },
  }
}

/// What agency asks the node service.
pub trait NodeService: Send + Sync {
  /// Whether a node's link is connected and was heard from lately.
  fn link_up<'a>(&'a self, node: &'a str) -> Asking<'a, bool>;
  /// Every node the service keeps, in the node list's order.
  fn node_rows(&self) -> Asking<'_, Vec<NodeRow>>;
  /// The arguments that open a batch SSH login to a node, ahead of the command, or none where the node
  /// has no SSH route.
  fn ssh_login<'a>(&'a self, node: &'a str) -> Asking<'a, Option<Vec<String>>>;
  /// The nodes as the service's reads answer them, in the node list's order; none by default.
  fn node_reads(&self) -> Asking<'_, Vec<NodeRead>> {
    Box::pin(async { Ok(vec![]) })
  }
  /// How SSH reaches a node, or none where it has no SSH reach or is not listed; none by default.
  fn ssh_reach<'a>(&'a self, _node: &'a str) -> Asking<'a, Option<SshReach>> {
    Box::pin(async { Ok(None) })
  }
  /// How a hub reaches a node, or none where it is no hub node or is not listed; none by default.
  fn hub_reach<'a>(&'a self, _node: &'a str) -> Asking<'a, Option<HubReach>> {
    Box::pin(async { Ok(None) })
  }
  /// This machine's standing as the service last read it; nothing read by default.
  fn machine_standing(&self) -> Asking<'_, Standing> {
    Box::pin(async { Ok(Standing::unread()) })
  }
  /// The machines the service tells changed from now on; by default a stream that tells none and never
  /// ends.
  fn pressures(&self) -> Asking<'_, Pressures> {
    Box::pin(async { Ok(futures_util::stream::pending().boxed()) })
  }
  /// A node taken off the service's list, answering whether it stood there; nothing taken by default.
  fn remove_node<'a>(&'a self, _node: &'a str) -> Asking<'a, bool> {
    Box::pin(async { Ok(false) })
  }
}

/// No node service deployed: no link stands, no node is kept, none is logged in to, this machine is
/// unread and no change is told.
pub struct NoNodeService;

impl NodeService for NoNodeService {
  fn link_up<'a>(&'a self, _node: &'a str) -> Asking<'a, bool> {
    Box::pin(async { Ok(false) })
  }

  fn node_rows(&self) -> Asking<'_, Vec<NodeRow>> {
    Box::pin(async { Ok(vec![]) })
  }

  fn ssh_login<'a>(&'a self, _node: &'a str) -> Asking<'a, Option<Vec<String>>> {
    Box::pin(async { Ok(None) })
  }
}

/// The node service asked through its published client.
pub struct PublishedNode {
  calls: NodeClient,
}

impl PublishedNode {
  pub fn new(address: PeerAddress) -> Result<Self, String> {
    let token = address.token.clone().unwrap_or_default();

    Ok(Self {
      calls: NodeClient::new(address.url.as_str(), token)?,
    })
  }

  async fn listed(&self) -> Result<Vec<Node>, PeerFault> {
    let mut nodes = vec![];
    let mut after: Option<String> = None;

    loop {
      let page = self
        .calls
        .nodes(&NodesQuery {
          after: after.clone(),
          limit: None,
        })
        .await
        .map_err(fault)?;
      let more = page.more;

      after = page.items.last().map(|node| node.name.clone());
      nodes.extend(page.items);

      if !more || after.is_none() {
        return Ok(nodes);
      }
    }
  }

  async fn target(&self, node: &str) -> Result<Option<Target>, PeerFault> {
    match self.calls.target(node).await {
      Ok(target) => Ok(Some(target)),
      Err(CallError::Answered { status: 404, .. }) => Ok(None),
      Err(why) => Err(fault(why)),
    }
  }
}

impl NodeService for PublishedNode {
  fn remove_node<'a>(&'a self, node: &'a str) -> Asking<'a, bool> {
    Box::pin(async move {
      match self.calls.delete_node(node).await {
        Ok(()) => Ok(true),
        Err(CallError::Answered { status: 404, .. }) => Ok(false),
        Err(why) => Err(fault(why)),
      }
    })
  }

  fn link_up<'a>(&'a self, node: &'a str) -> Asking<'a, bool> {
    Box::pin(async move {
      match self.calls.node(node).await {
        Ok(node) => Ok(node.up),
        Err(CallError::Answered { status: 404, .. }) => Ok(false),
        Err(why) => Err(fault(why)),
      }
    })
  }

  fn node_rows(&self) -> Asking<'_, Vec<NodeRow>> {
    Box::pin(async move { Ok(self.listed().await?.into_iter().map(row_of).collect()) })
  }

  fn ssh_login<'a>(&'a self, node: &'a str) -> Asking<'a, Option<Vec<String>>> {
    Box::pin(async move {
      let Some(target) = self.target(node).await? else {
        return Ok(None);
      };

      Ok((!target.ssh_args.is_empty()).then_some(target.ssh_args))
    })
  }

  fn node_reads(&self) -> Asking<'_, Vec<NodeRead>> {
    Box::pin(async move { Ok(self.listed().await?.into_iter().map(read_of).collect()) })
  }

  fn ssh_reach<'a>(&'a self, node: &'a str) -> Asking<'a, Option<SshReach>> {
    Box::pin(async move {
      let Some(target) = self.target(node).await? else {
        return Ok(None);
      };
      let (Some(host), Some(port), Some(user)) = (target.host, target.port, target.user) else {
        return Ok(None);
      };

      Ok(Some(SshReach {
        host,
        port,
        user,
        key_path: target.key_path,
        shell: target.shell.map(|shell| shell.name().to_owned()),
      }))
    })
  }

  fn hub_reach<'a>(&'a self, node: &'a str) -> Asking<'a, Option<HubReach>> {
    Box::pin(async move {
      let Some(target) = self.target(node).await? else {
        return Ok(None);
      };
      let (Some(address), Some(token_file), Some(agent)) =
        (target.hub_address, target.hub_token_file, target.agent_name)
      else {
        return Ok(None);
      };

      Ok(Some(HubReach {
        address,
        token_file,
        agent,
      }))
    })
  }

  fn machine_standing(&self) -> Asking<'_, Standing> {
    Box::pin(async move {
      let machine = self.calls.machine(THIS_MACHINE).await.map_err(fault)?;

      Ok(machine.standing.unwrap_or_else(Standing::unread))
    })
  }

  fn pressures(&self) -> Asking<'_, Pressures> {
    Box::pin(async move {
      let query = EventsQuery {
        kinds: Some(MACHINE_CHANGED.to_owned()),
        machine_name: Some(THIS_MACHINE.to_owned()),
        ..EventsQuery::default()
      };
      let told = self.calls.events(&query).await.map_err(fault)?.filter_map(|event| async move {
        match event {
          Ok(event) => match event.change() {
            Ok(Change::MachineChanged(machine)) => Some(Ok(*machine)),
            Ok(_) => None,
            Err(why) => Some(Err(PeerFault::Malformed {
              peer: PEER,
              detail: why.to_string(),
            })),
          },
          Err(why) => Some(Err(fault(why))),
        }
      });

      Ok(told.boxed())
    })
  }
}

/// The client the address chooses.
pub fn client(address: Option<PeerAddress>) -> Arc<dyn NodeService> {
  let Some(address) = address else {
    return Arc::new(NoNodeService);
  };

  match PublishedNode::new(address) {
    Ok(node) => Arc::new(node),
    Err(why) => {
      eprintln!("agency: the node service's address did not read: {why}");

      Arc::new(NoNodeService)
    }
  }
}
