//! The call clock's notes: each written as the sentence the unit's log has always read and its facts as the
//! run log's `toolClock` fields, and a sentence filed before the fields were kept read back into the same
//! facts (docs/agent-control.md, 「A run's tool calls are held to a clock」).

use agency_server::door::tool_clock::{ClockEvent, ClockNote, Cut, Outcome};
use serde_json::json;

// A note on a call as the watcher writes one.
fn note(event: ClockEvent, pid: Option<i32>, elapsed_ms: i64) -> ClockNote {
  ClockNote {
    run: "job-41".to_owned(),
    call: Some("toolu_1".to_owned()),
    tool: "Bash".to_owned(),
    command: "F=/tmp/scratch/ask.out; for i in $(seq 1 240); do grep -q '^exit".to_owned(),
    pid,
    elapsed_ms: Some(elapsed_ms),
    event,
  }
}

#[test]
fn every_note_reads_back_out_of_its_own_sentence() {
  let notes = [
    note(ClockEvent::Started, None, 2_000),
    note(ClockEvent::Running, Some(1_681_736), 62_000),
    note(ClockEvent::Asked { silent_ms: 311_000 }, Some(1_681_736), 311_000),
    note(ClockEvent::Kept { for_ms: 3_592_000 }, Some(1_681_736), 331_000),
    note(
      ClockEvent::AskFailed {
        why: "the ask route answered 503".to_owned(),
      },
      Some(7),
      331_000,
    ),
    note(ClockEvent::Cut(Cut::NoProgress), Some(7), 600_000),
    note(ClockEvent::Cut(Cut::NoProcess), None, 600_000),
    note(
      ClockEvent::Cut(Cut::Supervisor("it waits on another job's close".to_owned())),
      Some(7),
      900_000,
    ),
    note(ClockEvent::Cut(Cut::Stop), Some(7), 40_000),
    note(ClockEvent::Ended(Outcome::Completed), Some(7), 1_747_400),
    note(ClockEvent::Ended(Outcome::Failed), None, 1_500),
    note(ClockEvent::Ended(Outcome::EndedWithTurn), Some(7), 12_000),
  ];

  for written in notes {
    let sentence = written.sentence();
    let read = ClockNote::read(&sentence).unwrap_or_else(|| panic!("read back: {sentence}"));

    // A sentence carries no call id, and an ask's and a keep's carry neither the pid nor the time.
    let silent = matches!(
      written.event,
      ClockEvent::Asked { .. } | ClockEvent::Kept { .. } | ClockEvent::AskFailed { .. }
    );
    let unsaid_pid = silent || matches!(written.event, ClockEvent::Cut(Cut::NoProcess));
    let unsaid_time = matches!(written.event, ClockEvent::Kept { .. } | ClockEvent::AskFailed { .. });

    assert_eq!(read.run, written.run, "{sentence}");
    assert_eq!(read.call, None, "{sentence}");
    assert_eq!(read.tool, written.tool, "{sentence}");
    assert_eq!(read.command, written.command, "{sentence}");
    assert_eq!(read.event, written.event, "{sentence}");
    assert_eq!(read.pid, if unsaid_pid { None } else { written.pid }, "{sentence}");
    assert_eq!(read.elapsed_ms, if unsaid_time { None } else { written.elapsed_ms }, "{sentence}");
  }
}

#[test]
fn the_sentences_read_as_the_unit_s_log_has_always_read_them() {
  assert_eq!(
    note(ClockEvent::Started, None, 2_000).sentence(),
    "job-41: tool Bash started pid=- after 2.0s: F=/tmp/scratch/ask.out; for i in $(seq 1 240); do grep -q '^exit"
  );
  assert_eq!(
    note(ClockEvent::Running, Some(1_681_736), 1_566_000).sentence(),
    "job-41: tool Bash still running pid=1681736 after 1566s: F=/tmp/scratch/ask.out; for i in $(seq 1 240); do grep -q '^exit"
  );
  assert_eq!(
    note(ClockEvent::Kept { for_ms: 3_592_000 }, Some(1), 1).sentence(),
    "job-41: tool Bash F=/tmp/scratch/ask.out; for i in $(seq 1 240); do grep -q '^exit kept on the supervisor's call for 3592s more"
  );
}

#[test]
fn a_note_s_fields_name_the_call_and_what_befell_it() {
  let fields = note(ClockEvent::Asked { silent_ms: 311_000 }, Some(9), 311_000).fields();

  assert_eq!(
    fields,
    json!({ "toolClock": {
      "event": "asked",
      "call": "toolu_1",
      "tool": "Bash",
      "command": "F=/tmp/scratch/ask.out; for i in $(seq 1 240); do grep -q '^exit",
      "pid": 9,
      "elapsedMs": 311_000,
      "silentMs": 311_000,
    }})
  );
  assert_eq!(
    note(ClockEvent::Cut(Cut::Supervisor("idle".to_owned())), None, 5).fields()["toolClock"],
    json!({ "event": "cut", "call": "toolu_1", "tool": "Bash", "command": note(ClockEvent::Started, None, 0).command,
      "pid": null, "elapsedMs": 5, "cut": "supervisor", "why": "idle" })
  );
}

#[test]
fn a_line_the_clock_did_not_write_reads_as_none() {
  for text in [
    "the stop was not ended within its bound, so the turn is interrupted",
    "job-41: the call Bash sleep 9 was cut, its words put into the turn",
    "job-41: tool Bash started pid=x after 2.0s: sleep 9",
    "a sentence with: tool in it and nothing after",
  ] {
    assert_eq!(ClockNote::read(text), None, "{text}");
  }
}
