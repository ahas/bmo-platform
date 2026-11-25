//! A job's end reaching the room, its lease kept for the item's close or given back (docs/api.md,
//! 「assignments and jobs」): the order the room's supervisor is handed; an ended job's lease kept standing and
//! renewed until the close moves the item to done, so the item never passes through open; and a stopped job's
//! lease given back so the item is open for a rerun.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use agency_server::Service;
use agency_server::config::Config;
use agency_server::job_end::{delivered, delivered_with, instruction_body, order};
use agency_server::job_leases::{Kept, pass};
use agency_server::places::Ground;
use agency_server::queue::orderer::Orderer;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::IntoResponse;
use harness_providers::Machine;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::support::scratch::Scratch;
use crate::support::standing;
use crate::support::wording::assert_no_instruction;

fn member(report: Option<&str>, outcome: Option<&str>) -> Value {
  json!({
    "id": 906369,
    "itemId": 2224,
    "leaseId": 64,
    "workerName": "bmo-agency",
    "runId": 5938,
    "runName": "job-906369",
    "roomId": 90000001,
    "state": "ended",
    "outcome": outcome,
    "report": report,
  })
}

// A job its review took had its report told by the review's checks, so its end names that report and the item
// rather than carrying it again (docs/job-review.md, 「The job's end」).
#[test]
fn an_ended_job_s_order_names_the_report_its_review_passed_and_carries_none_of_it() {
  let said = order(&member(Some("The change landed in commit abc123.\n\nWhat is left: a deploy."), None));

  assert!(said.contains("item #2224"));
  assert!(said.contains("worker bmo-agency"));
  assert!(!said.contains("The change landed in commit abc123."), "{said}");
  assert!(said.contains("Its report is the one its review passed"), "{said}");
  assert!(said.contains("job_reviews on item #2224 reads it again."), "{said}");
  assert_no_instruction(&said);
}

// A job no review examined — one stopped, failed or blocked — carries its report whole.
#[test]
fn a_job_that_ended_unreviewed_carries_its_report_whole() {
  let mut stopped = member(
    Some("Stopped on the owner's word.\n\nThe branch holds the work so far."),
    Some("stopped"),
  );

  stopped["state"] = json!("stopped");

  let said = order(&stopped);

  assert!(said.contains("Its report, as the run left it:"), "{said}");
  assert!(said.contains("Stopped on the owner's word."));
  assert!(said.contains("The branch holds the work so far."));
}

#[test]
fn the_lease_line_names_the_lease_and_asks_nothing() {
  let said = order(&member(Some("The work is done."), None));

  assert!(said.contains("lease 64"), "the order names the lease its close ends: {said}");
  assert!(said.contains("not closed"));
  assert_no_instruction(&said);
}

#[test]
fn a_job_that_left_no_report_says_so_and_a_stop_carries_its_outcome() {
  let said = order(&member(None, Some("failed")));

  assert!(said.contains("ended, its outcome failed"));
  assert!(!said.contains("ended ended"));
  assert!(said.contains("The run left no report; its end is all there is to tell."));
}

#[test]
fn the_instruction_names_the_item_as_queue_news_and_the_room_the_job_answers() {
  let body = instruction_body(
    &member(Some("done"), None),
    &Orderer {
      room: Some(90000001),
      requester: Some("agent-a".to_owned()),
    },
  );

  assert_eq!(body["room"], 90000001);
  assert_eq!(body["sort"], "job");
  assert_eq!(body["agentName"], "agent-a");
  assert_eq!(body["kind"], "job-ended:2224");
  assert_eq!(body["events"], json!(["job-ended:#2224"]));
  assert_eq!(body["run"], 5938);
  assert_eq!(body["summary"], "The job of item #2224 ended");
  assert!(
    body["said"]["order"]
      .as_str()
      .unwrap()
      .contains("The job of item #2224 on worker bmo-agency ended.")
  );
}

#[derive(Clone, Debug)]
struct Call {
  method: Method,
  path: String,
  authorization: Option<String>,
  body: Value,
}

// The job service and the receiver as the end reaches them: every call heard, and item 2224's every state
// in order, the lease 64 standing on it from the start.
#[derive(Default)]
struct Heard {
  calls: Vec<Call>,
  item: Vec<String>,
}

type Calls = Arc<Mutex<Heard>>;

async fn stand_in(
  State(heard): State<Calls>,
  method: Method,
  uri: Uri,
  headers: HeaderMap,
  body: Bytes,
) -> impl IntoResponse {
  let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
  let mut heard = heard.lock().unwrap();

  heard.calls.push(Call {
    method: method.clone(),
    path: uri.path().to_owned(),
    authorization: headers.get("authorization").and_then(|value| value.to_str().ok()).map(str::to_owned),
    body: body.clone(),
  });

  if heard.item.is_empty() {
    heard.item.push("leased".to_owned());
  }

  let leased = heard.item.last().map(String::as_str) == Some("leased");

  match (method.as_str(), uri.path()) {
    // The item the job is a run of, which names the agent that filed it.
    ("GET", "/items/2224") => {
      (StatusCode::OK, axum::Json(json!({ "id": 2224, "requesterName": "agent-a" }))).into_response()
    }
    // The lease given back opens the item; renewed, it stands while the item is leased, and is gone after.
    ("DELETE", "/leases/64") => {
      heard.item.push("open".to_owned());
      StatusCode::NO_CONTENT.into_response()
    }
    ("PATCH", "/leases/64") if leased => (StatusCode::OK, axum::Json(json!({ "id": 64 }))).into_response(),
    ("PATCH", "/leases/64") => (StatusCode::GONE, axum::Json(json!({ "code": "gone" }))).into_response(),
    // The close moves a leased item to done, which ends its lease.
    ("PATCH", "/items/2224") if body["state"] == "done" && leased => {
      heard.item.push("done".to_owned());
      (StatusCode::OK, axum::Json(json!({ "id": 2224, "state": "done" }))).into_response()
    }
    ("PATCH", "/items/2224") => StatusCode::CONFLICT.into_response(),
    _ => StatusCode::NO_CONTENT.into_response(),
  }
}

async fn served() -> (String, Calls) {
  let heard: Calls = Arc::default();
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());
  let app = axum::Router::new().fallback(stand_in).with_state(heard.clone());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  (address, heard)
}

fn peers(address: &str) -> BTreeMap<String, String> {
  [
    ("BMO_JOB_URL".to_owned(), address.to_owned()),
    ("BMO_JOB_TOKEN".to_owned(), "job-token".to_owned()),
    ("BMO_RECEIVER_URL".to_owned(), address.to_owned()),
    ("BMO_RECEIVER_TOKEN".to_owned(), "receiver-token".to_owned()),
  ]
  .into()
}

fn ground(scratch: &Scratch, home: &std::path::Path) -> Ground {
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), home.to_string_lossy().into_owned()),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
    ],
    home.join("data"),
  );

  Ground::new(&service, &machine, None)
}

#[tokio::test]
async fn an_ended_job_keeps_its_lease_through_the_close_and_its_item_never_passes_through_open() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (address, heard) = served().await;
  let vars = peers(&address);
  let ended = member(Some("The work is done."), None);

  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
                             "started_at", "prompt")
         VALUES (906369, 2224, 64, 'harness', 1, 'bmo-agency', 'ended', 1, 'work')"#,
      vec![],
    )
    .await;

  // The end as the server delivers it: the lease kept for the close, and the room told.
  delivered(&ground(&scratch, home.path()), &ended).await;
  delivered_with(scratch.db(), &vars, &ended).await;

  {
    let heard = heard.lock().unwrap();
    let told = heard.calls.iter().find(|call| call.path == "/instructions").expect("the room is told");

    assert!(heard.calls.iter().all(|call| call.method != Method::DELETE), "no lease is given back");
    assert_eq!(told.method, Method::POST);
    assert_eq!(told.authorization.as_deref(), Some("Bearer receiver-token"));
    assert_eq!(told.body["room"], 90000001);
    assert_eq!(told.body["sort"], "job");
    assert_eq!(told.body["agentName"], "agent-a");
    assert_eq!(told.body["events"], json!(["job-ended:#2224"]));

    let order = told.body["said"]["order"].as_str().unwrap();

    assert!(order.contains("Its report is the one its review passed"), "{order}");
    assert!(order.contains("lease 64"), "the order names the lease its close ends: {order}");
  }

  // The lease pass keeps renewing it while the requester has not closed the item.
  assert_eq!(pass(scratch.db(), &vars).await.unwrap(), [(906369, Kept::Renewed)]);

  // The requester's close: the item moved to done on the lease that stands, which ends the lease.
  let closed = reqwest::Client::new()
    .patch(format!("{address}/items/2224"))
    .json(&json!({ "state": "done" }))
    .send()
    .await
    .unwrap();

  assert_eq!(closed.status(), StatusCode::OK, "the close found its item still leased");

  // The next pass finds the lease ended with the close, lets it go and takes no new one.
  assert_eq!(pass(scratch.db(), &vars).await.unwrap(), [(906369, Kept::Closed)]);
  assert_eq!(pass(scratch.db(), &vars).await.unwrap(), []);

  let heard = heard.lock().unwrap();

  assert_eq!(heard.item, ["leased", "done"], "the item never passed through open");
  assert!(heard.calls.iter().all(|call| !(call.method == Method::POST && call.path == "/leases")));
}

#[tokio::test]
async fn a_stopped_job_gives_its_lease_back_so_its_item_is_open_for_a_rerun() {
  let (address, heard) = served().await;
  let mut stopped = member(Some("Stopped on the owner's word."), Some("stopped"));

  stopped["state"] = json!("stopped");

  let scratch = Scratch::new().await;

  delivered_with(scratch.db(), &peers(&address), &stopped).await;

  let heard = heard.lock().unwrap();
  let lease = heard
    .calls
    .iter()
    .find(|call| call.method == Method::DELETE)
    .expect("the lease is given back");

  assert_eq!(lease.path, "/leases/64");
  assert_eq!(lease.authorization.as_deref(), Some("Bearer job-token"));
  assert_eq!(heard.item, ["leased", "open"]);

  let order = heard.calls.iter().find(|call| call.path == "/instructions").unwrap().body["said"]["order"]
    .as_str()
    .unwrap()
    .to_owned();

  assert!(!order.contains("lease 64"), "a stopped job's order names no lease: {order}");
}

/// A stand-in no call should reach, counting every request it is sent.
async fn unheard() -> (String, Arc<AtomicUsize>) {
  let calls: Arc<AtomicUsize> = Arc::default();
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());
  let counted = calls.clone();
  let app = axum::Router::new().fallback(move || {
    let counted = counted.clone();

    async move {
      counted.fetch_add(1, Ordering::SeqCst);

      StatusCode::NO_CONTENT
    }
  });

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  (address, calls)
}

#[tokio::test]
async fn a_test_ground_reaches_no_peer_even_one_it_names() {
  let scratch = Scratch::new().await;
  let (address, calls) = unheard().await;
  let vars: BTreeMap<String, String> = [
    ("BMO_JOB_URL".to_owned(), address.clone()),
    ("BMO_JOB_TOKEN".to_owned(), "job-token".to_owned()),
    ("BMO_RECEIVER_URL".to_owned(), address.clone()),
    ("BMO_RECEIVER_TOKEN".to_owned(), "receiver-token".to_owned()),
    ("BMO_ARTIFACT_URL".to_owned(), address.clone()),
    ("BMO_ARTIFACT_TOKEN".to_owned(), "artifact-token".to_owned()),
    ("BMO_TEST_GROUND".to_owned(), "1".to_owned()),
  ]
  .into();

  delivered_with(scratch.db(), &vars, &member(Some("The work is done."), None)).await;
  assert!(agency_server::job_leases::pass(scratch.db(), &vars).await.unwrap().is_empty());
  agency_server::watches::queue_idle::pass(scratch.db(), &vars, &crate::support::gates::none())
    .await
    .unwrap();

  assert_eq!(calls.load(Ordering::SeqCst), 0, "a test ground made a peer call");
}

#[tokio::test]
async fn a_test_server_s_environment_carries_no_peer_of_the_deployment() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");

  std::fs::create_dir_all(&data).unwrap();

  let server = standing::stand_server(home.path(), &data, &scratch.address(), "secret");
  let environment = std::fs::read(format!("/proc/{}/environ", server.pid())).unwrap();
  let held: Vec<String> = environment
    .split(|byte| *byte == 0)
    .map(|entry| String::from_utf8_lossy(entry).into_owned())
    .collect();

  for name in [
    "BMO_RECEIVER_URL",
    "BMO_RECEIVER_TOKEN",
    "BMO_RECEIVER_TOKEN_FILE",
    "BMO_JOB_URL",
    "BMO_JOB_TOKEN",
    "BMO_JOB_TOKEN_FILE",
    "AGENCY_BRAIN_URL",
    "BMO_GATE_URL",
    "BMO_ARTIFACT_URL",
  ] {
    assert!(
      !held.iter().any(|entry| entry.starts_with(&format!("{name}="))),
      "the test server inherited {name}"
    );
  }
}
