//! The person's supply reads over the own-agents routes and the carry judgement (docs/api.md, 「own agents」;
//! docs/supply.md, 「A closed provider is read as closed, not as absent」 and 「Organisation supply」): the
//! provider rows name who stands each closed — the owner's switch, or the chain with the outermost link that
//! said so — and none of a closed road's models carry a run whatever it would carry open; and a provider's
//! model list leaves out what another person owns and what is filed under a unit the reader does not stand on.

use std::collections::HashMap;
use std::sync::Arc;

use agency_server::control::{Front, router};
use agency_server::places::Ground;
use agency_server::supply;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::support::grounds::ground_with;
use crate::support::scratch::Scratch;

const TOKEN: &str = "secret";

// The account service standing in: `provider.deepseek.offered` closed for every person below unit 3, and unit 4
// on person 7's path alone.
async fn chain_service() -> (String, tokio::task::JoinHandle<()>) {
  let app = axum::Router::new().route(
    "/chain/kinds/{key}/resolve",
    axum::routing::get(
      |axum::extract::Path(key): axum::extract::Path<String>,
       axum::extract::Query(asked): axum::extract::Query<HashMap<String, String>>| async move {
        axum::Json(match (key.as_str(), asked.get("person").map(String::as_str)) {
          ("provider.deepseek.offered", _) => json!({
            "value": false,
            "layers": [{ "link": "platform", "value": true }, { "link": "unit:3", "value": false }],
          }),
          ("supply.organisation", Some("7")) => json!({
            "value": true,
            "layers": [],
            "path": ["platform", "unit:4", "person:7"],
          }),
          _ => json!({ "value": Value::Null, "layers": [] }),
        })
      },
    ),
  );
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());

  (address, tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }))
}

fn ground_on_chain(db: sea_orm::DatabaseConnection, home: &std::path::Path, address: String) -> Ground {
  let vars = std::collections::BTreeMap::from([
    ("WEB_ADDRESS".to_owned(), address),
    ("WEB_TOKEN".to_owned(), "chain".to_owned()),
  ]);

  ground_with(db, home, &vars)
}

async fn read(port: u16, person: &str, path: &str) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(TOKEN)
    .header("Owner-Key", person)
    .header("Owner-Role", "member")
    .send()
    .await
    .unwrap();
  let status = answer.status().as_u16();

  (status, answer.json::<Value>().await.unwrap_or(Value::Null))
}

async fn front(ground: Ground) -> u16 {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let app = router(Front::new(TOKEN.to_owned(), Arc::new(ground)).unwrap());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  port
}

// The provider rows as the person's reads stand them: the owner's switch names `owner`, the chain names
// `chain` with the outermost link that said so, and what stands open names nothing.
#[tokio::test(flavor = "multi_thread")]
async fn the_supply_reads_name_who_stands_each_provider_off() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (address, serving) = chain_service().await;
  let ground = ground_on_chain(scratch.db().clone(), home.path(), address);

  agency_server::control::harness::set_provider_enabled(&ground.db, "openrouter", false)
    .await
    .unwrap();

  let port = front(ground).await;
  let (status, view) = read(port, "7", "/own-agents/7").await;

  assert_eq!(status, 200);

  let held: Vec<(&str, Value, Value)> = view["providers"]
    .as_array()
    .unwrap()
    .iter()
    .map(|one| (one["name"].as_str().unwrap(), one["off"].clone(), one["from"].clone()))
    .collect();

  assert_eq!(
    held,
    [
      ("claude-code", Value::Null, Value::Null),
      ("codex", Value::Null, Value::Null),
      ("openrouter", json!("owner"), Value::Null),
      ("registered", Value::Null, Value::Null),
      ("deepseek", json!("chain"), json!("unit:3")),
    ],
    "the owner's switch stands first; the chain names the outermost link that closed it"
  );

  // The supply read the road chooser opens before the set answers the same, the closed provider shown as
  // closed and not as absent.
  let (status, sources) = read(port, "7", "/supply-sources").await;

  assert_eq!(status, 200);

  let held: Vec<(&str, Value, Value)> = sources["providers"]
    .as_array()
    .unwrap()
    .iter()
    .map(|one| (one["provider"].as_str().unwrap(), one["off"].clone(), one["from"].clone()))
    .collect();

  assert_eq!(
    held,
    [
      ("claude-code", Value::Null, Value::Null),
      ("codex", Value::Null, Value::Null),
      ("openrouter", json!("owner"), Value::Null),
      ("registered", Value::Null, Value::Null),
      ("deepseek", json!("chain"), json!("unit:3")),
    ]
  );

  serving.abort();
}

// A road the chain closes carries nothing for its people, whatever the same person's key would carry while
// the road stands (docs/supply.md, 「A closed provider is read as closed, not as absent」).
#[tokio::test(flavor = "multi_thread")]
async fn a_closed_road_carries_nothing_its_open_key_would_carry() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (address, serving) = chain_service().await;
  let ground = ground_on_chain(scratch.db().clone(), home.path(), address);

  crate::server::supply::personal_key(&ground.db, "deepseek").await;
  crate::server::supply::personal_key(&ground.db, "openrouter").await;

  assert_eq!(
    supply::carries(&ground, 7, "deepseek", &["deepseek-flash".to_owned()]).await.unwrap(),
    vec![false],
    "the closed road carries none of its models, key or no key"
  );
  assert_eq!(
    supply::carries(&ground, 7, "openrouter", &["router-flash".to_owned()]).await.unwrap(),
    vec![true],
    "a road the chain leaves open carries its person's own key still"
  );

  serving.abort();
}

// The models a person may stand their own agent on: what another person owns is left out, a model filed
// under a unit stands before the people whose path names that unit, and a model of no one's stands open to
// all (docs/api.md, 「own agents」).
#[tokio::test(flavor = "multi_thread")]
async fn the_models_of_a_provider_leave_out_what_another_owns_and_what_no_standing_unit_files() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (address, serving) = chain_service().await;
  let ground = ground_on_chain(scratch.db().clone(), home.path(), address);

  for name in ["mine", "open-router", "team-router", "theirs"] {
    scratch
      .run(
        r#"INSERT INTO "registered_models" ("name", "label", "model", "context_tokens", "answer_tokens",
             "thinking", "walked", "added_at") VALUES ($1, $1, 'served', 32768, 4096, 0, 1, 0)"#,
        vec![name.into()],
      )
      .await;
  }

  // Each owner is written whole after the row stands: one account's, one unit's, and one of no one's.
  for (name, column, value) in [
    ("mine", "owner_account", 7i64),
    ("team-router", "owner_unit", 4i64),
    ("theirs", "owner_account", 9),
  ] {
    scratch
      .run(
        &format!(r#"UPDATE "registered_models" SET "{column}" = $1 WHERE "name" = $2"#),
        vec![value.into(), name.into()],
      )
      .await;
  }

  scratch
    .run(
      r#"INSERT INTO "registered_model_addresses" ("registration", "node", "position", "url", "key_file", "key_owned")
         VALUES ('mine', 'n1', 1, 'http://[::1]:9/v1/chat/completions', NULL, 0),
                ('open-router', 'n1', 1, 'http://[::1]:9/v1/chat/completions', NULL, 0),
                ('team-router', 'n1', 1, 'http://[::1]:9/v1/chat/completions', NULL, 0),
                ('theirs', 'n1', 1, 'http://[::1]:9/v1/chat/completions', NULL, 0)"#,
      vec![],
    )
    .await;

  let port = front(ground).await;

  let ids = |view: &Value| {
    view["models"]
      .as_array()
      .unwrap()
      .iter()
      .map(|one| one["id"].as_str().unwrap().to_owned())
      .collect::<Vec<_>>()
  };

  let (status, view) = read(port, "7", "/own-agents/7/models/registered").await;

  assert_eq!(status, 200);
  assert_eq!(
    ids(&view),
    ["mine", "open-router", "team-router"],
    "the person's own, no one's, and the unit 4 one the person's path names — never another person's"
  );

  let (status, view) = read(port, "9", "/own-agents/9/models/registered").await;

  assert_eq!(status, 200);
  assert_eq!(
    ids(&view),
    ["open-router", "theirs"],
    "an account standing under no unit is handed no unit's model"
  );

  serving.abort();
}
