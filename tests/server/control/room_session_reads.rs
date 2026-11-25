//! A room's session read from its start: the state the start answers and the state a read answers at
//! once, before any turn has opened a generation, the same session read after one has, and the supervisor's
//! compaction threshold named on it.

use serde_json::{Value, json};

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

#[tokio::test(flavor = "multi_thread")]
async fn a_room_session_is_readable_from_its_start_before_any_turn() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  crate::support::rows::repository(scratch.db(), "helper", "/checkouts/helper", &[]).await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  let (status, added) = post(
    port,
    token,
    "/supervisors",
    json!({
      "name": "supervisor-a",
      "label": "Supervisor A",
      "provider": "claude-code",
      "model": "opus",
      "repository": "helper",
    }),
  )
  .await;

  assert_eq!(status, 201, "{added}");

  let (status, started) = post(
    port,
    token,
    "/sessions",
    json!({ "agentName": "supervisor-a", "scope": "room", "roomId": -7 }),
  )
  .await;

  assert_eq!(status, 201, "{started}");
  assert_eq!(
    (started["id"].as_str(), started["scope"].as_str()),
    (Some("supervisor-a@-7"), Some("room"))
  );
  assert_eq!(started["provider"], json!("claude-code"), "{started}");
  assert_eq!(started["model"], json!("opus"), "{started}");
  assert_eq!(started["generation"], json!(0), "{started}");

  let (status, state) = get(port, token, "/sessions/supervisor-a@-7").await;

  assert_eq!(status, 200, "{state}");
  assert_eq!((state["id"].as_str(), state["scope"].as_str()), (Some("supervisor-a@-7"), Some("room")));
  assert_eq!(state["provider"], json!("claude-code"), "{state}");
  assert_eq!(state["model"], json!("opus"), "{state}");
  assert_eq!(state["generation"], json!(0), "{state}");
  assert!(state["place"].as_str().is_some_and(|place| !place.is_empty()), "{state}");
  assert_eq!(state["context"], Value::Null, "no turn has left it a context: {state}");

  // Reading it wrote nothing of the room's session: the first turn still opens the first generation.
  assert_eq!(
    scratch
      .count(r#"SELECT count(*) FROM "room_sessions" WHERE "agent" = 'supervisor-a'"#, vec![])
      .await,
    0
  );

  // A generation the first turn opened is what the read answers after it.
  scratch
    .run(
      r#"INSERT INTO "room_sessions" ("agent", "chat_id", "generation", "opened_at", "harness_session", "harness_road")
         VALUES ('supervisor-a', -7, 1, 1, 'harness-1', 'claude-code::opus')"#,
      vec![],
    )
    .await;

  let (status, opened) = get(port, token, "/sessions/supervisor-a@-7").await;

  assert_eq!(status, 200, "{opened}");
  assert_eq!(
    (opened["provider"].as_str(), opened["model"].as_str()),
    (Some("claude-code"), Some("opus"))
  );
  assert_eq!(opened["generation"], json!(1), "{opened}");

  // The state names the supervisor's compaction threshold, read afresh from its row.
  scratch
    .run(r#"UPDATE "supervisors" SET "fold_ratio" = 0.6 WHERE "name" = 'supervisor-a'"#, vec![])
    .await;

  let (_, folded) = get(port, token, "/sessions/supervisor-a@-7").await;

  assert_eq!(folded["foldRatio"], json!(0.6), "{folded}");
}
