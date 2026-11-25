//! DeepSeek: present while the gateway's key file and its member's key file are private and hold keys; its
//! models and the words a run of one may go out with; and Claude Code under its road's home as its own.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use harness_providers::deepseek::*;
use harness_providers::{Machine, Provider, standing_provider};
use serde_json::{Value, json};

fn private(path: &Path, text: &str) {
  std::fs::create_dir_all(path.parent().unwrap()).unwrap();
  std::fs::write(path, text).unwrap();
  std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

fn member(name: &str, label: &str) -> Value {
  json!({
    "name": name,
    "road": "deepseek",
    "label": label,
    "provider": "deepseek",
    "model": name,
    "api_base": "https://api.deepseek.com/v1",
    "key_file": "deepseek.key",
    "context_tokens": 1048576,
    "answer_tokens": 32768,
    "thinking": true
  })
}

fn machine_with_members(dir: &Path, members: Value) -> Machine {
  let machine = Machine::with_env([], dir);
  private(&dir.join("env").join("llm-gateway.key"), "gateway\n");
  private(&dir.join("env").join("deepseek.key"), "secret\n");
  std::fs::create_dir_all(dir.join("llm-gateway")).unwrap();
  std::fs::write(dir.join("llm-gateway").join("members.json"), members.to_string()).unwrap();

  machine
}

#[test]
fn the_key_files_answer_only_while_they_are_private_and_hold_keys() {
  let dir = tempfile::tempdir().unwrap();
  let machine = Machine::with_env([], dir.path());

  assert!(DeepSeek.missing(&machine).is_some());

  let gateway = dir.path().join("env").join("llm-gateway.key");
  let key = DeepSeek::key_file(&machine);
  std::fs::create_dir_all(dir.path().join("llm-gateway")).unwrap();
  std::fs::write(
    dir.path().join("llm-gateway").join("members.json"),
    json!([member("deepseek-flash", "DeepSeek V4.1 Flash")]).to_string(),
  )
  .unwrap();
  private(&gateway, "gateway\n");
  private(&key, "secret\n");
  assert_eq!(DeepSeek.missing(&machine), None);

  std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
  assert!(DeepSeek.missing(&machine).is_some());

  std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
  std::fs::write(&key, "").unwrap();
  assert!(DeepSeek.missing(&machine).is_some());
}

#[tokio::test]
async fn the_catalogue_is_the_members_its_road_holds() {
  let dir = tempfile::tempdir().unwrap();
  let machine = machine_with_members(
    dir.path(),
    json!([
      member("deepseek-flash", "DeepSeek V4.1 Flash"),
      member("deepseek-v4-pro", "DeepSeek V4 Pro")
    ]),
  );
  let catalogue = DeepSeek.catalogue(&machine, None).await.unwrap();
  let ids: Vec<&str> = catalogue["models"]
    .as_array()
    .unwrap()
    .iter()
    .filter_map(|one| one["id"].as_str())
    .collect();

  assert_eq!(ids, ["deepseek-flash", "deepseek-v4-pro"]);

  let offered = DeepSeek.offered(&machine, None, 0.0).await.unwrap();

  assert_eq!(offered.models.len(), 2);
  assert_eq!(
    offered.models[0].efforts,
    [
      None,
      Some("none".into()),
      Some("low".into()),
      Some("medium".into()),
      Some("high".into()),
      Some("max".into())
    ]
  );
}

#[test]
fn a_run_without_thinking_names_its_own_off_word_and_a_level_stands_where_the_model_takes_it() {
  let levels = vec!["low".to_owned(), "medium".to_owned(), "high".to_owned()];

  assert_eq!(
    DeepSeek.run_effort(Some("high"), Some(false), Some("low"), &levels),
    Some("none".into())
  );
  assert_eq!(
    DeepSeek.run_effort(Some("high"), Some(true), Some("medium"), &levels),
    Some("medium".into())
  );
  assert_eq!(DeepSeek.run_effort(Some("max"), Some(true), Some("xhigh"), &levels), Some("max".into()));
  assert_eq!(DeepSeek.run_effort(Some("low"), None, None, &levels), Some("low".into()));
  assert_eq!(DeepSeek.claude_effort("high"), Some("high"));
  assert_eq!(DeepSeek.claude_effort("none"), Some("low"));
}

#[test]
fn claude_code_under_its_road_s_home_is_this_provider_s_and_another_road_s_is_not() {
  let dir = tempfile::tempdir().unwrap();
  let machine = Machine::with_env([], dir.path());
  let owner = |home: &str| {
    let env = BTreeMap::from([("CLAUDE_CONFIG_DIR".to_owned(), home.to_owned())]);

    standing_provider(&machine, &["claude".to_owned()], &env).map(|provider| provider.name())
  };
  let homes = dir.path().join("claude-roads");
  let deepseek = homes.join("deepseek").join("deepseek-flash");
  let openrouter = homes.join("openrouter").join("vendor-model");

  assert_eq!(owner(&deepseek.to_string_lossy()), Some("deepseek"));
  assert_eq!(owner(&openrouter.to_string_lossy()), Some("openrouter"));
  assert_eq!(owner("/somewhere/else"), Some("claude-code"));
}
