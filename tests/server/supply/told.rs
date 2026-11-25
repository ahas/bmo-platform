//! The facts of a person's supply told to their own supervisor (docs/supply.md, 「The facts told」): each rides the
//! room's rounds for good, so none opens a turn a closed supply could only fail; no source left is told once while
//! it stays so, whatever number of runs find none, and told afresh once a run of the supervisor there went out again.

use agency_server::supply::told::{self, Told};
use agency_server::supply::{Closed, Shortfall};
use serde_json::Value;

use crate::support::grounds::ground_with;
use crate::support::peers::stand;
use crate::support::scratch::Scratch;

fn closed() -> Closed {
  Closed::of(
    Shortfall::NoLendable,
    "global",
    "no platform login lent answers claude-code now".to_owned(),
    None,
  )
}

#[tokio::test(flavor = "multi_thread")]
async fn no_source_left_is_told_once_as_a_rider_and_afresh_after_a_run_went_out() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (peers, vars) = stand().await;
  let ground = ground_with(scratch.db().clone(), home.path(), &vars);
  let told = Told {
    supervisor: Some("u7".to_owned()),
    room: Some(12),
  };
  let instructed = || peers.lock().unwrap().instructed();

  // Run after run finds none left: the room is told once.
  for run in ["turn-1-1", "turn-2-2", "turn-3-3"] {
    told::closed(&ground, run, Some(7), None, "deepseek", "deepseek-flash", &told, &closed()).await;
  }

  let said = instructed();

  assert_eq!(said.len(), 1, "told once while none is left: {said:?}");
  assert_eq!(said[0]["kind"], "supply:supply-closed:u7:12");
  assert_eq!(said[0]["room"], 12);
  assert_eq!(said[0]["ridesForGood"], true, "the fact opens no turn of its own");
  assert_eq!(
    said[0]["line"],
    serde_json::json!({ "code": "supply-closed", "why": "no-lendable", "until": null }),
    "the room hears why at once, with no round going out"
  );

  // A run of the supervisor there went out on a source: the fact is withdrawn, and the next closing told afresh.
  told::reopened(&ground, &told).await;
  assert_eq!(peers.lock().unwrap().withdrawn().len(), 1, "the row no round took is withdrawn");

  told::closed(&ground, "turn-4-4", Some(7), None, "deepseek", "deepseek-flash", &told, &closed()).await;
  assert_eq!(instructed().len(), 2);

  // Another room of the same supervisor is told of its own.
  let elsewhere = Told {
    supervisor: Some("u7".to_owned()),
    room: Some(13),
  };

  told::closed(&ground, "turn-5-5", Some(7), None, "deepseek", "deepseek-flash", &elsewhere, &closed())
    .await;
  assert_eq!(instructed().len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn every_supply_fact_rides_and_opens_no_turn() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (peers, vars) = stand().await;
  let ground = ground_with(scratch.db().clone(), home.path(), &vars);
  let told = Told {
    supervisor: Some("u7".to_owned()),
    room: Some(12),
  };

  told::refused(&ground, &told, "turn-1-1", "personal", "the key was refused").await;

  let said: Vec<Value> = peers.lock().unwrap().instructed();

  assert_eq!(said.len(), 1);
  assert_eq!(said[0]["ridesForGood"], true, "{said:?}");
  assert!(
    said[0].get("line").is_none(),
    "only no source left is said to the room at once: {said:?}"
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn no_source_left_told_before_the_room_heard_its_line_is_told_again_with_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (peers, vars) = stand().await;
  let ground = ground_with(scratch.db().clone(), home.path(), &vars);
  let told = Told {
    supervisor: Some("u7".to_owned()),
    room: Some(12),
  };

  // The fact as it stood owed before it carried the line.
  agency_server::outbox::owe(
    &ground.db,
    ground.vars(),
    agency_server::outbox::Notice::instruction(
      serde_json::json!({ "agentName": "u7", "room": 12, "kind": "supply:supply-closed:u7:12" }),
      agency_server::outbox::Keep::Kept,
    ),
  )
  .await
  .unwrap();
  told::closed(&ground, "turn-1-1", Some(7), None, "claude-code", "a-model", &told, &closed()).await;
  told::closed(&ground, "turn-2-2", Some(7), None, "claude-code", "a-model", &told, &closed()).await;

  let said = peers.lock().unwrap().instructed();

  assert_eq!(said.len(), 2, "told once more, then no more: {said:?}");
  assert_eq!(said[1]["line"]["code"], "supply-closed");
}
