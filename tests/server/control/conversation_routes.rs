//! A conversation the caller opened, asked of a running server over its control routes: started on the
//! provider and model its body names, read back with its scope word and the pair it stands on, listed
//! among its agent's sessions, moved to another provider and model by its change route, and ended.

use serde_json::{Value, json};

use crate::support::scratch::Scratch;
use crate::support::standing;

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

async fn delete(port: u16, token: &str, path: &str) -> u16 {
  reqwest::Client::new()
    .delete(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .send()
    .await
    .unwrap()
    .status()
    .as_u16()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_server_opens_a_conversation_on_a_named_road_and_switches_both_of_its_parts() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  crate::support::rows::supervisor(scratch.db(), "chat-agent", &[]).await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  let (status, started) = post(
    port,
    token,
    "/sessions",
    json!({
      "agentName": "chat-agent",
      "scope": "conversation",
      "options": { "provider": "openrouter", "model": "deepseek/deepseek-chat", "cwd": "/tmp/run-a" },
    }),
  )
  .await;

  assert_eq!(status, 201, "{started}");
  assert_eq!(started["id"], "chat-agent@conversation:1");
  assert_eq!(started["scope"], json!("conversation"));
  assert_eq!(started["provider"], "openrouter");
  assert_eq!(started["model"], "deepseek/deepseek-chat");

  let (status, state) = get(port, token, "/sessions/chat-agent@conversation:1").await;

  assert_eq!(status, 200, "{state}");
  assert_eq!(state["scope"], json!("conversation"));
  assert_eq!(state["provider"], "openrouter");
  assert_eq!(state["model"], "deepseek/deepseek-chat");
  assert_eq!(state["place"], "/tmp/run-a");

  let (_, listed) = get(port, token, "/sessions?agentName=chat-agent").await;

  assert!(
    listed["items"]
      .as_array()
      .unwrap()
      .iter()
      .any(|item| item["id"] == "chat-agent@conversation:1"),
    "{listed}"
  );

  let (status, moved) = patch(
    port,
    token,
    "/sessions/chat-agent@conversation:1",
    json!({ "provider": "codex", "model": "gpt-5" }),
  )
  .await;

  assert_eq!(status, 200, "{moved}");
  assert_eq!((moved["provider"].as_str(), moved["model"].as_str()), (Some("codex"), Some("gpt-5")));

  let (status, moved) =
    patch(port, token, "/sessions/chat-agent@conversation:1", json!({ "model": "gpt-5-mini" })).await;

  assert_eq!(status, 200, "{moved}");
  assert_eq!(
    (moved["provider"].as_str(), moved["model"].as_str()),
    (Some("codex"), Some("gpt-5-mini"))
  );

  let (status, refused) = patch(port, token, "/sessions/chat-agent@conversation:1", json!({})).await;

  assert_eq!(status, 422, "{refused}");

  let (status, refused) = post(
    port,
    token,
    "/sessions",
    json!({ "agentName": "chat-agent", "scope": "conversation", "options": { "model": "m" } }),
  )
  .await;

  assert_eq!(status, 201, "a conversation naming no provider is Claude Code's: {refused}");
  assert_eq!(refused["provider"], "claude-code");

  assert_eq!(delete(port, token, "/sessions/chat-agent@conversation:1").await, 204);
  assert_eq!(get(port, token, "/sessions/chat-agent@conversation:1").await.0, 404);
}
