//! The nodes' workspaces: each labelled by its node and its state, and `nodes-moved` published once the
//! node service's list moves under them.

use std::sync::{Arc, Mutex};

use agency_server::Service;
use agency_server::config::Config;
use agency_server::peers::Asking;
use agency_server::peers::node::{NodeKind, NodeRow, NodeService};
use agency_server::places::Ground;
use agency_server::places::nodes::{NodeList, NodesMoved, label, settle};
use harness_providers::Machine;
use policy::node_states::NodeState;
use sea_orm::DatabaseConnection;

fn row(node: &str, label: Option<&str>, state: &str) -> NodeRow {
  NodeRow {
    node: node.into(),
    label: label.map(str::to_owned),
    kind: NodeKind::Ssh,
    state: state.into(),
    detail: None,
  }
}

/// A node service whose list is what the test last set.
#[derive(Default)]
struct Listed(Mutex<Vec<NodeRow>>);

impl NodeService for Listed {
  fn link_up<'a>(&'a self, _node: &'a str) -> Asking<'a, bool> {
    Box::pin(async { Ok(false) })
  }

  fn node_rows(&self) -> Asking<'_, Vec<NodeRow>> {
    let rows = self.0.lock().unwrap().clone();

    Box::pin(async move { Ok(rows) })
  }

  fn ssh_login<'a>(&'a self, _node: &'a str) -> Asking<'a, Option<Vec<String>>> {
    Box::pin(async { Ok(None) })
  }
}

#[test]
fn a_node_is_labelled_by_its_label_or_its_name_and_its_state() {
  let said = |state: &str| NodeState::of_column(state).said().word.clone();

  assert_eq!(label(&row("n1", Some("Bench"), "connected")), format!("Bench · {}", said("connected")));
  assert_eq!(label(&row("n1", None, "auth")), format!("n1 · {}", said("auth")));
}

#[test]
fn the_list_moves_on_a_node_added_relabelled_reordered_or_changing_state_and_not_on_its_first_reading() {
  let list = NodeList::default();
  let first = [row("a", Some("A"), "connected"), row("b", None, "connected")];

  assert!(!list.moved(&first));
  assert!(!list.moved(&first));
  assert!(list.moved(&[row("a", Some("A"), "auth"), row("b", None, "connected")]));
  assert!(list.moved(&[row("b", None, "connected"), row("a", Some("A"), "auth")]));
  assert!(list.moved(&[row("b", Some("B"), "connected"), row("a", Some("A"), "auth")]));
  assert!(list.moved(&[row("b", Some("B"), "connected")]));
}

#[tokio::test]
async fn nodes_moved_is_published_when_the_node_services_list_moves() {
  let mut service = Service::new(DatabaseConnection::Disconnected, Config::read(|_| None).unwrap());
  let listed = Arc::new(Listed::default());

  service.peers.node = listed.clone();

  let ground = Ground::new(&service, &Machine::with_env([], "/data"), None);
  let mut moved = ground.events.subscribe::<NodesMoved>();
  let list = NodeList::default();

  *listed.0.lock().unwrap() = vec![row("a", None, "connected")];
  settle(&ground, &list).await;
  assert_eq!(moved.ready(), None, "the first reading has nothing to have moved from");

  settle(&ground, &list).await;
  assert_eq!(moved.ready(), None);

  *listed.0.lock().unwrap() = vec![row("a", None, "unreachable")];
  settle(&ground, &list).await;
  assert_eq!(moved.ready(), Some(NodesMoved));
}
