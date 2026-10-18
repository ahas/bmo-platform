//! The checks (docs/accounts.md, 「Checks」): whether a sender holds a relation in a room, whether a message's
//! sender does, and whether a user holds a relation on a worker, a repository or a tool capability. Each is
//! one of sumida's point checks over the deployment's rules; nothing here decides by itself.

use sea_orm::ConnectionTrait;
use serde::Serialize;
use sumida::api::authz::Authz;
use sumida::api::authz::subject::SubjectSet;

use crate::Refusal;
use crate::receiver::Receiver;
use crate::rules::RULES;
use crate::users::{sender_subject, user_subject};

/// The kind a room is in the deployment's rules.
pub const ROOM: &str = "bmo_room";

/// What a check answered: whether it opens, the grant it asked for, and the user it was made for.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
  pub allowed: bool,
  pub grant: String,
  pub user: Option<i32>,
}

/// What a check on a message answered, with the sender it was made for.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MessageAnswer {
  pub allowed: bool,
  pub grant: String,
  pub user: Option<i32>,
  pub sender: Option<i64>,
}

/// The permission that opens a relation, as the deployment's rules name it.
fn permission(relation: &str) -> String {
  format!("may_{relation}")
}

/// A kind and relation the deployment's rules declare, or a refusal naming what they do not: a kind of
/// sumida's own is not the deployment's to check here.
pub fn declared(kind: &str, relation: &str) -> Result<(), Refusal> {
  let registry = Authz::registry()?;
  let ours = RULES.iter().any(|file| registry.source_of(kind) == Some(file.name));
  let Some(definition) = registry.get(kind).filter(|_| ours) else {
    return Err(Refusal::bad(format!("`{kind}` is no kind of the deployment's rules")));
  };
  let wanted = permission(relation);

  if definition.permissions.iter().any(|(name, _)| name == &wanted) {
    Ok(())
  } else {
    Err(Refusal::bad(format!("`{kind}` has no relation `{relation}`")))
  }
}

/// The grant a check asks for, as a refusal names it.
pub fn grant_of(kind: &str, object: &str, relation: &str) -> String {
  format!("{kind}:{object}#{relation}")
}

async fn checked<C: ConnectionTrait>(
  db: &C,
  kind: &str,
  object: &str,
  relation: &str,
  subject: &SubjectSet,
) -> Result<bool, Refusal> {
  Ok(Authz::check(kind, object, &permission(relation), subject, db).await?)
}

/// Whether a sender holds a relation in a room. A sender of no linked user is checked as the room's members
/// alone; no sender at all is the members alone as well.
pub async fn check_room<C: ConnectionTrait>(
  db: &C,
  room: i64,
  sender: Option<i64>,
  relation: &str,
) -> Result<Answer, Refusal> {
  declared(ROOM, relation)?;

  let object = room.to_string();
  let (subject, user) = sender_subject(db, room, sender).await?;

  Ok(Answer {
    allowed: checked(db, ROOM, &object, relation, &subject).await?,
    grant: grant_of(ROOM, &object, relation),
    user,
  })
}

/// Whether a message's sender holds a relation in the room the message stands in; the receiver says who
/// sent it, and refuses a message it does not hold there.
pub async fn check_message<C: ConnectionTrait>(
  db: &C,
  receiver: &dyn Receiver,
  room: i64,
  message: i64,
  relation: &str,
) -> Result<MessageAnswer, Refusal> {
  declared(ROOM, relation)?;

  let sender = receiver.sender_of(room, message).await?;
  let answer = check_room(db, room, sender, relation).await?;

  Ok(MessageAnswer {
    allowed: answer.allowed,
    grant: answer.grant,
    user: answer.user,
    sender,
  })
}

/// Whether a user holds a relation on a worker, a repository or a tool capability.
pub async fn check_object<C: ConnectionTrait>(
  db: &C,
  kind: &str,
  object: &str,
  relation: &str,
  user: i32,
) -> Result<Answer, Refusal> {
  declared(kind, relation)?;

  let subject = user_subject(db, user).await?;

  Ok(Answer {
    allowed: checked(db, kind, object, relation, &subject).await?,
    grant: grant_of(kind, object, relation),
    user: Some(user),
  })
}
