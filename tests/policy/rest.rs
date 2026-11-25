use policy::rest::*;

#[test]
fn a_rest_names_its_fallback_and_its_words() {
  assert!(REST.fallback_seconds > 0 && !REST.resting.trim().is_empty());
}
