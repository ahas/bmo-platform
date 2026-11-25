//! A room forgotten: every row of agency's own tables naming the room goes but a job's run-log lines and
//! the supervisors, which stay with their reason; a dry run writes nothing; and another room's rows are left.

use agency_server::rooms::{forget, names_a_room};

use crate::support::scratch::Scratch;

async fn seed(scratch: &Scratch, room: i64) {
  for (job, text) in [(None::<i64>, "said"), (Some(5), "a job's turn")] {
    scratch
      .run(
        r#"INSERT INTO "agent_log" ("agent", "run", "chat_id", "at", "kind", "text", "job_id")
           VALUES ('pal', 'r', $1, 1, 'text', $2, $3)"#,
        vec![room.into(), text.into(), job.into()],
      )
      .await;
  }

  scratch
    .run(
      r#"INSERT INTO "room_sessions" ("agent", "chat_id", "generation", "opened_at") VALUES ('pal', $1, 1, 1)"#,
      vec![room.into()],
    )
    .await;
}

#[test]
fn a_column_naming_a_room_is_told_by_its_name() {
  assert!(names_a_room("chat_id") && names_a_room("chat_title") && names_a_room("owner_chat"));
  assert!(!names_a_room("chatter") && !names_a_room("agent"));
}

#[tokio::test]
async fn a_room_forgotten_takes_its_rows_but_a_jobs_lines_and_leaves_another_rooms() {
  let scratch = Scratch::new().await;

  seed(&scratch, -7).await;
  seed(&scratch, -8).await;

  let rows = |room: i64| {
    scratch.count(
      r#"SELECT (SELECT COUNT(*) FROM "agent_log" WHERE "chat_id" = $1)
              + (SELECT COUNT(*) FROM "room_sessions" WHERE "chat_id" = $1)"#,
      vec![room.into()],
    )
  };
  let asked = forget(scratch.db(), -7, true).await.unwrap();

  assert_eq!(rows(-7).await, 3, "a dry run writes nothing");
  assert_eq!(asked.gone.get("agent_log"), Some(&1));
  assert_eq!(asked.gone.get("room_sessions"), Some(&1));
  assert_eq!(asked.stayed.len(), 1);
  assert_eq!((asked.stayed[0].table.as_str(), asked.stayed[0].rows), ("agent_log", 1));

  let forgotten = forget(scratch.db(), -7, false).await.unwrap();

  assert_eq!(forgotten, asked);
  assert_eq!(rows(-7).await, 1, "the job's line stays");
  assert_eq!(rows(-8).await, 3, "another room's rows are left");
}

#[tokio::test]
async fn a_room_forgotten_leaves_the_supervisor_whose_session_last_ran_there() {
  let scratch = Scratch::new().await;

  scratch
    .run(
      r#"INSERT INTO "supervisors" ("name", "label", "session_scope", "turn_shape", "warming", "fold_ratio",
           "builtin", "chat_id", "runs", "created_at", "updated_at")
         VALUES ('pal', 'Pal', 'agent', 'window', '{}', 0.5, 1, -7, 0, 1, 1)"#,
      vec![],
    )
    .await;

  let asked = forget(scratch.db(), -7, true).await.unwrap();

  assert_eq!(asked.gone.get("supervisors"), None, "{asked:?}");
  assert!(
    asked.stayed.iter().any(|stayed| stayed.table == "supervisors" && stayed.rows == 1),
    "{asked:?}"
  );

  forget(scratch.db(), -7, false).await.unwrap();

  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "supervisors" WHERE "name" = 'pal'"#, vec![])
      .await,
    1,
    "the supervisor stands after its room is forgotten"
  );
}
