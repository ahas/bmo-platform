//! The extension sumida's own tables are made with, made in `public` ahead of them. sumida makes it where its
//! connection's first schema is; in `public` every schema the service is read in reads it.

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
  fn name(&self) -> &str {
    "m20261019_000000_extensions"
  }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .get_connection()
      .execute_unprepared(r#"CREATE EXTENSION IF NOT EXISTS "uuid-ossp" SCHEMA public"#)
      .await?;

    Ok(())
  }

  async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
    // sumida's own migration takes it away as it goes down.
    Ok(())
  }
}
