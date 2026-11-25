//! Agency's outbox (docs/agent-control.md, 「An item's change is told to its orderer」): a notice the receiver
//! could not take stays owed and is told once it answers; a standing notice is told again at a start under its
//! kind and withdrawn once let go; a notice never told is let go without a withdrawal; and a review's check is
//! owed only while its job stands under review with that check due.

use agency_server::outbox::{Keep, Notice, let_go, owe, owed, restarted, send_due};
use serde_json::{Value, json};

use crate::support::peers::stand;
use crate::support::scratch::Scratch;

fn instruction(kind: &str) -> Value {
  json!({
    "room": -100,
    "said": { "order": format!("about {kind}") },
    "sort": "job",
    "kind": kind,
    "summary": kind,
    "events": [kind.replace(':', ":#")],
  })
}

async fn tries(scratch: &Scratch, key: &str) -> i64 {
  scratch
    .count(r#"SELECT COALESCE(MAX("tries"), -1) FROM "notices" WHERE "key" = $1"#, vec![key.into()])
    .await
}

#[tokio::test]
async fn a_notice_the_receiver_could_not_take_stays_owed_and_is_told_once_it_answers() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  peers.lock().unwrap().failing.insert("/instructions".to_owned());
  owe(db, &vars, Notice::instruction(instruction("job-ended:7"), Keep::Once))
    .await
    .unwrap();

  assert_eq!(tries(&scratch, "job-ended:7").await, 1, "the failed try is counted and the row stays");
  assert_eq!(send_due(db, &vars).await.unwrap(), 0, "the row waits out its wait before the next try");

  // The receiver answers again; a start makes every owed row due at once.
  peers.lock().unwrap().failing.clear();
  restarted(db).await.unwrap();

  assert_eq!(send_due(db, &vars).await.unwrap(), 1);
  assert_eq!(tries(&scratch, "job-ended:7").await, -1, "a notice told once is let go");

  let told = peers.lock().unwrap().instructed();

  assert_eq!(told.last().unwrap()["kind"], "job-ended:7");
}

#[tokio::test]
async fn a_standing_notice_is_told_again_at_a_start_under_its_kind_and_withdrawn_once_let_go() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  owe(db, &vars, Notice::instruction(instruction("job-review:8"), Keep::Standing))
    .await
    .unwrap();
  assert_eq!(send_due(db, &vars).await.unwrap(), 0, "told, and not due again before its retelling");

  restarted(db).await.unwrap();
  assert_eq!(send_due(db, &vars).await.unwrap(), 1, "told again as the server starts");

  let_go(db, &vars, "job-review:8").await.unwrap();
  assert!(owed(db, "job-review:8").await.unwrap().is_empty(), "no notice stays owed");

  let held = peers.lock().unwrap();
  let kinds: Vec<Value> = held.instructed().iter().map(|told| told["kind"].clone()).collect();

  assert_eq!(kinds, [json!("job-review:8"), json!("job-review:8")]);
  assert_eq!(held.withdrawn(), ["2"], "the copy told last withdrawn");
}

#[tokio::test]
async fn a_notice_never_told_is_let_go_without_a_withdrawal() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  peers.lock().unwrap().failing.insert("/instructions".to_owned());
  owe(db, &vars, Notice::instruction(instruction("question-awaits-answer:9"), Keep::Kept))
    .await
    .unwrap();
  let_go(db, &vars, "question-awaits-answer:9").await.unwrap();

  assert!(owed(db, "question-awaits-answer:9").await.unwrap().is_empty());
  assert!(peers.lock().unwrap().withdrawn().is_empty());
}

#[tokio::test]
async fn a_review_s_check_stays_owed_only_while_its_job_stands_under_review_with_it_due() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (_, vars) = stand().await;

  scratch
    .run(
      r#"INSERT INTO "jobs" ("item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "started_at", "prompt", "review_check")
         VALUES (11, 1, 'runner', 1, 'w1', 'reviewing', 1000, 'the work', 'promises'),
                (12, 2, 'runner', 2, 'w2', 'running', 1000, 'the work', NULL)"#,
      vec![],
    )
    .await;

  for item in [11, 12] {
    let kind = format!("job-review:{item}");

    owe(db, &vars, Notice::instruction(instruction(&kind), Keep::Standing)).await.unwrap();
  }

  agency_server::job_review::checks_owed(db, &vars).await.unwrap();

  let owed: Vec<String> = owed(db, "job-review:").await.unwrap().into_keys().collect();

  assert_eq!(owed, ["11"], "the check of a job whose turn runs again is let go");
}
