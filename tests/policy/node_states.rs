use policy::node_states::*;

#[test]
fn the_file_says_every_state_and_no_other() {
  assert_eq!(NODE_STATES.keys().copied().collect::<Vec<_>>(), NodeState::ALL.to_vec());
  assert_eq!(NodeState::Connected.said().word, "connected");
  assert_eq!(NodeState::Unknown.said().tone, Tone::Unknown);
  assert_eq!(NodeState::Auth.said().tone, Tone::Bad);
  assert_eq!(NodeState::of_column("hostkey"), NodeState::Hostkey);
  assert_eq!(NodeState::of_column("lost"), NodeState::Unknown);
}
