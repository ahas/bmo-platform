//! A turn's progress kept as one moment (docs/agent-control.md, 「A run's tool calls are held to a clock」, a job's
//! turn that makes no progress): a turn that stands still past the stall span is told once, and again only after
//! it moved and stood still again; a line alike the last few, digits aside, is no progress; nothing is told while
//! the supervisor already holds the wait. Every moment is the test's own.

use agency_server::door::stall::{Progress, StallChange, StallClock, Stalled, Waits, alike};
use serde_json::{Value, json};

const SPAN: i64 = 900_000;

fn clock() -> StallClock {
  StallClock {
    after_ms: SPAN,
    idle_cpu_percent: 5,
    recent: 4,
    lines: 5,
    chars: 300,
  }
}

fn stamp(ms: i64) -> String {
  chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms).unwrap().to_rfc3339()
}

fn called(command: &str, at: i64) -> Value {
  json!({
    "type": "assistant", "timestamp": stamp(at),
    "message": { "content": [
      { "type": "tool_use", "id": format!("t{at}"), "name": "Bash", "input": { "command": command } }
    ] }
  })
}

fn answered(text: &str, at: i64) -> Value {
  json!({
    "type": "user", "timestamp": stamp(at),
    "message": { "content": [
      { "type": "tool_result", "tool_use_id": format!("t{at}"), "content": text }
    ] }
  })
}

fn waits() -> Vec<Waits> {
  vec![Waits {
    tool: "Bash until rg -q '^exit ' run.txt; do sleep 30; done".to_owned(),
    lines: vec!["every card it may use is taken; asking again every minute".to_owned()],
  }]
}

fn changes(progress: &mut Progress) -> Vec<StallChange> {
  std::iter::from_fn(|| progress.take()).collect()
}

fn stalled_since(changes: &[StallChange]) -> Vec<i64> {
  changes
    .iter()
    .filter_map(|change| match change {
      StallChange::Stalled(Stalled { since, .. }) => Some(*since),
      StallChange::Resumed => None,
    })
    .collect()
}

#[test]
fn a_turn_that_stands_still_is_told_once_and_again_only_after_it_moved_and_stood_still_again() {
  let mut progress = Progress::new(clock());
  let mut records = vec![called("cargo build", 0)];

  progress.records(&records, 0, 1_000);

  let mut told = vec![];

  for now in (1_000..=3 * SPAN).step_by(10_000) {
    progress.judge(now, false, waits);
    told.extend(changes(&mut progress));
  }

  assert_eq!(
    stalled_since(&told),
    [0],
    "told once, its stall counted from the record's last line: {told:?}"
  );
  assert!(!told.contains(&StallChange::Resumed));

  let StallChange::Stalled(stall) = &told[0] else { unreachable!() };

  assert_eq!(stall.at, SPAN + 1_000, "told at the first read past the span");
  assert_eq!(stall.waits, waits(), "carrying what it waits on");

  // The model writes a new line: the stall is over, said once.
  let moved_at = 3 * SPAN + 5_000;

  records.push(answered("Finished release profile", moved_at));
  progress.records(&records, 0, moved_at);
  progress.judge(moved_at, false, waits);
  assert_eq!(changes(&mut progress), [StallChange::Resumed]);

  // Standing still again, it is told again, counted from the move.
  progress.judge(moved_at + SPAN - 1, false, waits);
  assert!(changes(&mut progress).is_empty(), "not before the span");
  progress.judge(moved_at + SPAN, false, waits);
  assert_eq!(stalled_since(&changes(&mut progress)), [moved_at]);
}

#[test]
fn a_turn_whose_model_keeps_writing_new_lines_is_never_told() {
  let mut progress = Progress::new(clock());
  let mut records = vec![];

  for (step, now) in (0..4 * SPAN).step_by(60_000).enumerate() {
    // Digits are no difference between two lines, so each crate is named by a letter.
    let name = ["store", "server", "policy", "herdr", "tools", "gateway", "siren"][step % 7];

    records.push(called(&format!("cargo test -p {name}"), now));
    records.push(answered(&format!("test result for {name}: ok"), now));
    progress.records(&records, 0, now);
    progress.judge(now, false, waits);
    assert!(changes(&mut progress).is_empty(), "a working turn is not told, at {now}");
  }
}

#[test]
fn a_poll_made_again_and_again_with_the_same_answer_is_no_progress() {
  let mut progress = Progress::new(clock());
  let mut records = vec![];
  let mut told = vec![];

  for (pass, now) in (0..2 * SPAN).step_by(60_000).enumerate() {
    records.push(called("sleep 60; tail -3 /tmp/run.txt", now));
    records.push(answered(&format!("asking again every minute ({pass} tries)"), now));
    progress.records(&records, 0, now);
    progress.judge(now, false, waits);
    told.extend(changes(&mut progress));
  }

  assert_eq!(stalled_since(&told).len(), 1, "the poll is told once: {told:?}");
  assert_eq!(alike("waited 12s, 3 ahead"), alike("waited 140s, 1 ahead"));
  assert_ne!(alike("Compiling store v0.1.0"), alike("Compiling server v0.1.0"));
}

#[test]
fn output_grown_with_a_new_last_line_is_progress_and_a_repeated_line_is_not() {
  let mut progress = Progress::new(clock());

  progress.records(&[], 0, 0);
  progress.output("run.txt", 77, || Some("asking again every minute (1)".to_owned()), 0);

  // The same line with a new count, every minute: the turn stands still.
  for (pass, now) in (60_000..=SPAN).step_by(60_000).enumerate() {
    let line = format!("asking again every minute ({})", pass + 2);

    progress.output("run.txt", 77 + 30 * (pass as u64 + 1), || Some(line), now);
  }

  progress.judge(SPAN, false, waits);
  assert_eq!(stalled_since(&changes(&mut progress)), [0]);

  // A new line moves it.
  progress.output("run.txt", 10_000, || Some("clip step finished".to_owned()), SPAN + 10_000);
  assert_eq!(changes(&mut progress), [StallChange::Resumed]);

  // A source that only stays at its size is no progress, nor is its first read; one grown with no line is.
  progress.output("run.txt", 10_000, || panic!("a source that did not grow is not read"), SPAN + 20_000);
  progress.output("agent-a.jsonl", 500, || None, SPAN + 20_000);
  progress.output("agent-a.jsonl", 900, || None, 2 * SPAN);
  progress.judge(2 * SPAN + SPAN / 2, false, waits);
  assert!(changes(&mut progress).is_empty(), "the subagent's record grew");
}

#[test]
fn processor_time_above_the_idle_share_is_progress_and_below_it_is_not() {
  let mut progress = Progress::new(clock());

  progress.records(&[], 0, 0);

  for now in (10_000..=SPAN).step_by(10_000) {
    progress.burned(100, 10_000, now);
  }

  progress.judge(SPAN, false, waits);
  assert_eq!(stalled_since(&changes(&mut progress)), [0], "a poll's share is no progress");

  progress.burned(9_000, 10_000, SPAN + 10_000);
  assert_eq!(changes(&mut progress), [StallChange::Resumed], "a build's is");
}

#[test]
fn nothing_is_told_while_the_supervisor_holds_the_wait_and_it_is_told_once_he_no_longer_does() {
  let mut progress = Progress::new(clock());

  progress.records(&[], 0, 0);
  progress.judge(2 * SPAN, true, || panic!("what it waits on is not read while it is not told"));
  assert!(changes(&mut progress).is_empty());

  progress.judge(2 * SPAN + 10_000, false, waits);
  assert_eq!(stalled_since(&changes(&mut progress)), [0]);
}

#[test]
fn a_turn_taken_up_says_its_first_move_so_a_stall_told_before_it_is_let_go() {
  let mut progress = Progress::taken_up(clock());

  progress.records(&[called("bmo check", 0)], 0, 5_000);
  assert!(changes(&mut progress).is_empty(), "the first read only learns the record");

  progress.records(&[called("bmo check", 0), answered("check passed", 6_000)], 0, 6_000);
  assert_eq!(changes(&mut progress), [StallChange::Resumed]);

  progress.records(
    &[called("bmo check", 0), answered("check passed", 6_000), called("git push", 7_000)],
    0,
    7_000,
  );
  assert!(changes(&mut progress).is_empty(), "said once");
}
