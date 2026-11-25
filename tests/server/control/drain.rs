//! The server draining on its stop (docs/api.md, 「health」, the drain; 「events」): an event stream open when
//! the drain begins ends with its `stopping` frame, the serving ends once the calls in hand have, and a call
//! that still reaches the front is refused `stopping` with when to ask again.

use std::sync::Arc;
use std::time::Duration;

use agency_server::control::{Front, router, serve};
use agency_server::places::Ground;
use futures_util::StreamExt;
use tokio::net::TcpListener;

use crate::support::grounds::seeded;
use crate::support::scratch::Scratch;

const TOKEN: &str = "secret";

#[tokio::test]
async fn an_open_stream_ends_with_its_stopping_frame_and_the_serving_ends_with_the_drain() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground: Arc<Ground> = Arc::new(seeded(scratch.db(), home.path()).await);
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let served = tokio::spawn(serve(listener, Front::new(TOKEN.to_owned(), ground.clone()).unwrap()));
  let stream = reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}/events"))
    .bearer_auth(TOKEN)
    .send()
    .await
    .unwrap();

  assert_eq!(stream.status(), 200);

  ground.drain.begin();

  // The stream is read to its end, which the drain gives it.
  let mut read = String::new();
  let mut bytes = stream.bytes_stream();

  while let Some(chunk) = tokio::time::timeout(Duration::from_secs(10), bytes.next()).await.unwrap() {
    read.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
  }

  assert!(read.contains("event: stopping"), "{read}");
  assert!(read.contains(r#""kind":"stopping""#), "{read}");

  // Nothing is left in hand, so the serving ends well within the drain's span.
  tokio::time::timeout(Duration::from_secs(10), served).await.unwrap().unwrap().unwrap();
}

#[tokio::test]
async fn a_call_reaching_a_draining_front_is_refused_stopping_with_when_to_ask_again() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground: Arc<Ground> = Arc::new(seeded(scratch.db(), home.path()).await);
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let app = router(Front::new(TOKEN.to_owned(), ground.clone()).unwrap());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  ground.drain.begin();

  let answer = reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}/quiet"))
    .bearer_auth(TOKEN)
    .send()
    .await
    .unwrap();

  assert_eq!(answer.status(), 503);
  assert_eq!(answer.headers()["retry-after"], policy::drain::DRAIN.retry_after_seconds.to_string());

  let said: serde_json::Value = answer.json().await.unwrap();

  assert_eq!(said["code"], "stopping");
}

#[tokio::test]
async fn the_quiet_route_answers_the_server_quiet_where_it_carries_nothing() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground: Arc<Ground> = Arc::new(seeded(scratch.db(), home.path()).await);
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let app = router(Front::new(TOKEN.to_owned(), ground).unwrap());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  let said: serde_json::Value = reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}/quiet"))
    .bearer_auth(TOKEN)
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();

  assert_eq!(said, serde_json::json!({ "quiet": true, "work": [] }));
}
