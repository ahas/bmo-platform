//! The OpenRouter tap: a run's stream and a whole answer handed back unchanged with their upstream and
//! usage read, a pin written into the request, and nothing outside the route passed on.

use std::sync::{Arc, Mutex};

use agency_server::job_run::openrouter_tap::*;
use axum::body::Body;
use axum::extract::Request;
use axum::response::Response;
use serde_json::{Value, json};
use tokio::sync::oneshot;

/// A stand-in gateway's server, taken down once this is dropped or closed.
struct Stopping(Option<oneshot::Sender<()>>);

impl Stopping {
  fn close(mut self) {
    if let Some(stop) = self.0.take() {
      let _ = stop.send(());
    }
  }
}

impl Drop for Stopping {
  fn drop(&mut self) {
    if let Some(stop) = self.0.take() {
      let _ = stop.send(());
    }
  }
}

// A gateway standing in on a loopback port: it answers every request with the body and type given,
// and keeps what it was asked.
async fn gateway(
  kind: &'static str,
  answer: &'static str,
) -> (String, Arc<Mutex<Vec<(String, String)>>>, Stopping) {
  let asked = Arc::new(Mutex::new(vec![]));
  let kept = asked.clone();
  let router = axum::Router::new().fallback(move |request: Request| {
    let kept = kept.clone();

    async move {
      let path = request.uri().path().to_owned();
      let body = axum::body::to_bytes(request.into_body(), usize::MAX).await.unwrap_or_default();

      kept.lock().unwrap().push((path, String::from_utf8_lossy(&body).into_owned()));

      Response::builder().header("content-type", kind).body(Body::from(answer)).unwrap()
    }
  });
  let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
  let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
  let (stop, stopped) = oneshot::channel::<()>();

  tokio::spawn(async move {
    let _ = axum::serve(listener, router)
      .with_graceful_shutdown(async {
        let _ = stopped.await;
      })
      .await;
  });

  // The gateway's own server goes down as the test drops what it was handed.
  (base, asked, Stopping(Some(stop)))
}

async fn post(url: &str, body: &str) -> String {
  reqwest::Client::new()
    .post(url)
    .body(body.to_owned())
    .send()
    .await
    .unwrap()
    .text()
    .await
    .unwrap()
}

const STREAM: &str = "data: {\"provider\":\"Anthropic\",\"usage\":{\"prompt_tokens\":100,\"completion_tokens\":5,\"prompt_tokens_details\":{\"cached_tokens\":60,\"cache_write_tokens\":10},\"cost\":0.5}}\n\ndata: {\"provider\":\"Google\",\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":1}}\n\ndata: [DONE]";

#[tokio::test]
async fn a_stream_is_handed_back_whole_and_its_upstream_and_usage_read() {
  let (base, _, gateway) = gateway("text/event-stream", STREAM).await;
  let tap = open(None, &base).await.unwrap();
  let answer = post(&format!("{}/chat/completions", tap.url), "{}").await;
  let seen = tap.observed();

  assert_eq!(answer, STREAM);
  assert_eq!(seen.upstream.as_deref(), Some("Google"));
  assert_eq!(seen.last.map(|last| last.input_tokens), Some(10));
  assert_eq!(
    seen.total,
    Some(Usage {
      input_tokens: 40,
      output_tokens: 6,
      cache_read_tokens: 60,
      cache_write_tokens: 10,
      cost: 0.5
    })
  );

  tap.close();
  gateway.close();
}

#[tokio::test]
async fn a_pinned_request_names_its_upstream_and_keeps_the_rest_of_its_preference() {
  let (base, asked, gateway) = gateway("application/json", "{}").await;
  let tap = open(
    Some(Pin {
      upstream: "Anthropic".into(),
      allow_fallbacks: true,
    }),
    &base,
  )
  .await
  .unwrap();

  post(&format!("{}/chat/completions", tap.url), r#"{"model":"m","provider":{"sort":"price"}}"#).await;
  post(&format!("{}/chat/completions", tap.url), "not json").await;

  let asked = asked.lock().unwrap().clone();
  let first: Value = serde_json::from_str(&asked[0].1).unwrap();

  assert_eq!(asked[0].0, "/chat/completions");
  assert_eq!(
    first["provider"],
    json!({ "sort": "price", "order": ["Anthropic"], "allow_fallbacks": true })
  );
  assert_eq!(asked[1].1, "not json");

  tap.close();
  gateway.close();
}

#[tokio::test]
async fn a_whole_answer_is_read_for_its_usage_and_a_path_outside_the_route_never_leaves() {
  let (base, asked, gateway) = gateway(
    "application/json",
    r#"{"provider":"OpenAI","usage":{"prompt_tokens":7,"completion_tokens":2}}"#,
  )
  .await;
  let tap = open(None, &base).await.unwrap();

  post(&format!("{}/chat/completions", tap.url), "{}").await;

  let elsewhere = reqwest::get(tap.url.replace("/api/v1", "/other")).await.unwrap().status();

  assert_eq!(elsewhere.as_u16(), 404);
  assert_eq!(asked.lock().unwrap().len(), 1);
  assert_eq!(tap.observed().upstream.as_deref(), Some("OpenAI"));
  assert_eq!(tap.observed().last.map(|last| (last.input_tokens, last.output_tokens)), Some((7, 2)));

  tap.close();
  gateway.close();
}

#[test]
fn usage_is_split_into_its_kinds_and_clamped() {
  let usage =
    usage_of(&json!({ "prompt_tokens": 5, "prompt_tokens_details": { "cached_tokens": 9 }, "cost": -1 }));

  assert_eq!(
    usage,
    Usage {
      input_tokens: 0,
      output_tokens: 0,
      cache_read_tokens: 9,
      cache_write_tokens: 0,
      cost: 0.0
    }
  );
}
