//! A head pane's death: the loop reports its exit code and last stderr line and starts the head again
//! however often it ends, stopping only on the head's exit 3 (docs/agent-control.md, 「One standing pane per
//! agent」), and the worker's running job ends failed with that in the report (「A pane that dies」).

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::control::work::pane_died;
use agency_server::places::Ground;
use harness_providers::Machine;

use crate::support::scratch::Scratch;

#[tokio::test]
async fn a_head_loop_starts_its_head_again_however_often_it_is_ended_and_stops_only_on_exit_three() {
  let home = tempfile::tempdir().unwrap();
  let scratch = Scratch::new().await;
  let variables = HashMap::<String, String>::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let ground = Ground::new(
    &service,
    &Machine::with_env(
      [("HOME".to_owned(), home.path().to_string_lossy().into_owned())],
      home.path().join("data"),
    ),
    None,
  );
  let starts = home.path().join("starts");
  let head = home.path().join("bin").join("pane-head");

  // A head ended by SIGTERM four times, as the bot ends a program to stand another and the agent host
  // ends a view giving way, and letting go of the pane on its fifth start.
  std::fs::create_dir_all(head.parent().unwrap()).unwrap();
  std::fs::write(
    &head,
    format!(
      "#!/bin/sh\necho start >> '{0}'\n[ \"$(wc -l < '{0}')\" -ge 5 ] && exit 3\nkill -TERM $$\n",
      starts.display()
    ),
  )
  .unwrap();
  std::fs::set_permissions(&head, std::fs::Permissions::from_mode(0o755)).unwrap();

  let command = ground.head_command("worker", home.path());
  let script = &command[2];

  assert!(script.contains("pane-deaths"), "{script}");
  assert!(script.contains("stderr.log"), "{script}");

  let mut looping = tokio::process::Command::new(&command[0]);

  looping
    .args(&command[1..])
    .env("BMO_HEAD_PAUSE", "0")
    .env_remove("AGENCY_CONTROL_TOKEN")
    .process_group(0)
    .kill_on_drop(true);

  let mut child = looping.spawn().unwrap();
  let group = child.id().unwrap() as i32;
  let ended = tokio::time::timeout(std::time::Duration::from_secs(20), child.wait()).await;

  unsafe {
    libc::killpg(group, libc::SIGKILL);
  }

  let status = ended.expect("the loop ends on the head's exit 3").unwrap();

  assert!(status.success(), "the loop ends 0 on the one exit it obeys");
  assert_eq!(std::fs::read_to_string(&starts).unwrap().lines().count(), 5);
}

#[tokio::test]
async fn the_head_loop_starts_pane_head_with_the_agents_name() {
  let home = tempfile::tempdir().unwrap();
  let scratch = Scratch::new().await;
  let variables = HashMap::<String, String>::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let ground = Ground::new(
    &service,
    &Machine::with_env(
      [("HOME".to_owned(), home.path().to_string_lossy().into_owned())],
      home.path().join("data"),
    ),
    None,
  );
  let argument = home.path().join("pane-head-argument");
  let head = home.path().join("bin").join("pane-head");

  std::fs::create_dir_all(head.parent().unwrap()).unwrap();
  std::fs::write(&head, format!("#!/bin/sh\nprintf '%s\\n' \"$1\" > '{}'\nexit 3\n", argument.display()))
    .unwrap();
  std::fs::set_permissions(&head, std::fs::Permissions::from_mode(0o755)).unwrap();

  let command = ground.head_command("worker", home.path());
  let status = std::process::Command::new(&command[0]).args(&command[1..]).status().unwrap();

  assert!(status.success(), "the loop ends 0 on the one exit it obeys");
  assert_eq!(std::fs::read_to_string(&argument).unwrap().trim(), "worker");
}

#[tokio::test]
async fn a_pane_death_ends_a_running_job_failed_with_its_code_and_last_line() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let variables = HashMap::<String, String>::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let ground = Ground::new(
    &service,
    &Machine::with_env(
      [("HOME".to_owned(), home.path().to_string_lossy().into_owned())],
      home.path().join("data"),
    ),
    None,
  )
  .with_var("BMO_TEST_GROUND", Some("1"));

  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state", "token_count", "started_at", "prompt")
         VALUES (70, 700, 7, 'harness', 1, 'worker-a', 'running', 0, $1, '# item')"#,
      vec![agency_server::now_ms().into()],
    )
    .await;

  let ended = pane_died(&ground, "worker-a", 1, "boom: the program stopped").await.unwrap();
  assert_eq!(ended, vec![70]);

  let row = store::one(
    scratch.db(),
    r#"SELECT "state", "outcome", "report" FROM "jobs" WHERE "id" = 70"#,
    vec![],
  )
  .await
  .unwrap()
  .unwrap();
  assert_eq!(
    (
      row.try_get::<String>("", "state").unwrap(),
      row.try_get::<String>("", "outcome").unwrap(),
    ),
    ("ended".to_owned(), "failed".to_owned())
  );
  let report: String = row.try_get("", "report").unwrap();
  assert!(report.contains("exit code 1"), "{report}");
  assert!(report.contains("boom: the program stopped"), "{report}");
}
