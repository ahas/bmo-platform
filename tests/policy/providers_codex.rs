use policy::providers::codex::CODEX;

#[test]
fn a_model_takes_the_lifetime_of_the_longest_start_it_begins() {
  assert_eq!(CODEX.cache.of("gpt-5.6-sol").map(|entry| entry.ttl_minutes), Some(30));
  assert!(CODEX.cache.of("gpt-5.5").is_none());
}
