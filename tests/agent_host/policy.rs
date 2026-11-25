//! The owner's switch on a provider read at a child's start, as the launcher reads it.

use agent_host::policy::provider_on;

#[test]
fn a_provider_is_on_unless_its_switch_holds_off() {
  assert!(provider_on(None));
  assert!(provider_on(Some("on")));
  assert!(!provider_on(Some("off")));
  // A word the switch never holds turns nothing off.
  assert!(provider_on(Some("not a switch")));
}
