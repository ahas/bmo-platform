//! What a job's orderer is told of the job (docs/agent-control.md, 「An item's change is told to its orderer」): a
//! job's wait as it begins, a job's end told to the orderer of a derived item up the chain, a failed job told in
//! the owner's private room as well where its orderer is another room, and a question past its due time or
//! settled on its recommendation told to the question's orderer.

use agency_server::job_end::delivered_with;
use agency_server::job_notices::{
  agent_refused, awaits_close, closed, failed, moved, owner_hears_failure, paused, question, question_due,
  question_settled, role_unknown, tell_waiting, unlaunchable, waiting,
};
use agency_server::queue::orderer::Orderer;
use serde_json::{Value, json};

use crate::support::peers::{item, stand};
use crate::support::scratch::Scratch;
use crate::support::wording::assert_no_instruction;

fn member(item: i64, outcome: Option<&str>, room: Option<i64>) -> Value {
  json!({
    "id": 77,
    "itemId": item,
    "leaseId": 0,
    "workerName": "w1",
    "roomId": room,
    "state": "ended",
    "outcome": outcome,
    "report": "what the run left",
  })
}

fn waiter(item: i64, room: Option<i64>) -> Value {
  json!({ "id": 70 + item, "itemId": item, "workerName": "w1", "roomId": room, "state": "starting" })
}

#[tokio::test]
async fn a_job_s_wait_whose_item_the_job_service_does_not_answer_is_told_in_the_room_it_was_launched_for() {
  let scratch = Scratch::new().await;
  let (peers, vars) = stand().await;

  peers.lock().unwrap().failing.insert("/items/43".to_owned());
  tell_waiting(scratch.db(), &vars, &waiter(43, Some(-430)), "it waits for a slot of the worker role")
    .await;

  let told = peers.lock().unwrap().instructed();

  assert_eq!(told.len(), 1, "{told:?}");
  assert_eq!((&told[0]["kind"], &told[0]["room"]), (&json!("job-waiting:43"), &json!(-430)));
}

#[tokio::test]
async fn a_job_s_wait_is_told_to_its_orderer_in_the_room_up_the_chain() {
  let scratch = Scratch::new().await;
  let (peers, vars) = stand().await;

  {
    let mut held = peers.lock().unwrap();

    held.items.insert(40, item(40, "done", json!({ "scope": "-400" })));
    held.items.insert(41, item(41, "leased", json!({ "raisedBy": 40 })));
    held.items.insert(42, item(42, "leased", json!({})));
  }

  tell_waiting(scratch.db(), &vars, &waiter(41, None), "no provider of the order has quota").await;
  tell_waiting(scratch.db(), &vars, &waiter(42, None), "it waits for a slot of the worker role").await;

  let told = peers.lock().unwrap().instructed();

  assert_eq!(told.len(), 2, "{told:?}");
  assert_eq!(
    (&told[0]["kind"], &told[0]["events"], &told[0]["room"], &told[0]["sort"]),
    (&json!("job-waiting:41"), &json!(["job-waiting:#41"]), &json!(-400), &json!("job"))
  );
  assert!(
    told[0]["said"]["order"]
      .as_str()
      .unwrap()
      .contains("no provider of the order has quota")
  );
  assert_eq!(
    told[1]["room"],
    Value::Null,
    "an item naming no room and no parent: the owner's private room"
  );
}

#[tokio::test]
async fn a_job_s_end_on_a_derived_item_is_told_to_the_orderer_of_its_parent_s_parent() {
  let scratch = Scratch::new().await;
  let (peers, vars) = stand().await;

  {
    let mut held = peers.lock().unwrap();

    held
      .items
      .insert(50, item(50, "done", json!({ "scope": "-500", "requesterName": "agent-a" })));
    held.items.insert(51, item(51, "done", json!({ "labels": { "raised-by": "50" } })));
    held.items.insert(52, item(52, "leased", json!({ "raisedBy": 51 })));
  }

  delivered_with(scratch.db(), &vars, &member(52, None, None)).await;

  let held = peers.lock().unwrap();
  let told = held.instructed();

  assert_eq!(told.len(), 1, "{told:?}");
  assert_eq!(
    (&told[0]["kind"], &told[0]["room"], &told[0]["agentName"]),
    (&json!("job-ended:52"), &json!(-500), &json!("agent-a"))
  );
}

#[tokio::test]
async fn a_failed_job_in_another_room_is_told_in_the_owner_s_room_as_well_and_not_twice_in_his_own() {
  let scratch = Scratch::new().await;
  let (peers, vars) = stand().await;

  peers.lock().unwrap().items.insert(60, item(60, "leased", json!({ "scope": "-600" })));
  peers.lock().unwrap().items.insert(61, item(61, "leased", json!({})));

  delivered_with(scratch.db(), &vars, &member(60, Some("failed"), None)).await;
  delivered_with(scratch.db(), &vars, &member(61, Some("blocked"), None)).await;

  let held = peers.lock().unwrap();
  let told: Vec<(Value, Value)> = held
    .instructed()
    .iter()
    .map(|body| (body["kind"].clone(), body["room"].clone()))
    .collect();

  assert_eq!(
    told,
    [
      (json!("job-ended:60"), json!(-600)),
      (json!("job-failed:60"), Value::Null),
      (json!("job-ended:61"), Value::Null),
    ],
    "each end told to its orderer, a failure in another room told in the owner's private room once"
  );
}

#[test]
fn a_failure_is_the_owner_s_only_for_failed_or_blocked_in_another_room() {
  let room = Orderer {
    room: Some(-1),
    requester: None,
  };

  assert!(owner_hears_failure(&member(1, Some("failed"), None), &room));
  assert!(owner_hears_failure(&member(1, Some("blocked"), None), &room));
  assert!(!owner_hears_failure(&member(1, Some("stopped"), None), &room));
  assert!(!owner_hears_failure(&member(1, None, None), &room));
  assert!(!owner_hears_failure(&member(1, Some("failed"), None), &Orderer::default()));
}

#[test]
fn a_failed_job_is_an_instruction_in_the_owner_s_private_room_naming_the_job_its_worker_outcome_and_room() {
  let told = failed(&member(1, Some("failed"), None), Some(-1));
  let order = told["said"]["order"].as_str().unwrap();

  assert_eq!(
    (&told["kind"], &told["events"], &told["room"], &told["sort"]),
    (&json!("job-failed:1"), &json!(["job-failed:#1"]), &Value::Null, &json!("job"))
  );

  for word in ["77", "#1", "w1", "failed", "-1"] {
    assert!(order.contains(word), "{word} in {order}");
  }

  assert_no_instruction(order);
}

#[test]
fn every_queue_order_carries_its_data_and_no_instruction() {
  let orderer = Orderer {
    room: Some(-1),
    requester: None,
  };
  let item = json!({ "id": 12, "title": "Fix it", "state": "held" });

  let notices = [
    (paused(12, "Fix it", "todo", &orderer), &["#12", "Fix it", "todo"][..]),
    (unlaunchable(12, "Fix it", "w9", &orderer), &["#12", "Fix it", "w9"][..]),
    (
      agent_refused(12, "Fix it", "nobody-07", "no agent the agency registers", &orderer),
      &["#12", "Fix it", "agent=nobody-07", "no agent the agency registers"][..],
    ),
    (
      role_unknown(12, "Fix it", "nobody", &["coder".to_owned(), "worker".to_owned()], &orderer),
      &["#12", "Fix it", "role=nobody", "coder, worker"][..],
    ),
    (moved(&item, 34, "w1", &orderer), &["#12", "Fix it", "held", "34", "w1"][..]),
    (closed(&item, "member", &orderer), &["#12", "Fix it", "member"][..]),
    (
      question(
        &json!({ "id": 55, "itemId": 12, "text": "Which one?", "choices": ["a", "b"] }),
        &orderer,
      ),
      &["#55", "#12", "Which one?", "a / b"][..],
    ),
    (waiting(12, "w1", "no quota", &orderer), &["#12", "w1", "no quota"][..]),
    (awaits_close(12, 64, &orderer), &["#12", "64"][..]),
  ];

  for (notice, data) in notices {
    let order = notice["said"]["order"].as_str().unwrap();

    for value in data {
      assert!(order.contains(value), "{value} in {order}");
    }

    assert_no_instruction(order);
  }
}

#[test]
fn a_question_past_its_due_time_is_an_instruction_in_its_orderer_s_room_carrying_the_question() {
  let orderer = Orderer {
    room: Some(-3),
    requester: None,
  };
  let told = question_due(
    &json!({
      "id": 9, "itemId": 3, "text": "which one?", "choices": ["a", "b"], "askerName": "harness", "dueAt": 5,
    }),
    &orderer,
  );
  let order = told["said"]["order"].as_str().unwrap();

  assert_eq!(
    (&told["kind"], &told["events"], &told["room"], &told["sort"]),
    (&json!("job-question-due:9"), &json!(["job-question-due:#9"]), &json!(-3), &json!("job"))
  );

  for word in ["#9", "#3", "harness", "which one?", "a / b"] {
    assert!(order.contains(word), "{word} in {order}");
  }

  assert_no_instruction(order);
}

#[test]
fn a_question_settled_on_its_recommendation_is_an_instruction_naming_the_answer_and_why() {
  let orderer = Orderer {
    room: Some(-4),
    requester: None,
  };
  let reply = json!({ "questionId": 40, "choice": "b", "text": "b", "why": "b keeps the contract" });
  let told = question_settled(&reply, Some(&json!({ "id": 40, "itemId": 95 })), &orderer);
  let order = told["said"]["order"].as_str().unwrap();

  assert_eq!(
    (&told["kind"], &told["events"], &told["room"], &told["sort"]),
    (
      &json!("job-question-settled:40"),
      &json!(["job-question-settled:#40"]),
      &json!(-4),
      &json!("job")
    )
  );

  for word in ["#40", "#95", ": b.", "b keeps the contract"] {
    assert!(order.contains(word), "{word} in {order}");
  }

  assert_no_instruction(order);

  // A free answer is told by its text; a question the service no longer holds names no item.
  let free =
    json!({ "questionId": 41, "choice": null, "text": "write it fresh", "why": "nothing to keep" });
  let told = question_settled(&free, None, &Orderer::default());
  let order = told["said"]["order"].as_str().unwrap();

  assert!(order.contains("write it fresh") && !order.contains("item #"), "{order}");
  assert_eq!(told["room"], Value::Null);
}
