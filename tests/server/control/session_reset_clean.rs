//! A reset of a supervisor's session asked over the control route (docs/session-resets.md, 「What a reset is」): one
//! naming no `clean` is kept as a reset carrying a note, one asking `clean` starts the session clean, and a clean ask
//! made while one already waits makes the waiting one clean without adding a row.

use serde_json::{Value, json};

use crate::support::scratch::Scratch;
use crate::support::standing;

async fn post(port: u16, token: &str, body: Value) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .post(format!("http://127.0.0.1:{port}/session-reset"))
    .bearer_auth(token)
    .json(&body)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_clean_ask_starts_the_session_clean_and_makes_a_waiting_one_clean() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  crate::support::rows::supervisor(scratch.db(), "chat-agent", &[("session_scope", "agent".into())]).await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let rows = |clean: bool| {
    let scratch = &scratch;

    async move {
      scratch
        .count(
          r#"SELECT COUNT(*) FROM "session_resets" WHERE "agent" = 'chat-agent' AND "done_at" IS NULL AND "clean" = $1"#,
          vec![clean.into()],
        )
        .await
    }
  };

  let (status, _) = post(port, token, json!({ "agent": "chat-agent", "by": "7" })).await;

  assert_eq!(status, 200);
  assert_eq!((rows(false).await, rows(true).await), (1, 0), "a reset naming no clean carries a note");

  let (status, _) = post(port, token, json!({ "agent": "chat-agent", "by": "7", "clean": true })).await;

  assert_eq!(status, 200);
  assert_eq!(
    (rows(false).await, rows(true).await),
    (0, 1),
    "the waiting ask is made clean, no row added"
  );
}
