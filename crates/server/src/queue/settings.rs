//! Agency's own settings rows (docs/store.md, 「The settings table」), read and written in the store's one
//! shape for a setting: one row a key, its value text.

use sea_orm::ConnectionTrait;
use store::{all, one, run};

use crate::Refusal;

/// The value a key holds, or none where no row holds it.
pub async fn get(db: &impl ConnectionTrait, key: &str) -> Result<Option<String>, Refusal> {
  let row = one(db, r#"SELECT "value" FROM "settings" WHERE "key" = $1"#, vec![key.into()]).await?;

  Ok(row.map(|row| row.try_get::<String>("", "value")).transpose()?)
}

/// The key set to the value, whatever it held before.
pub async fn set(db: &impl ConnectionTrait, key: &str, value: &str) -> Result<(), Refusal> {
  run(
    db,
    r#"INSERT INTO "settings" ("key", "value") VALUES ($1, $2)
       ON CONFLICT ("key") DO UPDATE SET "value" = excluded."value""#,
    vec![key.into(), value.into()],
  )
  .await?;

  Ok(())
}

/// The key's row gone, where there was one.
pub async fn forget(db: &impl ConnectionTrait, key: &str) -> Result<(), Refusal> {
  run(db, r#"DELETE FROM "settings" WHERE "key" = $1"#, vec![key.into()]).await?;

  Ok(())
}

/// The value a key held, taken so nothing reads it twice; none where no row held it.
pub async fn take(db: &impl ConnectionTrait, key: &str) -> Result<Option<String>, Refusal> {
  let row =
    one(db, r#"DELETE FROM "settings" WHERE "key" = $1 RETURNING "value""#, vec![key.into()]).await?;

  Ok(row.map(|row| row.try_get::<String>("", "value")).transpose()?)
}

/// The numbers after a prefix on every key that carries it, in order: the jobs a family of marks is
/// kept for.
pub async fn numbered(db: &impl ConnectionTrait, prefix: &str) -> Result<Vec<i64>, Refusal> {
  let mut ids: Vec<i64> = named(db, prefix).await?.iter().filter_map(|rest| rest.parse().ok()).collect();

  ids.sort_unstable();

  Ok(ids)
}

/// What follows a prefix on every key that carries it: the names a family of marks is kept for.
pub async fn named(db: &impl ConnectionTrait, prefix: &str) -> Result<Vec<String>, Refusal> {
  let rows = all(
    db,
    r#"SELECT "key" FROM "settings" WHERE "key" LIKE $1 ESCAPE '\'"#,
    vec![format!("{}%", prefix.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")).into()],
  )
  .await?;

  Ok(
    rows
      .iter()
      .filter_map(|row| row.try_get::<String>("", "key").ok())
      .filter_map(|key| key.strip_prefix(prefix).map(str::to_owned))
      .collect(),
  )
}
