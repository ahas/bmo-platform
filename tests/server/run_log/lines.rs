//! What one line of a Claude Code program's record leaves in the run log: its words, its calls, a
//! compaction, a call reached through the toolbook, and a result with its state and evidence.

use agency_server::run_log::json::Json;
use agency_server::run_log::lines::{record_lines, stream_lines};

fn event(text: &str) -> Json {
  Json::parse(text).unwrap()
}

#[test]
fn an_assistant_line_leaves_its_words_its_thinking_and_its_calls() {
  let lines = record_lines(&event(
    r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"thinking","thinking":""},{"type":"text","text":"hi"},{"type":"tool_use","id":"t1","name":"Agent","input":{"subagent_type":"scout","prompt":"look"}}]}}"#,
  ));

  assert_eq!(lines.len(), 2);
  assert_eq!((lines[0].kind, lines[0].text.as_str(), lines[0].source_block), ("text", "hi", Some(1)));
  assert_eq!(lines[1].name.as_deref(), Some("Agent(scout)"));
  assert_eq!(lines[1].text, r#"{"subagent_type":"scout","prompt":"look"}"#);
  assert_eq!((lines[1].state, lines[1].source_id.as_deref()), (Some("requested"), Some("m1")));
}

#[test]
fn an_assistant_line_keeps_the_request_its_message_answers_with_the_usage_the_provider_stated() {
  let lines = record_lines(&event(
    r#"{"type":"assistant","message":{"id":"m1","usage":{"input_tokens":12,"output_tokens":7,"cache_read_input_tokens":900,"cache_creation_input_tokens":30,"service_tier":"standard"},"content":[{"type":"text","text":"hi"},{"type":"tool_use","id":"t","name":"Bash","input":{"command":"ls"}}]}}"#,
  ));

  assert_eq!(lines.len(), 2);

  for line in &lines {
    let request = line
      .metadata
      .as_ref()
      .and_then(|metadata| metadata.get("request"))
      .expect("the request");

    assert_eq!(request.get("id").and_then(Json::as_str), Some("m1"));
    assert_eq!(
      request.get("usage").map(Json::stringify).as_deref(),
      Some(
        r#"{"input_tokens":12,"output_tokens":7,"cache_read_input_tokens":900,"cache_creation_input_tokens":30}"#
      )
    );
  }

  // A message that states no usage keeps none, and a user line never does.
  let bare = record_lines(&event(
    r#"{"type":"assistant","message":{"id":"m2","content":[{"type":"text","text":"hi"}]}}"#,
  ));

  assert!(bare[0].metadata.as_ref().and_then(|metadata| metadata.get("request")).is_none());
}

#[test]
fn a_compaction_is_a_line_of_its_own_with_how_it_was_set_off_and_how_full_the_session_was() {
  let lines = stream_lines(&event(
    r#"{"type":"system","subtype":"compact_boundary","compact_metadata":{"trigger":"auto","pre_tokens":167000}}"#,
  ));

  assert_eq!(lines.len(), 1);
  assert_eq!((lines[0].kind, lines[0].text.as_str()), ("compact", "auto · 167000 tokens before"));
}

#[test]
fn a_toolbook_call_is_logged_as_the_tool_it_reached() {
  let lines = record_lines(&event(
    r#"{"type":"assistant","uuid":"u","message":{"content":[{"type":"tool_use","id":"t","name":"mcp__toolbook__call","input":{"tool":"web/fetch","arguments":{"url":"x"}}}]}}"#,
  ));

  assert_eq!(lines[0].name.as_deref(), Some("mcp__web__fetch"));
  assert_eq!(lines[0].text, r#"{"url":"x"}"#);
  assert!(lines[0].metadata.is_some());
}

#[test]
fn a_result_line_carries_its_answer_its_state_and_its_evidence() {
  let lines = record_lines(&event(
    r#"{"type":"user","parent_tool_use_id":"p","toolUseResult":{"filePath":"/a","type":"update"},"message":{"content":[{"type":"tool_result","tool_use_id":"t","content":[{"type":"text","text":"ok"},{"type":"image"}]}]}}"#,
  ));

  assert_eq!(lines[0].text, "ok\n{\"type\":\"image\"}");
  assert_eq!((lines[0].state, lines[0].parent.as_deref()), (Some("succeeded"), Some("p")));
  assert_eq!(
    lines[0].metadata.as_ref().map(Json::stringify).as_deref(),
    Some(r#"{"toolUseResult":{"filePath":"/a","type":"update"}}"#)
  );
}
