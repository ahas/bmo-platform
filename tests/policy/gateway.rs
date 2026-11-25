use policy::gateway::*;

#[test]
fn a_stream_is_given_three_minutes_of_silence_before_it_is_cut() {
  assert_eq!(GATEWAY.stall_seconds, 180);
}
