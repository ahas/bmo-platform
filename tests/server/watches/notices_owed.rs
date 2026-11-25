//! Notices owed across a cut receiver and a stopped sender (docs/agent-control.md, 「An item's change is told to
//! its orderer」): the job service's events read while the receiver cuts every call are owed, the cursor moves
//! past them, and each is told once the receiver answers again; a sender stopped in the middle of a telling
//! leaves the notice owed, the next start tells it, a standing one again at every start under its kind, until
//! its condition ends and the copy told last is withdrawn; and a job's end or a review's check whose row a stop cut
//! off owed by the sender once the grace is past; and a job's wait, its end and its review's check each cut off by a
//! process stopped in the middle of its call to the job service, told once the next process starts and the
//! receiver, cutting every call as it restarts, answers again.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::control::work::hold_job;
use agency_server::job_end::delivered;
use agency_server::job_review::reported;
use agency_server::outbox::{Keep, Notice, owe, owed, restarted};
use agency_server::places::Ground;
use agency_server::queue::settings;
use agency_server::watches::job_events::{self, CURSOR_KEY};
use agency_server::watches::notices_owed::pass;
use harness_providers::Machine;
use policy::job_notices::JOB_NOTICES;
use policy::job_review::JOB_REVIEW;
use serde_json::{Value, json};

use crate::support::cut::{Front, Mode};
use crate::support::peers::{Peers, item, stand};
use crate::support::scratch::Scratch;

// The stand-in as the job service, and a front before it as the receiver.
async fn cut_receiver() -> (Peers, Front, BTreeMap<String, String>) {
  let (peers, mut vars) = stand().await;
  let front = Front::before(&vars["BMO_RECEIVER_URL"]).await;

  vars.insert("BMO_RECEIVER_URL".to_owned(), front.address.clone());

  (peers, front, vars)
}

fn event(id: i64, kind: &str, data: Value) -> Value {
  json!({ "id": id, "kind": kind, "at": id, "data": data })
}

fn kinds(peers: &Peers) -> Vec<Value> {
  peers.lock().unwrap().instructed().iter().map(|told| told["kind"].clone()).collect()
}

// The instructions told, each kind with its room.
fn rooms(peers: &Peers) -> BTreeMap<String, Value> {
  peers
    .lock()
    .unwrap()
    .instructed()
    .iter()
    .map(|told| (told["kind"].as_str().unwrap_or_default().to_owned(), told["room"].clone()))
    .collect()
}

async fn failed_tries(scratch: &Scratch) -> i64 {
  scratch.count(r#"SELECT COUNT(*) FROM "notices" WHERE "tries" > 0"#, vec![]).await
}

#[tokio::test]
async fn events_read_while_the_receiver_cuts_every_call_are_owed_and_told_once_it_answers_again() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, front, vars) = cut_receiver().await;

  settings::set(db, CURSOR_KEY, "60").await.unwrap();

  {
    let mut held = peers.lock().unwrap();

    held.items.insert(82, item(82, "leased", json!({ "scope": "-820" })));
    held.events = vec![
      event(
        61,
        "question.created",
        json!({ "id": 9, "itemId": 82, "text": "which?", "choices": [] }),
      ),
      event(
        62,
        "item.changed",
        item(
          84,
          "done",
          json!({ "previousState": "leased", "changedBy": "someone", "scope": "-840" }),
        ),
      ),
    ];
  }

  front.set(Mode::Cut);
  job_events::pass(db, &vars, &crate::support::gates::none()).await.unwrap();

  assert_eq!(
    settings::get(db, CURSOR_KEY).await.unwrap().as_deref(),
    Some("62"),
    "the cursor moved past both"
  );
  assert!(front.accepted() >= 2, "each notice was tried and its connection cut");
  assert!(kinds(&peers).is_empty(), "nothing reached the receiver");
  assert_eq!(failed_tries(&scratch).await, 2, "both owed, each with its failed try");

  // The receiver is back; the rows wait out their wait, then agency's start makes them due at once.
  front.set(Mode::Pass);
  assert_eq!(pass(db, &vars).await.unwrap(), 0, "no row is tried before its wait is out");

  restarted(db).await.unwrap();
  assert_eq!(pass(db, &vars).await.unwrap(), 2);
  let mut told = kinds(&peers);

  told.sort_by_key(Value::to_string);
  assert_eq!(told, [json!("item-closed:84"), json!("question-awaits-answer:9")], "each told once");

  restarted(db).await.unwrap();
  assert_eq!(pass(db, &vars).await.unwrap(), 0, "a notice told once is not told again");
  assert!(owed(db, "item-closed:").await.unwrap().is_empty());
  assert!(
    owed(db, "question-awaits-answer:9").await.unwrap()[""].told_at.is_some(),
    "the question kept, told"
  );
}

fn check(item: i64) -> Value {
  json!({
    "room": -110,
    "said": { "order": "the check due" },
    "sort": "job",
    "kind": format!("job-review:{item}"),
    "summary": "the check due",
    "events": [format!("job-review:#{item}")],
  })
}

#[tokio::test]
async fn a_sender_stopped_mid_telling_leaves_the_check_owed_and_each_start_tells_it_until_it_is_no_longer_due()
 {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, front, vars) = cut_receiver().await;

  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "started_at", "prompt", "review_check")
         VALUES (7, 11, 1, 'runner', 1, 'w1', 'reviewing', 1000, 'the work', 'promises')"#,
      vec![],
    )
    .await;

  // The first telling hangs on the receiver, and the process telling it stops in the middle.
  front.set(Mode::Hold);

  let telling = {
    let (db, vars) = (db.clone(), vars.clone());

    tokio::spawn(async move { owe(&db, &vars, Notice::instruction(check(11), Keep::Standing)).await })
  };

  front.heard().await;
  telling.abort();
  assert!(telling.await.unwrap_err().is_cancelled());

  let stood = owed(db, "job-review:11").await.unwrap().remove("").unwrap();

  assert_eq!(stood.told_at, None, "the stopped telling left the check owed, untold");
  front.set(Mode::Pass);
  assert_eq!(
    pass(db, &vars).await.unwrap(),
    0,
    "the stopped sender's hold on the row stands until it lapses"
  );

  // The next process starts: the check is told; and each start after tells it again under the same kind.
  restarted(db).await.unwrap();
  assert_eq!(pass(db, &vars).await.unwrap(), 1);
  restarted(db).await.unwrap();
  assert_eq!(pass(db, &vars).await.unwrap(), 1);
  assert_eq!(kinds(&peers), [json!("job-review:11"), json!("job-review:11")]);

  // The orderer moves and the job's turn runs: the check is no longer due, let go and its last copy withdrawn.
  scratch
    .run(r#"UPDATE "jobs" SET "state" = 'running', "review_check" = NULL WHERE "id" = 7"#, vec![])
    .await;
  pass(db, &vars).await.unwrap();
  restarted(db).await.unwrap();
  assert_eq!(pass(db, &vars).await.unwrap(), 0, "a check no longer due is told no more");
  assert!(owed(db, "job-review:").await.unwrap().is_empty());
  assert_eq!(peers.lock().unwrap().withdrawn(), ["2"], "the copy told last withdrawn");
}

// Room -120 is item 12's, -130 item 13's: the orderers of the jobs below.
async fn stand_items(peers: &Peers) {
  let mut held = peers.lock().unwrap();

  held.items.insert(12, item(12, "leased", json!({ "scope": "-120" })));
  held.items.insert(13, item(13, "leased", json!({ "scope": "-130" })));
}

#[tokio::test]
async fn a_job_s_end_whose_notice_a_stop_cut_off_is_owed_by_the_sender_once_and_a_fresh_one_is_left_to_its_path()
 {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;
  let now = agency_server::now_ms();

  stand_items(&peers).await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "outcome", "report", "started_at", "ended_at", "prompt")
         VALUES (8, 12, 1, 'runner', 1, 'w1', 'failed', 'failed', 'it broke', 1000, 2000, 'the work'),
                (9, 13, 2, 'runner', 2, 'w1', 'stopped', 'stopped', 'stopped', 1000, $1, 'the work')"#,
      vec![now.into()],
    )
    .await;

  pass(db, &vars).await.unwrap();

  assert_eq!(
    rooms(&peers),
    BTreeMap::from([("job-ended:12".to_owned(), json!(-120)), ("job-failed:12".to_owned(), Value::Null)]),
    "only the end past its grace, and its failure in the owner's private room"
  );
  assert_eq!(
    scratch.count(r#"SELECT COUNT(*) FROM "jobs" WHERE "end_owed""#, vec![]).await,
    1,
    "the end owed is marked so"
  );

  pass(db, &vars).await.unwrap();
  assert_eq!(peers.lock().unwrap().instructed().len(), 2, "an end owed is not owed twice");
}

fn told(heard: &str, words: &str, at: i64) -> String {
  json!({ "heard": heard, "words": words, "at": at }).to_string()
}

#[tokio::test]
async fn a_check_due_whose_row_a_stop_cut_off_is_owed_by_the_sender_with_the_words_it_came_due_with() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  stand_items(&peers).await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "started_at", "prompt", "review_check", "review_words", "review_told")
         VALUES (7, 12, 1, 'runner', 1, 'w1', 'reviewing', 1000, 'the work', 'promises', 'the report', $1),
                (6, 13, 2, 'runner', 2, 'w1', 'reviewing', 1000, 'the work', 'promises', 'the report', $2)"#,
      vec![
        told("answer", "It was done so.", 1000).into(),
        told("report", "the report", agency_server::now_ms()).into(),
      ],
    )
    .await;

  pass(db, &vars).await.unwrap();

  let told = peers.lock().unwrap().instructed();

  assert_eq!(told.len(), 1, "only the check past its grace: {told:?}");
  assert_eq!((&told[0]["kind"], &told[0]["room"]), (&json!("job-review:12"), &json!(-120)));

  let order = told[0]["said"]["order"].as_str().unwrap();

  assert!(
    order.contains(&JOB_REVIEW.answer_heading) && order.contains("It was done so."),
    "{order}"
  );
  assert!(owed(db, "job-review:12").await.unwrap()[""].told_at.is_some(), "kept standing");

  pass(db, &vars).await.unwrap();
  assert_eq!(
    peers.lock().unwrap().instructed().len(),
    1,
    "a check owed is not owed again by the sender"
  );
}

// A process's ground whose job service and receiver are the fronts given: no test ground, so it tells.
fn telling_ground(
  db: &sea_orm::DatabaseConnection,
  home: &std::path::Path,
  vars: &BTreeMap<String, String>,
) -> Ground {
  let variables: HashMap<String, String> = HashMap::new();
  let service = Service::new(db.clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let mut env = vec![
    ("HOME".to_owned(), home.to_string_lossy().into_owned()),
    ("PATH".to_owned(), "/nowhere".to_owned()),
  ];

  env.extend(vars.iter().map(|(name, value)| (name.clone(), value.clone())));

  Ground::new(&service, &Machine::with_env(env, home.join("data")), None)
}

// The stand-in as both peers, each behind a front of its own: what this process reaches.
async fn cut_peers() -> (Peers, Front, Front, BTreeMap<String, String>) {
  let (peers, mut vars) = stand().await;
  let jobs = Front::before(&vars["BMO_JOB_URL"]).await;
  let receiver = Front::before(&vars["BMO_RECEIVER_URL"]).await;

  vars.insert("BMO_JOB_URL".to_owned(), jobs.address.clone());
  vars.insert("BMO_RECEIVER_URL".to_owned(), receiver.address.clone());
  peers.lock().unwrap().items.insert(12, item(12, "leased", json!({ "scope": "-120" })));

  (peers, jobs, receiver, vars)
}

// The process telling stopped once its call to the job service is held: what a restart of agency does to it.
async fn stopped_mid_call(jobs: &Front, telling: tokio::task::JoinHandle<()>) {
  jobs.heard().await;
  telling.abort();
  assert!(telling.await.unwrap_err().is_cancelled());
}

// Past the grace by a millisecond.
fn grace_ms() -> i64 {
  i64::try_from(JOB_NOTICES.owe_grace_seconds).unwrap() * 1000 + 1
}

// The next process starts while the receiver is still restarting: what was cut off is owed, its try cut; then the
// receiver is back and the next start tells it. Answers what the receiver took on that start.
async fn started_again(
  db: &sea_orm::DatabaseConnection,
  vars: &BTreeMap<String, String>,
  jobs: &Front,
  receiver: &Front,
) -> usize {
  jobs.set(Mode::Pass);
  receiver.set(Mode::Cut);
  restarted(db).await.unwrap();
  assert_eq!(pass(db, vars).await.unwrap(), 0, "the receiver cut every call");
  receiver.set(Mode::Pass);
  restarted(db).await.unwrap();

  pass(db, vars).await.unwrap()
}

#[tokio::test]
async fn a_job_s_wait_cut_off_by_a_restart_before_its_row_is_told_once_the_next_process_and_the_receiver_are_up()
 {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let home = tempfile::tempdir().unwrap();
  let (peers, jobs, receiver, vars) = cut_peers().await;
  let ground = Arc::new(telling_ground(db, home.path(), &vars));

  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "started_at", "prompt")
         VALUES (21, 12, 1, 'runner', 1, 'w1', 'starting', 1000, 'the work')"#,
      vec![],
    )
    .await;
  jobs.set(Mode::Hold);

  let telling = {
    let ground = ground.clone();

    tokio::spawn(
      async move { hold_job(&ground, 21, Some("no provider of the order has quota"), true).await },
    )
  };

  stopped_mid_call(&jobs, telling).await;
  assert!(owed(db, "job-waiting:").await.unwrap().is_empty(), "the stop came before the row");
  scratch
    .run(r#"UPDATE "jobs" SET "wait_unowed_at" = "wait_unowed_at" - $1"#, vec![grace_ms().into()])
    .await;

  assert_eq!(started_again(db, &vars, &jobs, &receiver).await, 1);
  assert_eq!(kinds(&peers), [json!("job-waiting:12")]);
  assert_eq!(peers.lock().unwrap().instructed()[0]["room"], json!(-120));
  assert_eq!(pass(db, &vars).await.unwrap(), 0, "told once");
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "jobs" WHERE "wait_unowed_at" IS NULL"#, vec![])
      .await,
    1
  );
}

#[tokio::test]
async fn a_job_s_end_cut_off_by_a_restart_before_its_row_is_told_once_the_next_process_and_the_receiver_are_up()
 {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let home = tempfile::tempdir().unwrap();
  let (peers, jobs, receiver, vars) = cut_peers().await;
  let ground = Arc::new(telling_ground(db, home.path(), &vars));
  let member = json!({
    "id": 22, "itemId": 12, "leaseId": 3, "workerName": "w1", "roomId": null,
    "state": "failed", "outcome": "failed", "report": "it broke",
  });

  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "outcome", "report", "started_at", "ended_at", "prompt")
         VALUES (22, 12, 3, 'runner', 1, 'w1', 'failed', 'failed', 'it broke', 1000, $1, 'the work')"#,
      vec![agency_server::now_ms().into()],
    )
    .await;
  jobs.set(Mode::Hold);

  // The end is written; its delivery gives the lease back first, and the process stops while that call hangs.
  let telling = {
    let ground = ground.clone();

    tokio::spawn(async move { delivered(&ground, &member).await })
  };

  stopped_mid_call(&jobs, telling).await;
  assert!(owed(db, "job-ended:").await.unwrap().is_empty(), "the stop came before the row");
  scratch
    .run(r#"UPDATE "jobs" SET "ended_at" = "ended_at" - $1"#, vec![grace_ms().into()])
    .await;

  assert_eq!(
    started_again(db, &vars, &jobs, &receiver).await,
    2,
    "the end and its failure in the owner's private room"
  );
  assert_eq!(
    rooms(&peers),
    BTreeMap::from([("job-ended:12".to_owned(), json!(-120)), ("job-failed:12".to_owned(), Value::Null)])
  );
  restarted(db).await.unwrap();
  assert_eq!(pass(db, &vars).await.unwrap(), 0, "told once");
}

#[tokio::test]
async fn a_review_check_cut_off_by_a_restart_before_its_row_is_told_until_the_orderer_moves() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let home = tempfile::tempdir().unwrap();
  let (peers, jobs, receiver, vars) = cut_peers().await;
  let ground = Arc::new(telling_ground(db, home.path(), &vars));

  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "started_at", "prompt")
         VALUES (23, 12, 1, 'runner', 1, 'w1', 'running', 1000, 'the work')"#,
      vec![],
    )
    .await;
  jobs.set(Mode::Hold);

  // The report is written into the review and the check falls due; the process stops while the review's entry is
  // written to the job service, before the check's row.
  let telling = {
    let ground = ground.clone();

    tokio::spawn(async move { reported(&ground, 23, "The screen is changed.", None).await })
  };

  stopped_mid_call(&jobs, telling).await;
  assert!(owed(db, "job-review:").await.unwrap().is_empty(), "the stop came before the row");
  scratch
    .run(
      r#"UPDATE "jobs" SET "review_told" = jsonb_set("review_told"::jsonb, '{at}', to_jsonb(1000))::text"#,
      vec![],
    )
    .await;

  assert_eq!(started_again(db, &vars, &jobs, &receiver).await, 1);
  assert_eq!(kinds(&peers), [json!("job-review:12")]);

  let order = peers.lock().unwrap().instructed()[0]["said"]["order"].as_str().unwrap().to_owned();

  assert!(
    order.contains(&JOB_REVIEW.report_heading) && order.contains("The screen is changed."),
    "{order}"
  );

  // Standing, it is told again at the next start; once the orderer moves it is let go and withdrawn.
  restarted(db).await.unwrap();
  assert_eq!(pass(db, &vars).await.unwrap(), 1);
  scratch
    .run(r#"UPDATE "jobs" SET "review_turn" = 'question' WHERE "id" = 23"#, vec![])
    .await;
  restarted(db).await.unwrap();
  assert_eq!(pass(db, &vars).await.unwrap(), 0);
  assert!(owed(db, "job-review:").await.unwrap().is_empty());
  assert_eq!(peers.lock().unwrap().withdrawn().len(), 1, "the copy told last withdrawn");
}
