//! Where the host keeps its own under a data root, and which envelopes it takes.

use std::path::Path;

use agent_host::layout::{Layout, name_component};

#[test]
fn only_a_plain_envelope_directly_among_the_envelopes_is_taken() {
  let layout = Layout::new(Path::new("/data"));

  assert_eq!(
    layout.envelope("/data/executions/run-1").map(|(_, name)| name),
    Some("run-1".to_owned())
  );
  assert!(layout.envelope("/etc").is_none());
  assert!(layout.envelope("/data/executions/../secrets").is_none());
  assert!(layout.envelope("/data/executions/a/b").is_none());
  assert!(layout.envelope("/data/executions/has space").is_none());
  assert!(layout.envelope("/data/executions/agent-host").is_none());
  assert!(layout.envelope("/data/executions/hosts").is_none());
  assert!(layout.envelope("executions/run-1").is_none());
  assert_eq!(
    layout.envelope("/data/executions/x/../run-2").map(|(_, name)| name),
    Some("run-2".to_owned())
  );
}

#[test]
fn a_name_that_is_not_a_plain_word_is_hashed() {
  assert_eq!(name_component("chat"), "chat");
  assert!(name_component("../chat").starts_with("h-"));
  assert_eq!(name_component("../chat").len(), 18);
}
