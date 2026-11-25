//! `bmo-agency quiet` read as the testbed reads a quiet's code (docs/api.md, 「The command」): no server
//! listening and a server whose build answers no quiet route are quiet, work under way is 1 with a line for
//! each piece, and a refusal, a failed ask or an answer with no work list could not tell.

use agency_server::verbs::quiet::{Asked, QUIET, UNDER_WAY, UNTOLD, judged};
use serde_json::json;

#[test]
fn the_quiet_s_code_is_what_the_server_answered() {
  assert_eq!(judged(Asked::Unreached).0, QUIET);
  assert_eq!(judged(Asked::Answered(404, None)).0, QUIET);
  assert_eq!(
    judged(Asked::Answered(200, Some(json!({ "quiet": true, "work": [] })))),
    (QUIET, vec![])
  );

  let (code, lines) = judged(Asked::Answered(
    200,
    Some(json!({ "quiet": false, "work": [
      { "run": "chat:-5:1", "session": "bmo@-5", "agent": "bmo", "turnId": 7, "since": 1000 }
    ] })),
  ));

  assert_eq!(code, UNDER_WAY);
  assert_eq!(lines.len(), 1);
  assert!(lines[0].contains("chat:-5:1") && lines[0].contains("bmo@-5"), "{lines:?}");

  assert_eq!(judged(Asked::Answered(500, Some(json!({ "error": "store gone" })))).0, UNTOLD);
  assert_eq!(judged(Asked::Answered(200, Some(json!({})))).0, UNTOLD);
  assert_eq!(judged(Asked::Failed("refused".into())).0, UNTOLD);
}

// The command as the deployment runs it, against the port and token given, with nothing of the machine's own.
async fn quiet(port: u16, data: &std::path::Path) -> (Option<i32>, String) {
  let ran = tokio::process::Command::new(crate::support::binary::path("bmo-agency"))
    .arg("quiet")
    .env_clear()
    .env("PATH", "/usr/bin:/bin")
    .env("HOME", data)
    .env("BMO_DATA_DIR", data)
    .env("AGENCY_CONTROL_PORT", port.to_string())
    .env("AGENCY_CONTROL_TOKEN", "quiet-token")
    .output()
    .await
    .unwrap();

  (ran.status.code(), String::from_utf8_lossy(&ran.stdout).into_owned())
}

// A server standing in for agency's on a port of its own, answering the quiet route as it is told.
async fn answering(status: u16, body: serde_json::Value) -> u16 {
  use axum::routing::get;

  let app = axum::Router::new().route(
    "/quiet",
    get(move || {
      let body = body.clone();

      async move { (axum::http::StatusCode::from_u16(status).unwrap(), axum::Json(body)) }
    }),
  );
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  port
}

#[tokio::test]
async fn the_command_ends_quiet_with_no_server_or_an_older_one_and_names_the_work_a_server_holds() {
  let data = tempfile::tempdir().unwrap();
  // A port nothing listens on.
  let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();

  assert_eq!(quiet(free, data.path()).await.0, Some(0));

  let older = answering(404, json!({ "code": "not-found", "error": "not found" })).await;

  assert_eq!(quiet(older, data.path()).await.0, Some(0));

  let busy = answering(
    200,
    json!({ "quiet": false, "work": [
      { "run": "chat:-5:1", "session": "bmo@-5", "agent": "bmo", "turnId": 7, "since": 1000 }
    ] }),
  )
  .await;
  let (code, said) = quiet(busy, data.path()).await;

  assert_eq!(code, Some(1));
  assert!(said.contains("chat:-5:1"), "{said}");

  let failing = answering(500, json!({ "code": "failed", "error": "the store is gone" })).await;

  assert_eq!(quiet(failing, data.path()).await.0, Some(2));
}
