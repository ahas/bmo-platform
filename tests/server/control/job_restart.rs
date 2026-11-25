//! A job across restarts of agency's own process (docs/api.md, 「assignments and jobs」): a job launched on a
//! running server and waiting for a road, then for its agent's slot, is carried by no turn a program holds, so
//! the process is killed in the middle of each wait and another started on the same store and data root. Each
//! new process launches the job again instead of reaping it: one waiting for a road waits for it again, and one
//! waiting for its slot takes up the very slot request the killed one left waiting in the line, and once the
//! agent is free the job goes out through that request.

use std::time::Duration;

use axum::http::StatusCode;
use harness_providers::ARCHIVE_PROVIDERS;
use serde_json::{Value, json};

use agency_server::control::work::{REAPED, RESUMED};

use crate::support::grounds::{OrderEntry, set_provider_order, signed_in};
use crate::support::rows::repository;
use crate::support::scratch::Scratch;
use crate::support::standing;

const TOKEN: &str = "secret";

async fn get(port: u16, path: &str) -> Value {
  reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(TOKEN)
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap_or(Value::Null)
}

async fn post(port: u16, path: &str, body: Value) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .post(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(TOKEN)
    .json(&body)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

// The job as the server answers it once `until` holds of it, or as it stands after five seconds.
async fn job_once(port: u16, id: i64, until: impl Fn(&Value) -> bool) -> Value {
  let mut job = Value::Null;

  for _ in 0..600 {
    job = get(port, &format!("/jobs/{id}")).await;

    if until(&job) {
      break;
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
  }

  job
}

// The job's slot requests: each token and state, oldest first.
async fn requests(scratch: &Scratch, id: i64) -> Vec<(String, String)> {
  store::all(
    scratch.db(),
    r#"SELECT "token", "state" FROM "role_slot_requests" WHERE "owner" = $1 ORDER BY "sequence""#,
    vec![format!("job-{id}").into()],
  )
  .await
  .unwrap()
  .iter()
  .map(|row| (row.try_get("", "token").unwrap(), row.try_get("", "state").unwrap()))
  .collect()
}

// The job's slot requests once none of them is still held: the job's end is written before its turn's end
// gives its slot back, so a read made as the end shows may find the request not yet released.
async fn requests_let_go(scratch: &Scratch, id: i64) -> Vec<(String, String)> {
  let mut left = requests(scratch, id).await;

  for _ in 0..600 {
    if left.iter().all(|(_, state)| state != "active" && state != "waiting") {
      break;
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
    left = requests(scratch, id).await;
  }

  left
}

// The job aged past the launch window, so a start that did not launch it again would reap it at once.
async fn aged(scratch: &Scratch, id: i64) {
  scratch
    .run(
      r#"UPDATE "jobs" SET "started_at" = $1 WHERE "id" = $2"#,
      vec![(agency_server::now_ms() - 20 * 60 * 1000).into(), id.into()],
    )
    .await;
}

// How many times the job's log says a start launched it again.
async fn resumed(scratch: &Scratch, id: i64) -> i64 {
  scratch
    .count(
      r#"SELECT COUNT(*) FROM "agent_log" WHERE "job_id" = $1 AND "text" = $2"#,
      vec![id.into(), RESUMED.into()],
    )
    .await
}

// A tool host on the socket given, issuing one token to every record and taking every token back.
fn tool_host(socket: &std::path::Path) {
  let listener = tokio::net::UnixListener::bind(socket).unwrap();
  let app = axum::Router::new()
    .route(
      "/tool-sessions",
      axum::routing::post(|| async { (StatusCode::CREATED, axum::Json(json!({ "token": "job-token" }))) }),
    )
    .fallback(|| async { StatusCode::NO_CONTENT });

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_waiting_for_a_road_then_its_slot_is_launched_again_by_each_process_started_after_a_kill() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let login = home.path().join("login-a");
  let host = home.path().join("mcp-host.sock");

  std::fs::create_dir_all(&data).unwrap();
  signed_in(&login);
  std::fs::create_dir_all(home.path().join("helper")).unwrap();
  tool_host(&host);
  repository(scratch.db(), "helper", &home.path().join("helper").to_string_lossy(), &[]).await;
  scratch.run(r#"DELETE FROM "claude_accounts""#, vec![]).await;
  scratch
    .run(
      r#"INSERT INTO "claude_accounts" ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at")
         VALUES ('login-a', 'login-a', 1, 1, 'directory', $1, 0)"#,
      vec![login.to_string_lossy().into_owned().into()],
    )
    .await;
  // The login rests, so the job is taken while its agent is free and first waits for a road.
  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('rest_until:login-a', $1)"#,
      vec![(chrono::Utc::now().timestamp() + 3600).to_string().into()],
    )
    .await;
  set_provider_order(
    scratch.db(),
    &ARCHIVE_PROVIDERS
      .iter()
      .map(|name| OrderEntry {
        name: (*name).to_owned(),
        enabled: *name == "claude-code",
      })
      .collect::<Vec<_>>(),
  )
  .await
  .unwrap();

  let host_socket = host.to_string_lossy().into_owned();
  let variables = [("BMO_MCP_HOST_SOCKET", host_socket.as_str()), ("BMO_MCP_HOST_ISSUER_TOKEN", "issuer")];
  let first = standing::stand_server_with(home.path(), &data, &scratch.address(), TOKEN, &variables);

  crate::support::rows::seated_on_order(scratch.db(), "worker").await;

  let agent = store::one(
    scratch.db(),
    r#"SELECT "name" FROM "role_agents" WHERE "role" = 'worker' AND "provider" = 'claude-code'"#,
    vec![],
  )
  .await
  .unwrap()
  .expect("an agent of the worker role stands on Claude Code")
  .try_get::<String>("", "name")
  .unwrap();

  let item = json!({
    "title": "A harmless item",
    "brief": "Answer that nothing is to be done.",
    "notes": [],
    "labels": { "agent": agent },
  });
  let (status, assignment) = post(
    first.port,
    "/assignments",
    json!({ "itemId": 41, "holderName": "harness", "assigneeName": "helper", "item": item }),
  )
  .await;

  assert_eq!(status, 201, "{assignment}");

  let (status, job) = post(
    first.port,
    "/jobs",
    json!({
      "assignmentId": assignment["id"],
      "itemId": 41,
      "leaseId": 1,
      "holderName": "harness",
      "item": item,
    }),
  )
  .await;

  assert_eq!(status, 201, "{job}");

  let id = job["id"].as_i64().unwrap();
  let on_road = job_once(first.port, id, |job| job["heldFor"].is_string()).await;

  assert!(on_road["heldFor"].as_str().unwrap_or_default().contains("login-a"), "{on_road}");
  assert!(requests(&scratch, id).await.is_empty(), "no slot is asked while no road answers");

  // The process is killed in the middle of the wait for a road. The job is aged past the launch window, so a
  // start that did not launch it again would reap it at once.
  drop(first);
  aged(&scratch, id).await;
  // The wait the killed process wrote is cleared, so the wait read next is the new process's own.
  scratch
    .run(r#"UPDATE "jobs" SET "held_for" = NULL WHERE "id" = $1"#, vec![id.into()])
    .await;

  let middle = standing::stand_server_with(home.path(), &data, &scratch.address(), TOKEN, &variables);
  let for_road = |job: &Value| job["heldFor"].as_str().is_some_and(|why| why.contains("login-a"));
  let again = job_once(middle.port, id, for_road).await;

  assert_eq!(again["state"], "starting", "{again}");
  assert!(for_road(&again), "the job waits for a road again: {again}");
  assert_eq!(again["report"], Value::Null, "{again}");
  assert_eq!(resumed(&scratch, id).await, 1);

  // The agent takes another run meanwhile, and the login reopens: the job's turn now waits for its agent's slot.
  scratch
    .run(
      r#"INSERT INTO "role_slot_requests" ("token", "request_key", "owner", "source", "role", "path", "provider",
           "model", "effort", "capacity", "state", "enqueued_at", "class", "seat")
         SELECT 'busy-' || "name", 'busy-' || "name", 'busy', 'job', "role", '/elsewhere/', "provider", "model", '',
           1, 'active', 0, 'foreground', "name"
         FROM "role_agents" WHERE "name" = $1"#,
      vec![agent.clone().into()],
    )
    .await;

  scratch
    .run(r#"DELETE FROM "settings" WHERE "key" = 'rest_until:login-a'"#, vec![])
    .await;

  let (status, _) = post(middle.port, "/events", json!({ "kind": "rest.changed", "data": {} })).await;

  assert_eq!(status, 201);

  let waits = |job: &Value| job["heldFor"].as_str().is_some_and(|why| why.contains("waits for the agent"));
  let held = job_once(middle.port, id, waits).await;

  assert_eq!(held["state"], "starting", "{held}");
  assert!(waits(&held), "the job waits for its agent's slot: {held}");

  let left = requests(&scratch, id).await;

  assert_eq!(left.len(), 1, "{left:?}");
  assert_eq!(left[0].1, "waiting");

  let words =
    store::one(scratch.db(), r#"SELECT "turn_words" FROM "jobs" WHERE "id" = $1"#, vec![id.into()])
      .await
      .unwrap()
      .unwrap()
      .try_get::<Option<String>>("", "turn_words")
      .unwrap();

  assert!(
    words.is_some_and(|words| words.contains("Answer that nothing is to be done.")),
    "the job keeps the words of its first turn while it waits"
  );

  // The process is killed in the middle of the wait for the slot, the job aged again.
  drop(middle);
  aged(&scratch, id).await;
  scratch
    .run(r#"UPDATE "jobs" SET "held_for" = NULL WHERE "id" = $1"#, vec![id.into()])
    .await;

  let second = standing::stand_server_with(home.path(), &data, &scratch.address(), TOKEN, &variables);

  // The new process launched the job again before its routes stood, and the pass that ends a job left running
  // left it: it still waits, on the request the killed process left in the line, no second one asked.
  let held = job_once(second.port, id, waits).await;

  assert_eq!(held["state"], "starting", "{held}");
  assert!(waits(&held), "{held}");
  assert_eq!(held["report"], Value::Null, "{held}");
  assert_eq!(resumed(&scratch, id).await, 2);
  assert_eq!(requests(&scratch, id).await, left, "the request that waited is taken up again");

  // The agent is free again: the job is let in on that request and goes out. With no herdr to stand its turn
  // in, the turn fails and the job ends saying why, not reaped.
  scratch.run(r#"DELETE FROM "role_slot_requests" WHERE "owner" = 'busy'"#, vec![]).await;

  let ended =
    job_once(second.port, id, |job| !matches!(job["state"].as_str(), Some("starting" | "running"))).await;
  let report = ended["report"].as_str().unwrap_or_default();

  assert_eq!(ended["state"], "ended", "{ended}");
  assert_eq!(ended["outcome"], "failed", "{ended}");
  assert!(!report.contains(REAPED), "{ended}");

  let after = requests_let_go(&scratch, id).await;

  assert_eq!(after.len(), 1, "{after:?}");
  assert_eq!(
    after[0],
    (left[0].0.clone(), "released".to_owned()),
    "the job went out through that request"
  );
}
