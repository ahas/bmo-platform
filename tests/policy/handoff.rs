use policy::handoff::*;

#[test]
fn a_handoff_under_way_is_asked_again_after_a_pause() {
  assert!(HANDOFF.retry_seconds > 0);
  assert!(!HANDOFF.reasons.lost.is_empty() && !HANDOFF.reasons.replaced.is_empty());
}
