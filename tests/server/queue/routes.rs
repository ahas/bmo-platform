//! A job's selection: a provider this build carries and a model, a bare Claude model, or the automatic one.

use agency_server::queue::routes::{choice, is_selection};

#[test]
fn a_selection_names_a_provider_this_build_carries_and_a_model() {
  let read = choice("codex::gpt-5.5").unwrap();

  assert_eq!((read.provider, read.model), ("codex", "gpt-5.5"));
  assert!(choice("nobody::model").is_none());
  assert!(choice("codex::").is_none());
  assert!(is_selection("claude-opus-4-1"));
  assert!(is_selection("auto"));
  assert!(!is_selection("opus"));
}
