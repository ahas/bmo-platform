use agency_server::Service;
use agency_server::config::Config;
use agency_server::lines::token::*;
use agency_server::places::Ground;
use harness_providers::Machine;
use serde_json::json;

// The paragraph a stand-in tool host words, which no agency text holds.
pub(crate) const PARAGRAPH: &str = "Reach a tool with bmo use: list, describe, then call.";

/// A tool host standing on `socket` that answers `GET /discipline` as text to the issuer `token` alone.
pub(crate) fn tool_host(socket: &std::path::Path, token: &'static str) {
  let listener = tokio::net::UnixListener::bind(socket).unwrap();
  let app = axum::Router::new().route(
    "/discipline",
    axum::routing::get(move |headers: axum::http::HeaderMap| async move {
      let bearer = headers.get("authorization").and_then(|value| value.to_str().ok());

      if bearer == Some(format!("Bearer {token}").as_str()) {
        (
          axum::http::StatusCode::OK,
          [("content-type", "text/plain; charset=utf-8")],
          PARAGRAPH.to_owned(),
        )
      } else {
        (
          axum::http::StatusCode::UNAUTHORIZED,
          [("content-type", "application/json")],
          json!({ "error": "the token is not known", "code": "unauthorized" }).to_string(),
        )
      }
    }),
  );

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
}

// A ground whose tool host is the socket named, asked with the issuer token named.
fn ground_with_host(home: &std::path::Path, socket: &std::path::Path, issuer: &str) -> Ground {
  let variables: std::collections::HashMap<String, String> = std::collections::HashMap::new();
  let service = Service::new(
    sea_orm::DatabaseConnection::Disconnected,
    Config::read(|name| variables.get(name).cloned()).unwrap(),
  );
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), home.to_string_lossy().into_owned()),
      ("BMO_MCP_HOST_SOCKET".to_owned(), socket.to_string_lossy().into_owned()),
      ("BMO_MCP_HOST_ISSUER_TOKEN".to_owned(), issuer.to_owned()),
    ],
    home.join("data"),
  );

  Ground::new(&service, &machine, None)
}

#[tokio::test]
async fn the_tool_hosts_discipline_is_read_whole_with_the_issuer_token() {
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("mcp-host.sock");

  tool_host(&socket, "issuer");

  assert_eq!(
    discipline(&ground_with_host(home.path(), &socket, "issuer")).await.as_deref(),
    Ok(PARAGRAPH)
  );

  let refused = discipline(&ground_with_host(home.path(), &socket, "someone")).await.unwrap_err();

  assert!(refused.contains("401"), "{refused}");
}

#[tokio::test]
async fn a_tool_host_that_does_not_answer_gives_no_discipline_and_says_where_it_was_asked() {
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("nobody.sock");
  let why = discipline(&ground_with_host(home.path(), &socket, "issuer")).await.unwrap_err();

  assert!(why.contains(&socket.to_string_lossy().into_owned()), "{why}");
}

// The chat line names the gate's address and token file for the translate tool's record; a variable the
// deployment leaves unset or empty is handed nothing, and a line that names none hands none.
#[test]
fn a_line_hands_its_record_the_deployment_variables_it_names_where_they_are_set() {
  let home = tempfile::tempdir().unwrap();
  let variables: std::collections::HashMap<String, String> = std::collections::HashMap::new();
  let service = Service::new(
    sea_orm::DatabaseConnection::Disconnected,
    Config::read(|name| variables.get(name).cloned()).unwrap(),
  );
  let machine = Machine::with_env(
    [
      ("BMO_GATE_URL".to_owned(), " http://[::1]:9 ".to_owned()),
      ("BMO_GATE_TOKEN_FILE".to_owned(), String::new()),
      ("BMO_OTHER".to_owned(), "x".to_owned()),
    ],
    home.path().join("data"),
  );
  let ground = Ground::new(&service, &machine, None);

  assert_eq!(
    line_variables(&ground, "chat"),
    [("BMO_GATE_URL".to_owned(), "http://[::1]:9".to_owned())]
  );
  assert_eq!(
    line_variables(&ground, "worker"),
    line_variables(&ground, "chat"),
    "the worker names the gate's too"
  );
  assert!(line_variables(&ground, "translate").is_empty(), "a line naming none is handed none");
}

/// A tool host standing on `socket` that keeps one asynchronous call running under the record of `token` while
/// `running` holds, and one under another record always, answers them to the issuer's running read, and keeps the
/// tokens taken back from it.
fn host_keeping_a_call(
  socket: &std::path::Path,
  token: &str,
  running: std::sync::Arc<std::sync::atomic::AtomicBool>,
  taken_back: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
) {
  use sha2::Digest;

  let record = hex::encode(sha2::Sha256::digest(token.as_bytes()));
  let listener = tokio::net::UnixListener::bind(socket).unwrap();
  let app = axum::Router::new()
    .route(
      "/uses",
      axum::routing::get(move |query: axum::extract::Query<std::collections::HashMap<String, String>>| {
        let record = record.clone();
        let running = running.clone();

        async move {
          assert_eq!(query.get("state").map(String::as_str), Some("running"));

          let mut items = vec![json!({ "id": "other", "session": "another-record", "state": "running" })];

          if running.load(std::sync::atomic::Ordering::SeqCst) {
            items.push(json!({ "id": "deploy", "session": record, "state": "running" }));
          }

          axum::Json(json!({ "items": items }))
        }
      }),
    )
    .route(
      "/tool-sessions/{token}",
      axum::routing::delete(move |axum::extract::Path(token): axum::extract::Path<String>| {
        let taken_back = taken_back.clone();

        async move {
          taken_back.lock().unwrap().push(token);
          axum::http::StatusCode::NO_CONTENT
        }
      }),
    );

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
}

#[tokio::test]
async fn a_replaced_programs_token_stands_while_a_call_asked_under_it_runs_and_is_taken_back_once_it_ends()
{
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("mcp-host.sock");
  let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
  let taken_back = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

  host_keeping_a_call(&socket, "old-token", running.clone(), taken_back.clone());

  let ground = ground_with_host(home.path(), &socket, "issuer");

  let_go(&ground, "old-token", std::time::Duration::from_millis(20)).await;

  assert!(taken_back.lock().unwrap().is_empty(), "the record stands while its call runs");

  running.store(false, std::sync::atomic::Ordering::SeqCst);

  let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);

  while taken_back.lock().unwrap().is_empty() && std::time::Instant::now() < deadline {
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
  }

  assert_eq!(*taken_back.lock().unwrap(), ["old-token"], "taken back once its last call ended");
}

#[tokio::test]
async fn a_replaced_programs_token_with_no_call_of_its_own_running_is_taken_back_at_once() {
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("mcp-host.sock");
  let taken_back = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

  host_keeping_a_call(
    &socket,
    "old-token",
    std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    taken_back.clone(),
  );

  let ground = ground_with_host(home.path(), &socket, "issuer");

  let_go(&ground, "old-token", std::time::Duration::from_secs(3_600)).await;

  assert_eq!(
    *taken_back.lock().unwrap(),
    ["old-token"],
    "another record's running call holds nothing back"
  );
}
