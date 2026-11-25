//! The front's routes: each answered once, the session contract's and the boundary's own among them.

use agency_server::control::{moved_routes, route_faults, route_name};

#[test]
fn every_route_is_answered_once() {
  assert_eq!(route_faults(), Vec::<String>::new());
}

#[test]
fn the_boundary_s_routes_are_linked_in() {
  let mut linked: Vec<String> =
    moved_routes().map(|route| route_name(&route.method(), route.path())).collect();
  let mut stated: Vec<String> = agency_server::openapi::OPERATIONS
    .iter()
    .map(|operation| format!("{} {}", operation.method, operation.path))
    .collect();

  linked.sort();
  stated.sort();

  assert_eq!(linked, stated);
}

#[test]
fn the_committed_document_is_what_the_code_generates() {
  let committed = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/openapi.json"))
    .expect("the document is committed");

  assert_eq!(committed, agency_server::openapi::openapi_json());
}

#[test]
fn the_harness_plugin_lists_exactly_the_document_s_tools() {
  let listed: Vec<&str> =
    agency_server::mcp::tools(Some("chat-agent")).iter().map(|tool| tool.name).collect();
  let document = agency_server::openapi::openapi();
  let stated: Vec<&str> = document["x-tools"]
    .as_array()
    .expect("the document states its tools")
    .iter()
    .map(|tool| tool["name"].as_str().expect("a tool has a name"))
    .collect();

  assert_eq!(
    listed,
    ["file_item", "stop_job", "review_job", "assign_worker", "release_worker", "run_job"]
  );
  assert_eq!(listed, stated);
}
