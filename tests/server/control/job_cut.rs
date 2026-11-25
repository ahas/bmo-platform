//! A job's turn cut at its bound (docs/api.md, 「assignments and jobs」): the door marks the job stopped
//! before it cuts the turn, and the end of the turn reads that mark, so the job is written `stopped` with
//! outcome `stopped` and its item's lease is given back rather than kept for a close.

use std::collections::HashMap;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::control::work::job_turn_ended;
use agency_server::job_end::keeps_lease;
use agency_server::now_ms;
use agency_server::places::Ground;
use agency_server::queue::stops::{job_stop_reason, mark_job_stopped};
use agent_session::{Outcome, TurnEnd};
use harness_providers::Machine;
use serde_json::json;

use crate::support::scratch::Scratch;

#[tokio::test]
async fn a_job_cut_at_its_bound_is_stopped_and_its_lease_is_given_back() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), home.path().to_string_lossy().into_owned()),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
    ],
    home.path().join("data"),
  );
  let ground = Ground::new(&service, &machine, None);
  let began = now_ms();
  let why = "cut at its 240-minute bound: the job's turn held its item without ending";

  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "run_id", "run_name", "room_id", "state", "started_at", "prompt")
         VALUES (41, 7, 9, 'harness', 10, 'worker', 100, 'job-41', 5, 'running', $1, 'work')"#,
      vec![began.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "runs" ("id", "agent", "work_unit", "depth", "started_at")
         VALUES (100, 'worker', 'job-41', 0, $1)"#,
      vec![began.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at",
           "changed_at")
         VALUES (10, 7, 'harness', 'worker', 'running', $1, $1)"#,
      vec![began.into()],
    )
    .await;

  mark_job_stopped(scratch.db(), 41, why).await.unwrap();
  assert_eq!(job_stop_reason(scratch.db(), 41).await.unwrap().as_deref(), Some(why));

  job_turn_ended(&ground, 41, TurnEnd::of("job-41", Outcome::Stopped)).await;

  let row = store::one(
    scratch.db(),
    r#"SELECT "state", "outcome", "report", "lease_kept" FROM "jobs" WHERE "id" = 41"#,
    vec![],
  )
  .await
  .unwrap()
  .unwrap();

  assert_eq!(row.try_get::<String>("", "state").unwrap(), "stopped");
  assert_eq!(row.try_get::<String>("", "outcome").unwrap(), "stopped");
  assert_eq!(row.try_get::<String>("", "report").unwrap(), why);
  assert!(!row.try_get::<bool>("", "lease_kept").unwrap(), "a stopped job gives its lease back");
  assert!(!keeps_lease(&json!({
    "id": 41, "state": "stopped", "leaseId": 9
  })));
}
