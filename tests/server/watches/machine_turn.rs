//! Which jobs stand in the machine's turn (docs/role-agents.md, 「When an agent is busy」): a job testing or
//! waiting to test leaves its agent free, so the agent's next item is offered and let in beside it, and the
//! first job goes on with its seat once its test ends.

use agency_server::order_run::dispatch::{self, Asked, Wanted};
use agency_server::watches::machine_turn::{
  Reading, any_waiting_live, checkouts_by_name, mark, waits_live,
};
use agency_server::watches::queue_idle::{Open, held, idle_agents};

use crate::support::grounds::{deepseek_balance, seeded};
use crate::support::rows::repository;
use crate::support::scratch::Scratch;

fn ask<'a>(job: i64, path: &'a str, agent: &'a str) -> Asked<'a> {
  Asked {
    request_key: format!("job-{job}-1-1"),
    owner: format!("{}{job}", dispatch::JOB_OWNER),
    wanted: Wanted {
      role: "worker",
      path: Some(path),
      fixed: None,
      agent: Some(agent),
    },
    place_key: Some(path),
    ..Asked::default()
  }
}

fn item_for(id: i64, assignee: &str, agent: &str) -> Open {
  Open {
    id,
    title: format!("item {id}"),
    assignee: Some(assignee.to_owned()),
    agent: Some(agent.to_owned()),
    queue: "todo".to_owned(),
    ..Open::default()
  }
}

#[test]
fn the_testbed_names_a_checkout_by_its_root_and_a_commit_waiting_to_go_live_frees_its_job() {
  let printed = r#"{
    "queues": { "items": [
      { "name": "wide", "width": 24,
        "holder": { "scope": null, "checkout": "/checkouts/first/", "command": "cargo test", "pid": 1,
                    "askedAt": 1, "heldAt": 2, "kept": false, "front": false, "next": false },
        "waiting": [
          { "scope": null, "checkout": "/checkouts/third/crates/store", "command": "cargo check", "pid": 2,
            "askedAt": 3, "heldAt": null, "kept": false, "front": true, "next": false } ] },
      { "name": "narrow", "width": 1, "holder": null, "waiting": [] } ], "more": false },
    "closeLine": { "holder": null,
                   "waiting": [ { "scope": null, "checkout": "/checkouts/fourth", "askedAt": 4, "heldAt": null,
                                  "pid": 3 } ] }
  }"#;
  let reading = Reading {
    checkouts: Reading::testing_in(printed).unwrap(),
    awaiting_live: [71].into(),
  };

  assert!(reading.holds(Some("/checkouts/first"), 1), "a lane's holder");
  assert!(reading.holds(Some("/checkouts/third/"), 2), "a waiter in a folder under the checkout");
  assert!(reading.holds(Some("/checkouts/fourth"), 3), "a waiter in the close line");
  assert!(!reading.holds(Some("/checkouts/fir"), 4), "a name that only begins the same");
  assert!(!reading.holds(Some("/checkouts/second"), 5), "a checkout the testbed does not name");
  assert!(reading.holds(None, 71), "a job whose close's commit waits to go live");
  assert!(Reading::testing_in("not json").is_err());
}

#[test]
fn a_handed_over_commit_waits_until_its_repository_runs_it_or_a_run_of_it_ends_not_live() {
  let handed = "b".repeat(40);
  let older = "a".repeat(40);
  let newer = "c".repeat(40);
  let run = |commit: &str, outcome: Option<&str>| serde_json::json!({ "commit": commit, "ending": outcome.map(|outcome| serde_json::json!({ "outcome": outcome })) });

  assert!(
    waits_live(&handed, Some(&older), Some(&run(&older, Some("live"))), Some(false)),
    "its deployment not yet begun"
  );
  assert!(
    waits_live(&handed, Some(&older), Some(&run(&handed, None)), Some(false)),
    "its deployment under way"
  );
  assert!(
    !waits_live(&handed, Some(&handed), Some(&run(&handed, Some("live"))), None),
    "it went live, the close's label left behind"
  );
  assert!(
    !waits_live(&handed, Some(&newer), Some(&run(&newer, Some("live"))), Some(true)),
    "a later commit holding it went live"
  );
  assert!(
    !waits_live(&handed, Some(&older), Some(&run(&handed, Some("rolled-back"))), Some(false)),
    "its deployment ended not live, so its job is back at work"
  );
  assert!(!waits_live(&handed, None, None, None), "a record that does not read");
  assert!(!waits_live(&handed, Some(&newer), None, None), "a git that cannot tell");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_whose_only_job_waits_in_the_testbed_s_line_is_offered_its_next_item_and_let_in_on_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let db = &ground.db;

  deepseek_balance(&ground, 5.0).await;
  repository(db, "first", "/checkouts/first", &[]).await;
  repository(db, "second", "/checkouts/second", &[]).await;

  let agents = agency_server::role_agents::of_role(db, "worker").await.unwrap();
  let agent = agents[0].name.as_str();
  let state = async |token: &str| dispatch::request(db, token).await.unwrap().unwrap().state;
  let offered = async || {
    let standing = held(db, vec![item_for(52, "second", agent)]).await.unwrap();

    idle_agents(&standing)
      .iter()
      .filter(|idle| idle.agent.name == agent)
      .flat_map(|idle| idle.items.iter().map(|item| item.id))
      .collect::<Vec<_>>()
  };

  // The agent's only job runs in its checkout, let in on the agent.
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "state", "started_at", "prompt", "item_agent")
         VALUES (41, 51, 0, 'harness', 0, 'first', 'running', 0, 'work', $1)"#,
      vec![agent.into()],
    )
    .await;

  let first = agency_server::control::work::job_seat(&ground, 41, &ask(41, "/checkouts/first", agent))
    .await
    .unwrap()
    .unwrap();

  assert_eq!(first.seat.as_deref(), Some(agent));

  // While it writes, the agent is busy: its next item is not offered, and a job asking for it waits.
  assert!(offered().await.is_empty(), "a busy agent is not idle, its next item not offered");

  let second =
    dispatch::enqueue(&ground, &ask(42, "/checkouts/second", agent), dispatch::JOB, async |_, _| Ok(()))
      .await
      .unwrap();

  assert!(dispatch::acquire(&ground, &second).await.unwrap().is_none());

  // Its checkout waits in the testbed's line: the agent is free, its next item offered and let in beside it.
  let waiting_to_test = Reading {
    checkouts: ["/checkouts/first".to_owned()].into(),
    ..Reading::default()
  };

  assert_eq!(mark(db, &waiting_to_test).await.unwrap(), vec![41]);
  assert_eq!(offered().await, vec![52], "the agent is idle, its next item offered");

  let asked_again = dispatch::request(db, &second.token).await.unwrap().unwrap();
  let let_in = dispatch::acquire(&ground, &asked_again).await.unwrap().unwrap();

  assert_eq!(let_in.seat.as_deref(), Some(agent), "the second job is let in on the same agent");
  assert_eq!(state(&first.token).await, "active", "the first job keeps its seat while it tests");

  // The job of the item offered stands running in its own checkout.
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "state", "started_at", "prompt", "item_agent")
         VALUES (42, 52, 0, 'harness', 0, 'second', 'running', 0, 'work', $1)"#,
      vec![agent.into()],
    )
    .await;

  // With both jobs in the testbed's line the agent is free; once the first one's test ends it goes on on its
  // seat, and that job alone makes the agent busy again.
  let busy = async || {
    agency_server::role_agents::busy(db, "worker")
      .await
      .unwrap()
      .contains(&agent.to_owned())
  };
  let both = Reading {
    checkouts: ["/checkouts/first".to_owned(), "/checkouts/second".to_owned()].into(),
    ..Reading::default()
  };

  assert_eq!(mark(db, &both).await.unwrap(), vec![42]);
  assert!(!busy().await, "an agent whose every job tests is free");

  let first_tested = Reading {
    checkouts: ["/checkouts/second".to_owned()].into(),
    ..Reading::default()
  };

  assert_eq!(mark(db, &first_tested).await.unwrap(), vec![41]);
  assert_eq!(state(&first.token).await, "active", "the first job goes on on its seat");
  assert!(busy().await, "an agent whose job is back at work is busy");
}

#[tokio::test]
async fn a_close_s_commit_is_read_against_the_deployment_its_checkout_declares() {
  let home = tempfile::tempdir().unwrap();
  let checkout = home.path().join("checkout");
  let root = home.path().join("deploy-root");

  std::fs::create_dir_all(&checkout).unwrap();
  std::fs::create_dir_all(&root).unwrap();

  let git = |args: &[&str]| {
    let done = std::process::Command::new("git")
      .args(args)
      .current_dir(&checkout)
      .env("GIT_AUTHOR_NAME", "tester")
      .env("GIT_AUTHOR_EMAIL", "tester@example.com")
      .env("GIT_COMMITTER_NAME", "tester")
      .env("GIT_COMMITTER_EMAIL", "tester@example.com")
      .output()
      .unwrap();

    assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
    String::from_utf8(done.stdout).unwrap().trim().to_owned()
  };
  let commit = |message: &str| {
    git(&["commit", "-q", "--allow-empty", "-m", message]);
    git(&["rev-parse", "HEAD"])
  };
  let deployed = |commit: &str| {
    std::fs::write(root.join("deployed.json"), serde_json::json!({ "commit": commit }).to_string())
      .unwrap();
  };

  git(&["init", "-q"]);
  std::fs::write(checkout.join("testbed.toml"), "[deploy]\nroot = \"~/deploy-root\"\n").unwrap();

  let live = commit("live");
  let handed = commit("handed over");
  let waits =
    async || agency_server::watches::machine_turn::waiting_live(&checkout, &handed, home.path()).await;

  deployed(&live);
  assert!(waits().await, "the record runs an older commit");

  let later = commit("later");

  deployed(&later);
  assert!(!waits().await, "the record runs a commit that holds it");

  deployed(&"d".repeat(40));
  assert!(!waits().await, "the record runs a commit the checkout does not hold");

  std::fs::write(checkout.join("testbed.toml"), "[test]\n").unwrap();
  deployed(&live);
  assert!(!waits().await, "a checkout that declares no deploy root");
}

#[tokio::test]
async fn a_job_whose_work_spans_repositories_waits_until_every_waiting_pair_s_commit_is_live() {
  let home = tempfile::tempdir().unwrap();
  // A repository's checkout declaring its name and deploy root, holding a live commit and a handed-over one.
  let repository = |name: &str| {
    let checkout = home.path().join(name);
    let git = |args: &[&str]| {
      let done = std::process::Command::new("git")
        .args(args)
        .current_dir(&checkout)
        .env("GIT_AUTHOR_NAME", "tester")
        .env("GIT_AUTHOR_EMAIL", "tester@example.com")
        .env("GIT_COMMITTER_NAME", "tester")
        .env("GIT_COMMITTER_EMAIL", "tester@example.com")
        .output()
        .unwrap();

      assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
      String::from_utf8(done.stdout).unwrap().trim().to_owned()
    };

    std::fs::create_dir_all(&checkout).unwrap();
    std::fs::create_dir_all(home.path().join(format!("{name}-root"))).unwrap();
    git(&["init", "-q"]);
    std::fs::write(
      checkout.join("testbed.toml"),
      format!("name = \"{name}\"\n[deploy]\nroot = \"~/{name}-root\"\n"),
    )
    .unwrap();
    git(&["commit", "-q", "--allow-empty", "-m", "live"]);

    let live = git(&["rev-parse", "HEAD"]);

    git(&["commit", "-q", "--allow-empty", "-m", "handed over"]);

    let handed = git(&["rev-parse", "HEAD"]);

    (checkout, live, handed)
  };
  let deployed = |name: &str, commit: &str| {
    std::fs::write(
      home.path().join(format!("{name}-root")).join("deployed.json"),
      serde_json::json!({ "commit": commit }).to_string(),
    )
    .unwrap();
  };
  let (web, web_live, web_handed) = repository("web");
  let (drive, drive_live, drive_handed) = repository("drive");
  let closes = agency_server::watches::closes::of(&serde_json::json!({
    "close-phase": "waiting-batch", "close-commit": web_handed,
    "close-phase:web": "waiting-batch", "close-commit:web": web_handed,
    "close-phase:drive": "waiting-batch", "close-commit:drive": drive_handed,
  }));
  // The drive row goes by another name; its declaration names the repository.
  let checkouts = checkouts_by_name([("drive-row", drive.as_path()), ("web", web.as_path())]);
  let nowhere = std::collections::BTreeMap::new();
  let waits = async |checkouts| any_waiting_live(&closes, &web, checkouts, home.path()).await;

  assert_eq!(checkouts.keys().collect::<Vec<_>>(), ["drive", "web"]);

  deployed("web", &web_handed);
  deployed("drive", &drive_live);
  assert!(
    waits(&checkouts).await,
    "the job's own repository is live and the other one's commit waits"
  );
  assert!(!waits(&nowhere).await, "a repository with no checkout here");

  deployed("drive", &drive_handed);
  assert!(!waits(&checkouts).await, "every commit is live");

  let older = agency_server::watches::closes::of(
    &serde_json::json!({ "close-phase": "waiting-live", "close-commit": web_handed }),
  );

  deployed("web", &web_live);
  assert!(
    any_waiting_live(&older, &web, &nowhere, home.path()).await,
    "the summing pair alone, read in the job's own checkout"
  );
}
