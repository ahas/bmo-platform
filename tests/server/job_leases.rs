//! A running job's lease kept on the harness's behalf (docs/api.md, 「assignments and jobs」), whatever became of
//! the harness that took it, a restart of agency under the job among them: a lease that stands is renewed, one
//! run out is taken again on the same item and the job keeps the new id, an item another holder took or one no
//! longer open is left; a running job's checkout reads as held by it and its item waits on no free worker. A
//! running job's lease holds fixed what its launch read, and an ended job's holds nothing fixed. An ended job's
//! item left leased is told once for its lease, and not at all where its commit waits for its deployment.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use agency_server::job_leases::{Kept, hold_launch, launch_frozen, pass};
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{Method, StatusCode, Uri};
use axum::response::IntoResponse;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::support::rows::repository;
use crate::support::scratch::Scratch;

type Calls = Arc<Mutex<Vec<(String, String, Value)>>>;

// The job service: lease 7 stands; leases 8 and 9 have run out; item 21 is open with an assignee, item 22 is
// another holder's; a new lease on item 21 is 99.
async fn jobs(State(calls): State<Calls>, method: Method, uri: Uri, body: Bytes) -> impl IntoResponse {
  let body = serde_json::from_slice(&body).unwrap_or(Value::Null);

  calls.lock().unwrap().push((method.to_string(), uri.path().to_owned(), body));

  match (method.as_str(), uri.path()) {
    ("PATCH", "/leases/7") => (StatusCode::OK, axum::Json(json!({ "id": 7 }))),
    ("PATCH", _) => (StatusCode::GONE, axum::Json(json!({ "code": "gone" }))),
    ("GET", "/items/21") => (
      StatusCode::OK,
      axum::Json(json!({ "id": 21, "state": "open", "queueName": "work", "assigneeName": "worker-a" })),
    ),
    ("GET", "/items/22") => {
      (StatusCode::OK, axum::Json(json!({ "id": 22, "state": "leased", "queueName": "work" })))
    }
    ("POST", "/leases") => (StatusCode::CREATED, axum::Json(json!({ "id": 99 }))),
    _ => (StatusCode::NOT_FOUND, axum::Json(json!({}))),
  }
}

async fn seeded(scratch: &Scratch) {
  let now = agency_server::now_ms();

  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
                             "started_at", "prompt")
         VALUES (1, 20, 7, 'harness', 1, 'worker-a', 'running', $1, 'work'),
                (2, 21, 8, 'harness', 2, 'worker-a', 'running', $1, 'work'),
                (3, 22, 9, 'harness', 3, 'worker-b', 'starting', $1, 'work'),
                (4, 23, 10, 'harness', 4, 'worker-b', 'ended', $1, 'work')"#,
      vec![now.into()],
    )
    .await;
}

#[tokio::test]
async fn a_running_job_s_lease_is_renewed_or_taken_again_and_one_another_holds_is_left() {
  let scratch = Scratch::new().await;
  let calls: Calls = Arc::default();
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());
  let app = axum::Router::new().fallback(jobs).with_state(calls.clone());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  seeded(&scratch).await;

  let vars: BTreeMap<String, String> = [
    ("BMO_JOB_URL".to_owned(), address),
    ("BMO_JOB_TOKEN".to_owned(), "job-token".to_owned()),
  ]
  .into();
  let kept = pass(scratch.db(), &vars).await.unwrap();

  assert_eq!(kept[0], (1, Kept::Renewed));
  assert_eq!(kept[1], (2, Kept::Taken(99)));
  assert!(matches!(&kept[2], (3, Kept::Left(why)) if why.contains("leased")), "{kept:?}");
  assert_eq!(kept.len(), 3, "an ended job keeps no lease: {kept:?}");

  let lease = store::one(scratch.db(), r#"SELECT "lease_id" FROM "jobs" WHERE "id" = 2"#, vec![])
    .await
    .unwrap()
    .and_then(|row| row.try_get::<i64>("", "lease_id").ok());

  assert_eq!(lease, Some(99), "the job keeps the lease taken again, which its end gives back");

  let asked = calls.lock().unwrap().clone();
  let renewal = asked.iter().find(|(method, path, _)| method == "PATCH" && path == "/leases/7").unwrap();
  let taken = asked.iter().find(|(method, path, _)| method == "POST" && path == "/leases").unwrap();

  assert_eq!(renewal.2["ttlMs"], policy::job_leases::JOB_LEASES.ttl_ms);
  assert_eq!(
    renewal.2["frozen"],
    launch_frozen(1, None),
    "a running job's lease holds its launch fixed"
  );
  assert_eq!(
    taken.2,
    json!({ "queueName": "work", "itemId": 21, "holderName": "harness", "assigneeName": "worker-a",
            "ttlMs": policy::job_leases::JOB_LEASES.ttl_ms, "frozen": launch_frozen(2, None) })
  );

  // A deployment that names no job service keeps nothing.
  assert_eq!(pass(scratch.db(), &BTreeMap::new()).await.unwrap(), []);
}

#[tokio::test]
async fn an_ended_job_s_lease_kept_for_its_close_is_renewed_until_the_close_ends_it_and_never_taken_again()
{
  let scratch = Scratch::new().await;
  let calls: Calls = Arc::default();
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());
  let app = axum::Router::new().fallback(jobs).with_state(calls.clone());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
                             "started_at", "prompt", "lease_kept")
         VALUES (5, 20, 7, 'harness', 5, 'worker-a', 'ended', 1, 'work', true),
                (6, 21, 8, 'harness', 6, 'worker-a', 'ended', 1, 'work', true),
                (7, 22, 9, 'harness', 7, 'worker-b', 'stopped', 1, 'work', false)"#,
      vec![],
    )
    .await;

  let vars: BTreeMap<String, String> = [
    ("BMO_JOB_URL".to_owned(), address),
    ("BMO_JOB_TOKEN".to_owned(), "job-token".to_owned()),
  ]
  .into();

  // Lease 7 stands for its item's close and is renewed; lease 8 has ended with its item's close, and its item,
  // open or not, is not leased again.
  assert_eq!(pass(scratch.db(), &vars).await.unwrap(), [(5, Kept::Renewed), (6, Kept::Closed)]);
  assert!(
    !calls.lock().unwrap().iter().any(|(method, ..)| method == "POST"),
    "no lease is taken again"
  );
  assert!(
    calls
      .lock()
      .unwrap()
      .iter()
      .filter(|(method, ..)| method == "PATCH")
      .all(|(.., body)| body["frozen"].is_null()),
    "an ended job's lease holds nothing fixed"
  );
  assert_eq!(pass(scratch.db(), &vars).await.unwrap(), [(5, Kept::Renewed)]);
}

#[tokio::test]
async fn a_launched_job_s_lease_holds_fixed_the_labels_and_fields_its_launch_read_naming_its_agent() {
  let calls: Calls = Arc::default();
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());
  let app = axum::Router::new().fallback(jobs).with_state(calls.clone());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  let vars: BTreeMap<String, String> = [
    ("BMO_JOB_URL".to_owned(), address),
    ("BMO_JOB_TOKEN".to_owned(), "job-token".to_owned()),
  ]
  .into();

  hold_launch(&vars, 40, 7, Some("coder-a")).await;

  let asked = calls.lock().unwrap().clone();

  assert_eq!(asked.len(), 1, "{asked:?}");
  assert_eq!((asked[0].0.as_str(), asked[0].1.as_str()), ("PATCH", "/leases/7"));

  let frozen = &asked[0].2["frozen"];

  assert_eq!(frozen["labels"], json!(["agent", "role", "model", "repositories"]));
  assert_eq!(frozen["fields"], json!(["assigneeName", "scope"]));

  let reason = frozen["reason"].as_str().unwrap();

  assert!(reason.contains("job 40") && reason.contains("coder-a"), "{reason}");
}

#[tokio::test]
async fn a_running_job_s_item_waits_on_no_free_worker() {
  let scratch = Scratch::new().await;

  seeded(&scratch).await;

  let open = |id: i64| agency_server::watches::queue_idle::Open {
    id,
    title: format!("item {id}"),
    ..Default::default()
  };
  let standing = agency_server::watches::queue_idle::held(scratch.db(), vec![open(21), open(23), open(30)])
    .await
    .unwrap();
  let ids: Vec<i64> = standing.open.iter().map(|item| item.id).collect();

  assert_eq!(ids, [23, 30], "item 21's job is still at work: {ids:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_worker_with_a_running_job_reads_as_working_on_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");

  std::fs::create_dir_all(&data).unwrap();
  for worker in ["worker-a", "worker-c"] {
    repository(scratch.db(), worker, &format!("/checkouts/{worker}"), &[]).await;
  }
  seeded(&scratch).await;

  let server = crate::support::standing::stand_server(home.path(), &data, &scratch.address(), "secret");
  let read = |name: &'static str| async move {
    reqwest::Client::new()
      .get(format!("http://127.0.0.1:{}/repositories/{name}", server.port))
      .bearer_auth("secret")
      .send()
      .await
      .unwrap()
      .json::<Value>()
      .await
      .unwrap()["checkouts"][0]["job"]
      .clone()
  };
  let working = read("worker-a").await;
  let idle = read("worker-c").await;

  assert_eq!(
    (working["jobId"].as_i64(), working["itemId"].as_i64()),
    (Some(2), Some(21)),
    "{working}"
  );
  assert_eq!(idle, Value::Null);
}

// The jobs of the two passes below: 1 to 4 running, 5 and 6 ended with their lease kept.
async fn running_and_ended(scratch: &Scratch) {
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
                             "started_at", "prompt", "lease_kept")
         VALUES (1, 101, 11, 'harness', 1, 'w1', 'running', 1, 'work', false),
                (2, 102, 12, 'harness', 2, 'w1', 'running', 1, 'work', false),
                (3, 103, 13, 'harness', 3, 'w1', 'running', 1, 'work', false),
                (4, 104, 14, 'harness', 4, 'w1', 'running', 1, 'work', false),
                (5, 105, 15, 'harness', 5, 'w1', 'ended', 1, 'work', true),
                (6, 106, 16, 'harness', 6, 'w1', 'ended', 1, 'work', true)"#,
      vec![],
    )
    .await;
}

#[tokio::test]
async fn an_item_moved_under_a_running_job_is_told_once_to_its_orderer_and_the_carrying_item_s() {
  use crate::support::peers::{item, stand};

  let scratch = Scratch::new().await;
  let (peers, vars) = stand().await;

  running_and_ended(&scratch).await;

  {
    let mut held = peers.lock().unwrap();

    // 100 holds a room; 101 is held, raised by it; 102 dropped in a room of its own; 103 merged into 200, whose
    // room is another; 104 taken by another holder, its lease gone, a take refused. 105 and 106 stand leased.
    held.items.insert(100, item(100, "done", json!({ "scope": "-1000" })));
    held.items.insert(101, item(101, "held", json!({ "raisedBy": 100 })));
    held.items.insert(102, item(102, "dropped", json!({ "scope": "-1020" })));
    held
      .items
      .insert(103, item(103, "merged", json!({ "scope": "-1030", "mergedInto": 200 })));
    held.items.insert(200, item(200, "open", json!({ "scope": "-2000" })));
    held.items.insert(104, item(104, "open", json!({ "scope": "-1040" })));
    held.leases = [15, 16].into();
  }

  let kept = pass(scratch.db(), &vars).await.unwrap();

  assert!(kept.iter().take(4).all(|(_, done)| matches!(done, Kept::Left(_))), "{kept:?}");

  pass(scratch.db(), &vars).await.unwrap();

  let moved: Vec<(Value, Value)> = peers
    .lock()
    .unwrap()
    .instructed()
    .iter()
    .filter(|body| body["kind"].as_str().unwrap().starts_with("item-moved-under-job:"))
    .map(|body| (body["kind"].clone(), body["room"].clone()))
    .collect();

  assert_eq!(
    moved,
    [
      (json!("item-moved-under-job:101"), json!(-1000)),
      (json!("item-moved-under-job:102"), json!(-1020)),
      (json!("item-moved-under-job:103"), json!(-1030)),
      (json!("item-moved-under-job:103"), json!(-2000)),
      (json!("item-moved-under-job:104"), json!(-1040)),
    ],
    "each once, in its orderer's room, a merge's carrying item's too"
  );

  let said = peers.lock().unwrap().instructed()[0]["said"]["order"].as_str().unwrap().to_owned();

  assert!(said.contains("held") && said.contains("job 1"), "{said}");

  // The item moves again, to another state: told again; the job ends: its mark let go.
  peers
    .lock()
    .unwrap()
    .items
    .insert(101, item(101, "dropped", json!({ "raisedBy": 100 })));
  pass(scratch.db(), &vars).await.unwrap();
  scratch.run(r#"UPDATE "jobs" SET "state" = 'stopped' WHERE "id" = 1"#, vec![]).await;
  pass(scratch.db(), &vars).await.unwrap();

  let told_101 = peers
    .lock()
    .unwrap()
    .instructed()
    .iter()
    .filter(|body| body["kind"] == "item-moved-under-job:101")
    .count();

  assert_eq!(told_101, 2, "held, then dropped");
  assert_eq!(
    agency_server::queue::settings::get(scratch.db(), "job_moved_told:1").await.unwrap(),
    None,
    "the mark let go once the job no longer runs"
  );
}

#[tokio::test]
async fn an_ended_job_s_item_left_leased_is_told_once_for_its_lease() {
  use crate::support::peers::{item, stand};

  let scratch = Scratch::new().await;
  let (peers, vars) = stand().await;

  running_and_ended(&scratch).await;
  scratch.run(r#"DELETE FROM "jobs" WHERE "state" = 'running'"#, vec![]).await;

  {
    let mut held = peers.lock().unwrap();

    held.items.insert(100, item(100, "done", json!({ "scope": "-1000" })));
    held.items.insert(105, item(105, "leased", json!({ "labels": { "raised-by": "100" } })));
    held.items.insert(106, item(106, "leased", json!({})));
    held.leases = [15, 16].into();
  }

  let awaiting = |peers: &crate::support::peers::Peers| -> Vec<(Value, Value)> {
    peers
      .lock()
      .unwrap()
      .instructed()
      .iter()
      .filter(|body| body["kind"].as_str().unwrap().starts_with("job-awaits-close:"))
      .map(|body| (body["kind"].clone(), body["room"].clone()))
      .collect()
  };

  pass(scratch.db(), &vars).await.unwrap();
  assert!(awaiting(&peers).is_empty(), "the first pass only marks it");

  pass(scratch.db(), &vars).await.unwrap();
  pass(scratch.db(), &vars).await.unwrap();
  agency_server::outbox::restarted(scratch.db()).await.unwrap();
  agency_server::outbox::send_due(scratch.db(), &vars).await.unwrap();
  assert_eq!(
    awaiting(&peers),
    [
      (json!("job-awaits-close:105"), json!(-1000)),
      (json!("job-awaits-close:106"), Value::Null)
    ],
    "told once each for its lease, never again on a timer or a start, a derived item's in its parent's room, one \
     naming none in the owner's private room"
  );

  let said = peers.lock().unwrap().instructed()[0]["said"]["order"].as_str().unwrap().to_owned();

  assert!(said.contains("lease 15"), "{said}");

  // The close ends the lease: the mark let go.
  peers.lock().unwrap().leases.remove(&15);
  pass(scratch.db(), &vars).await.unwrap();
  assert_eq!(
    agency_server::queue::settings::get(scratch.db(), "job_close_mark:5").await.unwrap(),
    None
  );
  assert!(
    agency_server::queue::settings::get(scratch.db(), "job_close_mark:6")
      .await
      .unwrap()
      .is_some()
  );
}

#[tokio::test]
async fn an_ended_job_s_item_whose_commit_waits_for_its_deployment_is_never_told_as_left_leased() {
  use crate::support::peers::{item, stand};

  let scratch = Scratch::new().await;
  let (peers, vars) = stand().await;

  running_and_ended(&scratch).await;
  scratch.run(r#"DELETE FROM "jobs" WHERE "id" <> 5"#, vec![]).await;
  // A notice of it told before, standing.
  scratch
    .run(
      r#"INSERT INTO "notices" ("key", "route", "body", "keep", "instruction_id", "owed_at", "told_at")
         VALUES ('job-awaits-close:105', 'instruction', '{}', 'standing', 7, 1, 2)"#,
      vec![],
    )
    .await;

  {
    let mut held = peers.lock().unwrap();
    let labels = json!({ "labels": { "close-phase": "waiting-batch", "close-commit": "c1" } });

    held.items.insert(105, item(105, "leased", labels));
    held.leases = [15].into();
  }

  for _ in 0..3 {
    pass(scratch.db(), &vars).await.unwrap();
  }

  let held = peers.lock().unwrap();

  assert!(held.instructed().is_empty(), "{:?}", held.instructed());
  assert_eq!(held.withdrawn(), ["7"], "the notice told before withdrawn");
}
