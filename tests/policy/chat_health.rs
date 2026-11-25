use policy::chat_health::CHAT_HEALTH;
use policy::fill_marks;

#[test]
fn reads_its_lines_and_each_names_the_marks_it_is_filled_by() {
  assert!(CHAT_HEALTH.places.room.contains("{room}"));
  assert!(CHAT_HEALTH.account.run_log.contains("{run}"));
  assert!(CHAT_HEALTH.account.silent.contains("{agent}"));
  assert!(CHAT_HEALTH.recovered.failed.contains("{where}"));
  assert!(CHAT_HEALTH.causes.exit.contains("{code}"));
  assert!(CHAT_HEALTH.causes.signal.contains("{signal}"));
  assert!(CHAT_HEALTH.recovered.ttl_minutes > 0);
}

#[test]
fn an_account_fills_into_one_line() {
  let cause = fill_marks(&CHAT_HEALTH.causes.exit, &[("code", "3")]);
  let line = fill_marks(
    &CHAT_HEALTH.account.line,
    &[("opening", "A run failed"), ("room", ""), ("cause", &cause), ("said", ""), ("rest", "")],
  );

  assert!(line.contains("exit code 3"), "{line}");
  assert!(!line.contains('{'), "{line}");
}
