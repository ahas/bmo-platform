//! The gate kinds the gate declares (docs/api.md, 「Gate kinds」): the order kept in the store, an empty one
//! the gate answers kept as one, the switches an agent's turn reads from it, and the reasoning a gate's name
//! does not own.

use agency_server::gate_kinds::{self, Kind};
use bmo_gate_client::Gate;

use crate::support::gates::{self, Declaring};
use crate::support::grounds::ground_on;
use crate::support::scratch::Scratch;

fn gate(name: &str, part: &str, step: i64) -> Gate {
  Gate {
    gate_name: name.into(),
    part: part.into(),
    step,
    condition: None,
    switchable: false,
    answer_tokens: None,
  }
}

fn declared() -> Vec<Gate> {
  vec![
    gate("references", "pre", 0),
    gate("screen", "pre", 1),
    gate("screen", "post", 0),
    gate("format", "post", 0),
    gate("translator", "apart", 1),
  ]
}

#[tokio::test]
async fn the_gate_s_declaration_is_kept_in_its_order_and_written_over_whole() {
  let scratch = Scratch::new().await;
  let db = scratch.db();

  gate_kinds::store(db, &declared()).await.unwrap();

  let kinds = gate_kinds::declared(db).await.unwrap();

  assert_eq!(
    kinds.iter().map(|kind| (kind.part.as_str(), kind.key.as_str())).collect::<Vec<_>>(),
    [
      ("pre", "references"),
      ("pre", "screen"),
      ("post", "screen"),
      ("post", "format"),
      ("apart", "translator"),
    ]
  );
  assert!(gate_kinds::holds(db, "translator").await.unwrap());
  assert!(!gate_kinds::holds(db, "no-such-gate").await.unwrap());

  gate_kinds::store(db, &[gate("room-rules", "pre", 0)]).await.unwrap();

  assert_eq!(
    gate_kinds::declared(db).await.unwrap(),
    vec![Kind {
      key: "room-rules".into(),
      part: "pre".into(),
      step: 0,
      condition: None,
    }]
  );
  assert!(
    !gate_kinds::holds(db, "translator").await.unwrap(),
    "a declaration replaces the one standing"
  );
}

#[tokio::test]
async fn an_empty_order_the_gate_answers_is_kept_and_a_gate_not_deployed_leaves_what_stood() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let mut ground = ground_on(scratch.db().clone(), home.path());

  gate_kinds::store(scratch.db(), &declared()).await.unwrap();
  ground.peers.gate = gates::none();
  gate_kinds::refresh(&ground).await;

  assert_eq!(gate_kinds::declared(scratch.db()).await.unwrap().len(), declared().len());

  ground.peers.gate = std::sync::Arc::new(Declaring(Vec::new()));
  gate_kinds::refresh(&ground).await;

  assert!(
    gate_kinds::declared(scratch.db()).await.unwrap().is_empty(),
    "a gate declaring that no gate stands leaves no kind"
  );
}

#[tokio::test]
async fn a_chat_turn_reads_the_declared_pre_and_post_parts_once_each_in_their_steps() {
  let scratch = Scratch::new().await;
  let db = scratch.db();

  gate_kinds::store(db, &declared()).await.unwrap();

  let turn = gate_kinds::turn(&gate_kinds::declared(db).await.unwrap());
  let gates = turn.gates();

  assert_eq!(
    gates,
    vec![("pre", "references"), ("pre", "screen"), ("post", "format"),],
    "a gate in both parts keeps the first part it stands in, and no off-turn gate is a turn's"
  );
  assert_eq!(turn.pre.len(), 2, "the two pre-gates stand on two steps");
  assert_eq!(turn.post.len(), 1, "the post-gates start together");
}
