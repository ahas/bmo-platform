use policy::verb_words::VERB_WORDS;

#[test]
fn the_verb_words_read_with_the_marks_they_are_filled_by() {
  let words = &VERB_WORDS;

  assert!(words.claude_token.unknown_account.contains("{named}"));
  assert!(words.agents.provider_order.contains("{names}"));
  assert!(words.agents.node_checkout.base_on_node.contains("{base}"));
  assert!(words.agents.node_checkout.base_on_node.contains("{node}"));
  assert!(words.agents.node_placing.no_jobs.contains("{why}"));
  assert!(words.dev_servers.server.up.contains("{done}") && words.dev_servers.server.up.contains("{log}"));
  assert!(words.dev_servers.listing.worker.contains("{megabytes}"));
  assert!(words.dev_servers.server.not_started.contains("{log}"));
  assert!(!words.dev_servers.pane.pressed.contains('{'));
  assert!(words.dev_servers.pane.closing.contains("{why}"));
}
