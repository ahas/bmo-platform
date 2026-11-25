//! A pinned model is the one its pin names now (docs/repositories.md, 「A pinned model」; docs/supervisors.md,
//! 「The program on an ended session」): a repository's jobs stand on the registration its pin names, a pin
//! moved to another registration is that one, a supervisor's program standing on another model than its
//! order's registration is stale, and a road the owner moved on a supervisor's row replaces what its
//! sessions held and ends the program standing on them, so the next turn stands the road the row names now. A
//! room's supervisor's road moved hands each room's session over (`setting_handovers`).

use crate::support::rows::{repository, supervisor};
use crate::support::scratch::Scratch;

#[tokio::test]
async fn a_worker_s_model_is_the_pin_s_never_its_last_model() {
  let scratch = Scratch::new().await;

  repository(
    scratch.db(),
    "kiki",
    "/checkouts/kiki",
    &[("pinned_provider", "registered".into()), ("pinned_model", "flash-next".into())],
  )
  .await;

  assert_eq!(
    agency_server::herdr_backend::pinned_worker_model(scratch.db(), "kiki").await.unwrap(),
    Some("flash-next".to_owned())
  );

  scratch
    .run(
      r#"UPDATE "repositories" SET "pinned_model" = 'next-registration' WHERE "name" = 'kiki'"#,
      vec![],
    )
    .await;

  assert_eq!(
    agency_server::herdr_backend::pinned_worker_model(scratch.db(), "kiki").await.unwrap(),
    Some("next-registration".to_owned()),
    "a pin moved to another registration is the model, not the last one"
  );
}

#[tokio::test]
async fn a_supervisor_program_on_an_old_model_is_stale_for_its_pin_and_left_standing_for_its_handover() {
  use std::collections::HashMap;

  use agency_server::Service;
  use agency_server::config::Config;
  use agency_server::lines::place::{Place, Program};
  use agency_server::places::Ground;
  use agent_host::place::ResidentCommand;
  use harness_providers::Machine;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let service = Service::new(
    scratch.db().clone(),
    Config::read(|name| HashMap::<String, String>::new().get(name).cloned()).unwrap(),
  );
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), home.path().to_string_lossy().into_owned()),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
    ],
    home.path().join("data"),
  );
  let ground = Ground::new(&service, &machine, None);

  supervisor(
    scratch.db(),
    "kiki-chat",
    &[
      ("provider", "registered".into()),
      ("model", "flash-next".into()),
      ("last_model", "flash-next-coder".into()),
    ],
  )
  .await;

  let place = Place::of(&ground.roots.data, "kiki-chat");
  let program = Program {
    identity: "stale".into(),
    command: ResidentCommand {
      args: vec!["claude".into(), "--model".into(), "flash-next-coder@n3".into()],
      cwd: home.path().to_path_buf(),
      env: [].into(),
      login_file: None,
    },
    restart: None,
    opened: None,
  };

  std::fs::create_dir_all(&place.path).unwrap();
  std::fs::write(place.path.join("program.json"), serde_json::to_vec(&program).unwrap()).unwrap();

  assert!(agency_server::places::standby::stale_supervisor(&ground, "kiki-chat").await);
  assert_eq!(
    agency_server::places::standby::stale_supervisors(&ground).await,
    vec!["kiki-chat".to_owned()]
  );
  assert!(place.program().is_some(), "the check signals nothing: the handover lets the program go");
}

#[tokio::test]
async fn a_road_moved_on_the_row_takes_the_program_standing_on_the_sessions_it_held_down() {
  use std::collections::HashMap;
  use std::sync::Arc;

  use agency_server::Service;
  use agency_server::config::Config;
  use agency_server::herdr_backend::{HerdrSessions, agent_session};
  use agency_server::lines::place::{Place, Program};
  use agency_server::places::{Ground, Places};
  use agency_server::supervisor_turn::replacement::owed;
  use agent_host::place::ResidentCommand;
  use harness_providers::Machine;

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
  let place = Place::of(&ground.roots.data, "chat-agent");

  supervisor(
    scratch.db(),
    "chat-agent",
    &[
      ("provider", "registered".into()),
      ("model", "next-registration".into()),
      ("session_id", "s-1".into()),
    ],
  )
  .await;

  let standing = Program {
    identity: "old-road".into(),
    command: ResidentCommand {
      args: vec!["claude".into(), "--model".into(), "flash-next-coder@n3".into()],
      cwd: home.path().to_path_buf(),
      env: [].into(),
      login_file: None,
    },
    restart: Some(ResidentCommand {
      args: vec![
        "claude".into(),
        "--model".into(),
        "flash-next-coder@n3".into(),
        "--resume".into(),
        "s-1".into(),
      ],
      cwd: home.path().to_path_buf(),
      env: [].into(),
      login_file: None,
    }),
    opened: None,
  };

  std::fs::create_dir_all(&place.path).unwrap();
  std::fs::write(place.path.join("program.json"), serde_json::to_vec(&standing).unwrap()).unwrap();

  let sessions = Arc::new(HerdrSessions::new(Places::new(ground, Arc::default())));

  sessions.replaced_road("chat-agent").await;

  assert!(
    place.program().is_none(),
    "the program standing on the session the road moved off is taken down"
  );
  assert!(
    agent_session(scratch.db(), "chat-agent").await.unwrap().is_none(),
    "the session the row held is let go"
  );
  assert!(
    owed(scratch.db(), "chat-agent").await.unwrap().is_none(),
    "the replacement it owed is settled"
  );
}
