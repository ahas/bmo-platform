use agency_server::door::inbox::*;
use agency_server::door::surface::Run;
use serde_json::{Value, json};

fn run() -> Run {
  serde_json::from_value(json!({ "line": "worker", "model": "free/model", "prompt": "", "cwd": "/w" }))
    .unwrap()
}

#[test]
fn a_turn_is_its_lines_from_its_start_to_the_next_turns() {
  let records = vec![
    json!({ "kind": "start", "id": "a" }),
    json!({ "kind": "answer", "text": "first" }),
    json!({ "kind": "end" }),
    json!({ "kind": "start", "id": "b" }),
    json!({ "kind": "answer", "text": "second" }),
  ];

  assert_eq!(lines_of(&records, "a").map(<[Value]>::len), Some(3));
  assert_eq!(lines_of(&records, "b").map(<[Value]>::len), Some(2));
  assert_eq!(lines_of(&records, "c"), None);
}

#[test]
fn an_answer_carries_dsh_tokens_and_a_failure_carries_no_answer() {
  let (state, error, reply) = read_turn(
    &[
      json!({ "kind": "start", "id": "a" }),
      json!({ "kind": "answer", "text": " done \n", "model": "free/other", "usage": { "inputTokens": 30, "outputTokens": 4 } }),
      json!({ "kind": "end" }),
    ],
    &run(),
    "openrouter",
  );

  assert_eq!(
    (state, error, reply.text.as_str(), reply.model.as_deref()),
    (State::Ended, None, "done", Some("free/other"))
  );
  assert_eq!(reply.spend.iter().map(|one| one.tokens).collect::<Vec<_>>(), vec![34]);
  assert_eq!(reply.spend[0].cache_read_tokens, 0);

  // The share the cache gave counts toward the turn's tokens beside the input and output.
  let (_, _, reply) = read_turn(
    &[
      json!({ "kind": "start", "id": "a" }),
      json!({ "kind": "answer", "text": "done", "usage": { "inputTokens": 30, "outputTokens": 4, "cacheReadTokens": 70 } }),
      json!({ "kind": "end" }),
    ],
    &run(),
    "openrouter",
  );

  assert_eq!(
    reply
      .spend
      .iter()
      .map(|one| (one.tokens, one.input_tokens, one.cache_read_tokens))
      .collect::<Vec<_>>(),
    vec![(104, 30, 70)]
  );

  let (state, error, reply) = read_turn(
    &[
      json!({ "kind": "start", "id": "a" }),
      json!({ "kind": "answer", "text": "partial" }),
      json!({ "kind": "failed" }),
    ],
    &run(),
    "openrouter",
  );

  assert_eq!((state, error.as_deref(), reply.text.as_str()), (State::Ended, Some("dsh failed"), ""));
  assert!(reply.spend.is_empty());
}

#[test]
fn a_models_choice_in_the_record_is_read_back_as_the_models_id() {
  let (_, _, reply) = read_turn(
    &[
      json!({ "kind": "start", "id": "a" }),
      json!({ "kind": "answer", "text": "done", "model": r#"["openrouter","vendor/model"]"# }),
      json!({ "kind": "end" }),
    ],
    &run(),
    "openrouter",
  );

  assert_eq!(reply.model.as_deref(), Some("vendor/model"));
}

#[test]
fn a_turn_cut_short_reads_interrupted_and_one_with_no_end_still_works() {
  let interrupted =
    read_turn(&[json!({ "kind": "start" }), json!({ "kind": "interrupted" })], &run(), "openrouter");
  let working = read_turn(
    &[json!({ "kind": "start" }), json!({ "kind": "answer", "text": "x" })],
    &run(),
    "openrouter",
  );

  assert_eq!((interrupted.0, working.0), (State::Interrupted, State::Working));
}

#[test]
fn a_turns_lines_are_handed_as_written_while_it_works_and_whole_once_it_ends() {
  let dir = tempfile::tempdir().unwrap();
  let step = r#"{"kind":"step","id":"b","type":"assistant","message":{"content":[]}}"#;
  let write = |text: &str| std::fs::write(record_path(dir.path()), text).unwrap();

  write(&format!(
    "{{\"kind\":\"start\",\"id\":\"a\"}}\n{{\"kind\":\"end\",\"id\":\"a\"}}\n{{\"kind\":\"start\",\"id\":\"b\"}}\n{step}\n{{\"kind\":\"an"
  ));

  let (written, records) = record_of(dir.path());
  let span = span_of(&records, "b").unwrap();

  assert_eq!(written[span.clone()], [r#"{"kind":"start","id":"b"}"#, step]);
  assert_eq!(read_turn(&records[span], &run(), "openrouter").0, State::Working);

  write(&format!(
    "{{\"kind\":\"start\",\"id\":\"a\"}}\n{{\"kind\":\"end\",\"id\":\"a\"}}\n{{\"kind\":\"start\",\"id\":\"b\"}}\n{step}\n{{\"kind\":\"answer\",\"text\":\"done\"}}\n{{\"kind\":\"end\"}}\n"
  ));

  let (written, records) = record_of(dir.path());
  let span = span_of(&records, "b").unwrap();

  assert_eq!(written[span.clone()].len(), 4);
  assert_eq!(written[span.clone()][1], step);
  assert_eq!(read_turn(&records[span], &run(), "openrouter").2.text, "done");
}

#[test]
fn only_a_turns_own_variables_travel_with_it() {
  assert!(is_inbox_turn_variable(LOCAL_URL));
  assert!(is_inbox_turn_variable("BMO_TURN_CHAT"));
  assert!(!is_inbox_turn_variable("OPENROUTER_API_KEY"));
  assert!(!is_inbox_turn_variable("PATH"));
}

#[test]
fn the_context_is_the_last_answer_s_against_the_latest_window_named() {
  let place = tempfile::tempdir().unwrap();
  let write = |lines: &[Value]| {
    std::fs::write(
      record_path(place.path()),
      lines.iter().map(|line| format!("{line}\n")).collect::<String>(),
    )
    .unwrap()
  };
  let several = json!({ "kind": "answer", "id": "a", "text": "a", "context": { "used": 400, "size": 1000 },
                        "usage": { "inputTokens": 50, "cacheReadTokens": 1150, "outputTokens": 9 } });
  let one = json!({ "kind": "answer", "id": "b", "text": "b", "context": null,
                    "usage": { "inputTokens": 30, "cacheReadTokens": 400, "outputTokens": 20 } });

  assert_eq!(latest_context(place.path()), None, "a place with no record holds none");

  write(std::slice::from_ref(&several));
  assert_eq!(
    latest_context(place.path()),
    Some((400, 1000)),
    "the context a turn of several requests names"
  );

  write(&[several, one.clone()]);
  assert_eq!(
    latest_context(place.path()),
    Some((450, 1000)),
    "one request's tokens where it names none"
  );

  write(&[one]);
  assert_eq!(latest_context(place.path()), None, "no window named");
}
