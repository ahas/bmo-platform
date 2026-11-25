use agency_server::control::fault::Fault;
use agency_server::control::registry::*;
use harness_machine::gateway;
use policy::providers::registered::REGISTERED;

#[test]
fn an_address_is_held_to_the_chat_completions_path_over_http() {
  assert_eq!(
    checked_url("http://[::1]:9/v1/chat/completions").ok(),
    Some("http://[::1]:9/v1/chat/completions".to_owned())
  );

  for refusedurl in [
    "http://[::1]:9/v1/models",
    "ftp://host/v1/chat/completions",
    "https://user@host/v1/chat/completions",
    "https://host/v1/chat/completions?x=1",
    "not an address",
  ] {
    assert!(
      matches!(checked_url(refusedurl), Err(Fault::Field(field, _)) if field == "url"),
      "{refusedurl}"
    );
  }
}

#[test]
fn a_key_file_name_never_climbs_out_of_env() {
  assert!(checked_key_file(Some("registered/local-a.key")).is_ok());
  assert!(checked_key_file(None).is_ok());
  assert!(checked_key_file(Some("registered/../bot.env")).is_err());
  assert!(checked_key_file(Some("/etc/passwd")).is_err());
}

#[test]
fn a_registration_is_a_service_on_the_gateway_and_a_mapping_its_settings() {
  let service = OwnLlmService {
    name: PROVIDER,
    label: "Local".into(),
    short_label: "Local".into(),
    base_url: gateway::url(None),
    key_file: REGISTERED.presence.key_file.clone(),
    registration: "local-a".into(),
  };
  let thinking = Mapping {
    model: "local-a".into(),
    thinking: None,
    effort: Some("thinking".into()),
    answer_tokens: Some(2_048),
  };

  assert_eq!(
    own_llm_settings(&service, Some(&thinking)),
    OwnLlmSettings {
      provider: "registered",
      model: "local-a".into(),
      effort: Some("thinking".into()),
      thinking: true,
      answer_tokens: 2_048,
      context_tokens: None,
    }
  );
  assert_eq!(
    own_llm_settings(&service, None),
    OwnLlmSettings {
      provider: "registered",
      model: String::new(),
      effort: None,
      thinking: false,
      answer_tokens: DEFAULT_ANSWER_TOKENS,
      context_tokens: None,
    }
  );
}

#[tokio::test]
async fn an_address_keeps_role_files_as_its_change_names_and_as_it_stands_where_one_names_none() {
  let scratch = crate::support::scratch::Scratch::new().await;
  let db = scratch.db();
  let data = tempfile::tempdir().unwrap();
  let address = |body: serde_json::Value| {
    let mut asked = serde_json::json!({
      "action": "set-address", "name": "nodes", "node": "alpha",
      "url": "http://127.0.0.1:18021/v1/chat/completions",
    });

    asked.as_object_mut().unwrap().extend(body.as_object().unwrap().clone());
    asked
  };

  change(
    db,
    data.path(),
    &serde_json::json!({
      "action": "save", "name": "nodes", "model": "m",
      "contextTokens": 32_768, "answerTokens": 4_096,
    }),
  )
  .await
  .unwrap()
  .unwrap();
  change(db, data.path(), &address(serde_json::json!({}))).await.unwrap().unwrap();
  assert!(!role_files_of(db).await, "a new address keeps none unless it is named");

  change(db, data.path(), &address(serde_json::json!({ "roleFiles": true })))
    .await
    .unwrap()
    .unwrap();
  assert!(role_files_of(db).await);

  let moved = address(serde_json::json!({ "url": "http://127.0.0.1:18022/v1/chat/completions" }));

  change(db, data.path(), &moved).await.unwrap().unwrap();
  assert!(role_files_of(db).await, "a change naming none leaves it as it stands");

  let refused = change(db, data.path(), &address(serde_json::json!({ "roleFiles": "yes" })))
    .await
    .unwrap();

  assert!(refused.is_err(), "{refused:?}");

  let shown = view(db, data.path()).await.unwrap();

  assert!(shown.to_string().contains("\"roleFiles\":true"), "{shown}");
}

async fn role_files_of(db: &sea_orm::DatabaseConnection) -> bool {
  registrations(db)
    .await
    .unwrap()
    .into_iter()
    .find(|one| one.name == "nodes")
    .unwrap()
    .addresses[0]
    .role_files
}

// A role names a registration where any agent of it stands on it (docs/own-llm.md, 「Where it is set」): two
// agents of one role on two registrations name both, and an agent switched off names its own still.
#[tokio::test]
async fn a_role_names_every_registration_an_agent_of_it_stands_on() {
  let scratch = crate::support::scratch::Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = crate::support::grounds::seeded(scratch.db(), home.path()).await;
  let db = &ground.db;

  crate::support::rows::seats(db, "worker", &[("registered", "model-a"), ("registered", "model-b")]).await;
  scratch
    .run(r#"UPDATE "role_agents" SET "enabled" = 0 WHERE "name" = 'worker-02'"#, vec![])
    .await;

  for model in ["model-a", "model-b"] {
    assert!(
      referrers(db, model).await.unwrap().contains(&"role worker".to_owned()),
      "{model} is named by the worker role"
    );
  }

  assert!(!referrers(db, "model-c").await.unwrap().contains(&"role worker".to_owned()));
}
