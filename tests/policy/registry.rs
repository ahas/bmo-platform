use policy::registry::*;

#[test]
fn the_rules_read_and_the_least_answer_fits_inside_the_least_window() {
  assert!(REGISTRY.name_pattern.starts_with('^'));
  assert!(REGISTRY.answer_tokens.min < REGISTRY.context_tokens.min);
  assert!(REGISTRY.context_tokens.holds(65_536));
}
