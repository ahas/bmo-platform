use policy::fill_marks;
use policy::place_words::PLACE_WORDS;

#[test]
fn the_place_words_read_with_the_marks_they_are_filled_by() {
  assert!(PLACE_WORDS.copy_label.contains("{label}") && PLACE_WORDS.copy_label.contains("{name}"));
  assert!(!PLACE_WORDS.dev_tab.is_empty());
  assert!(PLACE_WORDS.standby.replaced_on.contains("{model}"));

  for line in [&PLACE_WORDS.standby.replaced, &PLACE_WORDS.standby.released] {
    assert!(line.contains("{was}") && line.contains("{pin}"));
  }
}

#[test]
fn a_standing_checkout_is_labelled_by_its_pool_and_its_number() {
  assert_eq!(
    fill_marks(&PLACE_WORDS.coworker_place, &[("pool", "repo"), ("number", "03")]),
    "co-worker repo 03"
  );
}
