//! A phone's key (docs/accounts.md, 「Phones」): a secured API key of sumida's bound to the user, made,
//! read, carried over from a token issued before the model, and deleted. The console keeps the phone's own
//! name and times beside the key's hash.

use chrono::Utc;
use rand::Rng;
use rand::distr::Alphanumeric;
use sea_orm::ConnectionTrait;
use serde::Serialize;
use sumida::api::service::api_key::hash_api_key;

use crate::ids::shown;
use crate::{Refusal, one, run};

/// A key as it is handed to the phone, once, with the hash it is kept by.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Made {
  pub key: String,
  pub hash: String,
}

/// The user a key belongs to.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Holder {
  pub user: String,
  pub name: String,
  pub hash: String,
}

/// The first characters of a key, which sumida keeps to tell keys apart on a screen.
fn prefix(key: &str) -> String {
  key.chars().take(8).collect()
}

async fn insert<C: ConnectionTrait>(db: &C, key: &str, user: i32) -> Result<String, Refusal> {
  let hash = hash_api_key(key);

  one(db, r#"SELECT "id" FROM "users" WHERE "id" = $1"#, vec![user.into()])
    .await?
    .ok_or_else(|| Refusal::absent(format!("no user {user}")))?;
  run(
    db,
    r#"INSERT INTO "api_keys" ("key", "key_prefix", "user_id", "expires_at", "is_secured", "created_at")
       VALUES ($1, $2, $3, NULL, true, $4)
       ON CONFLICT ("key") DO NOTHING"#,
    vec![hash.clone().into(), prefix(key).into(), user.into(), Utc::now().into()],
  )
  .await?;

  Ok(hash)
}

/// A new key for a user's phone.
pub async fn make<C: ConnectionTrait>(db: &C, user: i32) -> Result<Made, Refusal> {
  let key: String = rand::rng().sample_iter(&Alphanumeric).take(64).map(char::from).collect();
  let hash = insert(db, &key, user).await?;

  Ok(Made { key, hash })
}

/// A token issued before the model, carried onto a key of the same text, so the phone goes on presenting
/// what it holds.
pub async fn adopt<C: ConnectionTrait>(db: &C, token: &str, user: i32) -> Result<String, Refusal> {
  if token.len() < 32 {
    return Err(Refusal::bad("a phone's token is longer than that"));
  }

  insert(db, token, user).await
}

/// Whose a key is, where it is a live key of a user who may sign in.
pub async fn holder<C: ConnectionTrait>(db: &C, key: &str) -> Result<Holder, Refusal> {
  let hash = hash_api_key(key);
  let row = one(
    db,
    r#"SELECT u."id", u."username" FROM "api_keys" k JOIN "users" u ON u."id" = k."user_id"
       WHERE k."key" = $1 AND (k."expires_at" IS NULL OR k."expires_at" > now()) AND u."is_enabled""#,
    vec![hash.clone().into()],
  )
  .await?
  .ok_or_else(|| Refusal::new(axum::http::StatusCode::UNAUTHORIZED, "no live key answers to that"))?;

  Ok(Holder {
    user: shown(row.try_get("", "id")?),
    name: row.try_get("", "username")?,
    hash,
  })
}

/// A key taken away by its hash; how many went.
pub async fn delete<C: ConnectionTrait>(db: &C, hash: &str) -> Result<u64, Refusal> {
  run(db, r#"DELETE FROM "api_keys" WHERE "key" = $1"#, vec![hash.to_owned().into()]).await
}
