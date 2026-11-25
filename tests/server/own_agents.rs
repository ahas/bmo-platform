//! A person's own agents (docs/role-agents.md, 「A person's own agents」): the set their organisation supplies read
//! off the composition, each seat with the link that supplied it and its road; whether the person may change the set
//! and choose roads, the link that withheld it named; and a seat laid on another road no longer standing as supplied
//! until it is put back.

use std::collections::{BTreeMap, HashMap};

use agency_server::organisation::own_agents::{self, Supplied};
use agency_server::role_agents::{self, RoleAgent};
use axum::extract::{Path as Route, Query};
use serde_json::{Value, json};

use crate::support::grounds::ground_with;
use crate::support::scratch::Scratch;

const PERSON: i64 = 7;

// The account service standing in: the composition's layers as given, the own-agents permission withheld by unit 3,
// and every other kind saying nothing, so its default stands.
async fn account_service(composition: Value) -> (String, tokio::task::JoinHandle<()>) {
  let app = axum::Router::new()
    .route(
      "/chain/kinds/{key}",
      axum::routing::put(|axum::Json(body): axum::Json<Value>| async move { axum::Json(body) }),
    )
    .route(
      "/chain/kinds/{key}/resolve",
      axum::routing::get(move |Route(key): Route<String>, Query(_asked): Query<HashMap<String, String>>| {
        let composition = composition.clone();

        async move {
          axum::Json(match key.as_str() {
            "signup.composition" => json!({ "layers": composition }),
            "agency.own-agents" => {
              json!({ "value": false, "from": "unit:3", "layers": [{ "link": "unit:3", "value": false }] })
            }
            _ => json!({ "layers": [] }),
          })
        }
      }),
    );
  let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());

  (address, tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }))
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
    keeps_cache: false,
    sessions: policy::roles::Sessions::default(),
    fold_ratio: None,
    prompt: Some(format!("The {role} template's own words.")),
    bundles: None,
    skills: None,
    added_at: 1,
    owner_account: None,
    follows_asker: None,
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_supplied_set_is_the_composition_with_each_seat_s_origin_and_the_person_s_change_stands_over_it()
 {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (address, serving) = account_service(json!([
    { "link": "unit:3", "value": { "roles": { "reading": { "off": true } } } },
    { "link": "unit:4", "value": { "roles": { "checking": { "agents": [{}, {}] } } } },
  ]))
  .await;
  let vars =
    BTreeMap::from([("WEB_ADDRESS".to_owned(), address), ("WEB_TOKEN".to_owned(), "chain".to_owned())]);
  let ground = ground_with(scratch.db().clone(), home.path(), &vars);

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

  for role in ["making", "reading", "checking"] {
    role_agents::write(scratch.db(), &template(role)).await.unwrap();
  }

  let supply = own_agents::supply(&ground, PERSON).await.unwrap();
  let seats: Vec<(&str, &str, Option<&str>)> = supply
    .iter()
    .flat_map(|role| role.supplied.iter())
    .map(|seat: &Supplied| (seat.role.as_str(), seat.name.as_str(), seat.from.as_deref()))
    .collect();

  assert_eq!(
    seats,
    [
      ("making", "making-u7", None),
      ("checking", "checking-u7", Some("unit:4")),
      ("checking", "checking-u7-2", Some("unit:4"))
    ],
    "the platform's own rule supplies one seat of a role nobody named, a unit's composition its own, and a role off none"
  );

  let reading = supply.iter().find(|role| role.role.name == "reading").unwrap();

  assert!(reading.off);
  assert_eq!(reading.off_from.as_deref(), Some("unit:3"));

  let add = own_agents::may_add(&ground, PERSON).await.unwrap();
  let road = own_agents::may_choose_road(&ground, PERSON).await.unwrap();

  assert!(!add.allowed);
  assert_eq!(add.from.as_deref(), Some("unit:3"));
  assert!(road.allowed, "a permission nobody set stands allowed");
  assert_eq!(
    add.required("own-agents-withheld", "add agents").unwrap_err().code(),
    "own-agents-withheld"
  );

  let making = supply.iter().find(|role| role.role.name == "making").unwrap();
  let seat = &making.supplied[0];
  let mut agent = own_agents::seat(&ground, making, seat.name.clone(), PERSON, seat.road.as_ref())
    .await
    .unwrap();

  assert_eq!(agent.owner_account.as_deref(), Some("7"));
  assert_eq!(agent.prompt.as_deref(), Some("The making template's own words."));
  assert!(own_agents::as_supplied(&agent, seat));

  agent.enabled = false;
  assert!(!own_agents::as_supplied(&agent, seat), "a seat switched off is the person's own change");
  assert_eq!(own_agents::next_name(&ground, making, PERSON).await.unwrap(), "making-u7-2");

  serving.abort();
}
