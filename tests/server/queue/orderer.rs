//! The orderer of an item (docs/agent-control.md, 「An item's change is told to its orderer」): its own room,
//! else its parent's up the chain — the `raisedBy` field before the `raised-by` label — else the owner's
//! private room, where the chain reaches no room, a parent the job service no longer holds, an item already
//! passed or the policy's depth.

use agency_server::queue::orderer::{JobService, Orderer, parent_of, room_of};
use policy::job_notices::JOB_NOTICES;
use serde_json::json;

use crate::support::peers::{item, stand};

#[test]
fn the_field_names_the_parent_before_the_label() {
  assert_eq!(parent_of(&json!({ "raisedBy": 7, "labels": { "raised-by": "8" } })), Some(7));
  assert_eq!(parent_of(&json!({ "raisedBy": null, "labels": { "raised-by": "#8" } })), Some(8));
  assert_eq!(parent_of(&json!({ "labels": { "raised-by": 9 } })), Some(9));
  assert_eq!(parent_of(&json!({ "raisedBy": 0 })), None);
  assert_eq!(room_of(&json!({ "roomId": 5, "scope": "-7" })), Some(5));
  assert_eq!(room_of(&json!({ "scope": "-7" })), Some(-7));
  assert_eq!(room_of(&json!({ "scope": "" })), None);
}

#[tokio::test]
async fn the_orderer_is_found_up_the_chain_or_is_the_owner_s_private_room() {
  let (peers, vars) = stand().await;
  let service = JobService::of(&vars).unwrap();

  {
    let mut held = peers.lock().unwrap();

    // 1 asked in a room by its requester; 2 raised by 1 through the field, its label naming 9, which names a
    // room of its own; 3 raised by 2 through the label; 4 and 5 raise each other; 6's parent is gone.
    held
      .items
      .insert(1, item(1, "done", json!({ "scope": "-100", "requesterName": "agent-a" })));
    held
      .items
      .insert(2, item(2, "done", json!({ "raisedBy": 1, "labels": { "raised-by": "9" } })));
    held.items.insert(9, item(9, "done", json!({ "scope": "-900" })));
    held.items.insert(3, item(3, "done", json!({ "labels": { "raised-by": "2" } })));
    held.items.insert(4, item(4, "done", json!({ "raisedBy": 5 })));
    held.items.insert(5, item(5, "done", json!({ "raisedBy": 4 })));
  }

  let own = service.orderer_of(&json!({ "id": 10, "scope": "-50" })).await.unwrap();
  let parent = service.orderer_of(&json!({ "id": 11, "raisedBy": 1 })).await.unwrap();
  let grandparent = service.orderer_of(&json!({ "id": 12, "raisedBy": 3 })).await.unwrap();
  let field_first = service.orderer_of(&json!({ "id": 13, "raisedBy": 2 })).await.unwrap();
  let cycle = service.orderer_of(&json!({ "id": 14, "raisedBy": 4 })).await.unwrap();
  let gone = service
    .orderer_of(&json!({ "id": 15, "raisedBy": 6, "requesterName": "agent-b" }))
    .await
    .unwrap();
  let none = service.orderer_of(&json!({ "id": 16 })).await.unwrap();

  assert_eq!(own.room, Some(-50), "its own room");
  assert_eq!(
    parent,
    Orderer {
      room: Some(-100),
      requester: Some("agent-a".to_owned())
    },
    "its parent's room and requester"
  );
  assert_eq!(grandparent.room, Some(-100), "its grandparent's, up the chain");
  assert_eq!(field_first.room, Some(-100), "the raisedBy field, not the label");
  assert_eq!(cycle.room, None, "a chain going round ends in the owner's private room");
  assert_eq!(
    gone,
    Orderer {
      room: None,
      requester: Some("agent-b".to_owned())
    },
    "a parent the job service answers 404 for"
  );
  assert_eq!(none, Orderer::default(), "no room and no parent");

  let (missing, orderer) = service.orderer_of_id(99).await.unwrap();

  assert_eq!((missing, orderer), (None, Orderer::default()), "an item the service no longer holds");
}

#[tokio::test]
async fn a_chain_deeper_than_the_policy_s_depth_ends_in_the_owner_s_private_room() {
  let (peers, vars) = stand().await;
  let service = JobService::of(&vars).unwrap();
  let depth = JOB_NOTICES.orderer_depth as i64;

  {
    let mut held = peers.lock().unwrap();

    // 1 holds the room; 2 .. depth + 2 each raised by the one before.
    held.items.insert(1, item(1, "done", json!({ "scope": "-100" })));

    for id in 2..=depth + 2 {
      held.items.insert(id, item(id, "done", json!({ "raisedBy": id - 1 })));
    }
  }

  let within = service.orderer_of(&json!({ "id": 1000, "raisedBy": depth })).await.unwrap();
  let beyond = service.orderer_of(&json!({ "id": 1001, "raisedBy": depth + 2 })).await.unwrap();

  assert_eq!(within.room, Some(-100), "{depth} items up is within the depth");
  assert_eq!(beyond.room, None, "deeper than the depth");

  let reads = peers
    .lock()
    .unwrap()
    .calls
    .iter()
    .filter(|call| call.path.starts_with("/items/"))
    .count();

  assert_eq!(reads as i64, depth + depth, "the deeper chain stops after the depth's reads");
}
