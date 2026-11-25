//! What a session's turn has written so far (docs/api.md, 「Sessions」, the output read): the text blocks
//! its answers wrote in the record, in order; read while the turn goes and after it was stopped, begun
//! again by an attempt that goes out again and by the session's next turn; held under the turn's id, so
//! turns of one session are each kept apart; and nothing found for a turn a session never carried.

use std::collections::HashMap;
use std::sync::Arc;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::herdr_backend::HerdrSessions;
use agency_server::job_run::claude::turn::assistant_texts;
use agency_server::places::{Ground, Places};
use agency_server::seeds::deployment;
use agency_server::session_output::Outputs;
use agent_session::{Apart, FaultKind, Output, Scope, SessionId, Sessions, Start};
use harness_providers::Machine;
use serde_json::json;

use crate::support::scratch::Scratch;

fn said(texts: &[&str]) -> String {
  json!({
    "type": "assistant",
    "message": { "content": texts.iter().map(|text| json!({ "type": "text", "text": text })).collect::<Vec<_>>() },
  })
  .to_string()
}

#[test]
fn what_a_turn_wrote_is_every_text_block_its_answers_wrote_in_order() {
  let lines = [
    said(&["TITLE: the angle"]),
    json!({ "type": "assistant", "message": { "content": [{ "type": "tool_use", "name": "Bash" }] } })
      .to_string(),
    json!({ "type": "user", "message": { "content": [{ "type": "text", "text": "a tool's result" }] } })
      .to_string(),
    "not a record line".to_owned(),
    said(&["first finding", "second finding"]),
  ];

  assert_eq!(assistant_texts(&lines), ["TITLE: the angle", "first finding", "second finding"]);
}

fn session() -> SessionId {
  SessionId("branch-a@run:1".into())
}

#[test]
fn a_running_turns_words_are_read_as_they_are_written_and_stand_once_it_is_stopped() {
  let outputs = Outputs::default();

  assert_eq!(
    outputs.read(&session(), Some(7)),
    None,
    "a turn the session never carried has nothing to read"
  );

  outputs.begun(&session(), 7, "bmo-research-7");
  outputs.wrote(&session(), 7, &["TITLE: the angle".into()]);
  outputs.wrote(&session(), 7, &["first finding".into()]);

  assert_eq!(
    outputs.read(&session(), Some(7)),
    Some(Output {
      turn: 7,
      run: "bmo-research-7".into(),
      running: true,
      text: "TITLE: the angle\n\nfirst finding".into(),
      waiting: None,
    })
  );
  assert_eq!(
    outputs.read(&session(), None).unwrap().turn,
    7,
    "the session's latest turn is read where none is named"
  );

  outputs.ended(&session(), 7);

  assert_eq!(
    outputs.read(&session(), Some(7)),
    Some(Output {
      turn: 7,
      run: "bmo-research-7".into(),
      running: false,
      text: "TITLE: the angle\n\nfirst finding".into(),
      waiting: None,
    }),
    "a turn stopped from outside leaves what it wrote"
  );

  outputs.forget(&session());

  assert_eq!(outputs.read(&session(), Some(7)), None, "an ended session takes its words with it");
}

#[test]
fn an_attempt_that_goes_out_again_and_the_next_turn_begin_the_words_again() {
  let outputs = Outputs::default();

  outputs.begun(&session(), 7, "run-1");
  outputs.wrote(&session(), 7, &["on the first login".into()]);
  outputs.attempted(&session(), 7);
  outputs.wrote(&session(), 7, &["on the next one".into()]);

  assert_eq!(outputs.read(&session(), Some(7)).unwrap().text, "on the next one");

  outputs.ended(&session(), 7);
  outputs.begun(&session(), 8, "run-2");

  assert_eq!(
    outputs.read(&session(), Some(8)),
    Some(Output {
      turn: 8,
      run: "run-2".into(),
      running: true,
      text: String::new(),
      waiting: None,
    })
  );
  assert_eq!(
    outputs.read(&session(), Some(7)),
    None,
    "the turn before stands only until the session's next turn begins"
  );
  outputs.wrote(&SessionId("branch-b@run:2".into()), 8, &["elsewhere".into()]);
  assert_eq!(
    outputs.read(&session(), Some(8)).unwrap().text,
    "",
    "words are their own session's alone"
  );
}

#[test]
fn the_turns_of_one_session_are_each_kept_under_their_id() {
  let outputs = Outputs::default();
  let session = SessionId("worker-one@worker".into());

  outputs.begun(&session, 1, "run-a");
  outputs.begun(&session, 2, "run-b");
  outputs.wrote(&session, 1, &["a's words".into()]);
  outputs.wrote(&session, 2, &["b's words".into()]);

  assert_eq!(outputs.read(&session, Some(1)).unwrap().text, "a's words");
  assert_eq!(outputs.read(&session, Some(2)).unwrap().text, "b's words");

  outputs.ended(&session, 1);
  outputs.begun(&session, 3, "run-c");

  assert_eq!(outputs.read(&session, Some(1)), None, "an ended turn goes when another begins");
  assert_eq!(outputs.read(&session, Some(2)).unwrap().text, "b's words", "a turn still going stays");
}

#[tokio::test]
async fn every_scope_answers_a_turn_it_never_carried_as_nothing_found() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let at = |path: std::path::PathBuf| path.to_string_lossy().into_owned();
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), at(home.path().to_path_buf())),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
    ],
    home.path().join("data"),
  );

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let sessions = HerdrSessions::new(Places::new(Ground::new(&service, &machine, None), Arc::default()));
  let apart = sessions
    .start(Start {
      agent: "branch-a".into(),
      scope: Scope::Apart(Apart {
        provider: "claude-code".into(),
        model: "claude-sonnet-5".into(),
        cwd: "/tmp/run".into(),
        todo: None,
        role: None,
        ..Apart::default()
      }),
    })
    .await
    .unwrap();

  assert_eq!(sessions.output(&apart.id, Some(7)).await.unwrap_err().kind, FaultKind::UnknownTurn);
  assert_eq!(sessions.output(&apart.id, None).await.unwrap_err().kind, FaultKind::Unknown);
  assert_eq!(
    sessions.output(&SessionId("branch-a@run:99".into()), None).await.unwrap_err().kind,
    FaultKind::Unknown,
    "a session never started names nothing"
  );
}
