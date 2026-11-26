//! An account stands on several nodes (docs/person-nodes.md, 「Which node is a person's」): a person's record
//! holds one row a node, so the same account may name the machine its runs go out on and every other machine
//! it owns, and every one of them reads that account among its owners.

use sea_orm_migration::prelude::*;

pub const NAME: &str = "m20270125_000001_person_nodes_many";

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
        r#"ALTER TABLE "person_nodes" DROP CONSTRAINT "person_nodes_pkey";
           ALTER TABLE "person_nodes" ADD PRIMARY KEY ("owner_account", "node");"#,
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
