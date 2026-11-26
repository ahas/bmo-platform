//! Every owned row names its owner in the platform's two columns, `owner_account` or `owner_unit`, never both
//! and neither for the platform's own; a seat's account is a number, and the unit a run's ledger row names its
//! work by is the `work_unit` (docs/store.md, 「Its tables」).

use sea_orm_migration::prelude::*;

pub const NAME: &str = "m20270126_000001_platform_ownership";

pub struct Migration;

impl MigrationName for Migration {
  fn name(&self) -> &str {
    NAME
  }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .get_connection()
      .execute_unprepared(include_str!("platform_ownership.sql"))
      .await?;

    Ok(())
  }

  async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
    Err(DbErr::Migration(
      "a migration of the store is not undone; its database is dropped instead".into(),
    ))
  }
}
