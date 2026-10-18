//! What a room's delegations say when they are changed from a shell (docs/unified-bmo.md,
//! 「Delegations」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomGrants {
  pub owner_message: String,
}

crate::part!(pub ROOM_GRANTS: RoomGrants = "room-grants");
