//! An account's own node and its other designations (docs/person-nodes.md, 「Which node is a person's」): each
//! account is bound to one machine of its own, and stands on further nodes by designation, many accounts a
//! node and many nodes an account. An account's own node belongs to it alone.

use sea_orm_migration::prelude::*;

pub const NAME: &str = "m20270125_000003_person_nodes_own";

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
      .execute_unprepared(
        r#"ALTER TABLE "person_nodes" ADD COLUMN IF NOT EXISTS "own" boolean NOT NULL DEFAULT false;
           CREATE UNIQUE INDEX IF NOT EXISTS "person_nodes_one_own"
             ON "person_nodes" ("owner_account") WHERE "own";"#,
      )
      .await?;

    Ok(())
  }

  async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
    Err(DbErr::Migration(
      "a migration of the store is not undone; its database is dropped instead".into(),
    ))
  }
}
