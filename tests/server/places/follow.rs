//! A room session's follow pane: told by its mark and the agent it names, its tab named by the room's
//! title as the consumer that owns the room answers it.

use agency_server::hooks::Hooks;
use agency_server::places::follow::{FOLLOW_NAME, RoomTitle, follow_verb, follows_for};
use agency_server::places::home::standing_command;

#[test]
fn a_follow_pane_is_told_by_its_mark_and_the_agent_it_names() {
  let line =
    |agent: &str| standing_command(FOLLOW_NAME, &follow_verb("/deploy/bin/bmo-agency", 7, agent, "v1"));

  assert!(follows_for("pal-chat", &line("pal-chat")));
  assert!(!follows_for("pal-chat", &line("other")));
  assert!(!follows_for(
    "pal-chat",
    &standing_command("other", &["--agent".into(), "pal-chat".into()])
  ));
}

#[test]
fn the_follow_verb_names_the_room_the_agent_and_the_binarys_version() {
  assert_eq!(
    follow_verb("/deploy/bin/bmo-agency", 7, "pal", "v1"),
    ["/deploy/bin/bmo-agency", "supervisor", "follow", "7", "--agent", "pal", "--version=v1"]
  );
}

#[tokio::test]
async fn a_room_names_no_title_until_its_owner_answers_one() {
  let hooks = Hooks::default();

  assert_eq!(hooks.ask::<RoomTitle>(7).await, None);

  hooks.register::<RoomTitle, _, _>(|chat_id| async move { Some(format!("room {chat_id}")) });

  assert_eq!(hooks.ask::<RoomTitle>(7).await.as_deref(), Some("room 7"));
}
