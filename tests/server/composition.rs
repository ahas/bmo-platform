//! The agents an account is given (docs/role-agents.md, 「The agents an account is given」): what stands for a role
//! read off the chain's layers, and the agents made for a person — the platform's own rule where nothing is set,
//! a role off or given no agents where a unit of their path says so, and nothing made twice.

use std::collections::{BTreeMap, HashMap};

use agency_server::organisation::composition::{Road, compose, standing};
use agency_server::role_agents::{self, RoleAgent};
use axum::extract::{Path as Route, Query};
use serde_json::{Value, json};

use crate::support::grounds::ground_with;
use crate::support::scratch::Scratch;

const PERSON: i64 = 7;

#[test]
fn a_role_is_off_where_any_link_says_so_and_its_agents_are_the_nearest_link_s() {
  let road = |model: &str| json!({ "provider": "deepseek", "model": model });
  let answer = json!({ "layers": [
    { "link": "platform", "value": { "roles": { "making": { "agents": [road("flash")] } } } },
    { "link": "unit:3", "value": { "roles": { "reading": { "off": true } } } },
    { "link": "unit:4", "value": { "roles": { "making": { "agents": [road("pro"), road("flash")] } } } },
  ] });
  let stood = standing(Some(&answer));
  let making = &stood["making"];

  assert!(!making.off);
  assert_eq!(making.from.as_deref(), Some("unit:4"));
  assert_eq!(
    making
      .agents
      .as_deref()
      .map(|agents| agents.iter().map(|one| one.model.as_deref()).collect::<Vec<_>>()),
    Some(vec![Some("pro"), Some("flash")])
  );
  assert!(stood["reading"].off);
  assert_eq!(stood["reading"].agents, None::<Vec<Road>>);
  assert!(standing(None).is_empty(), "nothing said leaves every role to the platform's rule");
}

async fn account_service(layers: Value) -> (String, tokio::task::JoinHandle<()>) {
  let app = axum::Router::new()
    .route(
      "/chain/kinds/{key}",
      axum::routing::put(|axum::Json(body): axum::Json<Value>| async move { axum::Json(body) }),
    )
    .route(
      "/chain/kinds/{key}/resolve",
      axum::routing::get(
        move |Route(key): Route<String>, Query(_asked): Query<HashMap<String, String>>| {
          let layers = layers.clone();

          async move {
            let said = if key == "signup.composition" { layers } else { json!([]) };

            axum::Json(json!({ "layers": said }))
          }
        },
      ),
    );
  let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());

  (address, tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }))
}

// Two roles, `making` with an agent of no account standing as its template and `reading` with none of its own, and
// `checking` with its own template too.
async fn roles(scratch: &Scratch) {
  scratch
    .run(
      r#"INSERT INTO "roles" ("name", "label", "bundles", "max_concurrent", "position", "created_at", "updated_at",
           "line", "provider_order", "fold_ratio")
         VALUES ('making', 'Making', '[]', 1, 0, 1, 1, 'worker', '[]', 0.5),
                ('reading', 'Reading', '[]', 1, 1, 1, 1, 'worker', '[]', 0.5),
                ('checking', 'Checking', '[]', 1, 2, 1, 1, 'worker', '[]', 0.5)"#,
      vec![],
    )
    .await;

  for role in ["making", "checking"] {
    role_agents::write(scratch.db(), &template(role)).await.unwrap();
  }
}

fn template(role: &str) -> RoleAgent {
  RoleAgent {
    name: format!("{role}-01"),
    role: role.to_owned(),
    provider: None,
    model: None,
    effort: None,
    thinking: None,
    answer_tokens: None,
    advisor: None,
    enabled: true,
    keeps_cache: true,
    sessions: policy::roles::Sessions::default(),
    fold_ratio: None,
    prompt: Some(format!("The {role} template's own words.")),
    bundles: None,
    skills: None,
    added_at: 1,
    owner_account: None,
    owner_unit: None,
    home_node: None,
    follows_asker: None,
  }
}

async fn made_for(layers: Value) -> Vec<RoleAgent> {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (address, serving) = account_service(layers).await;
  let vars =
    BTreeMap::from([("WEB_ADDRESS".to_owned(), address), ("WEB_TOKEN".to_owned(), "chain".to_owned())]);
  let ground = ground_with(scratch.db().clone(), home.path(), &vars);

  roles(&scratch).await;

  let made = compose(&ground, PERSON).await.unwrap();

  assert!(compose(&ground, PERSON).await.unwrap().is_empty(), "asked again, nothing is made twice");
  serving.abort();
  made
}

#[tokio::test(flavor = "multi_thread")]
async fn with_nothing_set_every_role_with_a_template_gives_one_agent_carrying_its_settings() {
  let made = made_for(json!([])).await;
  let named: Vec<(&str, &str)> =
    made.iter().map(|agent| (agent.role.as_str(), agent.name.as_str())).collect();

  assert_eq!(named, [("making", "making-u7"), ("checking", "checking-u7")]);
  assert_eq!(made[0].owner_account, Some(7));
  assert_eq!(made[0].prompt.as_deref(), Some("The making template's own words."));
  assert!(made[0].keeps_cache);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_unit_turns_a_role_off_and_gives_another_no_agents() {
  let made = made_for(json!([
    { "link": "unit:3", "value": { "roles": { "making": { "off": true } } } },
    { "link": "unit:4", "value": { "roles": { "checking": { "agents": [] } } } },
  ]))
  .await;

  assert!(made.is_empty(), "{made:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn agents_naming_no_road_are_given_on_the_template_s_road_numbered_from_the_second() {
  let made = made_for(json!([
    { "link": "unit:4", "value": { "roles": { "checking": { "agents": [{}, {}] }, "making": { "off": true } } } },
  ]))
  .await;
  let named: Vec<&str> = made.iter().map(|agent| agent.name.as_str()).collect();

  assert_eq!(named, ["checking-u7", "checking-u7-2"]);
  assert!(
    made
      .iter()
      .all(|agent| agent.prompt.as_deref() == Some("The checking template's own words."))
  );
}
