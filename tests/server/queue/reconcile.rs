//! Execution rows closed on evidence: an outcome its host published, an ending its collector published, a
//! parent whose pane it shared ending, a supervisor and its group gone, a resident turn's hooks or program,
//! or nothing placed a day after it began — and never on an absence a launch still under way could explain.

use std::path::Path;

use agency_server::door::execution::ENVELOPE;
use agency_server::queue::reconcile::*;
use agent_host::identity::ProcessIdentity;
use serde_json::{Map, Value, json};

fn body(value: Value) -> Map<String, Value> {
  value.as_object().unwrap().clone()
}

fn write(directory: &Path, name: &str, value: &Value) {
  std::fs::create_dir_all(directory).unwrap();
  std::fs::write(directory.join(name), value.to_string()).unwrap();
}

// A pid no process has: past the kernel's ceiling.
const NO_PID: i64 = 999_999_999;
const NOW: i64 = 1_800_000_000_000;

#[test]
fn a_run_left_in_its_host_closes_on_the_outcome_its_host_published() {
  let dir = tempfile::tempdir().unwrap();
  let row = body(json!({ "id": "run-a", "line": "coordinator", "parent": null }));

  write(
    dir.path(),
    "started.json",
    &json!({ "pid": NO_PID, "birth": "1", "supervisor": { "pid": NO_PID, "birth": "1" } }),
  );
  assert_eq!(
    ending(&row, false, dir.path(), NOW),
    Some(Ending::new(
      "cancelled",
      Some(1),
      "interrupted: supervisor and acknowledged process group gone"
    ))
  );

  write(dir.path(), "outcome.json", &json!({ "exitCode": 0, "cause": "exited" }));
  assert_eq!(
    ending(&row, false, dir.path(), NOW),
    Some(Ending::new("exited", Some(0), "durable transport outcome"))
  );
}

#[test]
fn a_supervised_run_whose_supervisor_lives_stays_open() {
  let dir = tempfile::tempdir().unwrap();
  let own = ProcessIdentity::own();
  let row = body(json!({ "id": "run-b", "line": "coordinator" }));

  write(dir.path(), "started.json", &json!({ "pid": NO_PID, "birth": "1", "supervisor": own }));
  assert_eq!(ending(&row, false, dir.path(), NOW), None);
}

#[test]
fn a_row_never_placed_closes_only_a_day_after_it_began_and_one_in_a_pane_never_does() {
  let dir = tempfile::tempdir().unwrap();
  let missing = dir.path().join("run-c");
  let fresh = body(json!({ "id": "run-c", "line": "gate", "startedAt": NOW - 60_000 }));
  let old = body(json!({ "id": "run-c", "line": "gate", "startedAt": NOW - NEVER_PLACED_MS - 1 }));
  let in_pane = body(json!({ "id": "run-c", "line": "worker", "paneId": "w1:p1", "startedAt": 0 }));
  let resident = body(json!({ "id": "run-c", "line": "supervisor", "resident": {}, "startedAt": 0 }));

  assert_eq!(ending(&fresh, false, &missing, NOW), None);
  assert_eq!(
    ending(&old, false, &missing, NOW),
    Some(Ending::new("cancelled", Some(1), NEVER_PLACED))
  );
  assert_eq!(ending(&in_pane, false, &missing, NOW), None);
  assert_eq!(ending(&resident, false, &missing, NOW), None);

  // An envelope whose start was never acknowledged is no placing either.
  write(&missing, "command.json", &json!({}));
  assert_eq!(
    ending(&old, false, &missing, NOW),
    Some(Ending::new("cancelled", Some(1), NEVER_PLACED))
  );
}

#[test]
fn a_native_child_ends_with_the_parent_whose_pane_it_shared() {
  let dir = tempfile::tempdir().unwrap();
  let row = body(json!({ "id": "child-a", "parent": "run-d", "sharedPane": true }));

  assert_eq!(ending(&row, true, dir.path(), NOW), None);
  assert_eq!(
    ending(&row, false, dir.path(), NOW),
    Some(Ending::new("unknown", None, "ended with the parent turn whose pane it shared"))
  );
}

#[test]
fn a_pane_turn_closes_on_its_collector_s_ending_or_its_collector_dying_before_its_pane() {
  let dir = tempfile::tempdir().unwrap();
  let placed = body(json!({ "id": "turn-a", "paneId": "w1:p1" }));
  let unplaced = body(json!({ "id": "turn-a", "paneId": null }));

  write(dir.path(), ENVELOPE, &json!({ "owner": { "pid": NO_PID, "birth": "1" } }));
  assert_eq!(ending(&placed, false, dir.path(), NOW), None);
  assert_eq!(
    ending(&unplaced, false, dir.path(), NOW),
    Some(Ending::new(
      "cancelled",
      Some(1),
      "interrupted: the collector died before its turn had a pane"
    ))
  );

  write(dir.path(), ENVELOPE, &json!({ "owner": ProcessIdentity::own(), "cause": "exited" }));
  assert_eq!(
    ending(&placed, false, dir.path(), NOW),
    Some(Ending::new("exited", None, "the turn's own collector published how it ended"))
  );
}

#[test]
fn a_pane_turn_s_collector_says_it_is_over_once_it_published_its_end_or_is_gone() {
  let dir = tempfile::tempdir().unwrap();

  assert!(!collector_ended(dir.path()), "no collector says nothing");
  write(dir.path(), ENVELOPE, &json!({ "owner": ProcessIdentity::own() }));
  assert!(!collector_ended(dir.path()), "a collector standing carries its turn");
  write(dir.path(), ENVELOPE, &json!({ "owner": { "pid": NO_PID, "birth": "1" } }));
  assert!(collector_ended(dir.path()));
  write(dir.path(), ENVELOPE, &json!({ "owner": ProcessIdentity::own(), "cause": "exited" }));
  assert!(collector_ended(dir.path()));
}

#[test]
fn a_resident_turn_whose_collector_died_closes_on_its_program_s_hooks_or_its_program_gone() {
  let dir = tempfile::tempdir().unwrap();
  let place = dir.path().join("place");
  let resident = |program: Value| {
    let mut resident = program;

    resident["place"] = json!(place);
    body(json!({ "id": "turn-r", "paneId": "w1:p1", "startedAt": NOW, "resident": resident }))
  };
  let standing = resident(serde_json::to_value(ProcessIdentity::own()).unwrap());
  let gone_program = resident(json!({ "pid": NO_PID, "birth": "1" }));
  let hooks = |lines: &[Value]| {
    std::fs::create_dir_all(&place).unwrap();
    std::fs::write(
      place.join("hooks.jsonl"),
      lines.iter().map(|line| format!("{line}\n")).collect::<String>(),
    )
    .unwrap();
  };

  write(dir.path(), ENVELOPE, &json!({ "owner": { "pid": NO_PID, "birth": "1" } }));
  assert_eq!(ending(&standing, false, dir.path(), NOW), None);
  assert_eq!(
    ending(&gone_program, false, dir.path(), NOW),
    Some(Ending::new("cancelled", Some(1), PROGRAM_GONE))
  );

  // A stop written before the turn began is the turn before's; one since ends it.
  hooks(&[json!({ "at": NOW - 1, "kind": "stop" }), json!({ "at": NOW + 1, "kind": "submit" })]);
  assert_eq!(ending(&standing, false, dir.path(), NOW), None);

  hooks(&[json!({ "at": NOW + 1, "kind": "submit" }), json!({ "at": NOW + 2, "kind": "stop" })]);
  assert_eq!(
    ending(&standing, false, dir.path(), NOW),
    Some(Ending::new("exited", None, ENDED_BY_ITS_HOOKS))
  );

  // While its collector lives the turn is the collector's to close.
  write(dir.path(), ENVELOPE, &json!({ "owner": ProcessIdentity::own() }));
  assert_eq!(ending(&gone_program, false, dir.path(), NOW), None);
}

#[test]
fn an_id_that_is_no_plain_name_is_never_read_as_a_directory() {
  let dir = tempfile::tempdir().unwrap();

  write(dir.path(), "outcome.json", &json!({ "exitCode": 0, "cause": "exited" }));
  assert_eq!(ending(&body(json!({ "id": "../run-e" })), false, dir.path(), NOW), None);
}
