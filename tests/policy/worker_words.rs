use policy::worker_words::*;

#[test]
fn a_word_s_turn_is_clocked_and_what_a_word_may_carry_is_bounded() {
  assert!(WORKER_WORDS.answer_minutes > 0);
  assert!(WORKER_WORDS.most_words > 0);
  assert!(WORKER_WORDS.most_files > 0);
  assert!(WORKER_WORDS.most_file_bytes > 0);
  assert!(WORKER_WORDS.title.contains("{worker}"));
  assert!(WORKER_WORDS.held.pinned.contains("{provider}"));
}
