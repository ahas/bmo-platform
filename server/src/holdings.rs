//! What the service holds for a room (docs/accounts.md, 「A room's holdings」): the grants standing on it,
//! and the senders holding a grant there as their user. The receiver asks it before it removes a room.

use bmo_web_client::Holding as HeldHolding;
use sea_orm::ConnectionTrait;

use crate::checks::ROOM;
use crate::receiver::Holding;
use crate::users::member_of;
use crate::{Refusal, one};

/// The name the service declares itself a holder under.
pub const UNIT: &str = "web";

/// The holdings the service declares, with what a room's removal does to each.
pub const HOLDINGS: [Holding; 2] = [
  Holding {
    name: "room_grants",
    effect: "warns",
  },
  Holding {
    name: "room_senders",
    effect: "kept",
  },
];

async fn count<C: ConnectionTrait>(db: &C, sql: &str, values: Vec<sea_orm::Value>) -> Result<i64, Refusal> {
  Ok(
    one(db, sql, values)
      .await?
      .map(|row| row.try_get::<i64>("", "count"))
      .transpose()?
      .unwrap_or(0),
  )
}

/// What a room holds of the service's, each holding it has at least one of: the grants made on the room or
/// to its members, and the senders whose user holds a grant on the room.
pub async fn held<C: ConnectionTrait>(db: &C, room: i64) -> Result<Vec<HeldHolding>, Refusal> {
  let grants = count(
    db,
    r#"SELECT COUNT(*) AS "count" FROM "relation_tuples"
       WHERE ("resource_type" = $1 AND "resource_id" = $2) OR "subject_key" = $3"#,
    vec![ROOM.into(), room.to_string().into(), member_of(room).into()],
  )
  .await?;
  let senders = count(
    db,
    r#"SELECT COUNT(DISTINCT s."sender_id") AS "count" FROM "relation_tuples" t
       JOIN "user_senders" s ON t."subject_key" = 'user:' || s."user_id"
       WHERE t."resource_type" = $1 AND t."resource_id" = $2"#,
    vec![ROOM.into(), room.to_string().into()],
  )
  .await?;

  Ok(
    [("room_grants", grants), ("room_senders", senders)]
      .into_iter()
      .filter(|(_, count)| *count > 0)
      .map(|(code, count)| HeldHolding {
        code: code.to_owned(),
        count,
      })
      .collect(),
  )
}
