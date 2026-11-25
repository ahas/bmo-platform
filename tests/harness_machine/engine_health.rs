//! An engine's health: its answer read field by field, and the addresses asked in order.

use axum::Router;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::get;
use harness_machine::engine_health::*;
use serde_json::json;

#[test]
fn an_answer_is_read_field_by_field_and_a_wrong_shape_is_none() {
  assert_eq!(
    read_health(
      &json!({ "context": 4096, "model": "rvn.gguf", "in_hand": 0, "aside": { "reason": "training", "until": 99 } })
    ),
    Some(EngineHealth {
      context: Some(4096),
      model: Some("rvn.gguf".into()),
      in_hand: Some(0),
      aside: Some(Aside {
        reason: "training".into(),
        until: json!(99)
      }),
    })
  );
  assert_eq!(
    read_health(&json!({ "context": 0, "model": "", "in_hand": -1, "aside": { "reason": 3 } })),
    Some(EngineHealth {
      context: None,
      model: None,
      in_hand: None,
      aside: None
    })
  );
  assert_eq!(read_health(&json!("ok")), None);
}

// A stand-in engine on the loopback that answers its health only to the key it was given.
async fn engine(key: &'static str) -> String {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let at = listener.local_addr().unwrap();
  let app = Router::new().route(
    "/health",
    get(move |headers: HeaderMap| async move {
      let given = headers.get("authorization").and_then(|value| value.to_str().ok()).unwrap_or_default();

      if given == format!("Bearer {key}") {
        axum::Json(json!({ "model": "rvn.gguf", "context": 8192 })).into_response()
      } else {
        axum::http::StatusCode::UNAUTHORIZED.into_response()
      }
    }),
  );

  tokio::spawn(async move { axum::serve(listener, app).await });

  format!("http://{at}/v1/chat/completions")
}

#[tokio::test]
async fn the_first_address_with_a_key_that_answers_gives_the_health() {
  let root = tempfile::tempdir().unwrap();
  let url = engine("k-right").await;

  std::fs::create_dir_all(root.path().join("env")).unwrap();
  std::fs::write(root.path().join("env/wrong.key"), "k-wrong\n").unwrap();
  std::fs::write(root.path().join("env/right.key"), "k-right\n").unwrap();

  let addresses = |files: &[Option<&str>]| -> Vec<Address> {
    files
      .iter()
      .map(|file| Address {
        url: url.clone(),
        key_file: file.map(str::to_owned),
      })
      .collect()
  };

  assert_eq!(
    health(
      root.path(),
      &addresses(&[None, Some("missing.key"), Some("wrong.key"), Some("right.key")])
    )
    .await
    .and_then(|health| health.model),
    Some("rvn.gguf".into())
  );
  assert_eq!(health(root.path(), &addresses(&[None, Some("wrong.key")])).await, None);

  // An address that does not answer at all leaves the next to be asked.
  let unanswered = Address {
    url: "http://[::1]:9/v1/chat/completions".into(),
    key_file: Some("right.key".into()),
  };

  assert!(
    health(root.path(), &[unanswered, addresses(&[Some("right.key")]).remove(0)])
      .await
      .is_some()
  );
}
