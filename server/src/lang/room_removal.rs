//! What the removal of a room's registration says of the room before anything is taken
//! (docs/chat-providers.md, 「Removing a room's registration」).

use serde::Deserialize;

/// Why a room is not removed.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Refused {
  pub seeded: String,
  pub infra: String,
  pub owner: String,
  pub administrator: String,
  pub chat: String,
}

/// What the confirmation warns of.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Warned {
  pub answerers: String,
  pub items: String,
  pub questions: String,
  pub private: String,
  pub road: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomRemoval {
  pub refused: Refused,
  pub warned: Warned,
}

crate::part!(pub ROOM_REMOVAL: RoomRemoval = "room-removal");
