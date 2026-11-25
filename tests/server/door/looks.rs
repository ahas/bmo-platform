use agency_server::door::looks::*;

#[test]
fn the_trust_question_is_accepted_by_moving_down_from_the_cursor_to_the_accepting_row() {
  let screen = "Do you trust the files in this folder?\n\n ❯ 1. Yes, I trust this folder\n   2. No, exit\n";

  assert_eq!(folder_trust_keys(screen), Some(vec!["enter"]));

  let screen = " ❯ 1. No, exit\n   2. Yes, I trust this folder\n";

  assert_eq!(folder_trust_keys(screen), Some(vec!["down", "enter"]));
  assert_eq!(folder_trust_keys("❯ a prompt box\n"), None);
  assert_eq!(folder_trust_keys("   1. No, exit\n   2. Yes, I trust this folder\n"), None);
}

// The offer as a worker's pane drew it before its turn could be put in.
const PLUGIN_OFFER: &str = " LSP plugin recommendation\n   Plugin: rust-analyzer-lsp\n   Marketplace: \
                            claude-plugins-official\n   Triggered by: .rs files\n   Would you like to install \
                            this LSP plugin?\n   ❯ 1. Yes, install\n     2. No, not now\n     3. Never for this \
                            plugin\n     4. Disable all LSP recommendations\n";

#[test]
fn the_plugin_offer_is_declined_for_every_plugin_by_moving_down_from_the_cursor() {
  assert_eq!(plugin_offer_keys(PLUGIN_OFFER), Some(vec!["down", "down", "down", "enter"]));
  assert_eq!(
    plugin_offer_keys(
      &PLUGIN_OFFER.replace("❯ 1. Yes", "  1. Yes").replace("  4. Disable", "❯ 4. Disable")
    ),
    Some(vec!["enter"])
  );
  assert_eq!(
    plugin_offer_keys(&PLUGIN_OFFER.replace("LSP plugin recommendation", "A question")),
    None
  );
  assert_eq!(plugin_offer_keys(&PLUGIN_OFFER.replace('❯', " ")), None);
}
