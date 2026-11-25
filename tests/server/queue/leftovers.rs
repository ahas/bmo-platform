//! What runs and places leave in herdr: a pane whose run is over closes, a workspace nothing at work stands
//! in whose folder is gone closes whole, a checkout's stands only for its dev server, a standing agent's and a
//! node's are left to the standing passes, another root's is judged by this store's runs alone, only a marked
//! workspace's empty panes close, and what the sweep will not close is published once in the policy's words;
//! the sweep closes the pane of a run nothing carries any more with its row, keeps one whose collector stands,
//! and closes the row of such a run whose pane is already gone.

use std::collections::HashSet;
use std::sync::Arc;

use agency_server::door::execution::ENVELOPE;
use agency_server::queue::leftovers::{
  HerdrLeftovers, PaneView, Standing, Survivor, WorkspaceView, close_paneless, plan, sweep,
};
use agency_server::queue::reconcile::{PANE_GONE, PANE_SWEPT};
use agent_host::identity::ProcessIdentity;
use policy::fill_marks;
use policy::queue_words::QUEUE_WORDS;
use serde_json::json;

use crate::integration::common::herdr::HerdrModel;
use crate::support::grounds::seeded;
use crate::support::scratch::Scratch;

fn pane(id: &str, run: Option<&str>, command: &[&str]) -> PaneView {
  PaneView {
    pane: id.into(),
    run: run.map(str::to_owned),
    command: command.iter().map(|part| (*part).to_owned()).collect(),
    cwd_gone: false,
  }
}

fn gone(pane: PaneView) -> PaneView {
  PaneView {
    cwd_gone: true,
    ..pane
  }
}

fn workspace(id: &str, agent: Option<&str>, node: Option<&str>, panes: Vec<PaneView>) -> WorkspaceView {
  WorkspaceView {
    workspace: id.into(),
    label: format!("{id} label"),
    agent: agent.map(str::to_owned),
    node: node.map(str::to_owned),
    data: None,
    panes,
  }
}

fn names(names: &[&str]) -> HashSet<String> {
  names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn only_a_marked_workspaces_empty_panes_close_and_a_person_s_own_is_told() {
  let standing = Standing {
    homes: names(&["chat"]),
    ..Standing::default()
  };
  let planned = plan(
    &[
      // A standing agent's home: its shell beside its head goes.
      workspace(
        "w1",
        Some("chat"),
        None,
        vec![pane("p1", None, &["bmo-head-chat"]), pane("p2", None, &[])],
      ),
      // A gone agent's workspace with nothing kept goes whole.
      workspace("w2", Some("gone"), None, vec![pane("p3", None, &[])]),
      // A gone agent's workspace with a program left is told, its empty pane closed.
      workspace("w3", Some("gone"), None, vec![pane("p4", None, &["vim"]), pane("p5", None, &[])]),
      // An unmarked one carrying nothing may be a person's: told, never closed.
      workspace("w5", None, None, vec![pane("p7", None, &[])]),
      // A run at work anywhere keeps its pane.
      workspace("w6", Some("gone"), None, vec![pane("p8", Some("run-1"), &[])]),
    ],
    &standing,
  );

  assert_eq!(planned.close_panes, ["p2", "p5"]);
  assert_eq!(planned.close_workspaces, ["w2"]);
  assert_eq!(
    planned
      .survivors
      .iter()
      .map(|one| (one.workspace.as_str(), one.panes, one.marked))
      .collect::<Vec<_>>(),
    [("w3", 1, true), ("w5", 1, false)]
  );
}

#[test]
fn a_run_over_closes_its_pane_and_a_workspace_gone_from_its_folder_closes_whole() {
  let standing = Standing {
    homes: names(&["chat"]),
    repositories: names(&["web"]),
    over: names(&["run-over"]),
    ..Standing::default()
  };
  let planned = plan(
    &[
      // A run of its own whose run is over: its workspace goes, the run with it.
      workspace("w1", None, None, vec![pane("p1", Some("run-over"), &[])]),
      // A run at work stays, even in a removed folder.
      workspace("w2", None, None, vec![gone(pane("p2", Some("run-live"), &[]))]),
      // An earlier release's head of a name that no longer stands, in a removed folder, goes whole; so does a
      // revived one its token was lost from, counted under the name its head carries.
      workspace("w3", Some("gone"), None, vec![gone(pane("p3", None, &["bmo-head-gone"]))]),
      workspace("w4", None, None, vec![gone(pane("p4", None, &["bmo-head-gone"]))]),
      // A standing agent's home and a node's are left as they stand, whatever folder they stand in.
      workspace("w5", Some("chat"), None, vec![gone(pane("p5", Some("run-over"), &["bmo-head-chat"]))]),
      workspace("w6", None, Some("n1"), vec![gone(pane("p6", None, &["bmo-node-n1"]))]),
      // A revived home of a standing agent is that agent's, counted under its head's mark.
      workspace("w7", None, None, vec![gone(pane("p7", None, &["bmo-head-chat"]))]),
      // A checkout's workspace with nothing but a head goes, a past-count one among them; one whose dev
      // server stands keeps it, its run over closed beside it.
      workspace("w8", Some("web-w3"), None, vec![pane("p8", None, &["bmo-head-web-w3"])]),
      workspace(
        "w9",
        Some("web"),
        None,
        vec![
          pane("p9", None, &["bmo-head-web"]),
          pane("p10", None, &["bmo-agency", "dev-pane", "web"]),
          pane("p11", Some("run-over"), &[]),
        ],
      ),
      // A person's shell beside a run over stays, and is told.
      workspace("w10", None, None, vec![pane("p12", Some("run-over"), &[]), pane("p13", None, &[])]),
    ],
    &standing,
  );

  assert_eq!(planned.close_workspaces, ["w1", "w3", "w4", "w8"]);
  assert_eq!(planned.close_panes, ["p11", "p12"]);
  assert_eq!(planned.closed_runs, ["run-over", "run-over", "run-over"]);
  assert_eq!(planned.survivors.iter().map(|one| one.workspace.as_str()).collect::<Vec<_>>(), ["w10"]);
}

#[test]
fn a_workspace_of_another_data_root_is_judged_by_this_store_s_runs_alone() {
  let theirs = WorkspaceView {
    data: Some("/elsewhere/data".into()),
    ..workspace("w1", Some("gone"), None, vec![gone(pane("p1", None, &[]))])
  };
  let ours = Standing {
    data: Some("/here/data".into()),
    over: names(&["run-over"]),
    ..Standing::default()
  };

  assert_eq!(plan(std::slice::from_ref(&theirs), &ours), Default::default());
  // The machine's own root reads no copy's workspace but by its runs, and a copy reads the machine's own
  // unmarked workspaces by its runs alone: its run over goes, a person's shell beside one stays, untold.
  assert_eq!(plan(&[theirs], &Standing::default()), Default::default());

  let planned = plan(
    &[
      workspace("w2", None, None, vec![pane("p2", Some("run-over"), &[])]),
      workspace("w3", None, None, vec![pane("p3", Some("run-over"), &[]), gone(pane("p4", None, &[]))]),
    ],
    &ours,
  );

  assert_eq!(planned.close_workspaces, ["w2"]);
  assert_eq!(planned.close_panes, ["p3"]);
  assert!(planned.survivors.is_empty());
}

#[test]
fn what_the_sweep_would_not_close_is_said_in_the_policys_words() {
  let survivors = [
    Survivor {
      workspace: "w3".into(),
      label: "notes".into(),
      panes: 1,
      marked: true,
    },
    Survivor {
      workspace: "w5".into(),
      label: String::new(),
      panes: 2,
      marked: false,
    },
  ];
  let told = HerdrLeftovers::of(&survivors);
  let words = &QUEUE_WORDS.leftovers;
  let first = fill_marks(&words.workspace, &[("label", "notes"), ("panes", "1")]);
  let second = fill_marks(
    &words.unmarked,
    &[("workspace", &fill_marks(&words.workspace, &[("label", "w5"), ("panes", "2")]))],
  );

  assert_eq!(told.panes, 3);
  assert_eq!(told.workspaces.len(), 2);
  assert_eq!(
    told.account,
    fill_marks(&words.account, &[("panes", "3"), ("named", &format!("{first}, {second}"))])
  );
}

// A pane of its own carrying an execution's id as its run, the row open as the door left it, its collector the
// one given.
async fn run_pane(
  scratch: &Scratch,
  herdr: &HerdrModel,
  data: &std::path::Path,
  id: &str,
  owner: serde_json::Value,
) -> String {
  let (_, pane) = herdr.workspace(id, serde_json::Value::Null, Some(&["claude"]));

  herdr.call("pane.report_metadata", json!({ "pane_id": pane, "tokens": { "run": id } }));
  scratch
    .run(
      r#"INSERT INTO "agent_executions" ("id", "body") VALUES ($1, $2)"#,
      vec![
        id.into(),
        json!({ "id": id, "line": "login-renewal", "state": "running", "startedAt": 1, "endedAt": null,
                "paneId": pane })
        .to_string()
        .into(),
      ],
    )
    .await;

  let directory = data.join("executions").join(id);

  std::fs::create_dir_all(&directory).unwrap();
  std::fs::write(directory.join(ENVELOPE), json!({ "owner": owner }).to_string()).unwrap();

  pane
}

#[tokio::test(flavor = "multi_thread")]
async fn the_sweep_closes_the_pane_of_a_run_nothing_carries_with_its_row_and_keeps_one_carried() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let mut ground = seeded(scratch.db(), home.path()).await;
  let herdr = HerdrModel::stand(home.path().join("herdr.sock"));

  ground.herdr = Some(herdr::Herdr::there(herdr.socket.clone()));

  let ground = Arc::new(ground);
  let data = ground.roots.data.clone();
  let orphan =
    run_pane(&scratch, &herdr, &data, "run-orphan", json!({ "pid": 999_999_999, "birth": "1" })).await;
  let carried = run_pane(&scratch, &herdr, &data, "run-carried", json!(ProcessIdentity::own())).await;
  let standing = agency_server::queue::leftovers::standing(&ground).await.unwrap();

  sweep(&ground, ground.herdr.as_ref().unwrap(), &standing).await.unwrap().unwrap();

  let panes: Vec<String> = herdr.panes().into_iter().map(|one| one.id).collect();

  assert!(!panes.contains(&orphan), "the pane of a run nothing carries is closed: {panes:?}");
  assert!(panes.contains(&carried), "a run its collector still carries keeps its pane");

  let row = |id: &str| {
    let db = scratch.db().clone();
    let id = id.to_owned();

    async move {
      store::one(&db, r#"SELECT "body" FROM "agent_executions" WHERE "id" = $1"#, vec![id.into()])
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "body")
        .unwrap()
    }
  };

  assert!(row("run-orphan").await.contains(PANE_SWEPT), "its row closes with its pane");
  assert!(row("run-carried").await.contains("\"endedAt\":null"), "a carried run's row stays open");

  // A run whose pane went before anything closed its row has that row closed once its collector is gone, and
  // kept while its collector stands.
  let lost =
    run_pane(&scratch, &herdr, &data, "run-lost", json!({ "pid": 999_999_999, "birth": "1" })).await;
  let going = run_pane(&scratch, &herdr, &data, "run-going", json!(ProcessIdentity::own())).await;

  for pane in [&lost, &going] {
    herdr.call("pane.close", json!({ "pane_id": pane }));
  }

  assert_eq!(close_paneless(&ground, ground.herdr.as_ref().unwrap()).await.unwrap(), 1);
  assert!(row("run-lost").await.contains(PANE_GONE));
  assert!(row("run-going").await.contains("\"endedAt\":null"));
}
