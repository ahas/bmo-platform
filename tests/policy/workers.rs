use policy::workers::*;

#[test]
fn the_rules_read_and_the_first_support_worker_follows_the_base() {
  assert!(WORKERS.name_pattern.starts_with('^'));
  assert_eq!(WORKERS.first_support_slot, 2);
}

#[test]
fn a_job_holds_more_than_its_own_checkout_and_a_waiting_borrowing_speaks_within_the_call_clock_s_minute() {
  assert!(WORKERS.job_checkouts_most > 1);
  assert!(WORKERS.borrow_wait_seconds > 0 && WORKERS.borrow_wait_seconds < 60);
  assert!(WORKERS.loan_look_seconds > 0);
  assert!(WORKERS.park_message.contains("{branch}"));
}
