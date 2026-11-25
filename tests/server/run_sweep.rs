//! The runs a turn left open as it ended (docs/run-records.md, 「The run ledger」): a job's run whose job has
//! ended and the run of a turn agency carries, a chat turn's or a role session's, that no turn kept in flight
//! carries are closed as cut with a line in their logs saying why; a run whose turn still goes, whatever name the
//! turn is kept under, a carried run younger than the grace and a run that is neither are left as they stand.

use std::collections::HashMap;
use std::sync::Arc;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::places::Ground;
use agency_server::run_sweep::{CUT, pass};
use agency_server::{now_ms, turn_pickup};
use harness_providers::Machine;

use crate::support::scratch::Scratch;

fn ground(scratch: &Scratch, home: &std::path::Path) -> Arc<Ground> {
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), home.to_string_lossy().into_owned()),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
    ],
    home.join("data"),
  );

  Arc::new(Ground::new(&service, &machine, None))
}

async fn outcome(scratch: &Scratch, id: i64) -> Option<String> {
  store::one(scratch.db(), r#"SELECT "outcome" FROM "runs" WHERE "id" = $1"#, vec![id.into()])
    .await
    .unwrap()
    .and_then(|row| row.try_get::<Option<String>>("", "outcome").ok().flatten())
}

#[tokio::test]
async fn a_run_whose_turn_ended_without_closing_it_is_closed_as_cut_and_its_log_says_why() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let old = now_ms() - 10 * 60 * 1000;
  let fresh = now_ms() - 10 * 1000;

  scratch
    .run(
      r#"INSERT INTO "runs"
           ("id", "agent", "chat_id", "work_unit", "depth", "started_at", "finished_at", "outcome", "carried")
         VALUES (1, 'chat-agent', -5, 'chat:-5:1', 0, $1, NULL, NULL, true),
                (2, 'chat-agent', -5, 'chat:-5:2', 0, $1, NULL, NULL, true),
                (3, 'chat-agent', -5, 'chat:-5:3', 0, $2, NULL, NULL, true),
                (4, 'worker-a', NULL, 'job-41', 0, $1, NULL, NULL, false),
                (5, 'worker-a', NULL, 'job-42', 0, $1, NULL, NULL, false),
                (6, 'search', -5, 'search-6', 0, $1, NULL, NULL, false),
                (7, 'chat-agent', -5, 'chat:-5:7', 0, $1, $1, 'answered', true),
                (8, 'helper', -5, 'helper-8', 0, $1, NULL, NULL, true),
                (9, 'helper', -5, 'a-unit-of-its-own', 0, $1, NULL, NULL, true)"#,
      vec![old.into(), fresh.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "run_id",
                             "run_name", "state", "outcome", "started_at", "prompt")
         VALUES (41, 1, 1, 'holder', 1, 'worker-a', 4, 'job-41', 'ended', 'stopped', $1, 'work'),
                (42, 2, 2, 'holder', 2, 'worker-a', 5, 'job-42', 'running', NULL, $1, 'work')"#,
      vec![old.into()],
    )
    .await;

  // The second chat run's turn is kept in flight: it still goes.
  turn_pickup::keep(
    scratch.db(),
    &turn_pickup::InFlight {
      session: "chat-agent@-5".into(),
      turn: 2,
      run: "chat:-5:2".into(),
      key: None,
      agent: "chat-agent".into(),
      chat_id: Some(-5),
      ledger: Some(2),
      job: None,
      started_at: old,
    },
  )
  .await;

  // The second role run's turn is kept in flight under its run's name, which is not the unit its lines stand
  // under.
  turn_pickup::keep(
    scratch.db(),
    &turn_pickup::InFlight {
      session: "helper@run:1".into(),
      turn: 3,
      run: "helper-9".into(),
      key: None,
      agent: "helper".into(),
      chat_id: None,
      ledger: Some(9),
      job: None,
      started_at: old,
    },
  )
  .await;

  assert_eq!(pass(&ground).await.unwrap(), 3);
  assert_eq!(outcome(&scratch, 1).await.as_deref(), Some(CUT), "no turn goes under it");
  assert_eq!(outcome(&scratch, 2).await, None, "its turn is kept in flight");
  assert_eq!(outcome(&scratch, 3).await, None, "younger than the grace");
  assert_eq!(outcome(&scratch, 4).await.as_deref(), Some(CUT), "its job has ended");
  assert_eq!(outcome(&scratch, 5).await, None, "its job still runs");
  assert_eq!(outcome(&scratch, 8).await.as_deref(), Some(CUT), "no turn of the role's goes under it");
  assert_eq!(outcome(&scratch, 9).await, None, "the turn kept in flight carries it");
  assert_eq!(
    outcome(&scratch, 6).await,
    None,
    "a run that is no job's and no turn agency carries is not this sweep's"
  );
  assert_eq!(
    outcome(&scratch, 7).await.as_deref(),
    Some("answered"),
    "a closed run keeps its closing"
  );

  let told = scratch
    .count(
      r#"SELECT count(*) FROM "agent_log"
         WHERE (("agent" = 'chat-agent' AND "run" = 'chat:-5:1') OR ("agent" = 'worker-a' AND "run" = 'job-41'))
           AND "text" LIKE 'Closed as cut:%'"#,
      vec![],
    )
    .await;

  assert_eq!(told, 2, "each run's log says why it was closed");
  assert_eq!(pass(&ground).await.unwrap(), 0, "a second pass finds nothing left");
}
