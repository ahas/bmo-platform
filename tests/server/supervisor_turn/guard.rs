//! A turn that goes nowhere: a tool failing again and again, calls repeating what answered before, and the
//! calls the runs on the same words spent; however long they ran, time cuts nothing.

use agency_server::run_log::lines::Line;
use agency_server::supervisor_turn::guard::{Guard, Spent, likeness, spent_out};
use policy::supervisor_runner::Limits;

fn limits() -> Limits {
  Limits {
    failures: 3,
    repeats: 2,
    repeat_window: 4,
    likeness: 0.9,
    calls: 10,
  }
}

fn call(reference: &str, tool: &str, input: &str) -> Line {
  Line {
    kind: "tool",
    name: Some(tool.to_owned()),
    reference: Some(reference.to_owned()),
    text: input.to_owned(),
    ..Line::default()
  }
}

fn result(reference: &str, text: &str, failed: bool) -> Line {
  Line {
    kind: "tool-result",
    reference: Some(reference.to_owned()),
    text: text.to_owned(),
    state: Some(if failed { "failed" } else { "succeeded" }),
    ..Line::default()
  }
}

#[test]
fn likeness_is_the_share_of_pairs_two_texts_hold_in_common() {
  assert_eq!(likeness("night", "night"), 1.0);
  assert_eq!(likeness("a", "b"), 0.0);
  assert!((likeness("night", "nacht") - 0.25).abs() < 1e-9);
}

#[test]
fn the_same_tool_failing_again_and_again_cuts_the_turn() {
  let limits = limits();
  let mut guard = Guard::new(&limits, 0);

  for (at, input) in ["one", "two", "three"].into_iter().enumerate() {
    let reference = at.to_string();

    assert_eq!(guard.see(&call(&reference, "Bash", input)), None);

    let said = guard.see(&result(&reference, "no such file", true));

    if at < 2 {
      assert_eq!(said, None);
    } else {
      assert!(said.is_some_and(|said| said.contains("Bash") && said.contains('3')));
    }
  }
}

#[test]
fn calls_repeating_one_that_answered_the_same_cut_the_turn() {
  let limits = limits();
  let mut guard = Guard::new(&limits, 0);

  guard.see(&call("a", "Read", "/home/tester/file.txt"));
  guard.see(&result("a", "nothing here", false));
  guard.see(&call("b", "Read", "/home/tester/file.txt"));
  assert_eq!(guard.see(&result("b", "nothing here", false)), None);
  guard.see(&call("c", "Read", "/home/tester/file.txt"));
  assert!(guard.see(&result("c", "nothing here", false)).is_some());
}

#[test]
fn the_calls_count_on_from_what_the_runs_before_spent() {
  let limits = limits();
  let mut guard = Guard::new(&limits, 9);

  assert!(guard.see(&call("a", "Read", "x")).is_some_and(|said| said.contains("10")));
  assert_eq!(
    spent_out(
      &limits,
      Spent {
        calls: 3,
        ms: 24 * 60 * 60_000
      }
    ),
    None
  );
  assert!(spent_out(&limits, Spent { calls: 10, ms: 0 }).is_some_and(|said| said.contains("10")));
}
