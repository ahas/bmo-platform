//! A role's agent stands on a home node (docs/role-agents.md, 「What an agent is」): the node its runs go out
//! on, where the deployment places it, none for this machine.

use sea_orm_migration::prelude::*;

pub const NAME: &str = "m20270125_000002_agent_home_node";

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
      .execute_unprepared(r#"ALTER TABLE "role_agents" ADD COLUMN IF NOT EXISTS "home_node" text;"#)
      .await?;

    Ok(())
  }

  async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
    Err(DbErr::Migration(
      "a migration of the store is not undone; its database is dropped instead".into(),
    ))
  }
}
