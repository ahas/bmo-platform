use crate::support::wording::assert_no_instruction;
use policy::resident_turn::*;

#[test]
fn the_lead_names_the_paste_and_the_clocks_keep_their_order() {
  assert!(RESIDENT_TURN.pasted_lead.starts_with("The text pasted after this line"));
  assert!(RESIDENT_TURN.apart_lead.starts_with("The text pasted after this line"));
  assert_no_instruction(&RESIDENT_TURN.pasted_lead);
  assert_no_instruction(&RESIDENT_TURN.apart_lead);
  assert!(RESIDENT_TURN.round_lead.is_empty());
  assert!(RESIDENT_TURN.poll_ms < RESIDENT_TURN.redraw_ms);
  assert!(RESIDENT_TURN.gone_looks >= 1);
}
