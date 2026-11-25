//! Codex: its member road, the CLI catalogue a chooser still reads, and its failures.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use harness_providers::codex::*;
use harness_providers::{Headline, Machine, Provider, Window};
use serde_json::{Value, json};

fn private(path: &Path, text: &str) {
  std::fs::create_dir_all(path.parent().unwrap()).unwrap();
  std::fs::write(path, text).unwrap();
  std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

fn member(name: &str, label: &str) -> Value {
  json!({
    "name": name,
    "road": "codex",
    "label": label,
    "provider": "openai",
    "model": name,
    "api_base": "https://api.openai.com/v1",
    "key_file": "openai.key",
    "context_tokens": 400000,
    "answer_tokens": 32768,
    "thinking": true
  })
}

fn machine_with_members(dir: &Path, members: Value) -> Machine {
  let machine = Machine::with_env([], dir);
  private(&dir.join("env").join("llm-gateway.key"), "gateway\n");
  private(&dir.join("env").join("openai.key"), "secret\n");
  std::fs::create_dir_all(dir.join("llm-gateway")).unwrap();
  std::fs::write(dir.join("llm-gateway").join("members.json"), members.to_string()).unwrap();

  machine
}

#[test]
fn a_member_road_reads_no_window() {
  let window = |name, duration_minutes| Window {
    name,
    duration_minutes,
  };

  assert_eq!(Codex.headline(&[window("codex:codex:primary", Some(300))]), Headline::default());
  assert_eq!(Codex.headline(&[]), Headline::default());
}

/// keeps visible chooser fields and leaves hidden compatibility slugs out
#[test]
fn keeps_visible_chooser_fields_and_leaves_hidden_compatibility_slugs_out() {
  let read = read_models(
    r#"{"models":[
      {"slug":"gpt-6-astra","display_name":"GPT-6-Astra","description":"Most capable","context_window":1050000,
       "default_reasoning_level":"low","supported_reasoning_levels":[{"effort":"low"},{"effort":"medium"},{"other":1}],"visibility":"list"},
      {"slug":"old-alias","visibility":"hide"},{"slug":""},{"display_name":"no slug"},{"slug":"bare"}]}"#,
  )
  .unwrap();

  assert_eq!(
    read,
    [
      Model {
        id: "gpt-6-astra".into(),
        name: "GPT-6-Astra".into(),
        description: "Most capable".into(),
        context_length: Some(1_050_000),
        default_reasoning_effort: "low".into(),
        supported_reasoning_efforts: vec!["low".into(), "medium".into()],
      },
      Model {
        id: "bare".into(),
        name: "bare".into(),
        description: String::new(),
        context_length: None,
        default_reasoning_effort: "medium".into(),
        supported_reasoning_efforts: vec![],
      },
    ]
  );
}

/// refuses an output with no catalogue rather than showing an empty lie
#[test]
fn refuses_an_output_with_no_catalogue_rather_than_showing_an_empty_lie() {
  assert_eq!(read_models("{}").unwrap_err(), "codex models: the CLI returned no model list");
}

#[test]
fn a_failure_names_the_last_error_reported_then_the_last_word_said() {
  assert_eq!(
    failure(
      "{\"type\":\"error\",\"message\":\"one\"}\n{\"type\":\"turn.failed\",\"error\":{\"message\":\"two\"}}",
      "x",
      Some(1)
    ),
    "two"
  );
  assert_eq!(failure("", "first\nlast\n\n", Some(1)), "last");
  assert_eq!(failure("", "", Some(3)), "exit 3");
}

#[tokio::test]
async fn the_cli_catalogue_is_read_for_the_chooser_that_declares_a_member() {
  let dir = tempfile::tempdir().unwrap();
  let program = dir.path().join("codex");

  std::fs::write(&program, "#!/bin/sh\n[ \"$*\" = \"debug models --bundled\" ] || exit 2\necho '{\"models\":[{\"slug\":\"gpt-5.6-sol\"}]}'\n").unwrap();
  std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();

  let machine = Machine::with_env([("PATH".to_owned(), dir.path().display().to_string())], dir.path());

  assert_eq!(models(&machine).await.unwrap()[0].id, "gpt-5.6-sol");
}

#[tokio::test]
async fn the_catalogue_is_the_members_its_road_holds() {
  let dir = tempfile::tempdir().unwrap();
  let machine = machine_with_members(
    dir.path(),
    json!([member("gpt-5.6-sol", "GPT-5.6 Sol"), member("gpt-5.6-terra", "GPT-5.6 Terra")]),
  );
  let catalogue = Codex.catalogue(&machine, None).await.unwrap();
  let ids: Vec<&str> = catalogue["models"]
    .as_array()
    .unwrap()
    .iter()
    .filter_map(|one| one["id"].as_str())
    .collect();

  assert_eq!(ids, ["gpt-5.6-sol", "gpt-5.6-terra"]);
}

#[test]
fn the_key_files_answer_only_while_they_are_private_and_hold_keys() {
  let dir = tempfile::tempdir().unwrap();
  let machine = Machine::with_env([], dir.path());

  assert_eq!(Codex.missing(&machine).unwrap().code, "codex-key-unusable");

  private(&dir.path().join("env").join("llm-gateway.key"), "gateway\n");
  private(&dir.path().join("env").join("openai.key"), "secret\n");
  std::fs::create_dir_all(dir.path().join("llm-gateway")).unwrap();
  std::fs::write(
    dir.path().join("llm-gateway").join("members.json"),
    json!([member("gpt-5.6-sol", "GPT-5.6 Sol")]).to_string(),
  )
  .unwrap();

  assert_eq!(Codex.missing(&machine), None);

  std::fs::set_permissions(
    dir.path().join("env").join("openai.key"),
    std::fs::Permissions::from_mode(0o644),
  )
  .unwrap();
  assert!(Codex.missing(&machine).is_some());
}

#[test]
fn claude_code_under_its_road_s_home_is_its_own() {
  let dir = tempfile::tempdir().unwrap();
  let machine = Machine::with_env([], dir.path());
  let homes = dir.path().join("claude-roads");
  let env = BTreeMap::from([(
    "CLAUDE_CONFIG_DIR".to_owned(),
    homes.join("codex").join("gpt-5.6-sol").to_string_lossy().into_owned(),
  )]);

  assert!(Codex.owns_program(&machine, &["claude".to_owned()], &env));
  assert!(!Codex.owns_program(&machine, &["codex".to_owned()], &BTreeMap::new()));
}
