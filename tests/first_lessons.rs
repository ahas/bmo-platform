//! The install's first-lessons after-step, `deploy/first-lessons`, run against a brain standing in for the
//! published one (docs/supervisors.md, 「A fresh brain's starting lessons」): an empty brain of the first chat
//! agent is given every shipped lesson through the brain's own memory route, and a brain already holding a
//! memory is left as it is.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::{Value, json};

// How long one run of the script, which makes a few local calls, is given before it fails the test.
const BOUND: Duration = Duration::from_secs(30);
const TOKEN: &str = "brain-token";
const AGENT: &str = "first-agent";
const BRAIN: i64 = 7;

#[derive(Default)]
struct Brain {
  held: Vec<Value>,
  written: Vec<Value>,
}

type Held = Arc<Mutex<Brain>>;

fn allowed(headers: &HeaderMap) -> bool {
  headers.get("authorization").and_then(|value| value.to_str().ok()) == Some(&format!("Bearer {TOKEN}"))
}

async fn brains(headers: HeaderMap, Query(query): Query<HashMap<String, String>>) -> Response {
  if !allowed(&headers) {
    return StatusCode::UNAUTHORIZED.into_response();
  }
  let found = query.get("kind").map(String::as_str) == Some("agent")
    && query.get("agentKey").map(String::as_str) == Some(AGENT);
  let items = if found {
    vec![json!({ "id": BRAIN, "kind": "agent", "agentKey": AGENT })]
  } else {
    vec![]
  };

  axum::Json(json!({ "items": items, "more": false })).into_response()
}

async fn memories(State(brain): State<Held>, headers: HeaderMap, Path(id): Path<i64>) -> Response {
  if !allowed(&headers) || id != BRAIN {
    return StatusCode::NOT_FOUND.into_response();
  }
  let brain = brain.lock().unwrap();
  let items: Vec<Value> = brain.held.iter().chain(&brain.written).take(1).cloned().collect();

  axum::Json(json!({ "items": items, "more": false })).into_response()
}

async fn write(
  State(brain): State<Held>,
  headers: HeaderMap,
  Path(id): Path<i64>,
  body: String,
) -> Response {
  if !allowed(&headers) || id != BRAIN {
    return StatusCode::NOT_FOUND.into_response();
  }
  let memory: Value = serde_json::from_str(&body).unwrap();
  brain.lock().unwrap().written.push(memory.clone());

  (StatusCode::CREATED, axum::Json(memory)).into_response()
}

// A brain standing in, holding `held`, and a data root whose agency settings name it.
async fn stand(held: Vec<Value>) -> (Held, tempfile::TempDir) {
  let brain: Held = Arc::new(Mutex::new(Brain {
    held,
    written: vec![],
  }));
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());
  let app = axum::Router::new()
    .route("/brains", get(brains))
    .route("/brains/{id}/memories", get(memories).post(write))
    .with_state(brain.clone());
  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  let data = tempfile::tempdir().unwrap();
  std::fs::create_dir_all(data.path().join("env")).unwrap();
  std::fs::write(
    data.path().join("env/agency.env"),
    format!(
      "AGENCY_BRAIN_URL={address}\nAGENCY_BRAIN_TOKEN={TOKEN}\nAGENCY_BRAIN_ACCOUNT=administrator\nAGENCY_AGENT_KEY={AGENT}\n"
    ),
  )
  .unwrap();

  (brain, data)
}

// The script run in `mode` over the data root: its code and what it printed.
async fn step(mode: &str, data: &std::path::Path) -> (i32, String) {
  let script = concat!(env!("CARGO_MANIFEST_DIR"), "/deploy/first-lessons");
  let mut command = tokio::process::Command::new("sh");
  command
    .arg(script)
    .arg(mode)
    .env_clear()
    .env("PATH", std::env::var("PATH").unwrap_or_default())
    .env("HOME", data)
    .env("BMO_DATA_DIR", data)
    .stdin(std::process::Stdio::null())
    .kill_on_drop(true);
  let done = tokio::time::timeout(BOUND, command.output())
    .await
    .expect("the step did not end")
    .unwrap();
  let printed =
    format!("{}{}", String::from_utf8_lossy(&done.stdout), String::from_utf8_lossy(&done.stderr));

  (done.status.code().unwrap_or(-1), printed)
}

fn shipped() -> Vec<Value> {
  let text =
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/deploy/first-lessons.json")).unwrap();
  let file: Value = serde_json::from_str(&text).unwrap();

  file["lessons"].as_array().unwrap().clone()
}

#[test]
fn every_shipped_lesson_has_words_and_the_situation_that_recalls_it() {
  let lessons = shipped();

  assert!(!lessons.is_empty());
  for lesson in &lessons {
    for field in ["text", "recallCondition"] {
      let words = lesson[field].as_str().unwrap_or_default();
      assert!(!words.trim().is_empty(), "a lesson holds no {field}: {lesson}");
    }
  }
}

#[tokio::test]
async fn an_empty_brain_of_the_first_chat_agent_is_given_every_lesson() {
  let (brain, data) = stand(vec![]).await;

  assert_eq!(step("done", data.path()).await.0, 1, "an empty brain is work still to do");
  let (code, printed) = step("run", data.path()).await;
  assert_eq!(code, 0, "{printed}");

  let written = brain.lock().unwrap().written.clone();
  let lessons = shipped();
  assert_eq!(written.len(), lessons.len(), "{printed}");
  for (memory, lesson) in written.iter().zip(&lessons) {
    assert_eq!(memory["text"], lesson["text"]);
    assert_eq!(memory["recallCondition"], lesson["recallCondition"]);
    assert_eq!(memory["type"], "situation");
    assert_eq!(memory["labels"], json!({ "source": "seed" }));
    assert_eq!(memory["actorName"], "install");
  }
  assert_eq!(step("done", data.path()).await.0, 0, "a brain given its lessons is done");
}

#[tokio::test]
async fn a_brain_that_already_holds_a_memory_is_left_alone() {
  let (brain, data) = stand(vec![json!({ "id": 1, "type": "fact", "status": "invalidated" })]).await;

  assert_eq!(step("done", data.path()).await.0, 0, "a brain holding a memory has nothing to do");
  let (code, printed) = step("run", data.path()).await;
  assert_eq!(code, 0, "{printed}");
  assert!(brain.lock().unwrap().written.is_empty(), "{printed}");
}

#[tokio::test]
async fn no_brain_deployed_writes_nothing() {
  let data = tempfile::tempdir().unwrap();
  std::fs::create_dir_all(data.path().join("env")).unwrap();
  std::fs::write(data.path().join("env/agency.env"), format!("AGENCY_AGENT_KEY={AGENT}\n")).unwrap();

  assert_eq!(step("done", data.path()).await.0, 0);
  assert_eq!(step("run", data.path()).await.0, 0);
}
