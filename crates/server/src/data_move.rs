//! The data move's mark in agency's own settings (docs/store.md, 「The settings table」): whether the
//! store's rows were moved across. Agency reads the mark and never writes it; a store without it reads
//! as not moved.

use sea_orm::ConnectionTrait;

use crate::Refusal;

/// The settings key the move is marked done under.
pub const MOVED: &str = "users_moved_at";

/// Whether the move is done.
pub async fn moved<C: ConnectionTrait>(db: &C) -> Result<bool, Refusal> {
  Ok(
    store::one(db, r#"SELECT 1 AS "held" FROM "settings" WHERE "key" = $1"#, vec![MOVED.into()])
      .await?
      .is_some(),
  )
}
