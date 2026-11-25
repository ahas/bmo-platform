use policy::roads::*;

#[test]
fn the_road_words_read_and_name_what_each_says() {
  assert!(ROADS.reasons.balance.contains("{amount}") && !ROADS.reasons.no_login.is_empty());
  assert!(ROADS.logins.resting.contains("{account}") && ROADS.logins.resting.contains("{until}"));
  assert!(ROADS.closed.person.contains("{command}") && ROADS.closed.reopens.contains("{at}"));
  assert!(ROADS.job.continued.contains("{from}") && ROADS.job.continued.contains("{why}"));
  assert!(ROADS.job.moved.contains("{roads}") && ROADS.job.wait_seconds > 0);
}
