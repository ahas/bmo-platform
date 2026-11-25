//! What a supervisor's reset leaves on record (docs/supervisors.md, 「Starting it afresh」): for each room
//! session the conversation range ended, a session reset carried out at that moment.

use policy::control_words::CONTROL_WORDS;
use sea_orm::DatabaseConnection;
use store::{all, run};

use super::super::Refused;

/// The rooms whose session the supervisor holds now, read before the conversation range takes them.
pub async fn sessions(db: &DatabaseConnection, agent: &str) -> Result<Vec<i64>, Refused> {
  all(
    db,
    "SELECT chat_id FROM room_sessions WHERE agent = $1 ORDER BY chat_id",
    vec![agent.into()],
  )
  .await?
  .iter()
  .map(|row| Ok(row.try_get::<i64>("", "chat_id")?))
  .collect()
}

/// What one reset took, as its record holds it.
pub struct Taken<'a> {
  pub agent: &'a str,
  pub by: &'a str,
  pub at: i64,
  // The rooms whose session it ended.
  pub ended: &'a [i64],
}

/// Writes the reset's record: a carried-out session reset for each session it ended, any ask still
/// waiting for that session settled with it.
pub async fn write(db: &DatabaseConnection, taken: &Taken<'_>) -> Result<(), Refused> {
  for chat_id in taken.ended {
    run(
      db,
      "UPDATE session_resets SET done_at = $1 WHERE agent = $2 AND chat_id = $3 AND done_at IS NULL",
      vec![taken.at.into(), taken.agent.into(), (*chat_id).into()],
    )
    .await?;
    run(
      db,
      "INSERT INTO session_resets (agent, chat_id, asked_by, asked_at, reason, done_at)
       VALUES ($1, $2, $3, $4, $5, $6)",
      vec![
        taken.agent.into(),
        (*chat_id).into(),
        taken.by.into(),
        taken.at.into(),
        CONTROL_WORDS.supervisor_reset.reason.clone().into(),
        taken.at.into(),
      ],
    )
    .await?;
  }

  Ok(())
}
