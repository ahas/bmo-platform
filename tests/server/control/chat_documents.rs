//! The supervisor's own documents the owner keeps (docs/api.md, 「chat-documents」): each stands as its file
//! until a text is saved, as the owner's text while one is, and as its file again after a reset.

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

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_document_stands_as_its_file_until_a_text_is_saved_and_again_after_a_reset() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  // Every document is listed, each standing as its file until one is saved.
  let (status, listing) = get(port, token, "/chat-documents").await;
  assert_eq!(status, 200);
  let items = listing["items"].as_array().unwrap();

  assert_eq!(
    items.iter().map(|item| item["name"].as_str().unwrap()).collect::<Vec<_>>(),
    ["opening", "absolute-rules", "manual"]
  );
  assert!(items.iter().all(|item| item["source"] == "file"));

  // A save stands as the owner's text, and the listing reads it.
  let (status, saved) = patch(
    port,
    token,
    "/chat-documents/manual",
    json!({ "action": "save", "text": "Say it plainly.", "by": "the owner" }),
  )
  .await;
  assert_eq!(status, 200, "{saved}");
  assert_eq!(saved["source"], "stored");
  assert_eq!(saved["text"], "Say it plainly.");
  assert!(saved["version"].is_i64());

  let (_, listed) = get(port, token, "/chat-documents").await;
  assert_eq!(listed["items"][2]["source"], "stored");

  // A reset lets the file stand again.
  let (status, reset) = patch(port, token, "/chat-documents/manual", json!({ "action": "reset" })).await;
  assert_eq!(status, 200);
  assert_eq!(reset["source"], "file");

  // An unknown document is no document, and a save with no text is refused.
  let (status, _) =
    patch(port, token, "/chat-documents/other", json!({ "action": "save", "text": "x" })).await;
  assert_eq!(status, 404);

  let (status, _) =
    patch(port, token, "/chat-documents/manual", json!({ "action": "save", "text": "  " })).await;
  assert_eq!(status, 400);
}
