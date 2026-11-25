//! One run of a supervisor's turn: the lines its record leaves are labelled with the provider it went out on.

use std::time::Instant;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::door::turn::Watch;
use agency_server::places::Ground;
use agency_server::run_log::{RunLog, Target};
use agency_server::session_output::{OutputWatch, Outputs};
use agency_server::supervisor_turn::run::Watched;
use agent_session::SessionId;
use harness_providers::Machine;
use sea_orm::DatabaseConnection;
use serde_json::json;

// A supervisor turn may stand any provider's program, so the lines its record leaves are labelled with the
// provider the turn went out on rather than one fixed program's.
#[tokio::test]
async fn the_lines_a_turn_leaves_are_labelled_with_its_own_provider() {
  let service = Service::new(DatabaseConnection::Disconnected, Config::read(|_| None).unwrap());
  let machine = Machine::with_env([("HOME".to_owned(), "/home/tester".to_owned())], "/data");
  let ground = Ground::new(&service, &machine, None);
  let mut log = RunLog::new(Target {
    agent: "sample-agent".to_owned(),
    run: "chat:1:1".to_owned(),
    chat_id: Some(1),
    job_id: None,
  });
  let mut watched = Watched {
    ground: &ground,
    log: &mut log,
    source: "codex",
    run: "chat:1:1".to_owned(),
    guard: None,
    tally: None,
    started: Instant::now(),
    last_tally: Instant::now(),
    cut: None,
    shown: None,
  };
  let rollout = json!({ "type": "response_item", "payload": { "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": "done" }] } });
  let written = watched.feed(&[rollout.to_string()]).await;

  assert_eq!(
    written.iter().map(|line| line.source.as_deref()).collect::<Vec<_>>(),
    vec![Some("codex")]
  );
}

// A supervisor turn's answer is readable from its session while it is written, whichever road the line
// goes out on.
#[tokio::test]
async fn the_text_a_turn_writes_is_read_from_its_session_while_it_runs() {
  let service = Service::new(DatabaseConnection::Disconnected, Config::read(|_| None).unwrap());
  let machine = Machine::with_env([("HOME".to_owned(), "/home/tester".to_owned())], "/data");
  let ground = Ground::new(&service, &machine, None);
  let mut log = RunLog::new(Target {
    agent: "sample-agent".to_owned(),
    run: "chat:1:1".to_owned(),
    chat_id: Some(1),
    job_id: None,
  });
  let session = SessionId("sample-agent@1".to_owned());
  let outputs = Outputs::default();

  outputs.begun(&session, 5, "chat:1:1");

  let mut watched = Watched {
    ground: &ground,
    log: &mut log,
    source: "codex",
    run: "chat:1:1".to_owned(),
    guard: None,
    tally: None,
    started: Instant::now(),
    last_tally: Instant::now(),
    cut: None,
    shown: Some(OutputWatch {
      outputs: &outputs,
      session: &session,
      turn: 5,
    }),
  };
  let first = json!({ "type": "response_item", "payload": { "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": "first" }] } });
  let second =
    json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": "second" }] } });

  watched.record(&[first.to_string()]).await;
  watched.record(&[second.to_string()]).await;

  let read = outputs.read(&session, Some(5)).unwrap();

  assert!(read.running);
  assert_eq!(read.text, "first\n\nsecond");
}

// A pane program's record is its own shape: what the turn is doing as it goes and the answer it ended on
// are filed as the lines they are, so the room reading the run sees its progress and its spend, and each
// call it says began and settled beside them as the call's own lines.
#[tokio::test]
async fn a_pane_program_s_progress_and_answer_are_filed_as_their_lines() {
  let service = Service::new(DatabaseConnection::Disconnected, Config::read(|_| None).unwrap());
  let machine = Machine::with_env([("HOME".to_owned(), "/home/tester".to_owned())], "/data");
  let ground = Ground::new(&service, &machine, None);
  let mut log = RunLog::new(Target {
    agent: "sample-agent".to_owned(),
    run: "chat:1:1".to_owned(),
    chat_id: Some(1),
    job_id: None,
  });
  let mut watched = Watched {
    ground: &ground,
    log: &mut log,
    source: "residential",
    run: "chat:1:1".to_owned(),
    guard: None,
    tally: None,
    started: Instant::now(),
    last_tally: Instant::now(),
    cut: None,
    shown: None,
  };
  let progress = json!({ "kind": "progress", "id": "turn-1", "at": 1, "phase": "tool", "tool": "bash", "state": "in_progress" });
  let settled = json!({ "kind": "progress", "id": "turn-1", "at": 4, "phase": "tool", "tool": "tool", "state": "completed" });
  let answer =
    json!({ "kind": "answer", "id": "turn-1", "at": 5, "text": "done", "usage": { "inputTokens": 10 } });
  let written = watched.feed(&[progress.to_string(), settled.to_string(), answer.to_string()]).await;

  assert_eq!(
    written.iter().map(|line| line.kind).collect::<Vec<_>>(),
    vec!["progress", "tool", "progress", "tool-result", "answer"]
  );
  assert!(written[0].text.contains("\"phase\":\"tool\""));
  assert_eq!(
    (written[1].name.as_deref(), written[1].reference.as_deref()),
    (Some("bash"), Some("turn-1:1"))
  );
  assert_eq!(
    (written[3].reference.as_deref(), written[3].state),
    (Some("turn-1:1"), Some("succeeded"))
  );
  assert!(written[4].text.contains("\"inputTokens\":10"));
}
