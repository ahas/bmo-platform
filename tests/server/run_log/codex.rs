//! What a Codex run leaves in the run log: its diffs read as hunks, a record's changes by path, a resumed
//! thread read from where it stood, and the stream's and the rollout's items as lines.

use agency_server::run_log::codex::*;
use agency_server::run_log::json::Json;
use agency_server::run_log::lines::Line;

fn json(text: &str) -> Json {
  Json::parse(text).unwrap()
}

// What Codex writes into its session record when a patch has been applied, trimmed to what matters.
fn file_change(changes: &str) -> String {
  format!(
    r#"{{"timestamp":"2026-09-14T19:15:17.063Z","type":"event_msg","payload":{{"type":"item_completed","item":{{"type":"FileChange","id":"exec-1","changes":{changes},"status":"completed"}}}}}}"#
  ) + "\n"
}

const UPDATE: &str =
  r#"{"type":"update","unified_diff":"@@ -26,2 +26,3 @@\n \n+ligne née\n fin\n","move_path":null}"#;

#[test]
fn a_unified_diff_reads_as_hunks_leaving_out_what_is_no_line_of_the_file() {
  let hunks = unified_hunks(
    "--- a/x\n+++ b/x\n@@ -3 +3,2 @@\n-a\n+b\n+c\n\\ No newline at end of file\n@@ -10,2 +11,2 @@\n x\n\n",
  );

  assert_eq!(
    Json::Array(hunks).stringify(),
    r#"[{"oldStart":3,"oldLines":1,"newStart":3,"newLines":2,"lines":["-a","+b","+c"]},{"oldStart":10,"oldLines":2,"newStart":11,"newLines":2,"lines":[" x"," "]}]"#
  );
}

#[test]
fn a_record_s_change_names_its_paths_and_what_was_done_to_each() {
  let change = recorded_change(&json(&file_change(&format!(
    r#"{{"/r/a.ts":{UPDATE},"/r/b.ts":{{"type":"add","content":"b\n"}},"/r/c.ts":{{"type":"delete","content":"c\n"}},"/r/d.ts":{{"type":"update","unified_diff":"@@ -1 +1 @@\n-d\n+e\n","move_path":"/r/e.ts"}}}}"#
  ))))
  .unwrap();

  assert_eq!(change.paths, ["/r/a.ts", "/r/b.ts", "/r/c.ts", "/r/d.ts"]);
  assert_eq!(
    Json::Array(change.diffs).stringify(),
    concat!(
      r#"[{"filePath":"/r/a.ts","type":null,"hunks":[{"oldStart":26,"oldLines":2,"newStart":26,"newLines":3,"lines":[" ","+ligne née"," fin"]}],"content":null},"#,
      r#"{"filePath":"/r/b.ts","type":"create","hunks":[],"content":"b\n"},"#,
      r#"{"filePath":"/r/c.ts","type":"delete","hunks":[],"content":"c\n"},"#,
      r#"{"filePath":"/r/e.ts","type":null,"hunks":[{"oldStart":1,"oldLines":1,"newStart":1,"newLines":1,"lines":["-d","+e"]}],"content":null}]"#
    )
  );
  assert_eq!(recorded_change(&json(r#"{"type":"event_msg","payload":{"type":"token_count"}}"#)), None);
  assert_eq!(
    recorded_change(&json(
      r#"{"type":"response_item","payload":{"item":{"type":"FileChange","changes":{}}}}"#
    )),
    None
  );
}

#[test]
fn a_resumed_thread_is_read_from_where_it_stood_and_a_late_change_once_written() {
  let root = tempfile::tempdir().unwrap();
  let day = root.path().join("2026").join("09").join("14");
  let file = day.join("rollout-2026-09-14T22-59-04-t-resumed.jsonl");

  std::fs::create_dir_all(&day).unwrap();
  std::fs::write(&file, file_change(&format!(r#"{{"/r/old.ts":{UPDATE}}}"#))).unwrap();

  let mut changes = CodexChanges::new(root.path().to_owned());
  let append = |text: &str| {
    use std::io::Write;

    std::fs::OpenOptions::new()
      .append(true)
      .open(&file)
      .unwrap()
      .write_all(text.as_bytes())
      .unwrap();
  };

  changes.started("t-resumed");
  append(
    &(file_change(&format!(r#"{{"/r/skipped.ts":{UPDATE}}}"#))
      + &file_change(&format!(r#"{{"/r/a.ts":{UPDATE}}}"#))),
  );

  assert_eq!(changes.take(&["/r/old.ts".to_owned()]), None);
  assert!(changes.take(&["/r/a.ts".to_owned()]).is_some());

  // Half a line is not read until the rest of it is written.
  let line = file_change(&format!(r#"{{"/r/late.ts":{UPDATE}}}"#));

  append(&line[..40]);
  assert_eq!(changes.take(&["/r/late.ts".to_owned()]), None);
  append(&line[40..]);
  assert!(changes.take(&["/r/late.ts".to_owned()]).is_some());
}

#[test]
fn the_stream_s_items_are_its_words_its_thinking_its_calls_and_their_results() {
  let lines: Vec<Line> = [
    r#"{"type":"thread.started","thread_id":"t1"}"#,
    r#"{"type":"item.started","item":{"id":"i0","type":"command_execution","command":"ls","status":"in_progress"}}"#,
    r#"{"type":"item.completed","item":{"id":"i0","type":"command_execution","command":"ls","aggregated_output":"a\n","exit_code":0,"status":"completed"}}"#,
    r#"{"type":"item.completed","item":{"id":"i1","type":"reasoning","text":"réflexion"}}"#,
    r#"{"type":"item.started","item":{"id":"i2","type":"mcp_tool_call","server":"toolbook","tool":"call","arguments":{"tool":"web/web_fetch","arguments":{"url":"x"}}}}"#,
    r#"{"type":"item.completed","item":{"id":"i3","type":"file_change","changes":[{"path":"/r/a.ts","kind":"update"}],"status":"failed"}}"#,
    r#"{"type":"turn.failed","error":{"message":"quota"}}"#,
  ]
  .iter()
  .flat_map(|event| codex_lines(&json(event)))
  .collect();
  let seen: Vec<_> = lines
    .iter()
    .map(|line| (line.kind, line.name.as_deref(), line.text.as_str(), line.state))
    .collect();

  assert_eq!(
    seen,
    [
      ("note", None, "Codex thread t1", None),
      ("tool", Some("Bash"), r#"{"command":"ls"}"#, Some("running")),
      ("tool-result", Some("Bash"), "a\n", Some("succeeded")),
      ("thinking", None, "réflexion", None),
      ("tool", Some("web/web_fetch"), r#"{"url":"x"}"#, Some("running")),
      ("tool-result", Some("Edit"), r#"[{"path":"/r/a.ts","kind":"update"}]"#, Some("failed")),
      ("note", None, "quota", Some("failed")),
    ]
  );
  assert_eq!(
    stream_paths(lines[5].metadata.as_ref().unwrap().get("item").unwrap()),
    Some(vec!["/r/a.ts".to_owned()])
  );
}

#[test]
fn a_rollouts_response_items_are_the_turns_words_calls_and_answers_and_the_rest_is_none() {
  let records = [
    r#"{"type":"session_meta","payload":{"id":"thread-1","cwd":"/w"}}"#,
    r#"{"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"rules"}]}}"#,
    r#"{"type":"response_item","payload":{"type":"reasoning","summary":[{"type":"summary_text","text":"planning"}]}}"#,
    r#"{"type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"ls\"}","call_id":"call-1"}}"#,
    r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"call-1","output":"listed"}}"#,
    r#"{"type":"response_item","payload":{"type":"custom_tool_call","name":"exec","input":"run()","call_id":"call-2"}}"#,
    r#"{"type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"call-2","output":[{"type":"input_text","text":"ran"}]}}"#,
    r#"{"type":"event_msg","payload":{"type":"token_count","info":null}}"#,
    r#"{"type":"response_item","payload":{"type":"message","role":"assistant","id":"msg-1","content":[{"type":"output_text","text":"done"}]}}"#,
  ];
  let lines: Vec<Line> = records.iter().flat_map(|record| rollout_lines(&json(record))).collect();
  let seen: Vec<_> = lines
    .iter()
    .map(|line| {
      (
        line.kind,
        line.name.as_deref(),
        line.reference.as_deref(),
        line.text.as_str(),
        line.state,
      )
    })
    .collect();

  assert_eq!(
    seen,
    [
      ("thinking", None, None, "planning", None),
      ("tool", Some("exec_command"), Some("call-1"), r#"{"cmd":"ls"}"#, Some("requested")),
      ("tool-result", None, Some("call-1"), "listed", Some("unverified")),
      ("tool", Some("exec"), Some("call-2"), "run()", Some("requested")),
      ("tool-result", None, Some("call-2"), "ran", Some("unverified")),
      ("text", None, None, "done", None),
    ]
  );
  assert_eq!(lines[5].source_id.as_deref(), Some("msg-1"));
}
