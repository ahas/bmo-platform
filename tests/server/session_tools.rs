//! What a resident program's session holds of its line's servers, and what its error stream said
//! (docs/agent-control.md, 「dsh's providers: OpenRouter and DeepSeek」): the stream's new lines read once,
//! the servers dsh says did not activate read out of them, the tool host's account of what a session's
//! clients reached, and the fact a session is told of each absent server.

use agency_server::lines::dsh::inactive_servers;
use agency_server::lines::place::stderr_file;
use agency_server::session_tools::*;
use serde_json::json;

use crate::support::wording::assert_no_instruction;

// What dsh's launcher writes when two clients of a standing session did not activate, as measured.
const FAILED_START: &str = "dsh: warning: 2 entries did not activate
mcp-bmo (@deepseek-ai/dsh-mcp-client): Error: mcp-client(bmo): initial connection or tool synchronization failed
    at Object.apply [as callback] (file:///lib/index.js:832:67)
    at process.processTicksAndRejections (node:internal/process/task_queues:104:5)
SdkHttpError: Version negotiation failed: the server requires authorization (HTTP 401)
    at classifyHttpError (file:///client/dist/index.mjs:2485:11)
mcp-job (@deepseek-ai/dsh-mcp-client): ValidationError: invalid config:
    at Config (file:///lib/index.js:780:1)
";

fn lines(text: &str) -> Vec<String> {
  text.lines().map(str::to_owned).collect()
}

#[test]
fn each_client_that_did_not_activate_is_named_with_the_cause_under_its_stack() {
  assert_eq!(
    inactive_servers(&lines(FAILED_START)),
    vec![
      (
        "bmo".to_owned(),
        "Error: mcp-client(bmo): initial connection or tool synchronization failed: SdkHttpError: Version \
         negotiation failed: the server requires authorization (HTTP 401)"
          .to_owned()
      ),
      ("job".to_owned(), "ValidationError: invalid config:".to_owned()),
    ]
  );
  assert!(inactive_servers(&lines("dsh: ready\nsome other line\n")).is_empty());
}

#[test]
fn a_server_is_absent_where_dsh_says_so_or_the_host_saw_other_servers_reached_but_not_it() {
  let inactive = vec![("job".to_owned(), "it would not start".to_owned())];
  let reached = vec!["web".to_owned(), "toolbook".to_owned()];
  let named =
    |absent: Vec<(String, String)>| absent.into_iter().map(|(server, _)| server).collect::<Vec<_>>();

  assert_eq!(named(absent("chat", &inactive, None)), ["job"]);
  assert_eq!(
    named(absent("chat", &inactive, Some(&reached))),
    ["job", "bmo", "gate", "eye", "ear", "testbed", "voice", "research"]
  );
  // A host that saw no client since it started knows nothing either way.
  assert_eq!(named(absent("chat", &[], Some(&[]))), Vec::<String>::new());
  assert_eq!(
    named(absent(
      "chat",
      &[],
      Some(&[
        "bmo".into(),
        "web".into(),
        "gate".into(),
        "eye".into(),
        "ear".into(),
        "testbed".into(),
        "voice".into(),
        "research".into()
      ])
    )),
    Vec::<String>::new()
  );
}

#[test]
fn an_absent_server_is_one_fact_for_the_session_in_the_turns_room_of_its_own_kind() {
  let body = absent_instruction("agent-a", -7, "web", "it would not open");

  assert_eq!(body["agentName"], "agent-a");
  assert_eq!(body["room"], -7);
  assert_eq!(body["kind"], "session-tools:web");
  assert_eq!(body["said"]["order"], serde_json::Value::Null);

  let fact = body["said"]["fact"].as_str().unwrap();

  // The fact names the server and why; how a hosted tool is reached is the tools paragraph's.
  assert!(fact.contains("web") && fact.contains("it would not open"), "{fact}");
  assert!(!fact.contains("bmo use"), "{fact}");
  assert_no_instruction(fact);
}

#[test]
fn the_error_stream_is_read_once_line_by_line_and_again_from_its_head_once_started_afresh() {
  let place = tempfile::tempdir().unwrap();
  let file = stderr_file(place.path());

  assert!(error_news(place.path(), 10).is_empty(), "no stream is no news");

  std::fs::write(&file, "one\ntwo\nhalf").unwrap();
  assert_eq!(error_news(place.path(), 10), ["one", "two"]);
  assert!(error_news(place.path(), 10).is_empty(), "a line still being written waits");

  std::fs::write(&file, "one\ntwo\nhalf a line\nthree\nfour\n").unwrap();
  assert_eq!(error_news(place.path(), 1), ["half a line"], "the rest past the most is passed over");
  assert!(error_news(place.path(), 10).is_empty());

  std::fs::write(&file, "fresh\n").unwrap();
  assert_eq!(error_news(place.path(), 10), ["fresh"]);
}

#[tokio::test]
async fn the_servers_a_session_reached_are_read_from_the_tool_host_and_none_reached_is_unknown() {
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("host.sock");
  let listener = tokio::net::UnixListener::bind(&socket).unwrap();
  let app = axum::Router::new().route(
    "/tool-sessions/{token}",
    axum::routing::get(|axum::extract::Path(token): axum::extract::Path<String>| async move {
      let connected = if token == "reached" {
        json!([{ "namespace": "bmo", "at": 1 }, { "namespace": "web", "at": 2 }])
      } else {
        json!([])
      };

      axum::Json(json!({
        "id": "0f", "holderName": "agent-a", "namespaces": ["bmo", "web", "gate"], "capabilities": [], "grants": [],
        "shown": [], "variables": [], "labels": {}, "expiresAt": null, "createdAt": 1, "connected": connected,
      }))
    }),
  );

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  let variables: std::collections::HashMap<String, String> = std::collections::HashMap::new();
  let service = agency_server::Service::new(
    sea_orm::DatabaseConnection::Disconnected,
    agency_server::config::Config::read(|name| variables.get(name).cloned()).unwrap(),
  );
  let machine = harness_providers::Machine::with_env(
    [
      ("HOME".to_owned(), home.path().to_string_lossy().into_owned()),
      ("BMO_MCP_HOST_SOCKET".to_owned(), socket.to_string_lossy().into_owned()),
      ("BMO_MCP_HOST_ISSUER_TOKEN".to_owned(), "issuer".to_owned()),
    ],
    home.path().join("data"),
  );
  let ground = agency_server::places::Ground::new(&service, &machine, None);

  assert_eq!(
    agency_server::lines::token::connected(&ground, "reached").await,
    Some(vec!["bmo".to_owned(), "web".to_owned()])
  );
  assert_eq!(agency_server::lines::token::connected(&ground, "fresh").await, None);
}
