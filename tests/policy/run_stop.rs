use policy::run_stop::*;

#[test]
fn a_stop_asks_for_the_work_to_end_properly() {
  assert!(RUN_STOP.instruction.starts_with("Stop here."));
  assert!(RUN_STOP.instruction.contains("\n\nEnd properly"));
}
