use agency_server::doctor::*;
use agency_server::events::Event;

fn evidence(ending: RunEnding) -> Evidence {
  Evidence {
    room: None,
    ending,
    said: None,
    last_action: None,
    record: record_of(None, "agency.service"),
  }
}

#[test]
fn a_signal_is_told_apart_from_an_exit_and_silence_is_no_breakage() {
  assert_eq!(diagnose(&evidence(RunEnding::Exit { code: 1 })).signature, "exit:1");
  assert_eq!(diagnose(&evidence(RunEnding::Exit { code: 137 })).signature, "signal:137");
  assert!(!diagnose(&evidence(RunEnding::Silent)).broke);
}

#[test]
fn a_long_line_is_cut_with_its_mark() {
  assert_eq!(one_line("a  b\n c", 10).as_deref(), Some("a b c"));
  assert_eq!(one_line("abcdef", 3).as_deref(), Some("abc…"));
  assert_eq!(one_line("  ", 3), None);
}

#[test]
fn a_failure_no_run_was_had_for_points_at_the_named_units_journal() {
  assert_eq!(record_of(None, "agency.service"), "journalctl --user -u agency.service");
}

#[test]
fn a_run_come_back_is_published_as_a_recovery() {
  let standing = StandingFailure {
    signature: "exit:1".into(),
    cause: "the program exited".into(),
    room: None,
    count: 2,
    at: 0,
    told: Some(true),
  };
  let recovered = recovery_notice(&standing);

  assert_eq!(RunRecovered::NAME, "run-recovered");
  assert!(recovered.fact.contains(", 2 times in a row"), "{}", recovered.fact);
  assert!(recovered.fact.contains("the program exited"), "{}", recovered.fact);
  assert_eq!(recovered.kind, "doctor-recovered");
  assert_eq!(recovered.ttl_ms, 3_600_000);
}
