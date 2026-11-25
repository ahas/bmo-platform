//! A designated agent's session in each room it answers (docs/supervisors.md, 「A session in each room」): the room
//! sessions, the turns kept on each generation, the agents' windows in the rooms and the resets asked of a
//! room's session, as the TypeScript side's room session store reads and writes them.

use sea_orm::{ConnectionTrait, DatabaseConnection, QueryResult, TransactionTrait};

use store::{all, one, run};

use super::guard::Spent;
use crate::lines::fault;
use crate::{Refusal, now_ms};

/// The harness session a room's turns go on in, as the turn that last answered left it: its id, the provider
/// and model it was opened on as `<provider>::<model>`, and the place it stands on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Harness {
  pub id: String,
  pub road: String,
  pub placement: Option<String>,
}

/// What one agent's session in one room stands at.
#[derive(Clone, Debug)]
pub struct RoomSession {
  pub agent: String,
  pub chat_id: i64,
  pub generation: i64,
  pub harness: Option<Harness>,
  pub last_message_id: Option<i64>,
  pub system_hash: Option<String>,
  /// The note the generation before wrote as it closed at the handoff mark.
  pub handoff: Option<String>,
  pub spent: Spent,
}

/// The provider and model a harness session was opened on, as its road holds them.
pub fn harness_road(provider: &str, model: &str) -> String {
  format!("{provider}::{model}")
}

/// The id a supervisor's session in a room goes by to the receiver.
pub fn room_session_id(agent: &str, chat_id: i64) -> String {
  format!("{agent}@{chat_id}")
}

/// Forget the documents a session had loaded: a generation that opens reads nothing of the one it replaces.
pub async fn forget_loads(db: &impl ConnectionTrait, session: &str) -> Result<(), Refusal> {
  run(db, r#"DELETE FROM "session_loads" WHERE "session_id" = $1"#, vec![session.into()])
    .await
    .map(drop)
    .map_err(Refusal::from)
}

fn session_of(row: &QueryResult) -> Result<RoomSession, Refusal> {
  let id: Option<String> = row.try_get("", "harness_session")?;
  let road: Option<String> = row.try_get("", "harness_road")?;

  Ok(RoomSession {
    agent: row.try_get("", "agent")?,
    chat_id: row.try_get("", "chat_id")?,
    generation: row.try_get("", "generation")?,
    harness: match (id, road) {
      (Some(id), Some(road)) => Some(Harness {
        id,
        road,
        placement: row.try_get("", "placement")?,
      }),
      _ => None,
    },
    last_message_id: row.try_get("", "last_message_id")?,
    system_hash: row.try_get("", "system_hash")?,
    handoff: row.try_get("", "handoff_note")?,
    spent: Spent {
      calls: row.try_get("", "spent_calls")?,
      ms: row.try_get("", "spent_ms")?,
    },
  })
}

fn now() -> i64 {
  now_ms() / 1000
}

/// The agent's session in the room, or none where it has never had one.
pub async fn session(
  db: &impl ConnectionTrait,
  agent: &str,
  chat_id: i64,
) -> Result<Option<RoomSession>, Refusal> {
  one(
    db,
    "SELECT * FROM room_sessions WHERE agent = $1 AND chat_id = $2",
    vec![agent.into(), chat_id.into()],
  )
  .await?
  .as_ref()
  .map(session_of)
  .transpose()
}

/// Every session an agent holds, room by room.
pub async fn sessions_of(db: &impl ConnectionTrait, agent: &str) -> Result<Vec<RoomSession>, Refusal> {
  all(db, "SELECT * FROM room_sessions WHERE agent = $1 ORDER BY chat_id", vec![agent.into()])
    .await?
    .iter()
    .map(session_of)
    .collect()
}

/// A new generation of the agent's session in the room: the first where there was none, the next where its
/// turns go on in a fresh harness session. It holds no harness session until its first turn answers; the
/// last message taken stays, since opening takes nothing new.
pub async fn open_generation(
  db: &DatabaseConnection,
  agent: &str,
  chat_id: i64,
) -> Result<RoomSession, Refusal> {
  let transaction = db.begin().await?;
  let row = one(
    &transaction,
    "INSERT INTO room_sessions (agent, chat_id, generation, opened_at) VALUES ($1, $2, 1, $3)
     ON CONFLICT (agent, chat_id) DO UPDATE SET generation = room_sessions.generation + 1, opened_at = excluded.opened_at,
       harness_session = NULL, harness_road = NULL, placement = NULL, handoff_note = NULL,
       spent_calls = 0, spent_ms = 0
     RETURNING *",
    vec![agent.into(), chat_id.into(), now().into()],
  )
  .await?
  .ok_or_else(|| fault("the room session was not written"))?;

  forget_loads(&transaction, &room_session_id(agent, chat_id)).await?;
  transaction.commit().await?;

  session_of(&row)
}

/// The next generation of a session that handed itself over at the handoff mark, holding the note the
/// closing one wrote until its first turn answers.
pub async fn hand_over(
  db: &DatabaseConnection,
  agent: &str,
  chat_id: i64,
  note: &str,
) -> Result<RoomSession, Refusal> {
  let transaction = db.begin().await?;
  let row = one(
    &transaction,
    "UPDATE room_sessions SET generation = generation + 1, opened_at = $1, harness_session = NULL,
       harness_road = NULL, placement = NULL, handoff_note = $2, spent_calls = 0, spent_ms = 0
     WHERE agent = $3 AND chat_id = $4 RETURNING *",
    vec![now().into(), note.into(), agent.into(), chat_id.into()],
  )
  .await?
  .ok_or_else(|| fault("the room session to hand over is gone"))?;

  forget_loads(&transaction, &room_session_id(agent, chat_id)).await?;
  transaction.commit().await?;

  session_of(&row)
}

/// The system prompt a session's next turn is sent with, by its hash: a session that held another records
/// when it changed; one that held none records the hash and changes nothing else.
pub async fn mark_system(
  db: &DatabaseConnection,
  agent: &str,
  chat_id: i64,
  hash: &str,
) -> Result<RoomSession, Refusal> {
  let row = one(
    db,
    "UPDATE room_sessions SET
       system_reset_at = CASE WHEN system_hash IS NOT NULL AND system_hash <> $1 THEN $2 ELSE system_reset_at END,
       system_hash = $1
     WHERE agent = $3 AND chat_id = $4 RETURNING *",
    vec![hash.into(), now().into(), agent.into(), chat_id.into()],
  )
  .await?
  .ok_or_else(|| fault("the room session to mark is gone"))?;

  session_of(&row)
}

/// The hash of the system prompt the agent's one session across rooms was last sent with, or none where none
/// was recorded.
pub async fn agent_system_hash(db: &impl ConnectionTrait, agent: &str) -> Result<Option<String>, Refusal> {
  Ok(
    one(db, r#"SELECT "system_hash" FROM "supervisors" WHERE "name" = $1"#, vec![agent.into()])
      .await?
      .and_then(|row| row.try_get::<Option<String>>("", "system_hash").ok().flatten()),
  )
}

/// The hash of the system prompt the agent's one session across rooms is sent with, kept on its row for the
/// next turn to compare against.
pub async fn mark_agent_system(db: &impl ConnectionTrait, agent: &str, hash: &str) -> Result<(), Refusal> {
  run(
    db,
    r#"UPDATE "supervisors" SET "system_hash" = $2 WHERE "name" = $1"#,
    vec![agent.into(), hash.into()],
  )
  .await
  .map(drop)
  .map_err(Refusal::from)
}

/// Whether a session the agent already had reads a prompt that moved: it holds a session, and the prompt
/// composed for this turn is not the one its last turn was sent with. A session whose prompt was never
/// recorded hands over once, since what it read cannot be known.
pub fn prompt_moved(held: bool, recorded: Option<&str>, hash: &str) -> bool {
  held && recorded != Some(hash)
}

/// What runs on the words not yet taken have spent so far, written as a run goes so a restart keeps it.
pub async fn mark_spent(
  db: &DatabaseConnection,
  agent: &str,
  chat_id: i64,
  spent: Spent,
) -> Result<(), Refusal> {
  run(
    db,
    "UPDATE room_sessions SET spent_calls = $1, spent_ms = $2 WHERE agent = $3 AND chat_id = $4",
    vec![spent.calls.into(), spent.ms.into(), agent.into(), chat_id.into()],
  )
  .await
  .map(drop)
  .map_err(Refusal::from)
}

/// Whether a generation keeps any turn.
pub async fn keeps_turns(
  db: &DatabaseConnection,
  agent: &str,
  chat_id: i64,
  generation: i64,
) -> Result<bool, Refusal> {
  Ok(
    one(
      db,
      "SELECT 1 AS one FROM room_session_turns WHERE agent = $1 AND chat_id = $2 AND generation = $3 LIMIT 1",
      vec![agent.into(), chat_id.into(), generation.into()],
    )
    .await?
    .is_some(),
  )
}

/// One turn's part as it is kept: what the agent was given, or what it answered.
pub struct Kept<'a> {
  pub role: &'static str,
  pub text: &'a str,
}

/// One turn kept on the generation it went out on, in order after what the generation holds, with the harness
/// session it answered in and the last message it took written onto the session. A turn kept has taken its
/// words, so what the runs on them spent is cleared; one that answered in a harness session has taken the
/// handoff note it opened with.
pub async fn record_turn(
  db: &DatabaseConnection,
  session: &RoomSession,
  turns: &[Kept<'_>],
  harness: Option<&Harness>,
  last_message_id: Option<i64>,
) -> Result<(), Refusal> {
  let (agent, chat_id, generation) = (session.agent.as_str(), session.chat_id, session.generation);
  let transaction = db.begin().await?;
  let last: Option<i64> = one(
    &transaction,
    "SELECT MAX(position) AS position FROM room_session_turns WHERE agent = $1 AND chat_id = $2 AND generation = $3",
    vec![agent.into(), chat_id.into(), generation.into()],
  )
  .await?
  .map(|row| row.try_get("", "position"))
  .transpose()?
  .flatten();
  let mut position = last.unwrap_or(0);
  let at = now();

  for turn in turns {
    position += 1;
    run(
      &transaction,
      "INSERT INTO room_session_turns (agent, chat_id, generation, position, role, text, at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
      vec![
        agent.into(),
        chat_id.into(),
        generation.into(),
        position.into(),
        turn.role.into(),
        turn.text.into(),
        at.into(),
      ],
    )
    .await?;
  }

  run(
    &transaction,
    "UPDATE room_sessions SET spent_calls = 0, spent_ms = 0 WHERE agent = $1 AND chat_id = $2",
    vec![agent.into(), chat_id.into()],
  )
  .await?;

  if let Some(harness) = harness {
    run(
      &transaction,
      "UPDATE room_sessions SET harness_session = $1, harness_road = $2, placement = $3, handoff_note = NULL
       WHERE agent = $4 AND chat_id = $5",
      vec![
        harness.id.as_str().into(),
        harness.road.as_str().into(),
        harness.placement.clone().into(),
        agent.into(),
        chat_id.into(),
      ],
    )
    .await?;
  }

  if let Some(last) = last_message_id {
    run(
      &transaction,
      "UPDATE room_sessions SET last_message_id = $1 WHERE agent = $2 AND chat_id = $3",
      vec![last.into(), agent.into(), chat_id.into()],
    )
    .await?;
  }

  transaction.commit().await?;

  Ok(())
}

/// Whether a reset of the room's session was asked and not yet carried out.
pub async fn reset_waiting(db: &DatabaseConnection, agent: &str, chat_id: i64) -> Result<bool, Refusal> {
  Ok(
    one(
      db,
      "SELECT id FROM session_resets WHERE agent = $1 AND chat_id IS NOT DISTINCT FROM $2 AND done_at IS NULL ORDER BY id LIMIT 1",
      vec![agent.into(), chat_id.into()],
    )
    .await?
    .is_some(),
  )
}

/// The reset waiting for a session — the room's, or the one across rooms where `chat_id` is none — as its
/// carrying out reads it: whether it starts the session clean, and the account that asked where its asker is
/// one (docs/session-resets.md, 「What a reset is」); none where none waits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaitingReset {
  pub clean: bool,
  pub by: Option<i64>,
}

/// The reset waiting for a session, read as `WaitingReset`.
pub async fn waiting_reset(
  db: &DatabaseConnection,
  agent: &str,
  chat_id: Option<i64>,
) -> Result<Option<WaitingReset>, Refusal> {
  let row = one(
    db,
    "SELECT clean, asked_by FROM session_resets WHERE agent = $1 AND chat_id IS NOT DISTINCT FROM $2 AND done_at IS NULL ORDER BY id LIMIT 1",
    vec![agent.into(), chat_id.into()],
  )
  .await?;

  Ok(row.map(|row| WaitingReset {
    clean: row.try_get("", "clean").unwrap_or(false),
    by: row.try_get::<String>("", "asked_by").ok().and_then(|by| by.trim().parse().ok()),
  }))
}

/// Whether a reset waits for the agent's one session across rooms (the `agent` scope).
pub async fn reset_waiting_agent(db: &DatabaseConnection, agent: &str) -> Result<bool, Refusal> {
  Ok(
    one(
      db,
      "SELECT id FROM session_resets WHERE agent = $1 AND chat_id IS NULL AND done_at IS NULL ORDER BY id LIMIT 1",
      vec![agent.into()],
    )
    .await?
    .is_some(),
  )
}

/// The agent's one session reset settled as carried out now.
pub async fn settle_reset_agent(db: &DatabaseConnection, agent: &str) -> Result<(), Refusal> {
  run(
    db,
    "UPDATE session_resets SET done_at = $1 WHERE agent = $2 AND chat_id IS NULL AND done_at IS NULL",
    vec![now().into(), agent.into()],
  )
  .await
  .map(drop)
  .map_err(Refusal::from)
}

/// Every reset waiting for the room's session settled as carried out now.
pub async fn settle_reset(db: &DatabaseConnection, agent: &str, chat_id: i64) -> Result<(), Refusal> {
  run(
    db,
    "UPDATE session_resets SET done_at = $1 WHERE agent = $2 AND chat_id IS NOT DISTINCT FROM $3 AND done_at IS NULL",
    vec![now().into(), agent.into(), chat_id.into()],
  )
  .await
  .map(drop)
  .map_err(Refusal::from)
}
