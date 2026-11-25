//! The replacement a supervisor's session owes (docs/sessions.md, 「Session replacement」), written down so a
//! restart forgets none of it. The agent's one session across rooms owes one where a turn found the session's
//! prompt moved, its road moved or a document reload named the agent, the word kept being why, for the journal; a
//! room's own session owes its handover where its prompt, its road moved or a reset of it was asked, kept with why
//! and what the handoff turn goes out on.
//! Each is read off by what makes it while the session is quiet.

use indexmap::IndexMap;
use sea_orm::ConnectionTrait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::handoff::{Due, Speaker, Why};
use crate::Refusal;
use crate::queue::settings;

fn key(agent: &str) -> String {
  format!("handoff_owed:{agent}")
}

/// Why the agent's session owes a replacement, or none where it owes none.
pub async fn owed(db: &impl ConnectionTrait, agent: &str) -> Result<Option<String>, Refusal> {
  Ok(settings::get(db, &key(agent)).await?.filter(|why| !why.is_empty()))
}

/// The replacement written down as owed, with why; one owed already keeps its first reason.
pub async fn owe(db: &impl ConnectionTrait, agent: &str, why: &str) -> Result<(), Refusal> {
  if owed(db, agent).await?.is_some() {
    return Ok(());
  }

  settings::set(db, &key(agent), why).await
}

/// The debt settled: the replacement was made, or nothing was left to replace.
pub async fn settle(db: &impl ConnectionTrait, agent: &str) -> Result<(), Refusal> {
  settings::forget(db, &key(agent)).await
}

/// The family a room's own session's owed handover is kept under, by the agent and the room.
pub const ROOM_PREFIX: &str = "room_handoff_owed:";

fn room_key(agent: &str, chat_id: i64) -> String {
  format!("{ROOM_PREFIX}{agent}:{chat_id}")
}

/// The handover a room's own session owes, written down where its prompt or its road moved or a reset of it was
/// asked, with what the handoff turn goes out on: the supervisor, the provider, model and system prompt the
/// session was last sent, the generation's settings, the session and its place, and why.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomOwed {
  pub supervisor: String,
  #[serde(default)]
  pub label: Option<String>,
  #[serde(default)]
  pub pinned_model: Option<String>,
  pub provider: String,
  pub model: String,
  pub system: String,
  #[serde(default)]
  pub generation: IndexMap<String, Value>,
  pub session: String,
  #[serde(default)]
  pub placement: Option<String>,
  /// Why the session hands over; a record written before it named one is owed for its documents.
  #[serde(default)]
  pub why: Why,
}

impl RoomOwed {
  /// What a handover owed is written down as.
  pub fn of(speaker: &Speaker, due: &Due) -> Self {
    Self {
      supervisor: speaker.name.clone(),
      label: speaker.label.clone(),
      pinned_model: speaker.pinned_model.clone(),
      provider: due.provider.name().to_owned(),
      model: due.model.clone(),
      system: due.system.clone(),
      generation: due.generation.clone(),
      session: due.session.clone(),
      placement: due.placement.clone(),
      why: due.why,
    }
  }

  /// The supervisor and the handover read back; none where its provider is one this build no longer holds.
  pub fn due(&self) -> Option<(Speaker, Due)> {
    let provider = harness_providers::provider(&self.provider)?;

    Some((
      Speaker {
        name: self.supervisor.clone(),
        label: self.label.clone(),
        pinned_model: self.pinned_model.clone(),
      },
      Due {
        provider,
        model: self.model.clone(),
        system: self.system.clone(),
        generation: self.generation.clone(),
        session: self.session.clone(),
        placement: self.placement.clone(),
        ratio: 1.0,
        why: self.why,
      },
    ))
  }
}

/// The handover a room's own session owes written down; a later finding writes over it, so the handoff turn
/// goes out on the prompt the room's session was last sent.
pub async fn owe_room(
  db: &impl ConnectionTrait,
  agent: &str,
  chat_id: i64,
  owed: &RoomOwed,
) -> Result<(), Refusal> {
  let text = serde_json::to_string(owed).map_err(|why| Refusal::failed(why.to_string()))?;

  settings::set(db, &room_key(agent, chat_id), &text).await
}

/// The handover a room's own session owes, or none; a row that no longer reads is let go.
pub async fn room_owed(
  db: &impl ConnectionTrait,
  agent: &str,
  chat_id: i64,
) -> Result<Option<RoomOwed>, Refusal> {
  let Some(text) = settings::get(db, &room_key(agent, chat_id)).await? else {
    return Ok(None);
  };

  match serde_json::from_str(&text) {
    Ok(owed) => Ok(Some(owed)),
    Err(why) => {
      eprintln!(
        "supervisor {agent} in chat {chat_id}: the handover owed does not read, so it is let go: {why}"
      );
      settle_room(db, agent, chat_id).await?;

      Ok(None)
    }
  }
}

/// A room's handover settled: made, or nothing left to hand over.
pub async fn settle_room(db: &impl ConnectionTrait, agent: &str, chat_id: i64) -> Result<(), Refusal> {
  settings::forget(db, &room_key(agent, chat_id)).await
}

/// Every room whose own session owes a handover, by the agent and the room.
pub async fn rooms_owing(db: &impl ConnectionTrait) -> Result<Vec<(String, i64)>, Refusal> {
  Ok(
    settings::named(db, ROOM_PREFIX)
      .await?
      .iter()
      .filter_map(|name| {
        let (agent, chat) = name.rsplit_once(':')?;

        Some((agent.to_owned(), chat.parse().ok()?))
      })
      .collect(),
  )
}

fn lost_key(session: &str) -> String {
  format!("handoff_lost:{session}")
}

/// A supervisor's session, by the id the receiver names it by, written down as opened without the handoff it was
/// owed, with the moment, so its state says so until a turn begun after it has answered (docs/sessions.md, 「A
/// restart hands the session over」).
pub async fn lose(db: &impl ConnectionTrait, session: &str) -> Result<(), Refusal> {
  settings::set(db, &lost_key(session), &crate::now_ms().to_string()).await
}

/// Whether a supervisor's session was opened without the handoff it was owed and no turn begun since has answered.
pub async fn lost(db: &impl ConnectionTrait, session: &str) -> Result<bool, Refusal> {
  Ok(settings::get(db, &lost_key(session)).await?.is_some())
}

/// A turn of the session begun at `began` answered: a handoff lost before it began is told no more, since that
/// turn was the one the receiver opened on what it keeps; one lost while it ran, under the turn itself, is told
/// to the next.
pub async fn answered_since(db: &impl ConnectionTrait, session: &str, began: i64) -> Result<(), Refusal> {
  let at = settings::get(db, &lost_key(session)).await?.and_then(|at| at.parse::<i64>().ok());

  match at {
    Some(at) if at < began => settings::forget(db, &lost_key(session)).await,
    _ => Ok(()),
  }
}
