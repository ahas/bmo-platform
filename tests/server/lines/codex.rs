use std::collections::BTreeMap;

use agency_server::lines::codex::*;
use indexmap::IndexMap;
use serde_json::{Value, json};

#[test]
fn a_chat_allowed_all_it_shows_puts_every_gate_aside_and_a_line_with_no_tools_reads_only() {
  let chat = Shape::of_line("chat", "m", "high", true, "/w", None, None, "Bash,Read", "Bash,Read,mcp__bmo");

  assert!(chat.has_tools && chat.may_write && chat.bypass && !chat.worker);
  assert_eq!((chat.sandbox(), chat.reasoning.as_str()), ("danger-full-access", "high"));

  let gate = Shape::of_line("gate", "m", "high", false, "/w", None, None, "", "");

  assert!(!gate.has_tools && !gate.bypass);
  assert_eq!((gate.sandbox(), gate.reasoning.as_str()), ("read-only", "low"));
}

#[test]
fn a_bearer_header_names_the_variable_its_token_is_read_from() {
  let headers: IndexMap<String, Value> =
    [("Authorization".to_owned(), json!("Bearer ${BMO_TOOL_TOKEN}"))].into_iter().collect();
  let literal: IndexMap<String, Value> =
    [("Authorization".to_owned(), json!("Bearer abc"))].into_iter().collect();

  assert_eq!(bearer_variable(&headers).as_deref(), Some("BMO_TOOL_TOKEN"));
  assert_eq!(bearer_variable(&literal), None);
}

#[test]
fn a_server_is_handed_the_place_and_the_runs_own_variables_by_name() {
  let env: BTreeMap<String, String> = [("BMO_TURN_CHAT", "1"), ("PATH", "/bin"), ("BMO_DATA_DIR", "/d")]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value.to_owned()))
    .collect();

  assert_eq!(forwarded_names(&env), vec!["BMO_AGENT_PLACE", "BMO_DATA_DIR", "BMO_TURN_CHAT"]);
}

#[test]
fn a_turn_is_the_requests_its_rollout_counted_since_it_began() {
  let sessions = tempfile::tempdir().unwrap();
  let day = sessions.path().join("2026").join("10");
  let count = |input: i64, cached: i64| {
    json!({ "type": "event_msg", "payload": { "type": "token_count", "info": { "last_token_usage": {
      "input_tokens": input, "cached_input_tokens": cached, "output_tokens": 3 } } } })
    .to_string()
  };
  let started = json!({ "type": "event_msg", "payload": { "type": "task_started" } }).to_string();

  std::fs::create_dir_all(&day).unwrap();
  std::fs::write(
    day.join("rollout-thread-9.jsonl"),
    [
      count(50, 0),
      started,
      count(1200, 1000),
      count(0, 0),
      count(1300, 1200),
      "{\"cut".to_owned(),
    ]
    .join("\n"),
  )
  .unwrap();

  let requests = turn_requests(sessions.path(), "thread-9").unwrap();

  assert_eq!(
    requests.iter().map(|one| (one.input_tokens, one.cache_read_tokens)).collect::<Vec<_>>(),
    vec![(200, 1000), (100, 1200)]
  );
  assert!(turn_requests(sessions.path(), "thread-8").is_none());
}

#[test]
fn only_a_missing_rollout_loses_the_thread() {
  assert!(lost_thread("Error: No rollout found for thread id 019a"));
  assert!(!lost_thread("You've hit your usage limit"));
}
