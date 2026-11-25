use crate::support::wording::assert_no_instruction;
use policy::queue_idle::*;

#[test]
fn the_queue_is_looked_at_each_minute_and_an_agent_is_told_idle_after_its_launch_seconds() {
  assert!(QUEUE_IDLE.check_seconds > 0 && QUEUE_IDLE.launch_seconds > 0);
}

#[test]
fn an_idle_agent_s_order_and_a_draft_s_carry_their_marks_and_a_bounded_brief() {
  assert!(QUEUE_IDLE.draft_brief_lines > 0 && QUEUE_IDLE.draft_brief_chars > 0);

  for (order, marks) in [
    (
      &QUEUE_IDLE.agent_idle_order,
      &["{agent}", "{role}", "{provider}", "{model}", "{items}"][..],
    ),
    (&QUEUE_IDLE.agent_idle_item, &["{item}", "{title}"][..]),
    (
      &QUEUE_IDLE.draft_order,
      &["{item}", "{title}", "{raised_by}", "{repository}", "{brief}"][..],
    ),
  ] {
    for mark in marks {
      assert!(order.contains(mark), "{mark} in {order}");
    }

    assert_no_instruction(order);
  }
}
