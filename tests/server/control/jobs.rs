//! A job asked of a running server in the shapes the harness sends (docs/api.md, 「assignments and jobs」): a
//! launch taken while nothing in the order can answer, the job waiting `starting` with every provider's reason
//! and holding no slot, and going out once a login reopens; a turn that fails ending the job with why, written
//! into its log; the end releasing the assignment, so the worker is held again for the next item; and a job
//! whose first ask for a slot would wait written waiting, its launch answered with that wait.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::support::grounds::{OrderEntry, set_provider_order, signed_in};
use axum::http::StatusCode;
use harness_providers::ARCHIVE_PROVIDERS;
use serde_json::{Value, json};

use crate::support::binary;
use crate::support::rows::repository;
use crate::support::scratch::Scratch;
use crate::support::standing;

async fn get(port: u16, token: &str, path: &str) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

async fn post(port: u16, token: &str, path: &str, body: Value) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .post(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .json(&body)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

async fn patch(port: u16, token: &str, path: &str, body: Value) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .patch(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .json(&body)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

// Every job's item names its agent (docs/repositories.md, 「An agent an item names」): the worker role's first, as
// the store seeds it, where a test names no other.
const AGENT: &str = "worker-01";

fn item() -> Value {
  item_of(AGENT)
}

fn item_of(agent: &str) -> Value {
  json!({
    "title": "A harmless item",
    "brief": "Answer that nothing is to be done.",
    "notes": [],
    "labels": { "agent": agent },
  })
}

async fn assigned(port: u16, token: &str, item_id: i64) -> i64 {
  let (status, assignment) = post(
    port,
    token,
    "/assignments",
    json!({ "itemId": item_id, "holderName": "harness", "assigneeName": "helper", "item": item() }),
  )
  .await;

  assert_eq!(status, 201, "{assignment}");
  assignment["id"].as_i64().unwrap()
}

// A tool host on the socket given, issuing one token to every record and taking every token back: a run's
// program waits for its token until the host answers.
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

async fn run_job(port: u16, token: &str, assignment: i64, item_id: i64, item: Value) -> (u16, Value) {
  post(
    port,
    token,
    "/jobs",
    json!({
      "assignmentId": assignment,
      "itemId": item_id,
      "leaseId": 1,
      "holderName": "harness",
      "item": item,
    }),
  )
  .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_waits_for_a_login_ends_with_why_and_frees_its_worker() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let login = home.path().join("login-a");
  let host = home.path().join("mcp-host.sock");
  let token = "secret";
  let resting = chrono::Utc::now().timestamp() + 3600;

  std::fs::create_dir_all(&data).unwrap();

  // The server is handed the receiver's address and token as a test names them, as a worker's environment
  // would have handed it a live one. A test ground still makes no call: a job ending here must not post a
  // real job-ended instruction into a room.
  let heard: Arc<AtomicUsize> = Arc::default();
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let leaked_address = format!("http://{}", listener.local_addr().unwrap());
  let counted = heard.clone();

  tokio::spawn(async move {
    let routes = axum::Router::new().fallback(move || {
      let counted = counted.clone();

      async move {
        counted.fetch_add(1, Ordering::SeqCst);

        StatusCode::NO_CONTENT
      }
    });

    axum::serve(listener, routes).await.unwrap();
  });

  signed_in(&login);
  std::fs::create_dir_all(home.path().join("helper")).unwrap();
  tool_host(&host);
  repository(
    scratch.db(),
    "helper",
    &home.path().join("helper").to_string_lossy(),
    &[("support_count", 0i64.into())],
  )
  .await;
  scratch.run(r#"DELETE FROM "claude_accounts""#, vec![]).await;
  scratch
    .run(
      r#"INSERT INTO "claude_accounts" ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at")
         VALUES ('login-a', 'login-a', 1, 1, 'directory', $1, 0)"#,
      vec![login.to_string_lossy().into_owned().into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('rest_until:login-a', $1)"#,
      vec![resting.to_string().into()],
    )
    .await;
  // Claude Code is the one provider of the order the owner has on, so its logins are the whole order.
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

  let server = standing::stand_server_with(
    home.path(),
    &data,
    &scratch.address(),
    token,
    &[
      ("BMO_RECEIVER_URL", leaked_address.as_str()),
      ("BMO_RECEIVER_TOKEN", "leaked"),
      ("BMO_MCP_HOST_SOCKET", &host.to_string_lossy()),
      ("BMO_MCP_HOST_ISSUER_TOKEN", "issuer"),
    ],
  );
  let port = server.port;

  // The worker's agents stand one on each road of its row, Claude Code's among them, once the server has
  // settled the store.
  crate::support::rows::seated_on_order(scratch.db(), "worker").await;
  // The job's item names the agent on Claude Code's road, the one provider of the order the owner has on.
  let on_claude = store::one(
    scratch.db(),
    r#"SELECT "name" FROM "role_agents" WHERE "role" = 'worker' AND "provider" = 'claude-code'"#,
    vec![],
  )
  .await
  .unwrap()
  .expect("an agent of the worker role stands on Claude Code")
  .try_get::<String>("", "name")
  .unwrap();

  // An item naming a role the store does not hold is refused, and nothing is held for it.
  let (status, refused) = post(
    port,
    token,
    "/assignments",
    json!({ "itemId": 40, "holderName": "harness", "assigneeName": "helper", "roleName": "nobody", "item": item() }),
  )
  .await;

  assert_eq!(status, 422, "{refused}");
  assert_eq!(refused["code"], "unknown-role", "{refused}");

  // An assignee no repository holds a checkout by is no busy worker: it is refused as an unknown repository,
  // naming it and the repositories to choose from.
  let (status, refused) = post(
    port,
    token,
    "/assignments",
    json!({ "itemId": 40, "holderName": "harness", "assigneeName": "help", "item": item() }),
  )
  .await;

  assert_eq!((status, refused["code"].as_str()), (422, Some("unknown-repository")), "{refused}");
  assert!(refused["error"].as_str().unwrap().contains("named help;"), "{refused}");
  assert!(refused["error"].as_str().unwrap().ends_with(": helper"), "{refused}");

  let assignment = assigned(port, token, 41).await;

  // The repository's one checkout is held now, so another item waits for it to come free.
  let (status, refused) = post(
    port,
    token,
    "/assignments",
    json!({ "itemId": 42, "holderName": "harness", "assigneeName": "helper", "item": item() }),
  )
  .await;

  assert_eq!((status, refused["code"].as_str()), (422, Some("no-worker-free")), "{refused}");

  // An item naming no agent, or `auto`, is refused before any road is asked, nothing written and the assignment
  // left held, its sentence listing the agents of the role it would run as for its supervisor to name one.
  for labels in [json!({}), json!({ "agent": "auto" }), json!({ "agent": "auto", "model": "auto" })] {
    let (status, refused) = post(
      port,
      token,
      "/jobs",
      json!({
        "assignmentId": assignment,
        "itemId": 41,
        "leaseId": 1,
        "holderName": "harness",
        "item": { "title": "A harmless item", "brief": "Nothing.", "notes": [], "labels": labels },
      }),
    )
    .await;
    let said = refused["error"].as_str().unwrap_or_default();

    assert_eq!((status, refused["code"].as_str()), (422, Some("agent-unnamed")), "{refused}");
    assert!(said.contains("The agents of the worker role:"), "{refused}");
    assert!(said.contains(&format!("- {AGENT}: ")), "{refused}");
  }

  // An item naming an agent the agency does not register, one that follows a supervisor's road, or a role the
  // store does not hold, is refused the same way (docs/repositories.md, 「An agent an item names」).
  for (labels, code, quoted) in [
    (json!({ "agent": "nobody-07" }), "unknown-agent", "agent=nobody-07"),
    (json!({ "agent": "coder-01" }), "agent-names-no-road", "agent=coder-01"),
    (json!({ "role": "nothing" }), "unknown-role", "role=nothing"),
  ] {
    let (status, refused) = post(
      port,
      token,
      "/jobs",
      json!({
        "assignmentId": assignment,
        "itemId": 41,
        "leaseId": 1,
        "holderName": "harness",
        "item": { "title": "A harmless item", "brief": "Nothing.", "notes": [], "labels": labels },
      }),
    )
    .await;

    assert_eq!((status, refused["code"].as_str()), (422, Some(code)), "{refused}");
    assert!(refused["error"].as_str().unwrap().contains(quoted), "{refused}");
  }

  assert_eq!(scratch.count(r#"SELECT COUNT(*) FROM "jobs""#, vec![]).await, 0);
  assert_eq!(get(port, token, &format!("/assignments/{assignment}")).await.1["state"], "held");

  // Nothing in the order can answer: the job is taken all the same and waits `starting`, held with every
  // provider's reason and the moment the first login reopens, its assignment running and no slot held. It is
  // written so: the launch's own answer already names the wait.
  let (status, job) = run_job(port, token, assignment, 41, item_of(&on_claude)).await;

  assert_eq!(status, 201, "{job}");
  assert_eq!(job["state"], "starting", "{job}");

  let id = job["id"].as_i64().unwrap();
  let why = job["heldFor"].as_str().unwrap_or_default();

  assert!(why.contains("login-a is at its limit"), "{job}");
  assert!(why.contains("reopens on its own"), "{job}");

  let mut held = get(port, token, &format!("/jobs/{id}")).await.1;

  assert_eq!(held["state"], "starting", "{held}");
  assert_eq!(get(port, token, &format!("/assignments/{assignment}")).await.1["state"], "running");
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "role_slot_requests" WHERE "owner" = $1"#,
        vec![format!("job-{id}").into()],
      )
      .await,
    0,
    "a job waiting for a road holds no slot"
  );

  // The agent the item names carries another run, so once the login reopens and the rest's change wakes the
  // job, it is not let in on a slot: it still stands `starting`, its hold now naming the agent it waits for
  // (docs/api.md, 「assignments and jobs」).
  scratch
    .run(
      r#"INSERT INTO "role_slot_requests" ("token", "request_key", "owner", "source", "role", "path", "provider",
           "model", "effort", "capacity", "state", "enqueued_at", "class", "seat")
         SELECT 'busy-' || "name", 'busy-' || "name", 'busy', 'job', "role", '/elsewhere/', "provider", "model", '',
           1, 'active', 0, 'foreground', "name"
         FROM "role_agents" WHERE "role" = 'worker' AND "provider" = 'claude-code'"#,
      vec![],
    )
    .await;
  scratch
    .run(r#"DELETE FROM "settings" WHERE "key" = 'rest_until:login-a'"#, vec![])
    .await;

  let (status, _) = post(port, token, "/events", json!({ "kind": "rest.changed", "data": {} })).await;

  assert_eq!(status, 201);

  for _ in 0..600 {
    held = get(port, token, &format!("/jobs/{id}")).await.1;

    if held["heldFor"].as_str().is_some_and(|why| why.contains("waits for the agent")) {
      break;
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
  }

  assert_eq!(held["state"], "starting", "{held}");
  assert_eq!(
    held["heldFor"],
    format!("it waits for the agent {on_claude} of the worker role"),
    "{held}"
  );

  // The agents are free again: the job is let in, its hold is let go, and its turn, with no herdr to stand
  // it in, fails: the job ends saying why, in its report and in its log.
  scratch.run(r#"DELETE FROM "role_slot_requests" WHERE "owner" = 'busy'"#, vec![]).await;

  // A job's end is written in steps, its state first and its assignment's release last, its slot given back
  // between them, so the reads below wait for the release rather than for the state alone.
  let mut ended = Value::Null;

  for _ in 0..600 {
    ended = get(port, token, &format!("/jobs/{id}")).await.1;

    let over = ended["state"] != "starting" && ended["state"] != "running";

    if over && get(port, token, &format!("/assignments/{assignment}")).await.1["state"] == "released" {
      break;
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
  }

  assert_eq!(ended["state"], "ended", "{ended}");
  assert_eq!(ended["outcome"], "failed", "{ended}");
  assert!(!ended["report"].as_str().unwrap_or_default().is_empty(), "{ended}");
  assert_eq!(ended["heldFor"], Value::Null, "{ended}");
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "agent_log" WHERE "job_id" = $1 AND "kind" = 'note' AND "text" LIKE 'The job ended failed:%'"#,
        vec![id.into()],
      )
      .await,
    1
  );

  // The turn was a run of the ledger under the job's run name, closed as it failed, with nothing spent.
  let run = ended["runId"].as_i64().expect("the job carries its run");
  let (_, kept) = get(port, token, &format!("/runs/{run}")).await;

  assert_eq!(kept["unitName"], "job-1", "{kept}");
  assert_eq!(kept["state"], "closed", "{kept}");
  assert_eq!(kept["outcome"], "failed", "{kept}");
  assert_eq!(ended["usage"], Value::Null, "{ended}");
  assert_eq!(ended["tokenCount"], 0, "{ended}");

  // The job ran as its assignment's role, `worker` where the item named none, holding a slot of that role
  // keyed by its checkout, which its end gave back.
  assert_eq!(ended["roleName"], "worker", "{ended}");
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "role_slot_requests" WHERE "role" = 'worker' AND "source" = 'job'
             AND "place_key" = 'helper' AND "owner" = $1 AND "state" = 'released'"#,
        vec![format!("job-{id}").into()],
      )
      .await,
    1
  );

  // The end released the assignment, so the same worker is held for the next item.
  assert_eq!(get(port, token, &format!("/assignments/{assignment}")).await.1["state"], "released");

  let next = assigned(port, token, 42).await;

  // The agent is free and its road answers, but a request ahead in the line asks for that agent: the next job
  // is written waiting for it, and its launch's answer names the wait before its first turn has asked
  // (docs/api.md, 「assignments and jobs」).
  scratch
    .run(
      r#"INSERT INTO "role_slot_requests" ("token", "request_key", "owner", "source", "role", "path", "provider",
           "model", "effort", "capacity", "state", "enqueued_at", "class", "agent_wanted")
         VALUES ('ahead', 'ahead', 'ahead', 'job', 'worker', '/elsewhere/', 'claude-code', '', '', 1, 'waiting', 0,
           'foreground', $1)"#,
      vec![on_claude.clone().into()],
    )
    .await;

  let (status, waiting) = run_job(port, token, next, 42, item_of(&on_claude)).await;
  let waits = format!("it waits for the agent {on_claude} of the worker role");

  assert_eq!((status, waiting["state"].as_str()), (201, Some("starting")), "{waiting}");
  assert_eq!(waiting["heldFor"], waits, "{waiting}");

  // The request ahead asks no more, so the line lets the job in: its hold is let go as its turn goes out.
  let id = waiting["id"].as_i64().unwrap();
  let mut ended = Value::Null;

  for _ in 0..100 {
    ended = get(port, token, &format!("/jobs/{id}")).await.1;

    if ended["state"] == "ended" {
      break;
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
  }

  assert_eq!((ended["state"].as_str(), &ended["heldFor"]), (Some("ended"), &Value::Null), "{ended}");

  assert_eq!(heard.load(Ordering::SeqCst), 0, "the test server posted to the leaked receiver");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_assignment_whose_job_ended_is_released_as_the_assignments_are_read() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  repository(scratch.db(), "helper", &home.path().join("helper").to_string_lossy(), &[]).await;
  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at", "changed_at")
         VALUES (7, 41, 'harness', 'helper', 'running', 0, 0)"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
              "outcome", "token_count", "input_tokens", "output_tokens", "cache_read_tokens",
              "cache_write_tokens", "started_at", "ended_at", "prompt")
         VALUES (41, 1, 'harness', 7, 'helper', 'ended', 'failed', 1111, 1, 10, 1000, 100, 0, 1, '# A harmless item')"#,
      vec![],
    )
    .await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  assert_eq!(get(port, token, "/assignments/7").await.1["state"], "released");

  // The job answers what its turn spent, apart and together.
  let (_, jobs) = get(port, token, "/jobs").await;
  let job = &jobs["items"][0];

  assert_eq!(
    job["usage"],
    json!({ "inputTokens": 1, "outputTokens": 10, "cacheReadTokens": 1000, "cacheWriteTokens": 100 })
  );
  assert_eq!(job["tokenCount"], 1111);

  // A job and an assignment name the repository their checkout is of beside the checkout, and narrow by it.
  assert_eq!((&job["workerName"], &job["repositoryName"]), (&json!("helper"), &json!("helper")));
  assert_eq!(get(port, token, "/assignments/7").await.1["repositoryName"], "helper");
  assert_eq!(
    get(port, token, "/jobs?repositoryName=helper").await.1["items"]
      .as_array()
      .unwrap()
      .len(),
    1
  );
  assert_eq!(get(port, token, "/jobs?repositoryName=elsewhere").await.1["items"], json!([]));
  assert_eq!(get(port, token, "/jobs?roleName=nobody").await.1["items"], json!([]));
  assert_eq!(
    get(port, token, "/assignments?repositoryName=helper").await.1["items"]
      .as_array()
      .unwrap()
      .len(),
    1
  );
  assigned(port, token, 42).await;
}

// A job whose agent names no road is refused at its start, whatever holder asks it, since no supervisor asks a
// job: nothing is written and the assignment stays held (docs/repositories.md, 「An agent an item names」).
#[tokio::test(flavor = "multi_thread")]
async fn a_job_whose_agent_names_no_road_is_refused() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  std::fs::create_dir_all(home.path().join("helper")).unwrap();
  repository(scratch.db(), "helper", &home.path().join("helper").to_string_lossy(), &[]).await;
  scratch.run(r#"DELETE FROM "claude_accounts""#, vec![]).await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  // The worker's agents are switched on and name no road.
  scratch
    .run(
      r#"UPDATE "role_agents" SET "provider" = NULL, "model" = NULL WHERE "role" = 'worker'"#,
      vec![],
    )
    .await;

  let assignment = assigned(port, token, 60).await;
  let (status, refused) = post(
    port,
    token,
    "/jobs",
    json!({
      "assignmentId": assignment,
      "itemId": 60,
      "leaseId": 1,
      "holderName": "harness",
      "item": item(),
    }),
  )
  .await;

  assert_eq!((status, refused["code"].as_str()), (422, Some("agent-names-no-road")), "{refused}");

  let why = refused["error"].as_str().unwrap();

  assert!(why.contains(&format!("agent={AGENT}")), "{why}");
  assert_eq!(scratch.count(r#"SELECT COUNT(*) FROM "jobs""#, vec![]).await, 0);
  assert_eq!(get(port, token, &format!("/assignments/{assignment}")).await.1["state"], "held");
}

// A job's small-model instruction (docs/worker-bench.md, 「A small-model instruction」): refused naming the
// field it lacks, kept and answered whole, rendered under the item's title ahead of its brief, and
// composed into the one line a work-order step is written as.
#[tokio::test(flavor = "multi_thread")]
async fn a_job_carries_its_instruction_whole_into_its_prompt_and_a_step_s_line() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";
  let instruction = json!({
    "repository": "services/area",
    "documents": ["docs/area.md"],
    "sentences": ["A refused call names the field it lacks."],
    "checks": ["cargo test --test area"],
  });

  std::fs::create_dir_all(&data).unwrap();
  repository(scratch.db(), "helper", &home.path().join("helper").to_string_lossy(), &[]).await;
  scratch.run(r#"DELETE FROM "claude_accounts""#, vec![]).await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let assignment = assigned(port, token, 43).await;
  let job = |instruction: Value| {
    json!({
      "assignmentId": assignment,
      "itemId": 43,
      "leaseId": 1,
      "holderName": "harness",
      "item": item(),
      "instruction": instruction,
    })
  };

  let mut lacking = instruction.clone();
  lacking["sentences"] = json!([]);
  let (status, refused) = post(port, token, "/jobs", job(lacking)).await;

  assert_eq!(status, 400, "{refused}");
  assert!(refused["error"].as_str().unwrap().contains("instruction.sentences"), "{refused}");

  let (status, created) = post(port, token, "/jobs", job(instruction.clone())).await;

  assert_eq!(status, 201, "{created}");
  assert_eq!(created["instruction"], instruction);

  let id = created["id"].as_i64().unwrap();
  let prompt = get(port, token, &format!("/jobs/{id}/prompt")).await.1["text"]
    .as_str()
    .unwrap()
    .to_owned();
  let rendered = prompt.find("## Instruction").expect("the prompt renders the instruction");

  assert!(prompt.starts_with("# A harmless item\n\n## Instruction"), "{prompt}");
  assert!(rendered < prompt.find("Answer that nothing is to be done.").unwrap(), "{prompt}");
  assert!(prompt.contains("Repository: `services/area`"), "{prompt}");
  assert!(prompt.contains("- docs/area.md\n"), "{prompt}");
  assert!(prompt.contains("- A refused call names the field it lacks.\n"), "{prompt}");
  assert!(prompt.contains("- `cargo test --test area`\n"), "{prompt}");

  let (status, text) = post(port, token, "/instruction-texts", instruction).await;

  assert_eq!(status, 200, "{text}");
  assert_eq!(
    text["text"],
    "In `services/area`, reading docs/area.md: A refused call names the field it lacks. Checked by \
     `cargo test --test area`."
  );
}

// A tool host on the socket given that knows two session tokens, answering each one's record to the issuer token
// alone: the job's own, held by the checkout it runs in, and a neighbour's, held by another checkout.
fn session_host(socket: &std::path::Path) {
  let listener = tokio::net::UnixListener::bind(socket).unwrap();
  let app = axum::Router::new().route(
    "/tool-sessions/{token}",
    axum::routing::get(
      |headers: axum::http::HeaderMap, axum::extract::Path(token): axum::extract::Path<String>| async move {
        let issuer = headers.get("authorization").and_then(|value| value.to_str().ok()) == Some("Bearer issuer");
        let holder = match token.as_str() {
          "job-token" => Some("helper"),
          "neighbour-token" => Some("other"),
          _ => None,
        };

        match holder.filter(|_| issuer) {
          Some(holder) => (
            StatusCode::OK,
            axum::Json(json!({
              "id": "record", "holderName": holder, "namespaces": [], "capabilities": [], "grants": [],
              "shown": [], "variables": [], "labels": {}, "expiresAt": null, "createdAt": 0, "connected": [],
            })),
          ),
          None => (
            StatusCode::NOT_FOUND,
            axum::Json(json!({ "code": "not-found", "error": "no session stands for that token" })),
          ),
        }
      },
    ),
  );

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
}

// A job's program holds no control token and no port: it borrows with its own session token, the server reading
// whose it is from the tool host, on the port the deployment's settings under its data root name.
#[tokio::test(flavor = "multi_thread")]
async fn a_job_borrows_a_free_checkout_with_its_session_token_until_it_no_longer_runs() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let host = home.path().join("mcp-host.sock");
  let token = "secret";
  let other = home.path().join("other").to_string_lossy().into_owned();

  std::fs::create_dir_all(&data).unwrap();
  session_host(&host);
  repository(scratch.db(), "helper", &home.path().join("helper").to_string_lossy(), &[]).await;
  repository(scratch.db(), "other", &other, &[]).await;
  let server = standing::stand_server_with(
    home.path(),
    &data,
    &scratch.address(),
    token,
    &[
      ("BMO_MCP_HOST_SOCKET", &host.to_string_lossy()),
      ("BMO_MCP_HOST_ISSUER_TOKEN", "issuer"),
    ],
  );
  let port = server.port;

  // The job runs from here on: one standing as the server starts, with no run behind it, is reaped.
  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at", "changed_at")
         VALUES (7, 41, 'harness', 'helper', 'running', 0, 0)"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
              "run_name", "token_count", "started_at", "prompt")
         VALUES (3, 41, 1, 'harness', 7, 'helper', 'running', 'job-3', 0, 0, 'work')"#,
      vec![],
    )
    .await;

  // The verb run as a job's shell runs it: the job's run and session token and the data root, and no port, no
  // store and no control token, which `held` adds where a case names them. The port is the deployment's, in the
  // data root's `env/agency.env`, which the server's own environment came from.
  std::fs::create_dir_all(data.join("env")).unwrap();
  std::fs::write(data.join("env").join("agency.env"), format!("AGENCY_CONTROL_PORT={port}\n")).unwrap();

  let borrow = |run: Option<&str>, repository: &str, held: &[(&str, &str)]| {
    let mut command = std::process::Command::new(binary::path("bmo-agency"));

    command
      .env_clear()
      .env("PATH", std::env::var("PATH").unwrap_or_default())
      .env("HOME", home.path())
      .env("BMO_DATA_DIR", &data)
      .args(["job", "borrow", repository])
      .envs(held.iter().copied());

    if let Some(run) = run {
      command.env("BMO_RUN_NAME", run);
    }

    let output = command.output().unwrap();

    (
      output.status.code(),
      String::from_utf8_lossy(&output.stdout).into_owned(),
      String::from_utf8_lossy(&output.stderr).into_owned(),
    )
  };
  let own = [("BMO_MCP_TOKEN", "job-token")];
  let (code, _, said) = borrow(None, "other", &own);

  assert_eq!(code, Some(1), "outside a job: {said}");

  let (code, _, said) = borrow(Some("job-3"), "other", &[]);

  assert_eq!(code, Some(1), "holding neither token: {said}");
  assert!(said.contains("AGENCY_CONTROL_TOKEN") && said.contains("BMO_MCP_TOKEN"), "{said}");

  let (code, _, said) = borrow(Some("job-3"), "other", &[("BMO_MCP_TOKEN", "forged")]);

  assert_eq!(code, Some(1), "a token the tool host does not know: {said}");
  assert!(said.contains("unauthorized"), "{said}");

  let (code, _, said) = borrow(Some("job-3"), "other", &[("BMO_MCP_TOKEN", "neighbour-token")]);

  assert_eq!(code, Some(1), "another checkout's token: {said}");
  assert!(said.contains("runs in helper"), "{said}");

  let (code, _, said) = borrow(Some("job-3"), "helper", &own);

  assert_eq!(code, Some(1), "its own repository: {said}");
  assert!(said.contains("own repository"), "{said}");

  let (code, out, said) = borrow(Some("job-3"), "other", &own);

  assert_eq!(code, Some(0), "{said}");
  assert!(out.contains(&other), "it prints where the checkout stands: {out}");

  let (code, out, said) = borrow(Some("job-3"), "other", &[("AGENCY_CONTROL_TOKEN", token)]);

  assert_eq!(code, Some(0), "{said}");
  assert!(
    out.contains("already has"),
    "asked twice, with the control token, the same checkout: {out}"
  );

  let ask = json!({ "itemId": 50, "holderName": "harness", "assigneeName": "other", "item": { "id": 50 } });
  let (status, _) = post(port, "job-token", "/assignments", ask.clone()).await;

  assert_eq!(status, 401, "a session token reaches no route that does not take one");

  let (status, refused) = post(port, token, "/assignments", ask.clone()).await;

  assert_eq!(
    (status, refused["code"].as_str()),
    (422, Some("no-worker-free")),
    "held while the job runs"
  );

  scratch.run(r#"UPDATE "jobs" SET "state" = 'ended' WHERE "id" = 3"#, vec![]).await;

  let (status, taken) = post(port, token, "/assignments", ask).await;

  assert_eq!(status, 201, "the loan ended with the job: {taken}");
}

// A job a harness ends with nothing to say frees its worker as surely as one that left a report: the end
// releases its assignment, and the repository's checkout is held again for the next item.
#[tokio::test(flavor = "multi_thread")]
async fn a_job_that_ends_with_no_report_frees_its_worker() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  repository(scratch.db(), "helper", &home.path().join("helper").to_string_lossy(), &[]).await;
  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at", "changed_at")
         VALUES (7, 41, 'harness', 'helper', 'running', 0, 0)"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
              "token_count", "started_at", "prompt")
         VALUES (3, 41, 1, 'harness', 7, 'helper', 'running', 0, $1, '# A harmless item')"#,
      vec![agency_server::now_ms().into()],
    )
    .await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let (status, ended) = patch(port, token, "/jobs/3", json!({ "state": "ended" })).await;

  assert_eq!(status, 200, "{ended}");
  assert_eq!((ended["state"].as_str(), ended["outcome"].as_str()), (Some("ended"), None), "{ended}");
  assert_eq!(ended["report"], Value::Null, "the end carried no report: {ended}");
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "assignments" WHERE "id" = 7 AND "state" = 'released'"#, vec![])
      .await,
    1,
    "the end released the assignment"
  );
  assigned(port, token, 42).await;
}

// An ask for the same item by the same holder answers the hold it already stands in, so a lost answer
// asked again is no busy worker and opens no second hold.
#[tokio::test(flavor = "multi_thread")]
async fn an_assign_for_the_same_item_and_holder_answers_the_standing_hold() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  repository(
    scratch.db(),
    "helper",
    &home.path().join("helper").to_string_lossy(),
    &[("support_count", 0i64.into())],
  )
  .await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let ask = json!({ "itemId": 41, "holderName": "harness", "assigneeName": "helper", "item": item() });
  let (status, first) = post(port, token, "/assignments", ask.clone()).await;

  assert_eq!(status, 201, "{first}");

  let (status, again) = post(port, token, "/assignments", ask).await;

  assert_eq!(status, 200, "the standing hold is answered: {again}");
  assert_eq!(again["id"], first["id"]);
  assert_eq!(again["state"], "held");
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "assignments" WHERE "item_id" = 41 AND "state" = 'held'"#,
        vec![]
      )
      .await,
    1
  );

  // The hold still owns its worker: another item waits for it.
  let (status, refused) = post(
    port,
    token,
    "/assignments",
    json!({ "itemId": 42, "holderName": "harness", "assigneeName": "helper", "item": item() }),
  )
  .await;

  assert_eq!((status, refused["code"].as_str()), (422, Some("no-worker-free")), "{refused}");
}

// An assignment naming a repository takes any free checkout of it, a support worktree where the base is held, and
// is refused only once every one is; one naming a checkout takes that checkout alone.
#[tokio::test(flavor = "multi_thread")]
async fn an_assign_naming_a_repository_takes_any_free_checkout_of_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  repository(
    scratch.db(),
    "helper",
    &home.path().join("helper").to_string_lossy(),
    &[("support_count", 1i64.into())],
  )
  .await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let ask = |id: i64, assignee: &str| json!({ "itemId": id, "holderName": "harness", "assigneeName": assignee, "item": item() });
  let (status, base) = post(port, token, "/assignments", ask(61, "helper")).await;

  assert_eq!((status, base["workerName"].as_str()), (201, Some("helper")), "{base}");

  let (status, support) = post(port, token, "/assignments", ask(62, "helper")).await;

  assert_eq!(
    (status, support["workerName"].as_str()),
    (201, Some("helper-w2")),
    "the base held: {support}"
  );

  let (status, refused) = post(port, token, "/assignments", ask(63, "helper")).await;

  assert_eq!(
    (status, refused["code"].as_str()),
    (422, Some("no-worker-free")),
    "every checkout held: {refused}"
  );

  let (status, refused) = post(port, token, "/assignments", ask(64, "helper-w2")).await;

  assert_eq!(
    (status, refused["code"].as_str()),
    (422, Some("no-worker-free")),
    "the named checkout held: {refused}"
  );
}

// An assignment naming no assignee goes where its item goes: the repository the item it hands over names, else the
// checkout named `worker`, and never a checkout of another repository, even one standing first and free.
#[tokio::test(flavor = "multi_thread")]
async fn an_assign_naming_no_assignee_goes_where_its_item_goes() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  for name in ["alpha", "helper", "worker"] {
    repository(
      scratch.db(),
      name,
      &home.path().join(name).to_string_lossy(),
      &[("support_count", 0i64.into())],
    )
    .await;
  }

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let going_to = |id: i64, assignee: Option<&str>| {
    let mut item = item();

    if let Some(assignee) = assignee {
      item["assigneeName"] = json!(assignee);
    }

    json!({ "itemId": id, "holderName": "harness", "item": item })
  };
  let (status, held) = post(port, token, "/assignments", going_to(81, Some("helper"))).await;

  assert_eq!((status, held["workerName"].as_str()), (201, Some("helper")), "{held}");

  let (status, again) = post(port, token, "/assignments", going_to(81, Some("helper"))).await;

  assert_eq!((status, &again["id"]), (200, &held["id"]), "the standing hold answered: {again}");

  let (status, held) = post(port, token, "/assignments", going_to(82, None)).await;

  assert_eq!((status, held["workerName"].as_str()), (201, Some("worker")), "{held}");

  let (status, refused) = post(port, token, "/assignments", going_to(83, None)).await;

  assert_eq!((status, refused["code"].as_str()), (422, Some("no-worker-free")), "{refused}");
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "assignments" WHERE "worker_name" = 'alpha'"#, vec![])
      .await,
    0,
    "no ask took another repository's checkout"
  );
}

// An item is held on one checkout at a time: a hold another holder left on an item goes as the item is asked for
// again, and one a newer assignment of its item follows goes as the assignments are read.
#[tokio::test(flavor = "multi_thread")]
async fn a_hold_its_item_was_asked_for_again_after_is_released() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  for name in ["helper", "other"] {
    repository(scratch.db(), name, &home.path().join(name).to_string_lossy(), &[]).await;
  }

  scratch
    .run(
      r#"INSERT INTO "assignments" ("item_id", "holder_name", "worker_name", "state", "created_at", "changed_at")
         VALUES (70, 'left', 'helper', 'held', 0, 0), (72, 'left', 'other', 'held', 0, 0),
                (72, 'harness', 'helper', 'released', 1, 1)"#,
      vec![],
    )
    .await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let (status, held) = get(port, token, "/assignments?state=held").await;
  let ids: Vec<i64> =
    held["items"].as_array().unwrap().iter().filter_map(|one| one["id"].as_i64()).collect();

  assert_eq!(
    (status, ids),
    (200, vec![1]),
    "the hold its item was assigned again after is gone: {held}"
  );

  let ask = |id: i64, assignee: &str| json!({ "itemId": id, "holderName": "harness", "assigneeName": assignee, "item": item() });
  let (status, taken) = post(port, token, "/assignments", ask(70, "helper")).await;

  assert_eq!((status, taken["workerName"].as_str()), (201, Some("helper")), "{taken}");

  let (status, taken) = post(port, token, "/assignments", ask(73, "other")).await;

  assert_eq!((status, taken["workerName"].as_str()), (201, Some("other")), "{taken}");
}

// A hold given back frees the worker for the next item.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_frees_the_worker_for_the_next_item() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  repository(scratch.db(), "helper", &home.path().join("helper").to_string_lossy(), &[]).await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let assignment = assigned(port, token, 41).await;
  let (status, released) =
    patch(port, token, &format!("/assignments/{assignment}"), json!({ "state": "released" })).await;

  assert_eq!(status, 200, "{released}");
  assert_eq!(released["state"], "released", "{released}");
  assigned(port, token, 42).await;
}

// A session's end gives back the holds it took for turns that never started their jobs, and leaves a job's
// own assignment standing for that job's end.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_s_end_frees_the_holds_it_took_and_leaves_a_running_job_s() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  repository(scratch.db(), "helper", &home.path().join("helper").to_string_lossy(), &[]).await;
  crate::support::rows::supervisor(scratch.db(), "supervisor-a", &[("session_id", "s-1".into())]).await;
  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at", "changed_at")
         VALUES (7, 41, 'supervisor-a', 'helper', 'held', 0, 0),
                (8, 42, 'supervisor-a', 'helper', 'held', 0, 0)"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
              "token_count", "started_at", "prompt")
         VALUES (3, 42, 1, 'supervisor-a', 8, 'helper', 'running', 0, $1, '# A harmless item')"#,
      vec![agency_server::now_ms().into()],
    )
    .await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let answer = reqwest::Client::new()
    .delete(format!("http://127.0.0.1:{port}/sessions/supervisor-a"))
    .bearer_auth(token)
    .send()
    .await
    .unwrap();

  assert_eq!(answer.status().as_u16(), 204);
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "assignments" WHERE "id" = 7 AND "state" = 'released'"#, vec![])
      .await,
    1,
    "the session's own hold ended with it"
  );
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "assignments" WHERE "id" = 8 AND "state" = 'held'"#, vec![])
      .await,
    1,
    "a running job's assignment is left to that job's end"
  );
}

// Agency starts no job by itself (docs/role-agents.md, 「Which agent a run takes」): a launch onto an agent that
// carries a job, or any other run, is refused at once naming what it carries, with nothing written and nothing
// left waiting to start it later; once the agent is free, or the job on it paused, the launch goes.
#[tokio::test(flavor = "multi_thread")]
async fn a_launch_onto_an_agent_that_carries_a_run_is_refused_and_nothing_waits_to_start_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  std::fs::create_dir_all(home.path().join("helper")).unwrap();
  repository(scratch.db(), "helper", &home.path().join("helper").to_string_lossy(), &[]).await;
  scratch.run(r#"DELETE FROM "claude_accounts""#, vec![]).await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  // Another job stands on the agent under review, its review not paused.
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "started_at", "prompt", "item_agent", "review_parked")
         VALUES (900, 80, 0, 'harness', 0, 'elsewhere', 'reviewing', 0, 'work', $1, false)"#,
      vec![AGENT.into()],
    )
    .await;

  let assignment = assigned(port, token, 81).await;
  let launch = || run_job(port, token, assignment, 81, item());
  let (status, refused) = launch().await;

  assert_eq!((status, refused["code"].as_str()), (409, Some("agent-busy")), "{refused}");
  assert!(refused["error"].as_str().unwrap().contains("job 900 of item #80"), "{refused}");

  let nothing_started = async || {
    assert_eq!(scratch.count(r#"SELECT COUNT(*) FROM "jobs" WHERE "item_id" = 81"#, vec![]).await, 0);
    assert_eq!(scratch.count(r#"SELECT COUNT(*) FROM "role_slot_requests""#, vec![]).await, 0);
    assert_eq!(get(port, token, &format!("/assignments/{assignment}")).await.1["state"], "held");
  };

  nothing_started().await;

  // Paused under its review, that job answers for the agent no longer; a run let in on the agent refuses the
  // launch as well.
  scratch
    .run(r#"UPDATE "jobs" SET "review_parked" = true WHERE "id" = 900"#, vec![])
    .await;
  scratch
    .run(
      r#"INSERT INTO "role_slot_requests" ("token", "request_key", "owner", "source", "role", "path", "provider",
           "model", "effort", "capacity", "state", "enqueued_at", "class", "seat", "title")
         VALUES ('busy', 'busy', 'owner:x', 'cowork', 'worker', '/elsewhere/', 'p', 'm', '', 1, 'active', 0,
           'foreground', $1, 'a co-work branch')"#,
      vec![AGENT.into()],
    )
    .await;

  let (status, refused) = launch().await;

  assert_eq!((status, refused["code"].as_str()), (409, Some("agent-busy")), "{refused}");
  assert!(
    refused["error"].as_str().unwrap().contains("a cowork run, a co-work branch"),
    "{refused}"
  );
  scratch.run(r#"DELETE FROM "role_slot_requests""#, vec![]).await;
  nothing_started().await;

  // The agent free, two launches onto it at once: one goes and the other is refused.
  std::fs::create_dir_all(home.path().join("helper-two")).unwrap();
  repository(scratch.db(), "helper-two", &home.path().join("helper-two").to_string_lossy(), &[]).await;

  let (status, other) = post(
    port,
    token,
    "/assignments",
    json!({ "itemId": 82, "holderName": "harness", "assigneeName": "helper-two", "item": item() }),
  )
  .await;

  assert_eq!(status, 201, "{other}");

  let other = other["id"].as_i64().unwrap();
  let (first, second) = tokio::join!(launch(), run_job(port, token, other, 82, item()));
  let mut statuses = [first.0, second.0];

  statuses.sort_unstable();
  assert_eq!(statuses, [201, 409], "{} {}", first.1, second.1);

  // The job that went stands in the machine's turn: its agent is free, and the next launch onto it goes
  // through (docs/role-agents.md, 「When an agent is busy」).
  scratch.run(r#"UPDATE "jobs" SET "machine_turn" = true"#, vec![]).await;
  std::fs::create_dir_all(home.path().join("helper-three")).unwrap();
  repository(scratch.db(), "helper-three", &home.path().join("helper-three").to_string_lossy(), &[]).await;

  let (status, third) = post(
    port,
    token,
    "/assignments",
    json!({ "itemId": 83, "holderName": "harness", "assigneeName": "helper-three", "item": item() }),
  )
  .await;

  assert_eq!(status, 201, "{third}");

  let (status, launched) = run_job(port, token, third["id"].as_i64().unwrap(), 83, item()).await;

  assert_eq!(status, 201, "{launched}");
}

// A checkout of a repository of its own, and an assignment held on it for the item.
async fn assigned_in(
  port: u16,
  token: &str,
  home: &std::path::Path,
  db: &sea_orm::DatabaseConnection,
  item_id: i64,
) -> i64 {
  let name = format!("helper-{item_id}");

  std::fs::create_dir_all(home.join(&name)).unwrap();
  repository(db, &name, &home.join(&name).to_string_lossy(), &[]).await;

  let (status, assignment) = post(
    port,
    token,
    "/assignments",
    json!({ "itemId": item_id, "holderName": "harness", "assigneeName": name, "item": item() }),
  )
  .await;

  assert_eq!(status, 201, "{assignment}");
  assignment["id"].as_i64().unwrap()
}

fn item_labelled(labels: Value) -> Value {
  json!({ "title": "A harmless item", "brief": "Answer that nothing is to be done.", "notes": [], "labels": labels })
}

// An item naming a model and a role takes, at its launch, the first free agent of that role on that model, two
// launches at once taking two different agents; with every such agent busy it is refused and nothing waits to
// start it, an agent whose jobs all stand in the machine's turn counting as free; an `agent` label stands over
// the model; and a model no agent of the role runs is refused with the role's agents (docs/repositories.md, 「An
// agent an item names」).
#[tokio::test(flavor = "multi_thread")]
async fn a_launch_naming_a_model_takes_a_free_agent_of_its_role_on_that_model() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  scratch.run(r#"DELETE FROM "claude_accounts""#, vec![]).await;
  crate::support::rows::seats(
    scratch.db(),
    "worker",
    &[
      ("claude-code", "claude-opus-5"),
      ("claude-code", "claude-opus-5"),
      ("claude-code", "claude-sonnet-5"),
    ],
  )
  .await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let db = scratch.db();
  let on_model = |model: &str| item_labelled(json!({ "model": model, "role": "worker" }));
  let jobs_of = async |item: i64| {
    scratch
      .count(r#"SELECT COUNT(*) FROM "jobs" WHERE "item_id" = $1"#, vec![item.into()])
      .await
  };

  // A model no agent of the role runs: refused, naming the role's agents and their roads, nothing written.
  let lacking = assigned_in(port, token, home.path(), db, 70).await;
  let (status, refused) = run_job(port, token, lacking, 70, on_model("gpt-5.6-sol")).await;
  let why = refused["error"].as_str().unwrap_or_default();

  assert_eq!((status, refused["code"].as_str()), (422, Some("no-candidate")), "{refused}");
  assert!(why.contains("model=gpt-5.6-sol") && why.contains("the worker role"), "{why}");
  assert!(why.contains("- worker-03: claude-code/claude-sonnet-5"), "{why}");
  assert_eq!(jobs_of(70).await, 0);
  assert_eq!(get(port, token, &format!("/assignments/{lacking}")).await.1["state"], "held");

  // Two launches at once on a model two agents run, no Claude login open so each job waits on its agent: each
  // takes one.
  let (first, second) = (
    assigned_in(port, token, home.path(), db, 71).await,
    assigned_in(port, token, home.path(), db, 72).await,
  );
  let (one, two) = tokio::join!(
    run_job(port, token, first, 71, on_model("claude-opus-5")),
    run_job(port, token, second, 72, on_model("claude-opus-5")),
  );

  assert_eq!((one.0, two.0), (201, 201), "{} {}", one.1, two.1);

  let mut taken = [one.1["agentName"].clone(), two.1["agentName"].clone()];

  taken.sort_by_key(|name| name.to_string());
  assert_eq!(taken, [json!("worker-01"), json!("worker-02")]);

  // Every agent on the model busy, in its provider's spelling too: refused naming each, nothing waits.
  let third = assigned_in(port, token, home.path(), db, 73).await;
  let (status, refused) = run_job(port, token, third, 73, on_model("claude-code/claude-opus-5")).await;
  let why = refused["error"].as_str().unwrap_or_default();

  assert_eq!((status, refused["code"].as_str()), (409, Some("agent-busy")), "{refused}");
  assert!(
    why.contains("worker-01, which carries job") && why.contains("worker-02, which carries job"),
    "{why}"
  );
  assert_eq!(jobs_of(73).await, 0);
  assert_eq!(get(port, token, &format!("/assignments/{third}")).await.1["state"], "held");

  // An agent label stands over the model: the agent it names is taken though no agent on the model is free.
  let named = item_labelled(json!({ "agent": "worker-03", "model": "claude-opus-5", "role": "worker" }));
  let (status, launched) = run_job(port, token, third, 73, named).await;

  assert_eq!((status, launched["agentName"].as_str()), (201, Some("worker-03")), "{launched}");

  // An agent whose every job stands in the machine's turn is free for the model's next item.
  scratch
    .run(r#"UPDATE "jobs" SET "machine_turn" = true WHERE "item_agent" = 'worker-02'"#, vec![])
    .await;

  let fourth = assigned_in(port, token, home.path(), db, 74).await;
  let (status, launched) = run_job(port, token, fourth, 74, on_model("claude-opus-5")).await;

  assert_eq!((status, launched["agentName"].as_str()), (201, Some("worker-02")), "{launched}");
}
