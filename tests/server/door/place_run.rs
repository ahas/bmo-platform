use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use agency_server::door::place_run::*;
use agency_server::lines::place::{FAILURE_KIND, PLACE_VARIABLE};
use serde_json::Value;

fn place() -> tempfile::TempDir {
  tempfile::tempdir().expect("a temporary place")
}

#[test]
fn a_run_begun_is_ended_once_and_a_person_s_turn_is_never_marked_handed() {
  let dir = place();

  begin_run(
    dir.path(),
    "bmo-word-w-1",
    "the owner's word",
    10,
    BTreeMap::from([("BMO_RUN_NAME".into(), "r".into())]),
  )
  .unwrap();
  hand_run(dir.path(), true);
  assert!(handed_file(dir.path()).exists());
  hand_run(dir.path(), false);
  assert!(!handed_file(dir.path()).exists());
  end_run(dir.path(), 20).unwrap();
  end_run(dir.path(), 30).unwrap();

  let run = read_run(dir.path()).unwrap();

  assert_eq!((run.started_at, run.ended_at, run.person), (10, Some(20), false));
  assert_eq!(
    serde_json::to_value(&run).unwrap(),
    serde_json::json!({ "name": "bmo-word-w-1", "title": "the owner's word", "person": false, "startedAt": 10, "endedAt": 20, "env": { "BMO_RUN_NAME": "r" } })
  );

  std::fs::write(
    run_file(dir.path()),
    r#"{"name":null,"title":null,"person":true,"startedAt":1,"endedAt":null}"#,
  )
  .unwrap();
  hand_run(dir.path(), true);
  assert!(!handed_file(dir.path()).exists());
}

#[test]
fn a_failed_request_s_end_closes_the_run_and_is_the_latest_turn_s_until_a_prompt_or_a_stop_follows() {
  let dir = place();
  let failed = serde_json::json!({
    "hook_event_name": "StopFailure",
    "error": "server_error",
    "last_assistant_message": "API Error: 503 card busy",
  });

  begin_run(dir.path(), "chat-1", "a turn", 10, BTreeMap::new()).unwrap();
  record_call(dir.path(), "submit", &Value::Null, 11, false, None).unwrap();
  record_call(dir.path(), FAILURE_KIND, &failed, 12, false, None).unwrap();

  assert_eq!(read_run(dir.path()).unwrap().ended_at, Some(12));

  let failure = latest_failure(dir.path(), 10).expect("the turn's failure");

  assert_eq!(
    failure.failure(),
    ("API Error: 503 card busy".to_owned(), Some("server_error".to_owned()))
  );
  // Since a moment after it, there is none.
  assert!(latest_failure(dir.path(), 13).is_none());

  // A prompt after it is a turn the failure is not.
  record_call(dir.path(), "submit", &Value::Null, 14, false, None).unwrap();
  assert!(latest_failure(dir.path(), 10).is_none());

  // A failure naming nothing still says what it was.
  record_call(dir.path(), FAILURE_KIND, &Value::Null, 15, false, None).unwrap();
  assert_eq!(
    latest_failure(dir.path(), 10).unwrap().failure(),
    ("the request failed".to_owned(), None)
  );
  record_call(dir.path(), "stop", &Value::Null, 16, false, None).unwrap();
  assert!(latest_failure(dir.path(), 10).is_none());
}

#[test]
fn a_failure_ends_the_holds_count_as_a_stop_does() {
  let dir = place();

  begin_run(dir.path(), "job-1", "a job", 10, BTreeMap::new()).unwrap();
  record_call(dir.path(), "held", &Value::Null, 11, false, None).unwrap();
  record_call(dir.path(), "held", &Value::Null, 12, false, None).unwrap();
  assert_eq!(open_hold(dir.path(), STOP_HOLDS).unwrap().held, 2);

  // Kept open, as a round still owing a turn keeps it, so the holds are read again.
  record_call(dir.path(), FAILURE_KIND, &Value::Null, 13, true, None).unwrap();
  assert_eq!(read_run(dir.path()).unwrap().ended_at, None);
  assert_eq!(open_hold(dir.path(), STOP_HOLDS).unwrap().held, 0);
}

/// names the run going and when it started
#[test]
fn the_status_line_names_the_run_going_and_when_it_started_on_seouls_clock() {
  use policy::door_words::DOOR_WORDS;
  use policy::fill_marks;

  let started = chrono::NaiveDate::from_ymd_opt(2026, 9, 21)
    .and_then(|day| day.and_hms_opt(3, 22, 0))
    .map(|at| at.and_utc().timestamp_millis())
    .unwrap();
  let run = PlaceRun {
    name: Some("bmo-chat-2".into()),
    title: Some("room test".into()),
    person: false,
    started_at: started,
    ended_at: None,
    env: None,
  };

  assert_eq!(
    status_line(Some(&run)),
    fill_marks(
      &DOOR_WORDS.status_line.running,
      &[("name", "bmo-chat-2"), ("title", "room test"), ("at", "12:22")]
    )
  );
  assert_eq!(status_line(None), DOOR_WORDS.status_line.resting);
  assert_eq!(
    status_line(Some(&PlaceRun {
      ended_at: Some(started + 60_000),
      ..run.clone()
    })),
    fill_marks(&DOOR_WORDS.status_line.rested_after_run, &[("at", "12:23"), ("name", "bmo-chat-2")])
  );
  assert_eq!(
    status_line(Some(&PlaceRun {
      person: true,
      ..run
    })),
    fill_marks(&DOOR_WORDS.status_line.person, &[("at", "12:22")])
  );
}

#[test]
fn the_readings_around_a_turn_are_the_session_s_own_and_never_the_one_stamped_at_its_end() {
  let reading = |at: i64, session: &str, total: f64| StatusRecord {
    at,
    payload: serde_json::json!({ "session_id": session, "cost": { "total_cost_usd": total } }),
    login: None,
  };
  let records = [
    reading(5, "s", 1.0),
    reading(6, "other", 9.0),
    reading(20, "s", 2.0),
    reading(21, "s", 3.0),
  ];
  let newest_first = || records.iter().rev().cloned();
  let around = status_around(newest_first(), 10, 20, Some("s"));

  assert_eq!(around.before, Some(records[0].payload.clone()));
  assert_eq!(around.after, Some(records[3].payload.clone()));
  assert_eq!(status_around(newest_first(), 10, 20, None).before, Some(records[1].payload.clone()));
  assert_eq!(status_around(newest_first(), 4, 30, Some("s")), StatusAround::default());
}

#[test]
fn a_place_s_lines_that_do_not_parse_are_passed_over() {
  let dir = place();

  std::fs::write(
    dir.path().join("hooks.jsonl"),
    "{\"at\":1,\"kind\":\"stop\",\"payload\":{}}\nnot json\n\n{\"at\":2,\"kind\":\"submit\"}\n",
  )
  .unwrap();

  let hooks = read_hooks(dir.path(), 0);

  assert_eq!(
    hooks.iter().map(|hook| (hook.at, hook.kind.as_str())).collect::<Vec<_>>(),
    [(1, "stop"), (2, "submit")]
  );
}

#[test]
fn a_session_s_lines_are_taken_out_of_both_records_and_every_line_where_no_session_is_named() {
  let dir = place();
  let call = |session: &str, words: &str| serde_json::json!({ "at": 1, "payload": { "session_id": session, "prompt": words } });
  let other = [call("other", "kept"), serde_json::json!({ "at": 2, "payload": {} })];

  for file in [status_file(dir.path()), hooks_file(dir.path())] {
    write_lines(
      &file,
      &[call("taken", "taken words"), other[0].clone(), call("taken", "more"), other[1].clone()],
    );
    append_text(&file, "not json\n");
  }

  forget_records(dir.path(), Some("taken")).unwrap();

  let kept = format!("{}\n{}\nnot json\n", other[0], other[1]);

  assert_eq!(std::fs::read_to_string(status_file(dir.path())).unwrap(), kept);
  assert_eq!(std::fs::read_to_string(hooks_file(dir.path())).unwrap(), kept);
  assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);

  forget_records(dir.path(), None).unwrap();

  assert_eq!(std::fs::read_to_string(status_file(dir.path())).unwrap(), "");
  assert_eq!(std::fs::read_to_string(hooks_file(dir.path())).unwrap(), "");

  // A place that never kept a record has nothing to take.
  let empty = place();

  forget_records(empty.path(), Some("taken")).unwrap();
  forget_records(empty.path(), None).unwrap();
  assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}

fn append_text(path: &Path, text: &str) {
  use std::io::Write as _;

  std::fs::OpenOptions::new()
    .append(true)
    .open(path)
    .unwrap()
    .write_all(text.as_bytes())
    .unwrap();
}

fn write_lines(path: &Path, lines: &[Value]) {
  let text: String = lines.iter().map(|line| format!("{line}\n")).collect();

  std::fs::write(path, text).unwrap();
}

fn append_lines(path: &Path, lines: &[Value]) {
  for line in lines {
    append(path, line).unwrap();
  }
}

#[test]
fn a_reading_since_a_moment_goes_back_a_crossing_s_while_and_no_further() {
  let dir = place();
  let since = 1_000_000;
  let hook = |at: i64, kind: &str| serde_json::json!({ "at": at, "kind": kind, "payload": {} });

  write_lines(
    &dir.path().join("hooks.jsonl"),
    &[
      // Stamped after the moment, but behind a line older than any crossing: never reached.
      hook(since + 500, "stop"),
      hook(since - CROSSED_MS - 1, "submit"),
      hook(since + 10, "submit"),
      // Crossed with the line before it: passed over without ending the reading.
      hook(since - 10, "held"),
      hook(since + 20, "stop"),
    ],
  );

  assert_eq!(
    read_hooks(dir.path(), since)
      .iter()
      .map(|hook| (hook.at, hook.kind.as_str()))
      .collect::<Vec<_>>(),
    [(since + 10, "submit"), (since + 20, "stop")]
  );
}

#[tokio::test(start_paused = true)]
async fn the_reading_before_a_turn_is_found_however_far_back_and_the_one_after_as_it_arrives() {
  let dir = place();
  let path = status_file(dir.path());
  let (started_at, ended_at) = (10_000_000, 10_000_100);
  let reading = |at: i64, session: &str, total: i64| serde_json::json!({ "at": at, "payload": { "session_id": session, "cost": { "total_cost_usd": total } } });

  write_lines(
    &path,
    &[reading(1_000, "s", 1), reading(2_000, "other", 9), reading(started_at + 50, "s", 2)],
  );

  let arriving = {
    let path = path.clone();
    let mut after = reading(ended_at + 900, "s", 3);

    after["login"] = serde_json::json!({ "configDir": "/home/tester/.claude", "token": null });

    tokio::spawn(async move {
      tokio::time::sleep(Duration::from_secs(2)).await;
      append_lines(&path, &[reading(ended_at + 800, "other", 8), after]);
    })
  };
  let around = status_after_turn(
    dir.path(),
    started_at,
    ended_at,
    Some("s"),
    Duration::from_secs(1),
    Duration::from_secs(10),
    || true,
    || std::future::ready(false),
  )
  .await
  .expect("no stop stood over the run");

  arriving.await.unwrap();

  assert_eq!(around.before.map(|before| before["cost"]["total_cost_usd"].clone()), Some(1.into()));
  assert_eq!(around.after.map(|after| after["cost"]["total_cost_usd"].clone()), Some(3.into()));
  assert_eq!(around.login.map(|login| login.config_dir), Some("/home/tester/.claude".to_owned()));
}

// A program standing on a node answers its status line through the home bridge, and a call made while the node's
// link was being made again never reaches the place: the turn the record closed is not held on it.
#[tokio::test(start_paused = true)]
async fn a_reading_that_never_follows_a_turn_s_end_is_given_up_at_the_bound_with_the_program_alive() {
  let dir = place();
  let (started_at, ended_at) = (10_000_000, 10_000_100);
  let reading = |at: i64, total: i64| serde_json::json!({ "at": at, "payload": { "session_id": "s", "cost": { "total_cost_usd": total } } });

  write_lines(&status_file(dir.path()), &[reading(started_at - 10, 1), reading(started_at + 50, 2)]);

  let began = tokio::time::Instant::now();
  let around = status_after_turn(
    dir.path(),
    started_at,
    ended_at,
    Some("s"),
    Duration::from_secs(1),
    Duration::from_secs(10),
    || true,
    || std::future::ready(false),
  )
  .await
  .expect("no stop stood over the run");

  assert_eq!(around.before.map(|before| before["cost"]["total_cost_usd"].clone()), Some(1.into()));
  assert_eq!(around.after, None, "no reading followed the turn's end");
  assert_eq!(
    began.elapsed(),
    Duration::from_secs(10),
    "the wait ends at its bound, not before and not later"
  );
}

#[tokio::test(start_paused = true)]
async fn a_stop_ends_the_wait_for_the_reading_after_a_turn_whatever_the_program_does() {
  let dir = place();
  let stopped = std::sync::atomic::AtomicBool::new(false);
  let began = tokio::time::Instant::now();
  let waited = status_after_turn(
    dir.path(),
    10_000_000,
    10_000_100,
    Some("s"),
    Duration::from_secs(1),
    Duration::from_secs(600),
    || true,
    || {
      // The stop stands from the third look on.
      let now = began.elapsed() >= Duration::from_secs(2);

      stopped.store(now, std::sync::atomic::Ordering::SeqCst);
      std::future::ready(now)
    },
  )
  .await;

  assert_eq!(waited, None, "a stop ends the wait and the turn ends stopped");
  assert!(stopped.load(std::sync::atomic::Ordering::SeqCst));
  assert!(began.elapsed() < Duration::from_secs(4), "{:?}", began.elapsed());
}

#[test]
fn the_run_s_variables_go_into_the_place_and_the_program_s_environment_keeps_none_of_them() {
  let env = BTreeMap::from([
    ("PATH".to_owned(), "/bin".to_owned()),
    ("BMO_RUN_NAME".to_owned(), "r".to_owned()),
    ("BMO_TURN_ROOM".to_owned(), "1".to_owned()),
  ]);

  assert_eq!(run_variables(&env).keys().collect::<Vec<_>>(), ["BMO_RUN_NAME", "BMO_TURN_ROOM"]);
  assert_eq!(
    resident_environment(&env, Path::new("/p")),
    BTreeMap::from([("PATH".to_owned(), "/bin".to_owned()), (PLACE_VARIABLE.to_owned(), "/p".to_owned())])
  );
}

fn ended_record() -> Vec<Value> {
  vec![
    serde_json::json!({ "type": "user", "message": { "content": "the round" } }),
    serde_json::json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": "the answer" }] } }),
    serde_json::json!({ "type": "system", "subtype": "turn_duration" }),
  ]
}

/// the foreground rests, whatever the screen reads, once the place's last call bounding a turn is a stop stamped
/// since the moment asked and the record ends on a turn's end; a submit after it, or a stop before that moment,
/// is not rest
#[test]
fn the_foreground_rests_on_a_stop_since_the_prompt_and_a_record_ended() {
  let dir = place();
  let running =
    serde_json::json!({ "background_tasks": [{ "id": "a60", "type": "subagent", "status": "running" }] });

  record_call(dir.path(), "submit", &Value::Null, 10, false, None).unwrap();
  assert!(!foreground_rests(dir.path(), &ended_record(), 0), "a turn going");

  record_call(dir.path(), "stop", &running, 20, false, None).unwrap();
  assert!(foreground_rests(dir.path(), &ended_record(), 15));
  assert!(!foreground_rests(dir.path(), &ended_record(), 25), "a stop before the prompt");
  assert!(!foreground_rests(dir.path(), &ended_record()[..2], 15), "a record still writing");

  record_call(dir.path(), "submit", &Value::Null, 30, false, None).unwrap();
  assert!(!foreground_rests(dir.path(), &ended_record(), 15), "a turn going again");
}

/// the tasks a stop ends are those the last stop named running that nothing in the background record ended since,
/// and once written stopped they are no longer among them
#[test]
fn a_stop_ends_what_the_last_stop_named_running_and_nothing_since_ended() {
  let dir = place();
  let stop = serde_json::json!({ "background_tasks": [
    { "id": "a60", "type": "subagent", "status": "running", "description": "split" },
    { "id": "b1", "type": "shell", "status": "running", "description": "find" },
    { "id": "b2", "type": "shell", "status": "completed", "description": "done" },
  ] });

  record_call(dir.path(), "stop", &stop, 10, false, None).unwrap();

  let ids = |tasks: Vec<RunningTask>| tasks.into_iter().map(|task| task.id).collect::<Vec<_>>();

  assert_eq!(ids(last_stop_tasks(dir.path())), ["a60", "b1"]);

  write_stopped(dir.path(), &["b1".to_owned()], 11).unwrap();
  assert_eq!(ids(last_stop_tasks(dir.path())), ["a60"]);
}
