//! The base prompts an account names (docs/base-prompts.md, 「Saving」): a prompt an account names is
//! listed with the account, cannot be removed while it does, and an account names a prompt that is kept
//! or none. Saving the text of a base an account names asks no reset for the account.

use agency_server::control::claude_account_changes::{self as changes, BasePromptGiven, CredentialGiven};
use agency_server::control::claude_accounts::accounts;
use serde_json::{Value, json};

use crate::support::rows::repository;
use crate::support::scratch::Scratch;
use crate::support::standing;

async fn get(port: u16, token: &str, path: &str) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

async fn patch(port: u16, token: &str, path: &str, body: Value) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .patch(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .json(&body)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

fn directory(home: &std::path::Path) -> CredentialGiven {
  CredentialGiven {
    path: Some(home.to_string_lossy().into_owned()),
    ..CredentialGiven::default()
  }
}

async fn create(port: u16, token: &str, name: &str) {
  let (status, answer) = patch(
    port,
    token,
    &format!("/prompt-documents/{name}"),
    json!({ "action": "create", "name": name, "label": "Harness based", "text": "You are {{model}}." }),
  )
  .await;

  assert_eq!(status, 200, "{answer}");
}

async fn held(port: u16, token: &str, name: &str) -> Value {
  get(port, token, &format!("/prompt-documents/{name}")).await.1
}

async fn base_of(scratch: &Scratch, id: &str) -> Option<String> {
  accounts(scratch.db())
    .await
    .unwrap()
    .into_iter()
    .find(|one| one.id == id)
    .and_then(|one| one.base_prompt)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_prompt_an_account_names_is_listed_with_it_and_cannot_be_removed() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let logins = tempfile::tempdir().unwrap();
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  create(port, token, "harness-base").await;
  changes::add(
    scratch.db(),
    "deep",
    &directory(logins.path()),
    BasePromptGiven::Named("harness-base".into()),
  )
  .await
  .unwrap();

  // The listing reports the account beside the agents' choices.
  let (status, listing) = get(port, token, "/prompt-documents").await;
  assert_eq!(status, 200);

  let document = listing["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|item| item["name"] == "harness-base")
    .unwrap();

  assert_eq!(document["accounts"], json!([{ "id": "deep", "name": "deep" }]));
  assert_eq!(document["choosers"], json!([]));

  // An account naming it keeps it, and the refusal names the account.
  let (status, refused) =
    patch(port, token, "/prompt-documents/harness-base", json!({ "action": "remove" })).await;
  assert_eq!(status, 400);
  assert_eq!(refused["error"], "It cannot be removed while accounts name it: deep.");
  assert_eq!(held(port, token, "harness-base").await["text"], "You are {{model}}.");

  // An agent choosing it is named beside the account.
  scratch
    .run(
      r#"INSERT INTO "agent_base_prompts" ("agent", "prompt") VALUES ('w', 'harness-base')"#,
      vec![],
    )
    .await;

  let (status, refused) =
    patch(port, token, "/prompt-documents/harness-base", json!({ "action": "remove" })).await;
  assert_eq!(status, 400);
  assert_eq!(
    refused["error"],
    "It cannot be removed while agents choose it or accounts name it: agents w; accounts deep."
  );

  // With no agent choosing it, the account alone keeps it.
  scratch.run(r#"DELETE FROM "agent_base_prompts" WHERE "agent" = 'w'"#, vec![]).await;

  let (status, refused) =
    patch(port, token, "/prompt-documents/harness-base", json!({ "action": "remove" })).await;
  assert_eq!(status, 400);
  assert_eq!(refused["error"], "It cannot be removed while accounts name it: deep.");

  // Once the account names none, it goes.
  changes::update(scratch.db(), "deep", None, None, None, BasePromptGiven::None)
    .await
    .unwrap();
  assert_eq!(base_of(&scratch, "deep").await, None);

  let (status, _) =
    patch(port, token, "/prompt-documents/harness-base", json!({ "action": "remove" })).await;
  assert_eq!(status, 200);
  assert_eq!(get(port, token, "/prompt-documents/harness-base").await.0, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_account_names_a_kept_base_prompt_or_none() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let logins = tempfile::tempdir().unwrap();
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  // A prompt that is not kept cannot be named, and no account is written.
  let refused = changes::add(
    scratch.db(),
    "deep",
    &directory(logins.path()),
    BasePromptGiven::Named("nowhere".into()),
  )
  .await
  .unwrap_err();

  assert_eq!(refused.error, "There is no base prompt nowhere for the account to name.");
  assert!(accounts(scratch.db()).await.unwrap().iter().all(|one| one.name != "deep"));

  create(port, token, "harness-base").await;
  changes::add(
    scratch.db(),
    "deep",
    &directory(logins.path()),
    BasePromptGiven::Named(" harness-base ".into()),
  )
  .await
  .unwrap();
  assert_eq!(base_of(&scratch, "deep").await.as_deref(), Some("harness-base"));

  // A change to a prompt that is not kept is refused and the base the account holds stands.
  let refused =
    changes::update(scratch.db(), "deep", None, None, None, BasePromptGiven::Named("nowhere".into()))
      .await
      .unwrap_err();

  assert_eq!(refused.error, "There is no base prompt nowhere for the account to name.");
  assert_eq!(base_of(&scratch, "deep").await.as_deref(), Some("harness-base"));

  // A change that gives no base keeps the one it holds; none clears it.
  changes::update(scratch.db(), "deep", None, None, None, BasePromptGiven::Keep)
    .await
    .unwrap();
  assert_eq!(base_of(&scratch, "deep").await.as_deref(), Some("harness-base"));

  changes::update(scratch.db(), "deep", None, None, None, BasePromptGiven::None)
    .await
    .unwrap();
  assert_eq!(base_of(&scratch, "deep").await, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn saving_the_text_of_a_base_an_account_names_asks_no_reset_for_the_account() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let logins = tempfile::tempdir().unwrap();
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  create(port, token, "harness-base").await;
  changes::add(
    scratch.db(),
    "deep",
    &directory(logins.path()),
    BasePromptGiven::Named("harness-base".into()),
  )
  .await
  .unwrap();

  // A repository's checkout choosing it and keeping a session there, so a save reaches one agent.
  repository(scratch.db(), "w", &home.path().join("w").to_string_lossy(), &[]).await;
  scratch
    .run(
      r#"INSERT INTO "agent_base_prompts" ("agent", "prompt") VALUES ('w', 'harness-base')"#,
      vec![],
    )
    .await;
  agency_server::worker_session::records::record(
    scratch.db(),
    "w",
    "claude-opus-5-5",
    "",
    "session",
    None,
    None,
  )
  .await
  .unwrap();

  let (status, answer) = patch(
    port,
    token,
    "/prompt-documents/harness-base",
    json!({ "action": "save", "text": "You are {{model}}, revised." }),
  )
  .await;

  assert_eq!(status, 200, "{answer}");
  assert_eq!(held(port, token, "harness-base").await["text"], "You are {{model}}, revised.");

  // The agent's session is asked afresh; the account, which names the prompt, is asked nothing.
  let asked = scratch
    .count(r#"SELECT COUNT(*) FROM "session_resets" WHERE "agent" = 'w'"#, vec![])
    .await;
  let for_account = scratch
    .count(
      r#"SELECT COUNT(*) FROM "session_resets" WHERE "agent" IN ('deep', 'harness-base')"#,
      vec![],
    )
    .await;

  assert!(asked > 0);
  assert_eq!(for_account, 0);
}
