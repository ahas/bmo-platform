//! An event a verb's own process published, posted to the server by its kind: only a kebab-case name is a
//! kind.

use agency_server::control::events_stream::is_kind;

#[test]
fn a_kind_is_a_kebab_case_name() {
  for kind in ["worker-removed", "usage-spent", "a1-b2"] {
    assert!(is_kind(kind), "{kind}");
  }

  for word in [
    "",
    "Worker-removed",
    "worker_removed",
    "-worker",
    "worker-",
    "worker--removed",
    "worker removed",
  ] {
    assert!(!is_kind(word), "{word:?}");
  }
}
