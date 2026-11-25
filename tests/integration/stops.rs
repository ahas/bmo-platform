//! A run stopped from outside (docs/agent-control.md, 「Stopping」): a run in a pane asked and waited on until
//! it has ended, an execution's cancellation reaching each run it owns by what can reach it, and the runs a
//! change ended confirmed gone before their cancellation is taken back.

use crate::integration::common;

use std::path::PathBuf;
use std::time::Duration;

use agency_server::places::Ground;
use agency_server::queue::stops::{RunStop, StoppedRun, Stops};
use common::herdr::HerdrModel;
use common::{Scratch, UNIT, systemd_stand_in};
use harness_providers::Machine;
use policy::fill_marks;
use policy::queue_words::QUEUE_WORDS;
use policy::run_stop::RUN_STOP;
use serde_json::{Value, json};

struct Bench {
  scratch: Scratch,
  home: tempfile::TempDir,
  herdr: HerdrModel,
  ground: Ground,
}

impl Bench {
  async fn new() -> Self {
    Self::with(&[]).await
  }

  // The bench with the variables named set beside its own.
  async fn with(set: &[(&str, String)]) -> Self {
    let scratch = Scratch::new().await;
    let home = tempfile::tempdir().unwrap();
    let systemd = systemd_stand_in(&home.path().join("bin"));
    let config = home.path().join("config");

    std::fs::create_dir_all(config.join("herdr")).unwrap();

    let herdr = HerdrModel::stand(config.join("herdr").join("herdr.sock"));
    let data = home.path().join("data");
    let machine = Machine::with_env(
      [
        ("HOME".to_owned(), home.path().to_string_lossy().into_owned()),
        ("PATH".to_owned(), systemd.to_string_lossy().into_owned()),
        ("XDG_CONFIG_HOME".to_owned(), config.to_string_lossy().into_owned()),
      ],
      data,
    );
    let mut variables =
      vec![("AGENCY_SOURCE_DIR", home.path().join("source").to_string_lossy().into_owned())];

    variables.extend(set.iter().cloned());

    let service = common::service(&scratch, &variables);
    let ground = Ground::new(&service, &machine, Some(herdr::Herdr::there(herdr.socket.clone())));

    Self {
      scratch,
      home,
      herdr,
      ground,
    }
  }

  fn stops(&self) -> Stops {
    Stops::of(&self.ground, None)
  }

  fn executions(&self) -> PathBuf {
    self.home.path().join("data").join("executions")
  }

  async fn execution(&self, body: serde_json::Value) {
    let id = body["id"].as_str().unwrap().to_owned();

    self
      .scratch
      .run(
        r#"INSERT INTO "agent_executions" ("id", "body") VALUES ($1, $2)"#,
        vec![id.into(), body.to_string().into()],
      )
      .await;
  }

  async fn marked(&self, name: &str) -> bool {
    self
      .scratch
      .count(
        r#"SELECT COUNT(*) FROM "settings" WHERE "key" = $1"#,
        vec![format!("execution_cancel:{name}").into()],
      )
      .await
      == 1
  }

  // What happened in herdr, waited for while it is still going on in the background.
  async fn eventually(&self, done: impl Fn(&HerdrModel) -> bool) -> bool {
    for _ in 0..50 {
      if done(&self.herdr) {
        return true;
      }

      tokio::time::sleep(Duration::from_millis(100)).await;
    }

    false
  }
}

#[tokio::test]
async fn a_run_in_a_pane_is_asked_to_end_and_waited_on_however_long_its_ending_takes() {
  let bench = Bench::new().await;
  let herdr = bench.ground.herdr.clone().unwrap();
  let db = bench.scratch.db();

  assert_eq!(Stops::stop_run(db, &herdr, "bmo-task-nowhere").await, Ok(RunStop::Absent));

  // Asked, it takes its time over its ending, and is waited on until it has gone by itself.
  let (_, pane) = bench.herdr.workspace("work", serde_json::Value::Null, Some(&["claude"]));

  bench.herdr.carry(&pane, UNIT);

  let ends = {
    let (model, pane) = (bench.herdr.clone(), pane.clone());

    tokio::spawn(async move {
      tokio::time::sleep(Duration::from_millis(2_500)).await;
      model.call("pane.close", json!({ "pane_id": pane }));
    })
  };

  assert_eq!(Stops::stop_run(db, &herdr, UNIT).await, Ok(RunStop::Ended));
  assert!(ends.is_finished(), "the stop answered before the run ended");
  assert_eq!(bench.herdr.prompts(), [(pane.clone(), RUN_STOP.instruction.clone())]);
  assert_eq!(bench.herdr.asked("pane.close"), 0, "the stop never closed the pane under its run");

  // A run that ends itself once asked is never closed.
  let (_, pane) = bench.herdr.workspace("work", serde_json::Value::Null, Some(&["claude"]));

  bench.herdr.carry(&pane, "bmo-task-2");
  bench.herdr.ends_when_asked();
  assert_eq!(Stops::stop_run(db, &herdr, "bmo-task-2").await, Ok(RunStop::Ended));
  assert_eq!(bench.herdr.asked("pane.close"), 0);

  // A pane that will not take the words is closed at once, since nothing said to it would arrive.
  let (_, pane) = bench.herdr.workspace("work", serde_json::Value::Null, Some(&["claude"]));

  bench.herdr.carry(&pane, "bmo-task-3");
  bench.herdr.refuses_prompts();
  assert_eq!(Stops::stop_run(db, &herdr, "bmo-task-3").await, Ok(RunStop::Closed));
  assert_eq!(bench.herdr.asked("pane.close"), 1);
}

#[tokio::test]
async fn a_cancellation_reaches_each_run_it_owns_by_what_can_reach_it() {
  let bench = Bench::new().await;
  let (_, run_pane) = bench.herdr.workspace("work", serde_json::Value::Null, Some(&["claude"]));
  let (_, resident_pane) = bench.herdr.workspace("worker", serde_json::Value::Null, Some(&["claude"]));
  let (_, job_pane) = bench.herdr.workspace("job worker", serde_json::Value::Null, Some(&["claude"]));

  bench.herdr.carry(&run_pane, UNIT);
  bench.herdr.ends_when_asked();
  std::fs::create_dir_all(bench.executions().join("supervised")).unwrap();
  bench
    .execution(json!({ "id": UNIT, "paneId": run_pane, "panePath": true, "endedAt": null }))
    .await;
  bench.execution(json!({ "id": "supervised", "parent": UNIT, "endedAt": null })).await;
  bench
    .execution(json!({
      "id": "resident", "parent": UNIT, "paneId": resident_pane, "provider": "claude-code", "line": "chat",
      "resident": { "place": "/nowhere", "record": "/nowhere/record.jsonl" }, "endedAt": null,
    }))
    .await;
  bench
    .execution(json!({
      "id": "resident-job", "parent": UNIT, "paneId": job_pane, "provider": "claude-code", "line": "job",
      "resident": { "place": "/nowhere", "record": "/nowhere/record.jsonl" }, "endedAt": null,
    }))
    .await;
  bench
    .execution(json!({ "id": "shared", "parent": UNIT, "sharedPane": true, "endedAt": null }))
    .await;
  bench.execution(json!({ "id": "other", "parent": "bmo-task-9", "endedAt": null })).await;

  let stops = bench.stops();

  assert_eq!(stops.cancel(UNIT).await.unwrap(), (true, true));

  for name in [UNIT, "supervised", "resident", "resident-job", "shared"] {
    assert!(bench.marked(name).await, "{name} is marked");
  }

  assert!(!bench.marked("other").await);
  // A supervised child is reached by its cancel file, a turn that is its own pane's process by being
  // asked, a chat turn whose collector is gone by its provider's interrupt, and a job's resident turn
  // not at all: it ends on the stop words its collector put in.
  assert!(bench.executions().join("supervised").join("cancel").exists());
  assert!(
    bench
      .eventually(|herdr| herdr.prompts().iter().any(|(pane, _)| *pane == run_pane))
      .await
  );
  assert!(
    bench
      .eventually(|herdr| herdr.keys() == [(resident_pane.clone(), vec!["escape".to_owned()])])
      .await
  );
  assert!(!bench.herdr.prompts().iter().any(|(pane, _)| *pane == job_pane));
  assert!(!bench.herdr.keys().iter().any(|(pane, _)| *pane == job_pane));
}

#[tokio::test]
async fn the_runs_a_change_ended_are_confirmed_gone_before_their_cancellation_is_taken_back() {
  let bench = Bench::new().await;
  let stops = bench.stops();
  let runs = [StoppedRun {
    unit: Some("bmo-task-a".into()),
    pool_request: None,
  }];

  bench
    .scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('execution_cancel:bmo-task-a', '1')"#,
      vec![],
    )
    .await;

  // A unit systemd still holds refuses the change, and the cancellation stands.
  std::fs::write(bench.home.path().join("bin").join("alive"), "bmo-task-a\n").unwrap();

  let refused = stops.stop_runs(&runs, "task skipped").await.unwrap_err();

  assert_eq!(refused, fill_marks(&QUEUE_WORDS.stops.not_ended, &[("units", "bmo-task-a")]));
  assert!(bench.marked("bmo-task-a").await);

  std::fs::remove_file(bench.home.path().join("bin").join("alive")).unwrap();
  stops.stop_runs(&runs, "task skipped").await.unwrap();

  assert!(!bench.marked("bmo-task-a").await);
}

#[tokio::test]
async fn a_left_run_s_own_end_stands_over_its_launcher_s_running_mark() {
  use agency_server::door::execution::Row;
  use sea_orm::ConnectionTrait;

  let scratch = Scratch::new().await;
  let mut body = serde_json::Map::new();

  body.insert("id".into(), json!("bmo-execution-left-quick"));
  body.insert("root".into(), json!("bmo-people-sort-tester"));
  body.insert("state".into(), json!("launching"));
  body.insert("endedAt".into(), Value::Null);

  let mut launcher = Row::open(scratch.db(), body).await.unwrap();
  let mut run = launcher.clone();

  // The run ended and closed its row before its launcher marked it running.
  run.set("state", json!("exited"));
  run.set("endedAt", json!(1));
  run.save(scratch.db()).await.unwrap();
  launcher.set("state", json!("running"));

  assert!(!launcher.save_while_open(scratch.db()).await.unwrap());

  let stored = scratch
    .db()
    .query_one(sea_orm::Statement::from_sql_and_values(
      sea_orm::DbBackend::Postgres,
      r#"SELECT "body" FROM "agent_executions" WHERE "id" = $1"#,
      vec!["bmo-execution-left-quick".into()],
    ))
    .await
    .unwrap()
    .unwrap()
    .try_get::<String>("", "body")
    .unwrap();

  assert_eq!(serde_json::from_str::<Value>(&stored).unwrap()["state"], "exited");
}
