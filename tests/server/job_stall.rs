//! A job's turn that makes no progress told to its item's orderer (docs/agent-control.md, 「A run's tool calls are
//! held to a clock」): one instruction a stall, naming the job, the item, how long, and what it waits on with its
//! last lines; none owed twice while one stands for the item; withdrawn once the turn moves, and told again for
//! the next stall.

use agency_server::door::stall::{StallChange, Stalled, Waits};
use agency_server::job_stall::{JobStall, tell, withdraw};
use serde_json::json;

use crate::support::peers::{item, stand};
use crate::support::scratch::Scratch;
use crate::support::wording::assert_no_instruction;

const SINCE: i64 = 1_795_000_000_000;

fn stall(minutes: i64, waits: Vec<Waits>) -> Stalled {
  Stalled {
    since: SINCE,
    at: SINCE + minutes * 60_000,
    waits,
  }
}

fn clip_step() -> Vec<Waits> {
  vec![Waits {
    tool: "Bash until rg -q '^exit ' /tmp/run-clip.txt; do sleep 30; done".to_owned(),
    lines: vec!["every card it may use is taken; asking again every minute".to_owned()],
  }]
}

#[tokio::test]
async fn a_stalled_job_is_told_once_withdrawn_as_it_moves_and_told_again_as_it_stalls_again() {
  let scratch = Scratch::new().await;
  let (peers, vars) = stand().await;
  let member = json!({ "id": 908, "itemId": 51, "workerName": "w1", "roomId": null, "state": "running" });

  peers.lock().unwrap().items.insert(51, item(51, "leased", json!({ "scope": "-510" })));

  tell(scratch.db(), &vars, &member, &stall(16, clip_step())).await;
  tell(scratch.db(), &vars, &member, &stall(30, clip_step())).await;

  let told = peers.lock().unwrap().instructed();

  assert_eq!(told.len(), 1, "a stall that stands is not told twice: {told:?}");
  assert_eq!(
    (&told[0]["kind"], &told[0]["events"], &told[0]["room"], &told[0]["sort"]),
    (&json!("job-stalled:51"), &json!(["job-stalled:#51"]), &json!(-510), &json!("job"))
  );

  let order = told[0]["said"]["order"].as_str().unwrap();

  for said in [
    "job 908",
    "item #51",
    "w1",
    "16 minutes",
    "/tmp/run-clip.txt",
    "asking again every minute",
  ] {
    assert!(order.contains(said), "{said:?} in {order}");
  }

  assert_no_instruction(order);

  withdraw(scratch.db(), &vars, 51).await;
  assert_eq!(peers.lock().unwrap().withdrawn(), ["1"], "the copy told is withdrawn as the turn moves");

  tell(scratch.db(), &vars, &member, &stall(15, vec![])).await;

  let told = peers.lock().unwrap().instructed();

  assert_eq!(told.len(), 2, "the next stall is told again: {told:?}");
  assert!(
    told[1]["said"]["order"].as_str().unwrap().contains("no tool call"),
    "a stall with no call running says the model's turn stands: {}",
    told[1]["said"]["order"]
  );
}

#[tokio::test]
async fn a_job_with_no_item_is_told_to_nobody() {
  let scratch = Scratch::new().await;
  let (peers, vars) = stand().await;
  let member = json!({ "id": 9, "itemId": 0, "workerName": "w1", "state": "running" });

  tell(scratch.db(), &vars, &member, &stall(20, clip_step())).await;
  assert!(peers.lock().unwrap().instructed().is_empty());
}

// A job's turn's changes as its runner hands them: the job's row read from the store, its stall told once to the
// item's orderer however often the call clock says it, and let go as the turn moves, the turn's end then
// withdrawing nothing more.
#[tokio::test]
async fn a_job_s_turn_watch_reads_its_job_tells_its_stall_once_and_lets_it_go_as_it_moves() {
  let scratch = Scratch::new().await;
  let (peers, vars) = stand().await;

  peers.lock().unwrap().items.insert(52, item(52, "leased", json!({ "scope": "-520" })));
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "room_id",
                             "state", "started_at", "prompt")
         VALUES (908, 52, 1, 'harness-a', 1, 'w2', NULL, 'running', $1, 'work')"#,
      vec![SINCE.into()],
    )
    .await;

  let mut watch = JobStall::new(908);

  watch.heard(scratch.db(), &vars, &StallChange::Stalled(stall(16, clip_step()))).await;
  watch.heard(scratch.db(), &vars, &StallChange::Stalled(stall(40, clip_step()))).await;

  let told = peers.lock().unwrap().instructed();

  assert_eq!(told.len(), 1, "{told:?}");
  assert_eq!((&told[0]["kind"], &told[0]["room"]), (&json!("job-stalled:52"), &json!(-520)));
  assert!(told[0]["said"]["order"].as_str().unwrap().contains("job 908 of item #52 on w2"));

  watch.heard(scratch.db(), &vars, &StallChange::Resumed).await;
  watch.ended(scratch.db(), &vars).await;
  assert_eq!(peers.lock().unwrap().withdrawn(), ["1"], "withdrawn once");
}
