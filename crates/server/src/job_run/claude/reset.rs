//! A reset the owner asked of a worker's sessions while a job of its ran (docs/session-resets.md),
//! carried out by its next job as it starts, before the job looks for a session to resume.

use store::{one, run};

use crate::lines::place::{Place, Standby};
use crate::places::Ground;
use crate::{Refusal, now_ms};

/// The worker's sessions let go of where a reset waits for them: its records, and a session a person
/// opened in its standing program, which the job would otherwise take as its kept one. Answers whether
/// one waited.
pub async fn carry_out_worker_reset(ground: &Ground, worker: &str) -> Result<bool, Refusal> {
  let db = &ground.db;
  let waiting = one(
    db,
    r#"SELECT "id" FROM "session_resets" WHERE "agent" = $1 AND "chat_id" IS NULL AND "done_at" IS NULL ORDER BY "id" LIMIT 1"#,
    vec![worker.into()],
  )
  .await?;

  if waiting.is_none() {
    return Ok(false);
  }

  if let Some(path) = crate::worker_session::records::checkout_path(db, worker).await? {
    crate::apart_sessions::hand_over_path(db, &path).await?;
  }
  drop_spoken_standby(&Place::of(&ground.roots.data, worker));

  let settled = run(
    db,
    r#"UPDATE "session_resets" SET "done_at" = $1 WHERE "agent" = $2 AND "chat_id" IS NULL AND "done_at" IS NULL"#,
    vec![(now_ms() / 1000).into(), worker.into()],
  )
  .await?;

  Ok(settled > 0)
}

// A session a person opened in the standing program and spoke in, let go of.
fn drop_spoken_standby(place: &Place) {
  if Standby::read(&place.path).is_some_and(|standby| standby.record.exists()) {
    Standby::drop_from(&place.path);
  }
}
