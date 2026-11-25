//! A pane program's tool calls (docs/run-records.md, 「Agent log」): the progress lines the terminal client writes as dsh
//! reports a call over the Agent Client Protocol — asked, under way, settled — filed as a `tool` line under
//! the tool's name carrying its argument summary, and the `tool-result` that answers it, how it ended, how
//! long it ran and how large its result was. A line that names its call by `toolCallId` is matched to that
//! call; an older record that names none is matched first in, first out under the turn's id and the call's
//! place in it.

use agency_server::run_log::json::Json;
use agency_server::run_log::pane_calls::PaneCalls;
use serde_json::json;

fn progress(at: i64, fields: serde_json::Value) -> Json {
  let mut line = json!({ "kind": "progress", "id": "turn-7", "at": at });

  line.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
  Json::parse(&line.to_string()).unwrap()
}

#[test]
fn calls_named_by_their_id_carry_their_input_and_result_size_and_are_matched_by_it() {
  let mut calls = PaneCalls::default();
  let stream = [
    progress(1_000, json!({ "phase": "thinking", "tool": null })),
    progress(
      1_100,
      json!({ "phase": "tool", "tool": "bash", "toolCallId": "a", "state": "in_progress",
              "input": "{\"command\":\"cargo check\"}", "inputCut": false }),
    ),
    // The read is asked after bash, so it settles second though its result arrives first.
    progress(
      1_120,
      json!({ "phase": "tool", "tool": "read", "toolCallId": "b", "state": "in_progress",
              "input": "{\"path\":\"src/lib.rs…", "inputCut": true }),
    ),
    progress(
      1_350,
      json!({ "phase": "tool", "toolCallId": "b", "state": "completed", "resultBytes": 2048 }),
    ),
    progress(
      1_700,
      json!({ "phase": "tool", "toolCallId": "a", "state": "failed", "resultBytes": 120 }),
    ),
    progress(1_900, json!({ "phase": "writing", "tool": null })),
  ];
  let read: Vec<_> = stream
    .iter()
    .flat_map(|event| calls.lines(event))
    .map(|line| (line.kind, line.name, line.reference, line.state, line.text, line.upstream_truncated))
    .collect();

  assert_eq!(
    read,
    [
      (
        "tool",
        Some("bash".to_owned()),
        Some("a".to_owned()),
        Some("requested"),
        "{\"command\":\"cargo check\"}".to_owned(),
        false
      ),
      (
        "tool",
        Some("read".to_owned()),
        Some("b".to_owned()),
        Some("requested"),
        "{\"path\":\"src/lib.rs…".to_owned(),
        true
      ),
      (
        "tool-result",
        None,
        Some("b".to_owned()),
        Some("succeeded"),
        "completed in 230 ms, 2048 bytes".to_owned(),
        false
      ),
      (
        "tool-result",
        None,
        Some("a".to_owned()),
        Some("failed"),
        "failed in 600 ms, 120 bytes".to_owned(),
        false
      ),
    ]
  );
  assert_eq!(calls.made(), 2);
}

#[test]
fn an_older_record_with_no_ids_is_matched_first_in_first_out() {
  let mut calls = PaneCalls::default();
  let stream = [
    progress(1_100, json!({ "phase": "tool", "tool": "bash", "state": "pending" })),
    progress(1_120, json!({ "phase": "tool", "tool": "bash", "state": "in_progress" })),
    progress(1_350, json!({ "phase": "tool", "tool": "tool", "state": "completed" })),
    progress(
      1_500,
      json!({ "phase": "tool", "tool": "mcp__bmo__memory_list", "state": "in_progress" }),
    ),
    progress(1_510, json!({ "phase": "tool", "tool": "read", "state": "in_progress" })),
    progress(1_700, json!({ "phase": "tool", "tool": "tool", "state": "failed" })),
    progress(1_900, json!({ "phase": "tool", "tool": "tool", "state": "completed" })),
  ];
  let read: Vec<_> = stream
    .iter()
    .flat_map(|event| calls.lines(event))
    .map(|line| (line.kind, line.name, line.reference, line.state, line.text))
    .collect();

  assert_eq!(
    read,
    [
      (
        "tool",
        Some("bash".to_owned()),
        Some("turn-7:1".to_owned()),
        Some("requested"),
        String::new()
      ),
      (
        "tool-result",
        None,
        Some("turn-7:1".to_owned()),
        Some("succeeded"),
        "completed in 250 ms".to_owned()
      ),
      (
        "tool",
        Some("mcp__bmo__memory_list".to_owned()),
        Some("turn-7:2".to_owned()),
        Some("requested"),
        String::new()
      ),
      (
        "tool",
        Some("read".to_owned()),
        Some("turn-7:3".to_owned()),
        Some("requested"),
        String::new()
      ),
      (
        "tool-result",
        None,
        Some("turn-7:2".to_owned()),
        Some("failed"),
        "failed in 200 ms".to_owned()
      ),
      (
        "tool-result",
        None,
        Some("turn-7:3".to_owned()),
        Some("succeeded"),
        "completed in 390 ms".to_owned()
      ),
    ]
  );
  assert_eq!(calls.made(), 3);
}

#[test]
fn a_settling_shell_call_keeps_its_changed_files_in_the_tool_use_result() {
  let mut calls = PaneCalls::default();
  let diff = json!({
    "files": [{ "filePath": "/work/notes.txt", "hunks": [{ "oldStart": 1, "oldLines": 1,
      "newStart": 1, "newLines": 1, "lines": ["-one", "+two"] }] }],
    "moreFiles": 0,
    "changedFiles": ["/work/notes.txt"],
  });
  calls.lines(&progress(
    1_100,
    json!({ "phase": "tool", "tool": "bash", "toolCallId": "a", "state": "in_progress",
            "input": "{\"command\":\"printf two > notes.txt\"}", "inputCut": false }),
  ));
  let lines = calls.lines(&progress(
    1_200,
    json!({ "phase": "tool", "toolCallId": "a", "state": "completed", "bashEditDiff": diff }),
  ));
  let result = lines.iter().find(|line| line.kind == "tool-result").unwrap();
  let metadata = result.metadata.as_ref().expect("the result carries its metadata");
  let diff = metadata
    .get("toolUseResult")
    .and_then(|result| result.get("bashEditDiff"))
    .expect("the diff stands where Claude Code puts it");

  assert_eq!(diff.get("files").unwrap().as_array().unwrap().len(), 1);
  assert_eq!(diff.get("moreFiles").unwrap(), &Json::parse("0").unwrap());
  assert_eq!(
    diff.get("changedFiles").unwrap().as_array().unwrap()[0].as_str(),
    Some("/work/notes.txt")
  );
}

#[test]
fn a_settling_with_no_call_open_and_an_unnamed_start_file_nothing() {
  let mut calls = PaneCalls::default();

  assert!(
    calls
      .lines(&progress(1, json!({ "phase": "tool", "tool": "tool", "state": "completed" })))
      .is_empty()
  );
  assert!(
    calls
      .lines(&progress(2, json!({ "phase": "tool", "tool": "tool", "state": "in_progress" })))
      .is_empty()
  );
  assert_eq!(calls.made(), 0);
}
