//! What the brain reads of a running server over its control routes (docs/api.md, 「supervisors」): every
//! supervisor with its brain folder, and nothing of a recollector; the shared rulebooks by name; a verdict on
//! an ask that is not there; and a reload naming the agent and the rooms carried as published — on a store
//! the server settled itself.

use serde_json::{Value, json};

use crate::support::rows::supervisor;
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

#[tokio::test(flavor = "multi_thread")]
async fn a_running_server_answers_what_the_brain_reads() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  supervisor(scratch.db(), "chat-agent", &[("brain_folder", "".into())]).await;
  supervisor(
    scratch.db(),
    "supervisor",
    &[
      ("brain_folder", "agents/supervisor".into()),
      ("session_scope", "room".into()),
      ("turn_shape", "pieces".into()),
    ],
  )
  .await;
  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('reset_point:supervisor', '1700000000123')"#,
      vec![],
    )
    .await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let (status, listed) = get(port, token, "/supervisors").await;

  assert_eq!(status, 200, "{listed}");

  let agent = |name: &str| {
    listed["items"]
      .as_array()
      .unwrap()
      .iter()
      .find(|agent| agent["name"] == name)
      .cloned()
      .unwrap_or_else(|| panic!("{name} is listed"))
  };

  assert_eq!(agent("chat-agent")["brainFolder"], json!(""));
  assert_eq!(agent("supervisor")["brainFolder"], json!("agents/supervisor"));
  // Its last reset's point is read again here, and whether its removal is refused as seeded.
  assert_eq!(agent("supervisor")["resetPoint"], json!(1_700_000_000_123_i64));
  assert_eq!(agent("chat-agent")["resetPoint"], json!(null));
  assert_eq!(agent("supervisor")["seeded"], json!(false));

  for agent in listed["items"].as_array().unwrap() {
    let fields = agent.as_object().unwrap();

    assert!(!fields.keys().any(|field| field.to_lowercase().contains("recollector")), "{agent}");
    assert_ne!(agent["kind"], json!("recollector"), "{agent}");
  }

  let (status, document) = get(port, token, "/prompt-documents/plain-writing").await;

  assert_eq!(status, 200, "{document}");
  assert_eq!(document["name"], json!("plain-writing"));
  assert_eq!(document["kind"], json!("rulebook"));
  assert_eq!(get(port, token, "/prompt-documents/no-such-rulebook").await.0, 404);

  assert_eq!(
    get(port, token, "/overtime-requests").await,
    (200, json!({ "items": [], "more": false }))
  );
  assert_eq!(
    patch(port, token, "/overtime-requests/1", json!({ "verdict": "cut", "why": "stuck" }))
      .await
      .0,
    404
  );
  assert_eq!(patch(port, token, "/overtime-requests/1", json!({ "verdict": "wait" })).await.0, 400);

  let moved = json!({ "paths": ["agents/supervisor/people/a.md"], "now": false, "agent": "supervisor", "rooms": [-7] });
  let (status, answered) = post(port, token, "/document-moves", moved).await;

  assert_eq!(status, 201, "{answered}");
  assert_eq!(answered["paths"], json!(["agents/supervisor/people/a.md"]));
  assert_eq!(answered["agentName"], json!("supervisor"));
  assert_eq!(answered["roomIds"], json!([-7]));
  assert_eq!(post(port, token, "/document-moves", json!({ "paths": ["a.md"] })).await.0, 201);
}
