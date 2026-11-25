use policy::fill_marks;
use policy::pool_warm::POOL_WARM;

#[test]
fn the_margin_leaves_a_lifetime() {
  assert!(POOL_WARM.margin_minutes > 0);
  assert!(!POOL_WARM.native.refusal.trim().is_empty());
}

#[test]
fn a_pings_words_read_with_the_marks_they_are_filled_by() {
  let words = &POOL_WARM.pings;

  assert_eq!(
    fill_marks(&POOL_WARM.branch.title, &[("place", "co-worker repo 03")]),
    "Warming co-worker repo 03"
  );
  assert!(words.off.contains("{mode}"));
  assert!(words.no_key.contains("{variable}"));
  assert!(words.other_upstream.contains("{upstream}"));

  for line in [&words.no_prefix, &words.no_account, &words.no_pane, &words.no_upstream] {
    assert!(!line.trim().is_empty());
  }
}
