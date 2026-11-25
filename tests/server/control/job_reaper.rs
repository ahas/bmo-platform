//! A job left running with no live execution behind it (docs/api.md, 「assignments and jobs」): the reaper
//! reads the executions and runs the store already keeps, ends the job as stopped with why, closes its run,
//! releases its assignment, gives its slot back, and leaves a job a live execution or another machine still
//! stands behind.

use std::collections::HashMap;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::control::work::reap_stale;
use agency_server::now_ms;
use agency_server::order_run::dispatch;
use agency_server::places::Ground;
use harness_providers::Machine;

use crate::support::grounds::{deepseek_balance, job_slot, seeded, slot_now};
use crate::support::rows::repository;
use crate::support::scratch::Scratch;

fn ground(scratch: &Scratch, machine: &Machine) -> Ground {
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());

  Ground::new(&service, machine, None)
}

fn machine(home: &std::path::Path) -> Machine {
  let at = |path: std::path::PathBuf| path.to_string_lossy().into_owned();

  Machine::with_env(
    [
      ("HOME".to_owned(), at(home.to_path_buf())),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
    ],
    home.join("data"),
  )
}

#[tokio::test]
async fn a_job_with_no_live_execution_is_stopped_and_a_live_one_is_left() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = machine(home.path());
  let ground = ground(&scratch, &machine);
  let now = now_ms();
  let old = now - 20 * 60 * 1000;
  let later = now - 10 * 60 * 1000;

  repository(scratch.db(), "bmo-cli", "/checkouts/bmo-cli", &[]).await;
  repository(scratch.db(), "node-worker", "/checkouts/node-worker", &[("node", "n3".into())]).await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "run_id", "run_name", "room_id", "state", "started_at", "prompt")
         VALUES
           (1, 11, 'harness', 10, 'bmo-cli', 100, 'job-1', 7, 'running', $1, 'one'),
           (2, 12, 'harness', 20, 'bmo-cli', 200, 'job-2', 7, 'running', $3, 'two'),
           (3, 13, 'harness', 30, 'node-worker', 300, 'job-3', 7, 'running', $1, 'three'),
           (4, 14, 'harness', 40, 'bmo-cli', 400, 'job-4', 7, 'running', $2, 'four'),
           (5, 15, 'harness', 50, 'bmo-cli', 500, 'job-5', 7, 'ended', $2, 'five')"#,
      vec![old.into(), now.into(), later.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "runs" ("agent", "unit", "depth", "started_at", "id") VALUES
           ('bmo-cli', 'job-1', 0, $1, 100),
           ('bmo-cli', 'job-2', 0, $3, 200),
           ('node-worker', 'job-3', 0, $1, 300),
           ('bmo-cli', 'job-4', 0, $2, 400),
           ('bmo-cli', 'job-5', 0, $2, 500)"#,
      vec![old.into(), now.into(), later.into()],
    )
    .await;
  scratch
    .run(
      r#"UPDATE "runs" SET "finished_at" = $1, "outcome" = 'answered' WHERE "id" = 500"#,
      vec![now.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "agent_executions" ("id", "body") VALUES ('live-execution', $1)"#,
      vec![
        serde_json::json!({
          "id": "live-execution", "agent": "bmo-cli", "line": "worker", "state": "running",
          "startedAt": later + 10, "endedAt": null
        })
        .to_string()
        .into(),
      ],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at",
           "changed_at")
         VALUES (10, 1, 'harness', 'bmo-cli', 'running', $1, $1),
                (20, 2, 'harness', 'bmo-cli', 'running', $1, $1),
                (30, 3, 'harness', 'node-worker', 'running', $1, $1),
                (40, 4, 'harness', 'bmo-cli', 'running', $1, $1),
                (50, 5, 'harness', 'bmo-cli', 'running', $1, $1)"#,
      vec![now.into()],
    )
    .await;

  assert_eq!(reap_stale(&ground).await.unwrap(), 1);

  let job = |id: i64| {
    let db = scratch.db();
    async move {
      store::one(
        db,
        r#"SELECT "state", "outcome", "report", "ended_at" FROM "jobs" WHERE "id" = $1"#,
        vec![id.into()],
      )
      .await
      .unwrap()
      .unwrap()
    }
  };
  let stopped = job(1).await;

  assert_eq!(stopped.try_get::<String>("", "state").unwrap(), "stopped");
  assert_eq!(stopped.try_get::<String>("", "outcome").unwrap(), "stopped");
  assert!(
    stopped
      .try_get::<String>("", "report")
      .unwrap()
      .contains("agency process that ran this job went away")
  );
  assert!(stopped.try_get::<Option<i64>>("", "ended_at").unwrap().is_some());
  for id in [2, 3, 4] {
    assert_eq!(job(id).await.try_get::<String>("", "state").unwrap(), "running");
  }
  assert_eq!(job(5).await.try_get::<String>("", "state").unwrap(), "ended");

  let run = |id: i64| {
    let db = scratch.db();
    async move {
      store::one(db, r#"SELECT "finished_at", "outcome" FROM "runs" WHERE "id" = $1"#, vec![id.into()])
        .await
        .unwrap()
        .unwrap()
    }
  };

  assert_eq!(run(100).await.try_get::<String>("", "outcome").unwrap(), "stopped");
  assert!(run(100).await.try_get::<Option<i64>>("", "finished_at").unwrap().is_some());
  assert!(run(200).await.try_get::<Option<i64>>("", "finished_at").unwrap().is_none());

  let assignment = |id: i64| {
    let db = scratch.db();
    async move {
      store::one(db, r#"SELECT "state" FROM "assignments" WHERE "id" = $1"#, vec![id.into()])
        .await
        .unwrap()
        .unwrap()
    }
  };

  assert_eq!(assignment(10).await.try_get::<String>("", "state").unwrap(), "released");
  assert_eq!(assignment(20).await.try_get::<String>("", "state").unwrap(), "running");
  assert_eq!(assignment(30).await.try_get::<String>("", "state").unwrap(), "running");
  assert_eq!(assignment(40).await.try_get::<String>("", "state").unwrap(), "running");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_reaped_as_stale_gives_its_slot_back_and_its_waiting_ask_is_cancelled() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let old = now_ms() - 20 * 60 * 1000;

  deepseek_balance(&ground, 5.0).await;
  scratch
    .run(r#"UPDATE "roles" SET "max_concurrent" = 1 WHERE "name" = 'worker'"#, vec![])
    .await;
  crate::support::rows::reseated(scratch.db()).await;
  repository(scratch.db(), "bmo-cli", "/checkouts/bmo-cli", &[]).await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "run_name", "room_id", "state", "started_at", "prompt")
         VALUES (21, 1, 0, 'harness', 0, 'bmo-cli', 'job-21', 7, 'running', $1, 'one'),
                (22, 2, 0, 'harness', 0, 'bmo-cli', NULL, 7, 'starting', $1, 'two')"#,
      vec![old.into()],
    )
    .await;

  // The first job holds the role's one slot, bound to a run no pass ever sees standing; the second waits.
  let held = job_slot(&ground, 21, true).await;
  let waiting = job_slot(&ground, 22, false).await;

  assert_eq!(reap_stale(&ground).await.unwrap(), 2);

  let given = slot_now(&ground, &held.token).await;

  assert_eq!(
    (given.state.as_str(), given.ended_by.as_deref()),
    ("released", Some(dispatch::BY_OWNER)),
    "{given:?}"
  );
  assert!(given.outcome.as_deref().is_some_and(|why| why.contains("went away")), "{given:?}");

  let cancelled = slot_now(&ground, &waiting.token).await;

  assert_eq!(
    cancelled.state, "cancelled",
    "a reaped job's waiting ask leaves the line: {cancelled:?}"
  );
  assert_eq!(
    dispatch::used(&ground.db, "worker", None).await.unwrap(),
    0,
    "no slot of the role stays taken"
  );
}
