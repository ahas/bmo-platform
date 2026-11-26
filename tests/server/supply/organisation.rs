//! The sources a run walks and organisation supply (docs/supply.md, 「The sources a run walks」 and 「Organisation
//! supply」): the hook's order followed as it stands, organisation supply opened by the platform's word alone, and a
//! run taking the nearest unit's key of its path through a reservation that names the unit.
//!
//! The account service stands in on the loopback: the person stands in unit 4 under the company, unit 3, and unit
//! 4 holds a key of its own; the hook's order is the consumer's word on which sources the person may take.

use std::collections::{BTreeMap, HashMap};

use agency_server::supply::sources;
use agency_server::supply::{Choice, Shortfall, Source, choose, personal_member};
use axum::extract::{Path as Route, Query};
use policy::supply::SUPPLY;
use serde_json::{Value, json};

use super::{Consumer, PERSON, asked, personal_key};
use crate::support::grounds::ground_with;
use crate::support::scratch::Scratch;

fn order(words: &[&str]) -> Vec<String> {
  words.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn organisation_supply_opens_on_the_platform_s_word_alone() {
  let answered = |layers: Value, value: bool| json!({ "value": value, "layers": layers });

  assert!(sources::organisation_open(Some(&answered(
    json!([{ "link": "platform", "value": true, "rule": "unit:3" }]),
    true
  ))));
  assert!(
    !sources::organisation_open(Some(&answered(json!([{ "link": "unit:3", "value": true }]), true))),
    "a unit's word alone never opens it"
  );
  assert!(!sources::organisation_open(Some(&answered(
    json!([{ "link": "platform", "value": true }, { "link": "unit:4", "value": false }]),
    false
  ))));
  assert!(!sources::organisation_open(None));
  assert_eq!(
    sources::path_units(Some(&json!({ "path": ["platform", "unit:3", "unit:4", "person:7"] }))),
    vec![4, 3],
    "the nearest unit first"
  );
}

// The chain as the stand-in answers it for the person: the account service's `supply.organisation`, open for them,
// whose path names their units.
fn resolved(key: &str) -> Value {
  let path = json!(["platform", "unit:3", "unit:4", format!("person:{PERSON}")]);

  // The provider families the walk asks stay on: this stand-in's word is about the order's settings.
  if key.starts_with("provider.") {
    return json!({ "value": true, "layers": [{ "link": "platform", "value": true }], "path": path });
  }

  assert_eq!(key, "supply.organisation", "agency reads no other supply setting");

  json!({
    "value": true,
    "layers": [{ "link": "platform", "value": true, "rule": "unit:3" }],
    "path": path,
  })
}

async fn account_service() -> (String, tokio::task::JoinHandle<()>) {
  account_service_with(resolved).await
}

// The same, the chain answering each setting as `resolved` says.
async fn account_service_with(resolved: fn(&str) -> Value) -> (String, tokio::task::JoinHandle<()>) {
  let app = axum::Router::new()
    .route(
      "/chain/kinds/{key}",
      axum::routing::put(|axum::Json(body): axum::Json<Value>| async move { axum::Json(body) }),
    )
    .route(
      "/chain/kinds/{key}/resolve",
      axum::routing::get(
        move |Route(key): Route<String>, Query(asked): Query<HashMap<String, String>>| async move {
          assert_eq!(asked.get("person"), Some(&PERSON.to_string()));

          axum::Json(resolved(&key))
        },
      ),
    );
  let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());
  let serving = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  (address, serving)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_s_run_takes_the_nearest_unit_s_key_through_a_reservation_naming_the_unit() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (address, serving) = account_service().await;
  let vars =
    BTreeMap::from([("WEB_ADDRESS".to_owned(), address), ("WEB_TOKEN".to_owned(), "chain".to_owned())]);
  let ground = ground_with(scratch.db().clone(), home.path(), &vars);
  let consumer = Consumer::stand().await;

  personal_key(scratch.db(), "deepseek").await;
  scratch
    .run(
      r#"INSERT INTO "personal_supply" ("owner_unit", "road", "secret", "place", "added_at")
         VALUES (4, 'deepseek', 'team-key', 1, 0)"#,
      vec![],
    )
    .await;
  consumer.register(&ground);
  // The consumer closed the person's own supply and opened their organisation's before the platform's.
  consumer.answers("order", json!({ "sources": ["organisation", "plan"] }));
  consumer.answers(
    "reserve:organisation",
    json!({ "accepted": true, "reservation": "r-org", "amountUsd": 1.0 }),
  );

  let chosen = match choose(&ground, &asked(Some(PERSON), "deepseek")).await.unwrap() {
    Choice::Go(chosen) => chosen,
    Choice::Closed(closed) => panic!("the run was due to go out: {closed:?}"),
  };
  let Source::Organisation { unit, reservation } = chosen.source else {
    panic!("the organisation's supply was due: {:?}", chosen.source);
  };

  assert_eq!(unit, 4);
  assert_eq!(
    chosen.order,
    order(&["organisation", "plan"]),
    "the hook's order is walked as it stands"
  );
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "supply_reservations" WHERE "id" = $1 AND "unit_id" = 4"#,
        vec![reservation.into()]
      )
      .await,
    1
  );
  // The unit's own key row answers under its variant, not the member's name as the unit never owns it.
  let machine = harness_providers::Machine::of_process(home.path().join("data"));

  assert_eq!(
    personal_member(Some(&chosen), &machine, "deepseek-flash").as_deref(),
    Some("deepseek-flash@o4")
  );

  consumer.down().await;
  serving.abort();
}

// A member the unit itself owns, beside one of no one's: the run of the unit's own member keeps the member's
// own name, which the reservation naming the unit answers, while a member of no one's on the unit's road goes
// out under the unit's variant (docs/supply.md, 「Organisation supply」).
#[test]
fn the_units_own_member_answers_under_its_own_name_and_another_under_the_units_variant() {
  let home = tempfile::tempdir().unwrap();
  let dir = home.path().join("data/llm-gateway");

  std::fs::create_dir_all(&dir).unwrap();
  std::fs::write(
    dir.join("members.json"),
    json!([
      { "name": "team-flash", "road": "deepseek", "provider": "deepseek", "model": "team-flash-model",
        "api_base": "http://127.0.0.1:9/v1", "key_file": "team.key", "unit": 4,
        "context_tokens": 131072, "answer_tokens": 8192, "thinking": false },
      { "name": "plain-flash", "road": "deepseek", "provider": "deepseek", "model": "plain-flash-model",
        "api_base": "http://127.0.0.1:9/v1", "key_file": "plain.key",
        "context_tokens": 131072, "answer_tokens": 8192, "thinking": false },
    ])
    .to_string(),
  )
  .unwrap();

  let machine = harness_providers::Machine::of_process(home.path().join("data"));
  let chosen = |model: &str| agency_server::supply::Chosen {
    run: "job-41".to_owned(),
    account: Some(PERSON),
    provider: "deepseek".to_owned(),
    model: model.to_owned(),
    source: Source::Organisation {
      unit: 4,
      reservation: 1,
    },
    order: Vec::new(),
    units: vec![4],
    step: 0,
  };

  assert_eq!(personal_member(Some(&chosen("team-flash")), &machine, "team-flash"), None);
  assert_eq!(
    personal_member(Some(&chosen("plain-flash")), &machine, "plain-flash").as_deref(),
    Some("plain-flash@o4")
  );
}

// A registration filed under a unit is the unit's own supply: the walk of a person whose path names the unit
// takes the run's model as the unit's registration, and no platform source carries what a unit is filed under
// (docs/supply.md, 「Organisation supply」 and 「Owned supply」).
#[tokio::test(flavor = "multi_thread")]
async fn a_registration_filed_under_a_unit_answers_the_walk_of_its_peoples_run() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (address, serving) = account_service().await;
  let vars =
    BTreeMap::from([("WEB_ADDRESS".to_owned(), address), ("WEB_TOKEN".to_owned(), "chain".to_owned())]);
  let ground = ground_with(scratch.db().clone(), home.path(), &vars);
  let consumer = Consumer::stand().await;

  scratch
    .run(
      r#"INSERT INTO "registered_models" ("name", "label", "model", "context_tokens", "answer_tokens",
           "thinking", "walked", "added_at", "owner_unit")
         VALUES ('team-router', 'Team Router', 'router-1', 131072, 8192, 0, 0, 0, 4)"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "registered_models" ("name", "label", "model", "context_tokens", "answer_tokens",
           "thinking", "walked", "added_at")
         VALUES ('open-router', 'Open Router', 'router-1', 131072, 8192, 0, 0, 0)"#,
      vec![],
    )
    .await;

  consumer.register(&ground);
  consumer.answers("order", json!({ "sources": ["organisation", "plan"] }));
  consumer.answers(
    "reserve:organisation",
    json!({ "accepted": true, "reservation": "r-org", "amountUsd": 1.0 }),
  );

  let ask = |model: &'static str| agency_server::supply::Asked {
    kind: agency_server::supply::Kind::Job,
    id: 41,
    agent: super::AGENT,
    account: Some(PERSON),
    provider: "registered",
    model,
    job: Some(41),
    supervisor: None,
    room: None,
  };

  let chosen = match choose(&ground, &ask("team-router")).await.unwrap() {
    Choice::Go(chosen) => chosen,
    Choice::Closed(closed) => panic!("the unit's registration was due to answer: {closed:?}"),
  };

  assert!(
    matches!(chosen.source, Source::Organisation { unit: 4, .. }),
    "the unit's own registration opens its unit: {:?}",
    chosen.source
  );

  // What a unit is filed under stands with the unit's things: the platform's sources carry none of it,
  // and a registration of no one's stands open on them still.
  assert!(
    !agency_server::supply::platform_open(&ground, PERSON, "registered", "team-router")
      .await
      .unwrap()
  );
  assert!(
    agency_server::supply::platform_open(&ground, PERSON, "registered", "open-router")
      .await
      .unwrap()
  );

  // Filed under no unit, the same registration answers no organisation walk.
  assert!(
    agency_server::supply::organisation_source(&ground, &[4], "registered", "open-router")
      .await
      .unwrap()
      .is_none()
  );

  consumer.down().await;
  serving.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_order_that_names_no_source_closes_the_run_saying_so() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (address, serving) =
    account_service_with(|_| json!({ "value": false, "layers": [], "path": ["platform", "unit:3"] })).await;
  let vars =
    BTreeMap::from([("WEB_ADDRESS".to_owned(), address), ("WEB_TOKEN".to_owned(), "chain".to_owned())]);
  let ground = ground_with(scratch.db().clone(), home.path(), &vars);
  let consumer = Consumer::stand().await;

  consumer.register(&ground);
  // Every source closed above the person: the consumer's order names none.
  consumer.answers("order", json!({ "sources": [] }));

  let Choice::Closed(closed) = choose(&ground, &asked(Some(PERSON), "deepseek")).await.unwrap() else {
    panic!("the run was due to be closed");
  };

  assert_eq!(closed.why, Shortfall::Withheld);
  assert_eq!(closed.reason, SUPPLY.words.withheld);
  assert!(
    closed
      .sentence
      .starts_with("No supply is left for this run: your organisation's settings"),
    "{}",
    closed.sentence
  );
  assert!(!consumer.ops().contains(&"reserve".to_owned()), "nothing is reserved");

  consumer.down().await;
  serving.abort();
}
