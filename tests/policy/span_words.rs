use policy::fill_marks;
use policy::span_words::SPAN_WORDS;

#[test]
fn reads_its_words_and_each_names_the_marks_it_is_filled_by() {
  assert!(SPAN_WORDS.total.contains("{tokens}"));
  assert_eq!(fill_marks(&SPAN_WORDS.clock.seconds, &[("seconds", "3")]), "3s");
  assert_eq!(fill_marks(&SPAN_WORDS.clock.minutes, &[("minutes", "8"), ("seconds", "12")]), "8m 12s");
  assert_eq!(fill_marks(&SPAN_WORDS.clock.hours, &[("hours", "1"), ("minutes", "22")]), "1h 22m");

  for kind in [
    &SPAN_WORDS.kinds.input,
    &SPAN_WORDS.kinds.output,
    &SPAN_WORDS.kinds.cache_read,
    &SPAN_WORDS.kinds.cache_write,
  ] {
    assert!(!kind.is_empty());
  }
}
