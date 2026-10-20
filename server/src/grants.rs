//! Granting and revoking (docs/accounts.md, 「Grants」). The granter is the sender of the message a grant is
//! made on, as the receiver says, and sumida checks they may grant what they give, so a grant is never made
//! on anyone's word but the holder's of that authority. The granter's words stand beside the tuple.

pub use bmo_web_client::{GrantRow, Granted, Revoked};
use chrono::{DateTime, Utc};
use sea_orm::ConnectionTrait;
use serde::Deserialize;
use sumida::api::authz::Authz;
use sumida::api::authz::write::{SubjectKey, TupleFilter, TupleWrite};

use crate::checks::{ROOM, declared, grant_of};
use crate::ids::shown;
use crate::receiver::Receiver;
use crate::users::{member_of, sender_subject, user_for_sender, user_of_sender};
use crate::{Refusal, now_ms, one, run};

/// What a grant names: a room and a relation there, or a kind, an object and a relation.
#[derive(Debug, Deserialize)]
pub struct Target {
  pub room: Option<i64>,
  pub kind: Option<String>,
  pub object: Option<String>,
  pub relation: String,
}

/// Whom a grant is to: a sender id, a user, or every member of the room.
#[derive(Debug, Deserialize)]
pub struct Holder {
  pub sender: Option<i64>,
  #[serde(default, deserialize_with = "crate::ids::user_or_none")]
  pub user: Option<i32>,
  #[serde(default)]
  pub members: bool,
}

/// The message a grant is made or revoked on.
#[derive(Debug, Deserialize)]
pub struct On {
  pub room: i64,
  pub id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantAsk {
  #[serde(flatten)]
  pub target: Target,
  pub to: Holder,
  pub words: Option<String>,
  /// Epoch milliseconds.
  pub expires_at: Option<i64>,
  pub message: On,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokeAsk {
  #[serde(flatten)]
  pub target: Target,
  pub to: Holder,
  pub message: On,
}

/// The grants standing on a room: those made on the room itself, with the words beside each, in the order
/// they were written; a grant past its expiry is gone.
pub async fn standing_in<C: ConnectionTrait>(db: &C, room: i64) -> Result<Vec<GrantRow>, Refusal> {
  let rows = crate::all(
    db,
    r#"SELECT t."relation", t."subject_key",
              CAST(EXTRACT(EPOCH FROM t."expires_at") * 1000 AS bigint) AS "expires_at", w."words"
       FROM "relation_tuples" t
       LEFT JOIN "grant_words" w ON w."resource_type" = t."resource_type" AND w."resource_id" = t."resource_id"
         AND w."relation" = t."relation" AND w."subject_key" = t."subject_key"
       WHERE t."resource_type" = $1 AND t."resource_id" = $2
         AND (t."expires_at" IS NULL OR t."expires_at" > now())
       ORDER BY t."id""#,
    vec![ROOM.into(), room.to_string().into()],
  )
  .await?;

  rows
    .iter()
    .map(|row| {
      Ok(GrantRow {
        relation: row.try_get("", "relation")?,
        subject_key: row.try_get("", "subject_key")?,
        expires_at: row.try_get("", "expires_at")?,
        words: row.try_get("", "words")?,
      })
    })
    .collect()
}

/// The kind and object a target names.
fn object_of(target: &Target) -> Result<(String, String), Refusal> {
  match (&target.room, &target.kind, &target.object) {
    (Some(room), None, None) => Ok((ROOM.to_owned(), room.to_string())),
    (None, Some(kind), Some(object)) if kind != ROOM => Ok((kind.clone(), object.clone())),
    _ => Err(Refusal::bad("name a room, or a kind other than a room with its object")),
  }
}

/// The granter's subjects, read off the message the grant is made on; a grant on a room is made on a
/// message in that room.
async fn granter<C: ConnectionTrait>(
  db: &C,
  receiver: &dyn Receiver,
  kind: &str,
  object: &str,
  on: &On,
) -> Result<sumida::api::authz::subject::SubjectSet, Refusal> {
  if kind == ROOM && object != on.room.to_string() {
    return Err(Refusal::forbidden(format!(
      "a grant in room {object} is made on a message in that room, and message {} stands in room {}",
      on.id, on.room
    )));
  }

  let sender = receiver.sender_of(on.room, on.id).await?;
  let (subject, _) = sender_subject(db, on.room, sender).await?;

  if !Authz::check(kind, object, "grant", &subject, db).await? {
    return Err(Refusal::forbidden(format!(
      "the sender of message {} in room {} may not grant on {kind}:{object}",
      on.id, on.room
    )));
  }

  Ok(subject)
}

/// The subject a holder is, made into a user where a sender has none and `make` says so.
async fn holder_key<C: ConnectionTrait>(
  db: &C,
  kind: &str,
  object: &str,
  to: &Holder,
  make: bool,
) -> Result<Option<(SubjectKey, Option<i32>)>, Refusal> {
  match (to.sender, to.user, to.members) {
    (Some(sender), None, false) => {
      let user = if make {
        Some(user_for_sender(db, sender).await?)
      } else {
        user_of_sender(db, sender).await?
      };

      Ok(user.map(|user| (SubjectKey::user(user), Some(user))))
    }
    (None, Some(user), false) => {
      one(db, r#"SELECT "id" FROM "users" WHERE "id" = $1"#, vec![user.into()])
        .await?
        .ok_or_else(|| Refusal::absent(format!("no user {user}")))?;

      Ok(Some((SubjectKey::user(user), Some(user))))
    }
    (None, None, true) if kind == ROOM => {
      let room: i64 = object.parse().map_err(|_| Refusal::bad(format!("`{object}` is no room")))?;

      Ok(Some((SubjectKey::parse(&member_of(room))?, None)))
    }
    _ => Err(Refusal::bad("name one holder: a sender, a user, or the room's members")),
  }
}

/// A grant written on the granter's authority, with the granter's words beside it where any.
pub async fn grant<C: ConnectionTrait>(
  db: &C,
  receiver: &dyn Receiver,
  ask: GrantAsk,
) -> Result<Granted, Refusal> {
  let (kind, object) = object_of(&ask.target)?;

  declared(&kind, &ask.target.relation)?;

  let subject = granter(db, receiver, &kind, &object, &ask.message).await?;
  let (key, user) = holder_key(db, &kind, &object, &ask.to, true)
    .await?
    .ok_or_else(|| Refusal::bad("the holder could not be named"))?;
  let mut write = TupleWrite::new(&kind, &object, &ask.target.relation, key.clone());

  if let Some(at) = ask.expires_at {
    let at =
      DateTime::<Utc>::from_timestamp_millis(at).ok_or_else(|| Refusal::bad("the expiry is no time"))?;

    write = write.expiring(at);
  }
  Authz::write(write, &subject, db).await?;

  if let Some(words) = ask.words.as_deref().map(str::trim).filter(|words| !words.is_empty()) {
    run(
      db,
      r#"INSERT INTO "grant_words"
           ("resource_type", "resource_id", "relation", "subject_key", "words", "message_chat", "message_id", "written_at")
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT ("resource_type", "resource_id", "relation", "subject_key") DO UPDATE SET
           "words" = EXCLUDED."words", "message_chat" = EXCLUDED."message_chat",
           "message_id" = EXCLUDED."message_id", "written_at" = EXCLUDED."written_at""#,
      vec![
        kind.clone().into(),
        object.clone().into(),
        ask.target.relation.clone().into(),
        key.key().into(),
        words.to_owned().into(),
        ask.message.room.into(),
        ask.message.id.into(),
        now_ms().into(),
      ],
    )
    .await?;
  }

  Ok(Granted {
    grant: grant_of(&kind, &object, &ask.target.relation),
    subject: key.key(),
    user: user.map(shown),
  })
}

/// A grant taken back on the granter's authority, with its words.
pub async fn revoke<C: ConnectionTrait>(
  db: &C,
  receiver: &dyn Receiver,
  ask: RevokeAsk,
) -> Result<Revoked, Refusal> {
  let (kind, object) = object_of(&ask.target)?;

  declared(&kind, &ask.target.relation)?;

  let subject = granter(db, receiver, &kind, &object, &ask.message).await?;
  let grant = grant_of(&kind, &object, &ask.target.relation);
  let Some((key, _)) = holder_key(db, &kind, &object, &ask.to, false).await? else {
    return Ok(Revoked { grant, revoked: 0 });
  };
  let filter = TupleFilter::object(&kind, &object)
    .relations(&[ask.target.relation.as_str()])
    .subject(key.clone());
  let revoked = Authz::revoke(filter, &subject, db).await?;

  run(
    db,
    r#"DELETE FROM "grant_words"
       WHERE "resource_type" = $1 AND "resource_id" = $2 AND "relation" = $3 AND "subject_key" = $4"#,
    vec![kind.into(), object.into(), ask.target.relation.clone().into(), key.key().into()],
  )
  .await?;

  Ok(Revoked {
    grant,
    revoked: revoked as i64,
  })
}
