//! Usage by person and by source (docs/api.md, 「accounts, rest and usage」; docs/supply.md, 「The ledger」): the
//! ledger's runs, their cost and source as their closing and their mark wrote them, grouped by the account a
//! supervisor's run was for, `none` for the deployment's own, and by the source each went out on, every group
//! with what it cost; the jobs' model groups with what their turns' costs added up to; and another word refused
//! naming the two.

use std::sync::Arc;

use agency_server::control::work::add_job_cost;
use agency_server::now_ms;
use agency_server::run_ledger::{self, Closing, Opening, Spent};
use reqwest::Method;
use serde_json::{Value, json};

use super::{PERSON, call, front};
use crate::support::grounds::ground_on;
use crate::support::rows::supervisor;
use crate::support::scratch::Scratch;

// A run of an agent started and closed now, having spent `input` tokens at a cost, marked with its source
// where it names one.
async fn spent_run(scratch: &Scratch, agent: &str, input: i64, cost_micros: i64, source: Option<&str>) {
  let now = now_ms();
  let opening = Opening {
    agent: agent.to_owned(),
    chat_id: None,
    work_unit: format!("{agent}-{input}"),
    depth: 0,
    cap_ms: None,
    started_at: now,
    started_by: None,
    search_id: None,
    round: None,
    message_id: None,
    title: None,
    carried: false,
  };
  let id = run_ledger::open(scratch.db(), &opening).await.unwrap();
  let closing = Closing {
    finished_at: now,
    outcome: "done".to_owned(),
    context_tokens: None,
    spent: Some(Spent {
      input_tokens: input,
      cost_micros,
      ..Spent::default()
    }),
  };

  run_ledger::close(scratch.db(), id, &closing).await.unwrap();

  if let Some(source) = source {
    run_ledger::mark_supply(scratch.db(), id, source).await.unwrap();
  }
}

// The groups an answer holds, each its key, its tokens, its turns and its cost.
fn groups(usage: &Value) -> Vec<(String, i64, Value, Value)> {
  usage["groups"]
    .as_array()
    .unwrap_or_else(|| panic!("no groups: {usage}"))
    .iter()
    .map(|group| {
      (
        group["key"].as_str().unwrap().to_owned(),
        group["tokens"].as_i64().unwrap(),
        group["turns"].clone(),
        group["costUsd"].clone(),
      )
    })
    .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_groups_the_ledger_s_runs_by_person_and_by_source_and_the_jobs_by_model_with_their_cost() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  supervisor(scratch.db(), "sup-seven", &[("owner_account", PERSON.into())]).await;
  spent_run(&scratch, "sup-seven", 1_000, 1_500_000, Some("plan")).await;
  spent_run(&scratch, "sup-seven", 500, 500_000, Some("personal")).await;
  // The deployment's own run reported no cost and was marked with no source.
  spent_run(&scratch, "deployment-own", 100, 0, None).await;

  let now = now_ms();
  let job: i64 = store::one(
    scratch.db(),
    r#"INSERT INTO "jobs" ("item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
         "token_count", "started_at", "prompt", "model")
       VALUES (1, 1, 'sup-seven', 1, 'worker-a', 'ended', 90, $1, 'p', 'opus') RETURNING "id""#,
    vec![now.into()],
  )
  .await
  .unwrap()
  .unwrap()
  .try_get("", "id")
  .unwrap();
  let turn = |cost_micros| {
    Some(Spent {
      cost_micros,
      ..Spent::default()
    })
  };

  add_job_cost(scratch.db(), job, turn(750_000), None).await;
  add_job_cost(scratch.db(), job, turn(750_000), Some("plan")).await;
  add_job_cost(scratch.db(), job, None, None).await;

  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "jobs" WHERE "id" = $1 AND "cost_micros" = 1500000 AND "supply_source" = 'plan'"#,
        vec![job.into()]
      )
      .await,
    1
  );

  let served = front(Arc::new(ground_on(scratch.db().clone(), home.path()))).await;
  let usage = async |by: &str| {
    let path = format!("/usage?after={}&before={}&groupBy={by}", now - 60_000, now + 60_000);

    call(served.port, Method::GET, &path, None).await
  };

  let (status, by_account) = usage("account").await;

  assert_eq!(status, 200, "{by_account}");
  assert_eq!(
    groups(&by_account),
    [
      ("7".to_owned(), 1_500, json!(2), json!(2.0)),
      ("none".to_owned(), 100, json!(1), Value::Null)
    ]
  );

  let (_, by_supply) = usage("supply").await;

  assert_eq!(
    groups(&by_supply),
    [
      ("plan".to_owned(), 1_000, json!(1), json!(1.5)),
      ("personal".to_owned(), 500, json!(1), json!(0.5)),
      ("platform".to_owned(), 100, json!(1), Value::Null)
    ]
  );

  let (_, by_model) = usage("model").await;

  assert_eq!(groups(&by_model), [("opus".to_owned(), 90, Value::Null, json!(1.5))]);

  let (status, refused) = usage("nothing").await;

  assert_eq!(status, 400);
  assert!(refused.to_string().contains("account or supply"), "{refused}");
  served.down().await;
}
