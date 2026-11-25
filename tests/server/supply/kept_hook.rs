//! The supply hook's registration outlives a restart (docs/api.md, 「hooks」): registered across the boundary with
//! a token, its address is kept in the settings and its token in a private file, and a fresh set of hooks is
//! registered again from them under that token; taken away, both go; a point that does not outlive a restart
//! keeps nothing. Registered and silent, the point answers otherwise than with nobody registered.

use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

use agency_server::hooks::{HookPoint, Hooks, KEPT_PREFIX, is_kept, kept_token_path, restore};
use agency_server::supply::Kind;
use agency_server::supply::hook::{MoneyAsk, OrderAsk, SupplyAsk, SupplyHook};
use reqwest::Method;
use serde_json::{Value, json};

use super::{Consumer, PERSON, call, front};
use crate::support::grounds::ground_on;
use crate::support::scratch::Scratch;

fn order_ask() -> OrderAsk {
  OrderAsk {
    run: "job-41-1".to_owned(),
    account: PERSON,
    provider: "deepseek".to_owned(),
    model: "deepseek-flash".to_owned(),
    kind: Kind::Job,
    job: Some(41),
    supervisor: None,
    room: None,
  }
}

// The registration the settings keep at a point, where they keep one.
async fn kept(scratch: &Scratch, point: &str) -> Option<Value> {
  store::one(
    scratch.db(),
    r#"SELECT "value" FROM "settings" WHERE "key" = $1"#,
    vec![format!("{KEPT_PREFIX}{point}").into()],
  )
  .await
  .unwrap()
  .map(|row| serde_json::from_str(&row.try_get::<String>("", "value").unwrap()).unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn the_supply_hook_s_registration_is_kept_restored_under_its_token_and_let_go_with_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = Arc::new(ground_on(scratch.db().clone(), home.path()));
  let token_file = kept_token_path(&ground.roots.data, "supply");
  let consumer = Consumer::stand().await;
  let served = front(ground.clone()).await;

  consumer.answers("order", json!({ "sources": ["plan"] }));

  let (status, answered) = call(
    served.port,
    Method::PUT,
    "/hooks/supply",
    Some(json!({ "url": consumer.url, "token": "consumer-token" })),
  )
  .await;

  assert_eq!(status, 200, "{answered}");
  assert!(ground.hooks.registered::<SupplyHook>());
  assert_eq!(
    kept(&scratch, "supply").await,
    Some(json!({ "url": consumer.url, "tokenFile": token_file.to_string_lossy() }))
  );
  assert_eq!(std::fs::read_to_string(&token_file).unwrap(), "consumer-token");
  assert_eq!(std::fs::metadata(&token_file).unwrap().permissions().mode() & 0o777, 0o600);

  // A fresh set of hooks, as the server starts, is registered again and asks under the kept token.
  let fresh = Hooks::default();

  assert_eq!(restore(&fresh, scratch.db()).await.unwrap(), 1);
  assert_eq!(
    fresh.ask::<SupplyHook>(SupplyAsk::Order(order_ask())).await.sources,
    Some(vec!["plan".to_owned()])
  );
  assert_eq!(consumer.heard()[0].bearer.as_deref(), Some("consumer-token"));

  let (status, _) = call(served.port, Method::DELETE, "/hooks/supply", None).await;

  assert_eq!(status, 204);
  assert!(!ground.hooks.registered::<SupplyHook>());
  assert!(!is_kept(scratch.db(), "supply").await.unwrap());
  assert!(!token_file.exists());
  served.down().await;
  consumer.down().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_point_that_does_not_outlive_a_restart_keeps_nothing() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = Arc::new(ground_on(scratch.db().clone(), home.path()));
  let served = front(ground.clone()).await;

  let (status, answered) = call(
    served.port,
    Method::PUT,
    "/hooks/room-title",
    Some(json!({ "url": "http://127.0.0.1:9/room-title", "token": "consumer-token" })),
  )
  .await;

  assert_eq!(status, 200, "{answered}");
  assert!(!is_kept(scratch.db(), "room-title").await.unwrap());
  assert!(!kept_token_path(&ground.roots.data, "room-title").exists());
  assert_eq!(restore(&Hooks::default(), scratch.db()).await.unwrap(), 0);
  served.down().await;
}

#[test]
fn registered_and_silent_the_supply_point_answers_personal_supply_alone_where_nobody_registered_none() {
  let money = MoneyAsk {
    run: "job-41-1".to_owned(),
    step: 1,
    account: PERSON,
    source: "plan".to_owned(),
    reservation: None,
    spent_usd: None,
    cost_usd: None,
    key: "job-41-1:1:reserve".to_owned(),
  };

  assert_eq!(SupplyHook::default_answer(SupplyAsk::Order(order_ask())).sources, None);
  assert_eq!(
    SupplyHook::unanswered(SupplyAsk::Order(order_ask())).sources,
    Some(vec!["personal".to_owned()])
  );

  for answer in [
    SupplyHook::default_answer(SupplyAsk::Reserve(money.clone())),
    SupplyHook::unanswered(SupplyAsk::Reserve(money)),
  ] {
    assert_eq!((answer.accepted, answer.taken), (Some(false), Some(false)));
  }
}
