//! `bmo-agency supervisor follow` (docs/supervisors.md, 「A session in each room」): the hint a room prints for a
//! supervisor's answer, run as it stands, with the room's number negative.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::integration::common::Scratch;
use crate::support::binary;
use crate::support::rows::supervisor;

// The room a supervisor answers in, as the hint prints it: a chat's number, which is negative.
const ROOM: i64 = -9_000_000_000_002;
const SHARED: i64 = -9_000_000_000_003;
const AGENT: &str = "supervisor-a";
const ANSWER: &str = "The answer the supervisor wrote.";

#[tokio::test]
async fn the_follow_hint_a_room_prints_runs_with_a_negative_room_number() {
  let scratch = Scratch::new().await;

  supervisor(
    scratch.db(),
    AGENT,
    &[
      ("label", "The supervisor".into()),
      ("session_scope", "room".into()),
      ("turn_shape", "pieces".into()),
    ],
  )
  .await;
  scratch
    .run(
      r#"INSERT INTO "room_sessions" ("agent", "chat_id", "generation", "opened_at")
         VALUES ($1, $2, 1, 1)"#,
      vec![AGENT.into(), ROOM.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "room_session_turns"
           ("agent", "chat_id", "generation", "position", "role", "text", "at")
         VALUES ($1, $2, 1, 1, 'assistant', $3, 1)"#,
      vec![AGENT.into(), ROOM.into(), ANSWER.into()],
    )
    .await;

  let home = tempfile::tempdir().unwrap();
  let printed = home.path().join("printed");
  let refused = home.path().join("refused");
  let room = ROOM.to_string();
  let out = std::fs::File::create(&printed).unwrap();
  let err = std::fs::File::create(&refused).unwrap();
  let mut child = Command::new(binary::path("bmo-agency"))
    .args(["supervisor", "follow", room.as_str(), "--agent", AGENT])
    .current_dir(home.path())
    .env("HOME", home.path())
    .env("BMO_DATA_DIR", home.path().join("data"))
    .env("BMO_ARCHIVE_URL", scratch.address())
    .stdin(Stdio::null())
    .stdout(Stdio::from(out))
    .stderr(Stdio::from(err))
    .spawn()
    .unwrap();

  // The verb follows the session for as long as it runs, so the turn is waited for and the process ended.
  let deadline = Instant::now() + Duration::from_secs(15);
  let mut said = String::new();

  while Instant::now() < deadline {
    said = std::fs::read_to_string(&printed).unwrap_or_default();

    if said.contains(ANSWER) {
      break;
    }

    if let Some(status) = child.try_wait().unwrap() {
      let why = std::fs::read_to_string(&refused).unwrap_or_default();

      panic!("the hint ended {status} before it printed the session: {why}");
    }

    std::thread::sleep(Duration::from_millis(20));
  }

  let _ = child.kill();
  let _ = child.wait();

  assert!(
    said.contains("══ generation 1") && said.contains(ANSWER),
    "the hint printed nothing of the session: {said:?}; {}",
    std::fs::read_to_string(&refused).unwrap_or_default()
  );
}

#[tokio::test]
async fn a_reset_reads_the_rooms_it_names_as_negative_numbers_too() {
  let home = tempfile::tempdir().unwrap();
  let alone = ROOM.to_string();
  let shared = SHARED.to_string();
  let output = Command::new(binary::path("bmo-agency"))
    .args([
      "supervisor",
      "reset",
      AGENT,
      "--alone",
      alone.as_str(),
      "--shared",
      shared.as_str(),
      "--dry-run",
    ])
    .current_dir(home.path())
    .env("HOME", home.path())
    .env_remove("BMO_CONTROL_TOKEN")
    .stdin(Stdio::null())
    .output()
    .unwrap();
  let why = String::from_utf8_lossy(&output.stderr);

  assert_eq!(output.status.code(), Some(1), "{why}");
  assert!(!why.contains("unexpected argument"), "{why}");
}
