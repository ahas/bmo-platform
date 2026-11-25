use crate::support::wording::assert_no_instruction;
use policy::job_notices::*;

#[test]
fn the_orderer_chain_is_bounded_and_every_order_carries_its_data_and_no_instruction() {
  assert!(JOB_NOTICES.orderer_depth > 0);
  assert!(!JOB_NOTICES.any_worker.is_empty());
  assert_eq!(JOB_NOTICES.wait_labels, ["follows", "waiting-on"]);

  for (order, marks) in [
    (&JOB_NOTICES.paused_order, &["{item}", "{title}", "{queue}"][..]),
    (&JOB_NOTICES.unlaunchable_order, &["{item}", "{title}", "{assignee}"][..]),
    (&JOB_NOTICES.agent_order, &["{item}", "{title}", "{label}", "{why}"][..]),
    (&JOB_NOTICES.role_order, &["{item}", "{title}", "{label}", "{roles}"][..]),
    (
      &JOB_NOTICES.moved_order,
      &["{item}", "{title}", "{state}", "{merged}", "{job}", "{worker}"][..],
    ),
    (&JOB_NOTICES.closed_order, &["{item}", "{title}", "{by}"][..]),
    (&JOB_NOTICES.wait_ended_order, &["{item}", "{title}", "{label}", "{waited}"][..]),
    (&JOB_NOTICES.waited_item, &["{item}", "{title}", "{state}"][..]),
    (&JOB_NOTICES.question_order, &["{question}", "{item}", "{text}", "{choices}"][..]),
    (&JOB_NOTICES.waiting_order, &["{item}", "{worker}", "{why}"][..]),
    (&JOB_NOTICES.close_order, &["{item}", "{lease}"][..]),
    (
      &JOB_NOTICES.stalled_order,
      &["{job}", "{item}", "{worker}", "{minutes}", "{since}", "{waiting}"][..],
    ),
    (&JOB_NOTICES.stalled_call, &["{tool}", "{lines}"][..]),
    (
      &JOB_NOTICES.question_due_order,
      &["{question}", "{item}", "{asker}", "{text}", "{choices}"][..],
    ),
    (&JOB_NOTICES.question_settled_order, &["{question}", "{item}", "{answer}", "{why}"][..]),
    (&JOB_NOTICES.question_settled_item, &["{item}"][..]),
    (&JOB_NOTICES.failed_order, &["{job}", "{item}", "{worker}", "{outcome}", "{room}"][..]),
  ] {
    for mark in marks {
      assert!(order.contains(mark), "{mark} in {order}");
    }

    assert_no_instruction(order);
  }
}

#[test]
fn the_outbox_sends_often_waits_longer_after_each_failure_and_retells_a_standing_notice() {
  assert!(JOB_NOTICES.send_seconds > 0 && JOB_NOTICES.answer_seconds > 0);
  assert!(
    0 < JOB_NOTICES.retry_first_seconds
      && JOB_NOTICES.retry_first_seconds <= JOB_NOTICES.retry_most_seconds
  );
  assert!(JOB_NOTICES.retell_seconds >= JOB_NOTICES.send_seconds);
  assert!(
    JOB_NOTICES.owe_grace_seconds > JOB_NOTICES.answer_seconds,
    "a change's own path is given longer than one try to owe its notice"
  );
}
