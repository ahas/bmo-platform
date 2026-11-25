//! A room's session taken afresh, or handed over, clears what the turns before it spent (docs/supervisors.md,
//! 「A turn that goes nowhere」): a stale spent figure never judges the next turn over the bound at once. A
//! generation that opens also forgets the documents the ended session had loaded, while a session that goes
//! on in the one it held keeps them (docs/supervisors.md, 「A session in each room」).

use crate::support::rows::supervisor;
use crate::support::scratch::Scratch;
use agency_server::herdr_backend::keep_agent_session;
use agency_server::supervisor_turn::sessions::{self, Harness, Kept, hand_over, open_generation};

async fn loaded(scratch: &Scratch, session: &str) -> i64 {
  scratch
    .count(r#"SELECT COUNT(*) FROM "session_loads" WHERE "session_id" = $1"#, vec![session.into()])
    .await
}

#[tokio::test]
async fn a_fresh_generation_clears_the_spent_figures() {
  let scratch = Scratch::new().await;

  crate::support::rows::supervisor(
    scratch.db(),
    "kiki-chat",
    &[("session_scope", "room".into()), ("turn_shape", "pieces".into())],
  )
  .await;
  scratch
    .run(
      r#"INSERT INTO "room_sessions" ("agent", "chat_id", "generation", "opened_at", "spent_calls", "spent_ms")
         VALUES ('kiki-chat', -9000000000002, 1, 0, 3, 1801569)"#,
      vec![],
    )
    .await;

  let session = open_generation(scratch.db(), "kiki-chat", -9_000_000_000_002).await.unwrap();

  assert_eq!((session.spent.calls, session.spent.ms), (0, 0));
  assert_eq!(session.generation, 2);

  scratch
    .run(
      r#"UPDATE "room_sessions" SET "spent_calls" = 5, "spent_ms" = 900000
         WHERE "agent" = 'kiki-chat'"#,
      vec![],
    )
    .await;
  let handed = hand_over(scratch.db(), "kiki-chat", -9_000_000_000_002, "a note").await.unwrap();

  assert_eq!((handed.spent.calls, handed.spent.ms), (0, 0));
}

#[tokio::test]
async fn a_room_generation_that_opens_forgets_the_documents_loaded_under_its_id() {
  let scratch = Scratch::new().await;
  let agent = "kiki-chat";
  let chat = -9_000_000_000_002;
  let id = format!("{agent}@{chat}");
  let elsewhere = "another-agent@-9";

  supervisor(
    scratch.db(),
    agent,
    &[("session_scope", "room".into()), ("turn_shape", "pieces".into())],
  )
  .await;
  scratch
    .run(
      r#"INSERT INTO "room_sessions" ("agent", "chat_id", "generation", "opened_at", "harness_session", "harness_road")
         VALUES ($1, $2, 1, 1, 's-1', 'registered::m')"#,
      vec![agent.into(), chat.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "session_loads" ("session_id", "documents", "changed_at")
         VALUES ($1, '["a document"]', 1), ($2, '["another document"]', 1)"#,
      vec![id.clone().into(), elsewhere.into()],
    )
    .await;

  open_generation(scratch.db(), agent, chat).await.unwrap();

  assert_eq!(loaded(&scratch, &id).await, 0, "a generation that opens holds nothing read");
  assert_eq!(loaded(&scratch, elsewhere).await, 1, "another session's loads stand");

  scratch
    .run(
      r#"INSERT INTO "session_loads" ("session_id", "documents", "changed_at")
         VALUES ($1, '["a document"]', 1)"#,
      vec![id.clone().into()],
    )
    .await;

  hand_over(scratch.db(), agent, chat, "the note").await.unwrap();

  assert_eq!(loaded(&scratch, &id).await, 0, "a handover opens a generation that holds nothing");
}

#[tokio::test]
async fn a_resumed_room_session_keeps_the_documents_it_loaded() {
  let scratch = Scratch::new().await;
  let agent = "kiki-chat";
  let chat = -9_000_000_000_002;
  let id = format!("{agent}@{chat}");

  supervisor(
    scratch.db(),
    agent,
    &[("session_scope", "room".into()), ("turn_shape", "pieces".into())],
  )
  .await;
  scratch
    .run(
      r#"INSERT INTO "room_sessions" ("agent", "chat_id", "generation", "opened_at", "harness_session", "harness_road")
         VALUES ($1, $2, 1, 1, 's-1', 'registered::m')"#,
      vec![agent.into(), chat.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "session_loads" ("session_id", "documents", "changed_at")
         VALUES ($1, '["a document"]', 1)"#,
      vec![id.clone().into()],
    )
    .await;

  let session = sessions::session(scratch.db(), agent, chat).await.unwrap().unwrap();
  sessions::record_turn(
    scratch.db(),
    &session,
    &[
      Kept {
        role: "user",
        text: "the turn",
      },
      Kept {
        role: "assistant",
        text: "the answer",
      },
    ],
    Some(&Harness {
      id: "s-1".into(),
      road: "registered::m".into(),
      placement: None,
    }),
    None,
  )
  .await
  .unwrap();

  assert_eq!(loaded(&scratch, &id).await, 1, "a turn on the session it held keeps what it loaded");
}

#[tokio::test]
async fn a_session_across_rooms_let_go_forgets_its_loads_and_one_resumed_keeps_them() {
  let scratch = Scratch::new().await;
  let agent = "chat-agent";

  supervisor(scratch.db(), agent, &[]).await;
  scratch
    .run(
      r#"INSERT INTO "session_loads" ("session_id", "documents", "changed_at")
         VALUES ($1, '["a document"]', 1)"#,
      vec![agent.into()],
    )
    .await;

  // The session resumed: what it had read stands.
  keep_agent_session(scratch.db(), agent, Some(("s-1", -5))).await.unwrap();
  assert_eq!(loaded(&scratch, agent).await, 1, "a resumed session keeps what it loaded");

  // The session let go: the generation that replaces it holds nothing.
  keep_agent_session(scratch.db(), agent, None).await.unwrap();
  assert_eq!(loaded(&scratch, agent).await, 0, "a generation that opens holds nothing");
}

// A session the program cannot resume is let go like one it no longer holds: the CLI names both cases,
// and a turn is asked again once on a fresh session either way.
#[test]
fn a_session_the_program_cannot_continue_is_read_as_lost() {
  use agency_server::lines::pieces::{LOST_SESSION, STALE_SESSION, lost_session};

  assert!(lost_session(&format!("Error: {LOST_SESSION} abc")));
  assert!(lost_session(&format!(
    "Error: {STALE_SESSION}. Provide a prompt to continue the conversation."
  )));
  assert!(!lost_session("the pane went away before it answered"));
}
