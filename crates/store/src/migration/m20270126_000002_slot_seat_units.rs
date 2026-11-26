//! A slot request keeps the units of its account's path, so the seats it may take and the cap it stands under
//! are judged from its own row (docs/role-slots.md, 「Accounts」; docs/role-agents.md, 「A unit's seats run
//! their people's work」).

use sea_orm_migration::prelude::*;

pub const NAME: &str = "m20270126_000002_slot_seat_units";

pub struct Migration;

impl MigrationName for Migration {
  fn name(&self) -> &str {
    NAME
  }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager.get_connection().execute_unprepared(include_str!("slot_seat_units.sql")).await?;

    Ok(())
  }

  async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
    Err(DbErr::Migration(
      "a migration of the store is not undone; its database is dropped instead".into(),
    ))
  }
}
