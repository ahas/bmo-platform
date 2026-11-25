//! A supervisor's turn on a road that cannot answer, sent as the receiver sends one (docs/supervisors.md, 「Where
//! a supervisor's turn runs」): the turn ends failed with the sentence that says why, at once; nothing goes out,
//! no program is stood in the supervisor's place, and Claude Code, though it is on and could answer, does not
//! stand in for the road.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::support::rows::supervisor;
use crate::support::scratch::Scratch;
use agency_server::Service;
use agency_server::config::Config;
use agency_server::herdr_backend::HerdrSessions;
use agency_server::lines::place::Place;
use agency_server::places::{Ground, Places};
use agency_server::seeds::deployment;
use agent_session::{Outcome, RoomBlock, SessionId, Sessions, Turn, TurnContext};
use harness_providers::Machine;

#[tokio::test(flavor = "multi_thread")]
async fn a_room_turn_on_a_road_that_cannot_answer_fails_with_why_and_nothing_stands_in() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let agent = "room-chat";
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), home.path().to_string_lossy().into_owned()),
      ("PATH".to_owned(), "/nowhere".to_owned()),
    ],
    home.path().join("data"),
  );

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();
  supervisor(
    scratch.db(),
    agent,
    &[
      ("session_scope", "room".into()),
      ("turn_shape", "pieces".into()),
      ("provider", "codex".into()),
      ("model", "gpt-5.6-sol".into()),
    ],
  )
  .await;
  // Its road's provider is off; Claude Code stays on, as every other provider does.
  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('provider_switch:codex', 'off'),
           ('provider_switch:claude-code', 'on')
         ON CONFLICT ("key") DO UPDATE SET "value" = EXCLUDED."value""#,
      vec![],
    )
    .await;

  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let ground = Ground::new(&service, &machine, None);
  let data = ground.roots.data.clone();
  let sessions = Arc::new(HerdrSessions::new(Places::new(ground, Arc::default())));
  let turn = Turn {
    session: SessionId(format!("{agent}@-5")),
    prompt: "Say something.".into(),
    context: TurnContext {
      room: Some(RoomBlock {
        chat_id: -5,
        text: "the room's block".into(),
        mark: "[a]".into(),
      }),
      ..TurnContext::default()
    },
  };
  let ended = tokio::time::timeout(Duration::from_secs(30), sessions.send_turn(turn))
    .await
    .expect("a turn on a road that cannot answer ends at once rather than waiting")
    .unwrap();

  assert_eq!(ended.outcome, Outcome::Failed, "{ended:?}");

  let reason = ended.reason.clone().unwrap_or_default();

  assert!(reason.to_lowercase().contains("codex"), "the reason names the road: {reason}");
  assert_eq!(ended.went_out, None, "nothing went out: {ended:?}");
  assert!(Place::of(&data, agent).program().is_none(), "no program was stood in its place");
  assert_eq!(
    scratch
      .count(
        r#"SELECT count(*) FROM "supervisors" WHERE "name" = $1 AND "last_mode" IS NULL"#,
        vec![agent.into()],
      )
      .await,
    1,
    "no provider answered for it"
  );
}
