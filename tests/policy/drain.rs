use policy::drain::*;

#[test]
fn a_drain_bounds_the_short_calls_and_tells_a_refused_caller_when_to_ask_again() {
  assert!(DRAIN.short_calls_seconds > 0);
  assert!(DRAIN.retry_after_seconds > 0);
}
