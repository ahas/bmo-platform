//! The test of a supply source and the address route (docs/supply.md, 「Testing a source」; docs/api.md, 「accounts,
//! rest and usage」 and 「providers, models and model files」), through the agency's own client against a gateway
//! standing in: a person tests their own supply on each road and the platform its own, a provider switched off
//! answers so first, the gateway's answer is read into the outcome a person is told, and nothing is reserved; an
//! address of a registration is set naming a key file of the platform's secret space, by the platform alone.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};

use agency_server::control::harness::set_provider_enabled;
use agency_server::control::registry;
use agency_server::control::supply_test::{Outcome, gateway_outcome};
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use bmo_agency_client::{AgencyClient, ModelAddressAsk, SupplyTestAsk, SupplyTested};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::owned_accounts::{client, front, refused};
use crate::support::grounds::{deepseek_road, ground_with};
use crate::support::scratch::Scratch;

// Each request the gateway standing in was sent: the model it named and the key it carried.
type Sent = Arc<Mutex<Vec<(String, String)>>>;

// A gateway that serves the platform's DeepSeek member and the registration `hy`, refuses `dead`'s key as its
// provider would, and holds no person's member yet.
async fn completed(
  State(sent): State<Sent>,
  headers: HeaderMap,
  Json(body): Json<Value>,
) -> impl IntoResponse {
  let model = body["model"].as_str().unwrap_or_default().to_owned();
  let key = headers
    .get("authorization")
    .and_then(|value| value.to_str().ok())
    .and_then(|value| value.strip_prefix("Bearer "))
    .unwrap_or_default()
    .to_owned();

  sent.lock().unwrap().push((model.clone(), key));

  match model.as_str() {
    "deepseek-flash" | "hy" => {
      (StatusCode::OK, Json(json!({ "choices": [{ "message": { "content": "Hi" } }] })))
    }
    "dead" => (
      StatusCode::UNAUTHORIZED,
      Json(
        json!({ "error": { "message": "Incorrect API key provided", "type": "authentication_error", "code": "401" } }),
      ),
    ),
    _ => (
      StatusCode::BAD_REQUEST,
      Json(json!({ "error": {
        "message": format!("Invalid model name passed in model={model}"),
        "type": "invalid_request_error",
        "code": "400",
      } })),
    ),
  }
}

async fn gateway() -> (String, Sent) {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());
  let sent: Sent = Arc::default();
  let app = axum::Router::new()
    .route("/v1/chat/completions", post(completed))
    .with_state(sent.clone());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  (base, sent)
}

fn ask(road: &str, model: Option<&str>) -> SupplyTestAsk {
  SupplyTestAsk {
    road: road.to_owned(),
    model: model.map(str::to_owned),
    local: None,
  }
}

async fn tested(client: &AgencyClient, road: &str, model: Option<&str>) -> (bool, Option<String>) {
  let answer: SupplyTested = client.test_supply(&ask(road, model)).await.unwrap();

  assert_eq!(answer.road, road);
  assert_eq!(answer.reason.is_some(), answer.reason_code.is_some());

  (answer.ok, answer.reason_code)
}

fn code(word: &str) -> (bool, Option<String>) {
  (false, Some(word.to_owned()))
}

const OK: (bool, Option<String>) = (true, None);

// One call of the platform's answered as it came: its status and its body, the refusal's `field` among it.
async fn raw(port: u16, method: reqwest::Method, path: &str, body: Value) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .request(method, format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth("secret")
    .json(&body)
    .send()
    .await
    .unwrap();
  let status = answer.status().as_u16();

  (status, answer.json().await.unwrap_or(Value::Null))
}

async fn registrations(scratch: &Scratch) {
  scratch.run(r#"DELETE FROM "registered_model_addresses""#, vec![]).await;
  scratch.run(r#"DELETE FROM "registered_models""#, vec![]).await;
  scratch
    .run(
      r#"INSERT INTO "registered_models" ("name", "label", "model", "context_tokens", "answer_tokens",
            "thinking", "walked", "added_at", "keeps_research", "card_service", "role_files")
         VALUES ('hy', 'hy', 'served', 32768, 2048, 0, 0, 1, 0, NULL, 0),
                ('dead', 'dead', 'served', 32768, 2048, 0, 0, 2, 0, NULL, 0),
                ('bare', 'bare', 'served', 32768, 2048, 0, 0, 3, 0, NULL, 0),
                ('carded', 'carded', 'served', 32768, 2048, 0, 0, 4, 0, 'gguf', 0)"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "registered_model_addresses" ("registration", "node", "position", "url", "key_file", "key_owned")
         VALUES ('hy', 'n1', 1, 'http://[::1]:9/v1/chat/completions', NULL, 0),
                ('dead', 'n1', 1, 'http://[::1]:9/v1/chat/completions', NULL, 0)"#,
      vec![],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_tests_their_own_supply_and_the_platform_its_own_and_nothing_is_reserved() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let login = home.path().join("login");
  let (base, sent) = gateway().await;

  deepseek_road(home.path());
  std::fs::create_dir_all(&login).unwrap();
  registrations(&scratch).await;
  // Person 7 holds a Claude account that is on but signed in nowhere, and a key for DeepSeek's road; person 9
  // holds nothing; the platform holds no Claude account.
  scratch.run(r#"DELETE FROM "claude_accounts""#, vec![]).await;
  scratch
    .run(
      r#"INSERT INTO "claude_accounts"
           ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at", "user_id")
         VALUES ('seven', 'seven', 1, 1, 'directory', $1, 0, 7)"#,
      vec![login.to_string_lossy().into_owned().into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('users_administrator', '1')
         ON CONFLICT ("key") DO UPDATE SET "value" = EXCLUDED."value""#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "personal_supply" ("user_id", "road", "secret", "place", "added_at")
         VALUES (7, 'deepseek', 'ds', 1, 0)"#,
      vec![],
    )
    .await;

  let variables = BTreeMap::from([("BMO_LLM_GATEWAY_URL".to_owned(), base)]);
  let port = front(ground_with(scratch.db().clone(), home.path(), &variables)).await;
  let seven = client(port, Some(("7", "member")));
  let nine = client(port, Some(("9", "member")));
  let platform = client(port, None);

  // A person with nothing of their own finds no source on any road, the registry's among them.
  for road in ["claude-code", "codex", "deepseek", "openrouter", "registered"] {
    assert_eq!(tested(&nine, road, None).await, code("no-source"), "{road}");
  }
  assert_eq!(tested(&nine, "registered", Some("hy")).await, code("no-source"));

  // A login on but signed in nowhere waits for its person; a key whose personal member the gateway does not serve
  // yet is asked again later, the request naming that member under the person's own call key.
  assert_eq!(tested(&seven, "claude-code", None).await, code("login-needed"));
  assert_eq!(tested(&seven, "deepseek", None).await, code("not-ready"));

  {
    let sent = sent.lock().unwrap();
    let (model, key) = sent.last().unwrap();

    assert_eq!(model, "deepseek-flash@u7");
    assert!(key.starts_with("bmo-call-"), "the person's call key carries the request");
  }

  // The platform's member answers under the gateway's own key; a registration answers, or its provider refuses its
  // key, or nothing serves it; the platform holds no Claude account.
  assert_eq!(tested(&platform, "deepseek", None).await, OK);
  assert_eq!(sent.lock().unwrap().last().unwrap(), &("deepseek-flash".to_owned(), "a key".to_owned()));
  assert_eq!(tested(&platform, "registered", Some("hy")).await, OK);
  assert_eq!(tested(&platform, "registered", Some("dead")).await, code("refused"));
  assert_eq!(tested(&platform, "registered", Some("bare")).await, code("no-source"));
  assert_eq!(tested(&platform, "claude-code", None).await, code("no-source"));
  assert_eq!(tested(&platform, "codex", None).await, code("no-source"));

  // A road the test does not know, and a registration the platform's test does not name or the registry does not
  // hold, are refused with the field.
  for (body, field) in [
    (json!({ "road": "nowhere" }), "road"),
    (json!({}), "road"),
    (json!({ "road": "registered" }), "model"),
    (json!({ "road": "registered", "model": "none-such" }), "model"),
  ] {
    let (status, answer) = raw(port, reqwest::Method::POST, "/supply-test", body).await;

    assert_eq!((status, answer["field"].as_str()), (400, Some(field)));
  }

  // A provider the owner switched off answers so first, for a person and the platform alike.
  set_provider_enabled(scratch.db(), "deepseek", false).await.unwrap();

  assert_eq!(tested(&platform, "deepseek", None).await, code("off"));
  assert_eq!(tested(&seven, "deepseek", None).await, code("off"));

  // Nothing of it is reserved, settled or marked refused.
  for table in ["supply_reservations", "supply_settlements"] {
    let count = scratch.count(&format!(r#"SELECT COUNT(*) AS "count" FROM "{table}""#), vec![]).await;

    assert_eq!(count, 0, "{table}");
  }
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) AS "count" FROM "personal_supply" WHERE "refused_at" IS NOT NULL"#,
        vec![]
      )
      .await,
    0
  );
}

#[test]
fn the_gateway_s_answer_is_read_into_what_a_person_is_told() {
  let error = |message: &str, kind: &str, code: &str| json!({ "error": { "message": message, "type": kind, "code": code } });

  assert_eq!(gateway_outcome(200, &json!({ "choices": [] })), Outcome::Answers);
  // A person's member the launcher has not laid yet: the front judges it by the platform's beside it.
  assert_eq!(
    gateway_outcome(402, &error("no open reservation", "insufficient_quota", "no-supply")),
    Outcome::NotReady
  );
  assert_eq!(
    gateway_outcome(403, &error("an account's own", "permission_error", "not-your-member")),
    Outcome::NotReady
  );
  assert_eq!(
    gateway_outcome(400, &error("Invalid model name passed in model=m@u7", "invalid_request_error", "400")),
    Outcome::NotReady
  );
  assert_eq!(
    gateway_outcome(404, &error("model not found", "not_found_error", "404")),
    Outcome::NotReady
  );
  // The provider refusing the key.
  for (status, kind) in
    [(401, "authentication_error"), (402, "insufficient_quota"), (403, "permission_error")]
  {
    assert!(
      matches!(gateway_outcome(status, &error("no", kind, "x")), Outcome::Refused(_)),
      "{status}"
    );
  }
  assert!(matches!(
    gateway_outcome(400, &error("bad key", "authentication_error", "400")),
    Outcome::Refused(_)
  ));
  // The front refusing the key the test sent, a failure behind it and a body that is no JSON reach no answer.
  assert!(matches!(
    gateway_outcome(401, &error("The gateway's key is missing or wrong", "authentication_error", "401")),
    Outcome::Failed(_)
  ));
  assert!(matches!(gateway_outcome(500, &error("boom", "api_error", "500")), Outcome::Failed(_)));
  assert!(matches!(
    gateway_outcome(503, &json!("the gateway has no table to serve yet")),
    Outcome::Failed(_)
  ));
  assert!(matches!(
    gateway_outcome(429, &error("rate limited", "rate_limit_error", "429")),
    Outcome::Failed(_)
  ));

  assert_eq!(Outcome::Answers.code(), None);
  assert_eq!(Outcome::NoAddress("hy".to_owned()).code(), Some("no-source"));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_address_names_a_key_file_of_the_platform_s_space_set_by_the_platform_alone() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let env = data.join("env");

  std::fs::create_dir_all(&env).unwrap();
  registrations(&scratch).await;

  // An address whose key agency wrote, which naming a key file in its place takes away.
  let given = json!({
    "action": "set-address", "name": "hy", "node": "n2", "url": "http://[::1]:9/v1/chat/completions", "key": "a-key",
  });

  assert!(registry::made(scratch.db(), &data, &given).await.is_ok());

  let owned = env.join("registered").join("hy-n2.key");

  assert!(owned.exists());

  let port = front(ground_with(scratch.db().clone(), home.path(), &BTreeMap::new())).await;
  let platform = client(port, None);
  let address = |key_file: Option<&str>| ModelAddressAsk {
    url: "http://[::1]:9/v1/chat/completions".to_owned(),
    key_file: key_file.map(str::to_owned),
  };

  // The console wrote the secret; agency names it and never touches it.
  let secret = env.join("hy-secret");

  std::fs::write(&secret, "a key\n").unwrap();
  std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).unwrap();

  let model = platform.set_model_address("hy", "n2", &address(Some("hy-secret"))).await.unwrap();

  assert_eq!(model.name, "hy");
  assert!(!owned.exists(), "the key agency wrote is taken away");

  let held = registry::registration(scratch.db(), "hy").await.unwrap().unwrap();
  let n2 = held.addresses.iter().find(|one| one.node == "n2").unwrap();

  assert_eq!((n2.key_file.as_deref(), n2.key_owned), (Some("hy-secret"), false));

  // A new address with no key, then the named file left where it stands once the address names none.
  platform.set_model_address("registered::hy", "n3", &address(None)).await.unwrap();
  platform.set_model_address("hy", "n2", &address(None)).await.unwrap();

  let held = registry::registration(scratch.db(), "hy").await.unwrap().unwrap();

  assert_eq!(
    held
      .addresses
      .iter()
      .map(|one| (one.node.as_str(), one.key_file.clone()))
      .collect::<Vec<_>>(),
    [("n1", None), ("n2", None), ("n3", None)]
  );
  assert!(secret.exists(), "a key file agency did not write stays");

  // A registration the registry does not hold is none, and what the registry refuses is refused with its field;
  // a member's call is refused before any of it (`access`).
  assert_eq!(refused(platform.set_model_address("none-such", "n1", &address(None)).await).0, 404);
  assert_eq!(refused(platform.set_model_address("codex::gpt", "n1", &address(None)).await).0, 404);

  for (path, body, field) in [
    ("/models/hy/addresses/n2", json!({ "url": "http://[::1]:9/v1/models" }), "url"),
    ("/models/hy/addresses/n2", json!({}), "url"),
    (
      "/models/hy/addresses/n2",
      json!({ "url": "http://[::1]:9/v1/chat/completions", "keyFile": "../escape" }),
      "keyFile",
    ),
    (
      "/models/hy/addresses/n2",
      json!({ "url": "http://[::1]:9/v1/chat/completions", "keyFile": "users/7/ds" }),
      "keyFile",
    ),
    (
      "/models/hy/addresses/Bad_Node",
      json!({ "url": "http://[::1]:9/v1/chat/completions" }),
      "node",
    ),
    (
      "/models/carded/addresses/n1",
      json!({ "url": "http://[::1]:9/v1/chat/completions" }),
      "node",
    ),
  ] {
    let (status, answer) = raw(port, reqwest::Method::PUT, path, body).await;

    assert_eq!((status, answer["field"].as_str()), (400, Some(field)), "{path}");
  }
}
