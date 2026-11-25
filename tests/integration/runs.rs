//! `bmo-agency run list` and `bmo-agency run stop` (docs/agent-control.md, 「Reading everything running at
//! once」, 「Stopping one from a shell」): the picture read off a store of the test's own, every row's stop
//! target, and the target the stop verb takes.

use agency_server::places::Ground;
use agency_server::verbs::run_list::{
  self, ExecutionRow, JobRow, MachineRow, PaneRow, Panes, Picture, RunRow,
};
use agency_server::verbs::run_stop::{Target, target_of};
use common::{Scratch, UNIT};
use harness_providers::Machine;
use serde_json::json;

use crate::integration::common;

fn picture() -> Picture {
  Picture {
    runs: vec![RunRow {
      id: 1,
      agent: "worker".into(),
      unit: UNIT.into(),
      started_at: 1,
    }],
    jobs: vec![JobRow {
      id: 2,
      item_id: 2038,
      worker: "worker".into(),
      state: "running".into(),
      started_at: 1,
      unit: Some(UNIT.into()),
    }],
    executions: vec![ExecutionRow {
      id: "gate-1".into(),
      line: "gate".into(),
      detail: "claude-code".into(),
      started_at: Some(1),
    }],
    panes: Panes::Held(vec![PaneRow {
      run: UNIT.into(),
      label: "worker".into(),
      status: "working".into(),
    }]),
    machine: MachineRow::default(),
  }
}

#[test]
fn every_row_prints_the_target_a_stop_takes() {
  let text = run_list::text(&picture()).join("\n");

  assert!(text.contains(&format!("stop unit:{UNIT}")));
  assert!(text.contains("stop item:2038"));
  assert!(text.contains("stop execution:gate-1"));
  assert!(text.contains("[runs]"));
  assert!(text.contains("[jobs]"));
  assert!(text.contains("[executions]"));
  assert!(text.contains("[herdr]"));
  assert!(text.contains("[machine]"));

  let structure = run_list::value(&picture());

  assert_eq!(structure["runs"][0]["stop"], json!({ "kind": "unit", "id": UNIT }));
  assert_eq!(structure["jobs"][0]["stop"], json!({ "kind": "item", "id": "2038" }));
  assert_eq!(structure["executions"][0]["stop"], json!({ "kind": "execution", "id": "gate-1" }));
  assert_eq!(structure["herdr"]["panes"][0]["status"], "working");
}

#[test]
fn a_herdr_that_did_not_answer_is_not_shown_as_holding_nothing() {
  let mut unread = picture();

  unread.panes = Panes::Unreached("no socket".into());

  let text = run_list::text(&unread).join("\n");

  assert!(text.contains("herdr did not answer: no socket"));
  assert_eq!(run_list::value(&unread)["herdr"]["answered"], false);
}

#[test]
fn a_stop_target_is_read_by_its_kind_and_nothing_else() {
  assert_eq!(target_of("unit:bmo-task-1"), Some(Target::Unit(UNIT.into())));
  assert_eq!(target_of("execution:gate-1"), Some(Target::Execution("gate-1".into())));
  assert_eq!(target_of("item:2038"), Some(Target::Item(2038)));
  assert_eq!(target_of("item:not-a-number"), None);
  assert_eq!(target_of("worker:one"), None);
  assert_eq!(target_of("unit:"), None);
  assert_eq!(target_of("unit"), None);
}

#[tokio::test]
async fn the_picture_reads_the_store_s_open_runs_jobs_and_executions() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let service = common::service(&scratch, &[("BMO_DATA_DIR", data.to_string_lossy().into_owned())]);
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), home.path().to_string_lossy().into_owned()),
      ("XDG_CONFIG_HOME".to_owned(), home.path().join("config").to_string_lossy().into_owned()),
    ],
    &data,
  );
  let ground = Ground::new(&service, &machine, None);

  scratch
    .run(
      r#"INSERT INTO "runs" ("agent", "unit", "depth", "started_at") VALUES ('worker', $1, 0, 1)"#,
      vec![UNIT.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
              "state", "started_at", "prompt")
         VALUES (2038, 7, 'tester', 1, 'worker', 'running', 1, 'the item')"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "agent_executions" ("id", "body") VALUES ($1, $2)"#,
      vec![
        "gate-1".into(),
        json!({ "id": "gate-1", "line": "gate", "startedAt": 1, "endedAt": null })
          .to_string()
          .into(),
      ],
    )
    .await;

  let picture = run_list::picture(&ground).await.unwrap();

  assert_eq!(picture.runs.len(), 1);
  assert_eq!(picture.runs[0].unit, UNIT);
  assert_eq!(picture.jobs.len(), 1);
  assert_eq!(picture.jobs[0].item_id, 2038);
  assert_eq!(picture.executions.len(), 1);
  assert_eq!(picture.executions[0].target(), "execution:gate-1");
}
