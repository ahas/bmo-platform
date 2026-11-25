use agency_server::control::claude_account_changes::BasePromptGiven;
use agency_server::control::claude_accounts::CredentialKind;
use agency_server::control::fallback::*;
use serde_json::json;

#[test]
fn a_credential_is_sent_as_one_of_its_kinds_and_a_path() {
  let given = credential_given(&json!({ "kind": "endpoint", "path": "/x" })).unwrap();

  assert_eq!(given.kind, Some(CredentialKind::Endpoint));
  assert_eq!(given.path.as_deref(), Some("/x"));
  assert!(credential_given(&json!({ "kind": "directory" })).is_ok());
  assert!(credential_given(&json!({})).unwrap().is_empty());
  assert!(credential_given(&json!({ "kind": null })).is_err());
  assert!(credential_given(&json!({ "kind": "cookie" })).is_err());
  assert!(credential_given(&json!({ "path": 3 })).is_err());
}

#[test]
fn an_endpoint_s_parts_and_a_base_prompt_are_read_from_the_body() {
  let given = credential_given(&json!({
    "kind": "endpoint",
    "path": "/home",
    "endpoint": "https://api.deepseek.com/anthropic",
    "keyFile": "/keys/deepseek",
    "models": { "opus": "deepseek-v4-pro", "sonnet": "deepseek-flash" }
  }))
  .unwrap();

  assert_eq!(given.endpoint.as_deref(), Some("https://api.deepseek.com/anthropic"));
  assert_eq!(given.key_file.as_deref(), Some("/keys/deepseek"));

  let models = given.models.expect("the model map is read");

  assert_eq!(models.opus.as_deref(), Some("deepseek-v4-pro"));
  assert_eq!(models.sonnet.as_deref(), Some("deepseek-flash"));
  assert_eq!(models.haiku, None);

  assert!(credential_given(&json!({ "endpoint": 3 })).is_err());
  assert!(credential_given(&json!({ "keyFile": 3 })).is_err());
  assert!(credential_given(&json!({ "models": "opus=x" })).is_err());
  assert!(credential_given(&json!({ "models": { "opus": 4 } })).is_err());
  assert!(credential_given(&json!({ "models": { "large": "x" } })).is_err());

  assert_eq!(base_prompt_given(&json!({})).unwrap(), BasePromptGiven::Keep);
  assert_eq!(base_prompt_given(&json!({ "basePrompt": null })).unwrap(), BasePromptGiven::None);
  assert_eq!(base_prompt_given(&json!({ "basePrompt": "none" })).unwrap(), BasePromptGiven::None);
  assert_eq!(
    base_prompt_given(&json!({ "basePrompt": "dsh" })).unwrap(),
    BasePromptGiven::Named("dsh".into())
  );
  assert!(base_prompt_given(&json!({ "basePrompt": 2 })).is_err());
}
