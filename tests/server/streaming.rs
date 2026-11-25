//! The block a session's answer is writing, read from the model gateway (docs/api.md, 「sessions」): the gateway's
//! answer for the program session, behind the gateway's key, and none where it holds none or does not answer.

use std::os::unix::fs::PermissionsExt;

use agency_server::streaming::open_block;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use harness_providers::Machine;
use policy::providers::registered::REGISTERED;
use serde_json::json;

// A gateway standing in: session-a's answer writes a text block, every other session's writes none; the key is
// asked for as the gateway asks for it.
async fn stand_in() -> String {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());
  let app = axum::Router::new().route(
    "/gateway/streams/{session}",
    get(|Path(session): Path<String>, headers: HeaderMap| async move {
      let keyed =
        headers.get("authorization").and_then(|value| value.to_str().ok()) == Some("Bearer a key");

      match (keyed, session.as_str()) {
        (false, _) => StatusCode::UNAUTHORIZED.into_response(),
        (true, "session-a") => {
          axum::Json(json!({ "index": 1, "kind": "text", "bytes": 12 })).into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
      }
    }),
  );

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  base
}

fn machine(data: &std::path::Path, gateway: &str) -> Machine {
  let key = data.join("env").join(&REGISTERED.presence.key_file);

  std::fs::create_dir_all(key.parent().unwrap()).unwrap();
  std::fs::write(&key, "a key\n").unwrap();
  std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();

  Machine::with_env([("BMO_LLM_GATEWAY_URL".to_owned(), gateway.to_owned())], data.to_path_buf())
}

// "the open block of a program session is read from the gateway with its key, and none is read where it holds
// none or does not answer"
#[tokio::test]
async fn a_session_s_open_block_is_the_gateway_s_and_none_where_it_holds_none() {
  let data = tempfile::tempdir().unwrap();
  let machine = machine(data.path(), &stand_in().await);

  assert_eq!(
    open_block(&machine, "session-a").await,
    Some(json!({ "index": 1, "kind": "text", "bytes": 12 }))
  );
  assert_eq!(open_block(&machine, "session-b").await, None);

  let gone = std::net::TcpListener::bind("[::1]:0").unwrap().local_addr().unwrap();
  let unanswered = self::machine(data.path(), &format!("http://[::1]:{}", gone.port()));

  assert_eq!(open_block(&unanswered, "session-a").await, None);
}
