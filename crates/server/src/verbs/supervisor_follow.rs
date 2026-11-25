//! A supervisor's session in one room followed as its standing pane prints it (docs/supervisors.md, 「The
//! session」): every turn as it was given and every answer as it was written, generation by generation, and
//! then each turn as it is appended, for as long as the process runs.

use std::path::Path;
use std::time::Duration;

use chrono::{FixedOffset, TimeZone};
use policy::fill_marks;
use policy::verb_words::VERB_WORDS;
use sea_orm::DatabaseConnection;
use store::{all, one};

use super::VerbError;

/// Where a reader stands in a session: the generation it is reading and the last position it printed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FollowPlace {
  pub generation: i64,
  pub position: i64,
}

/// How often a following pane looks for new turns.
pub const FOLLOW_POLL: Duration = Duration::from_secs(1);

// A moment in epoch seconds on the deployment's clock (UTC+9).
fn stamp(at: i64) -> String {
  FixedOffset::east_opt(9 * 3600)
    .and_then(|clock| clock.timestamp_opt(at, 0).single())
    .map_or_else(String::new, |at| at.format("%Y-%m-%d %H:%M:%S").to_string())
}

/// The room a word names among the agent's room sessions, by its number, and the agent's label.
pub async fn follow_room(
  db: &DatabaseConnection,
  agent: &str,
  room: &str,
) -> Result<(i64, String), VerbError> {
  let words = &VERB_WORDS.supervisor_follow;
  let Some(row) =
    one(db, r#"SELECT "name", "label" FROM "supervisors" WHERE "name" = $1"#, vec![agent.into()]).await?
  else {
    return Err(VerbError::Refused(fill_marks(&words.unknown_agent, &[("agent", agent)])));
  };
  let label = row.try_get::<Option<String>>("", "label")?.unwrap_or_else(|| agent.to_owned());
  let mut rooms = vec![];

  for session in all(
    db,
    r#"SELECT "chat_id" FROM "room_sessions" WHERE "agent" = $1 ORDER BY "chat_id""#,
    vec![agent.into()],
  )
  .await?
  {
    rooms.push(session.try_get::<i64>("", "chat_id")?);
  }

  match rooms.iter().find(|chat_id| chat_id.to_string() == room.trim()) {
    Some(chat_id) => Ok((*chat_id, label)),
    None => {
      let held: Vec<String> = rooms.iter().map(i64::to_string).collect();

      Err(VerbError::Refused(fill_marks(
        &words.no_session,
        &[("label", &label), ("room", room), ("rooms", &held.join(", "))],
      )))
    }
  }
}

/// What the reader has not printed yet, from where it stands up to what the store holds now, and where it
/// stands after. A reader that has printed nothing starts at the first generation. Nothing where the agent
/// holds no session in the room.
pub async fn follow_step(
  db: &DatabaseConnection,
  agent: &str,
  chat_id: i64,
  label: &str,
  place: Option<FollowPlace>,
) -> Result<(Vec<String>, Option<FollowPlace>), VerbError> {
  let words = &VERB_WORDS.supervisor_follow;
  let Some(session) = one(
    db,
    r#"SELECT "generation", "opened_at" FROM "room_sessions" WHERE "agent" = $1 AND "chat_id" = $2"#,
    vec![agent.into(), chat_id.into()],
  )
  .await?
  else {
    return Ok((vec![], place));
  };
  let newest: i64 = session.try_get("", "generation")?;
  let opened_at: Option<i64> = session.try_get("", "opened_at")?;
  let mut lines = vec![];
  let mut at = place.unwrap_or_default();

  for generation in at.generation.max(1)..=newest {
    if generation != at.generation {
      let head = match opened_at.filter(|_| generation == newest) {
        Some(opened) => fill_marks(
          &words.generation_opened,
          &[("generation", &generation.to_string()), ("at", &stamp(opened))],
        ),
        None => fill_marks(&words.generation, &[("generation", &generation.to_string())]),
      };

      lines.extend([head, String::new()]);
      at = FollowPlace {
        generation,
        position: 0,
      };
    }

    let turns = all(
      db,
      r#"SELECT "position", "role", "text", "at" FROM "room_session_turns"
         WHERE "agent" = $1 AND "chat_id" = $2 AND "generation" = $3 AND "position" > $4 ORDER BY "position""#,
      vec![agent.into(), chat_id.into(), generation.into(), at.position.into()],
    )
    .await?;

    for turn in turns {
      let role: String = turn.try_get("", "role")?;
      let who = if role == "assistant" { label } else { words.room.as_str() };

      lines.extend([
        fill_marks(&words.turn, &[("who", who), ("at", &stamp(turn.try_get("", "at")?))]),
        turn.try_get("", "text")?,
        String::new(),
      ]);
      at = FollowPlace {
        generation,
        position: turn.try_get("", "position")?,
      };
    }
  }

  Ok((lines, Some(at)))
}

/// The session printed as it was kept and then each turn as it is appended, for as long as the process
/// runs.
pub async fn follow(data_root: &Path, agent: &str, room: &str) -> Result<(), VerbError> {
  let db = super::store(data_root).await?;
  let (chat_id, label) = follow_room(&db, agent, room).await?;
  let mut place = None;

  loop {
    let (lines, next) = follow_step(&db, agent, chat_id, &label, place).await?;

    if !lines.is_empty() {
      println!("{}", lines.join("\n"));
    }

    place = next;
    tokio::time::sleep(FOLLOW_POLL).await;
  }
}
