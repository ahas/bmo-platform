//! The supply hook's wire contract (docs/api.md, 「Hook points」; docs/supply.md, 「The supply hook」): the asks a
//! person's run posts as it starts are the ones captured in `captured-asks.json`, field for field and type for type.
//! The same captured file stands in the web console's tests, which post it to its hook route, so a change of shape
//! on either side fails a test on that side.

use agency_server::supply::{Choice, choose};
use serde_json::Value;

use super::{Consumer, PERSON, asked};
use crate::support::grounds::{deepseek_road, ground_on};
use crate::support::scratch::Scratch;

/// The asks as they were captured, the run's moment fixed.
fn captured() -> Vec<Value> {
  serde_json::from_str(include_str!("captured-asks.json")).expect("the captured asks are JSON")
}

// The run's supply key with its moment fixed to the captured one, in every place the body names it.
fn fixed(body: &Value, run: &str, captured_run: &str) -> Value {
  serde_json::from_str(&body.to_string().replace(run, captured_run)).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_s_run_posts_the_captured_order_and_reserve_asks() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let consumer = Consumer::stand().await;

  deepseek_road(home.path());
  consumer.register(&ground);
  consumer.answers("order", serde_json::json!({ "sources": ["global"] }));
  consumer.answers(
    "reserve:global",
    serde_json::json!({ "accepted": true, "reservation": "r-global", "amountUsd": 1.0, "expiresAt": agency_server::now_ms() + 3_600_000 }),
  );

  let chosen = match choose(&ground, &asked(Some(PERSON), "deepseek")).await.unwrap() {
    Choice::Go(chosen) => chosen,
    Choice::Closed(closed) => panic!("the run was due to go out: {closed:?}"),
  };
  let captured = captured();
  let captured_run = captured[0]["ask"]["run"].as_str().unwrap().to_owned();

  // The run is named by its supply key, a string: its kind, its number and the moment it chose.
  let moment = chosen.run.strip_prefix("job-41-").expect("a job's run key names its kind and number");
  assert!(moment.parse::<i64>().is_ok(), "the key ends in the moment: {}", chosen.run);

  let heard: Vec<Value> = consumer
    .heard()
    .iter()
    .map(|heard| fixed(&heard.body, &chosen.run, &captured_run))
    .collect();

  assert_eq!(heard, captured, "the asks posted differ from the captured contract");
  consumer.down().await;
}
