//! The registered models: present while the gateway's key file is private and holds a key; and the road a
//! standing Claude Code program is on, read from its records home and its base URL.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;

use harness_providers::registered::*;
use harness_providers::{Machine, Provider, standing_provider};

#[test]
fn the_key_file_answers_only_while_it_is_private_and_holds_a_key() {
  let dir = tempfile::tempdir().unwrap();
  let machine = Machine::with_env([], dir.path());
  let key = Registered::key_file(&machine);

  assert!(Registered.missing(&machine).is_some());

  std::fs::create_dir_all(key.parent().unwrap()).unwrap();
  std::fs::write(&key, "secret\n").unwrap();
  std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
  assert!(Registered.missing(&machine).is_some());

  std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
  assert_eq!(Registered.missing(&machine), None);

  std::fs::write(&key, "").unwrap();
  assert!(Registered.missing(&machine).is_some());
}

// The provider that owns a Claude Code program standing with this environment.
fn owner(machine: &Machine, home: &std::path::Path, base_url: Option<&str>) -> &'static str {
  let mut env = BTreeMap::from([("CLAUDE_CONFIG_DIR".to_owned(), home.to_string_lossy().into_owned())]);

  if let Some(url) = base_url {
    env.insert("ANTHROPIC_BASE_URL".to_owned(), url.to_owned());
  }

  standing_provider(machine, &["claude".to_owned()], &env).unwrap().name()
}

#[test]
fn the_gateway_program_is_the_registrys_and_a_deepseek_accounts_is_claude_codes() {
  let dir = tempfile::tempdir().unwrap();
  let gateway = "http://gateway.test:9000";
  let machine = Machine::with_env([("BMO_LLM_GATEWAY_URL".to_owned(), gateway.to_owned())], dir.path());
  let registration = Registered::homes(&machine).join("member");
  let account = dir.path().join("env").join("deepseek");

  // A registration's own home, and a program handed no such home but pointed at the gateway's address.
  assert_eq!(owner(&machine, &registration, Some(gateway)), "registered");
  assert_eq!(owner(&machine, &account, Some(&format!("{gateway}/"))), "registered");

  // An endpoint of a Claude account's own — a DeepSeek one — under that account's home stays Claude Code's.
  assert_eq!(owner(&machine, &account, Some("https://api.deepseek.com/anthropic")), "claude-code");
  assert_eq!(owner(&machine, &account, None), "claude-code");

  // A registration's home names its road whichever endpoint it is pointed at.
  assert_eq!(owner(&machine, &registration, Some("https://api.deepseek.com/anthropic")), "registered");
}

// A registration's effort words say whether it thinks, which Claude Code says as `medium` and `low`; a word
// that is no registration's has no Claude Code word (docs/own-llm.md, 「How requests go」).
#[test]
fn a_registrations_thinking_words_are_claude_codes_medium_and_low() {
  assert_eq!(Registered.claude_effort("thinking"), Some("medium"));
  assert_eq!(Registered.claude_effort("no-thinking"), Some("low"));
  assert_eq!(Registered.claude_effort("high"), None);
}
