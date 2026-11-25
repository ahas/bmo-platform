use policy::turn_keys::*;

#[test]
fn a_keyed_turns_end_is_kept_past_a_restart() {
  assert!(TURN_KEYS.kept_seconds >= 3600);
}
