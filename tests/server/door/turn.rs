//! A turn put into a program's pane ending on its program's error (docs/agent-control.md, 「Telling that a turn's
//! program is gone」): a failed request the program reported through its failure hook ends the wait the moment
//! it is read, whatever herdr reads the pane as, and the error the program wrote in the answer's place once the
//! pane is no longer working.

use agency_server::door::turn::{end_written, over_on_error, rests_on_an_end};

#[test]
fn a_reported_failure_ends_the_wait_however_herdr_reads_the_pane() {
  // herdr can go on reading `working` for a look or two after the program gave the turn up.
  assert!(over_on_error(true, false, true), "reported while herdr reads working");
  assert!(over_on_error(false, false, true), "reported at rest");
  assert!(over_on_error(true, true, true));
}

#[test]
fn an_error_written_in_the_answer_s_place_ends_the_wait_once_the_pane_is_not_working() {
  assert!(!over_on_error(true, true, false), "still working on past it");
  assert!(over_on_error(false, true, false));
}

#[test]
fn a_turn_that_goes_on_past_a_retry_inside_its_program_is_untouched() {
  // No failure reported and no error in the record: a request the program retried and got past.
  assert!(!over_on_error(true, false, false));
  assert!(!over_on_error(false, false, false));
}

#[test]
fn a_turn_a_failed_request_ended_is_not_waited_on_for_an_end_line_it_never_writes() {
  // The record holds a partial answer and the program's error, and no line a turn ends with.
  assert!(end_written(false, false, true));
  assert!(end_written(true, false, false));
  assert!(end_written(false, true, false));
  assert!(!end_written(false, false, false), "a turn still writing its answer is waited on");
}

#[test]
fn a_round_whose_latest_turn_failed_is_read_as_over_once_its_program_rests() {
  assert!(rests_on_an_end(true, false, true), "no end line follows a failed request");
  assert!(rests_on_an_end(true, true, false));
  assert!(!rests_on_an_end(false, false, true), "a program still working owes its turn");
  assert!(!rests_on_an_end(true, false, false), "a record ending mid-turn has more to come");
}
