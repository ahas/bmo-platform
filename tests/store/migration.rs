//! The migrations make every table of the store in an empty database from the 1.0.0 baseline, with every
//! migration made since laid on top, and record what ran in the store's own record; an install holding the
//! record of the last release before 1.0.0 is taken as it stands and gains only what came after the baseline,
//! and one holding part of it is refused (docs/store.md, 「Its tables」).

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use store::migration::{
  BASELINE, BEFORE, MIGRATIONS_TABLE, Migrator, OWN_TABLES, OWNERSHIP, POST_BASELINE, SEAT_UNITS,
};

use crate::support::scratch::Scratch;

// The shapes the migrations after the baseline make, applied by hand beside it.
const OWNERSHIP_SQL: &str = include_str!("../../crates/store/src/migration/platform_ownership.sql");
const SEAT_UNITS_SQL: &str = include_str!("../../crates/store/src/migration/slot_seat_units.sql");

async fn tables(scratch: &Scratch) -> Vec<String> {
  let rows = store::all(
    scratch.db(),
    "SELECT table_name::text AS name FROM information_schema.tables \
     WHERE table_schema = current_schema() AND table_type = 'BASE TABLE' ORDER BY table_name",
    vec![],
  )
  .await
  .unwrap();

  rows.iter().map(|row| row.try_get::<String>("", "name").unwrap()).collect()
}

#[tokio::test]
async fn the_migrations_make_every_table_of_the_store_and_nothing_else() {
  let scratch = Scratch::blank().await;

  store::migration::up(scratch.db()).await.unwrap();

  let mut expected: Vec<String> = OWN_TABLES.iter().map(|name| (*name).to_owned()).collect();
  expected.push(MIGRATIONS_TABLE.to_owned());
  expected.sort();
  assert_eq!(tables(&scratch).await, expected);
}

#[tokio::test]
async fn what_ran_is_recorded_in_the_stores_own_record_and_nothing_is_left_pending() {
  let scratch = Scratch::new().await;

  let ran = scratch
    .count(
      &format!(r#"SELECT COUNT(*) FROM "{MIGRATIONS_TABLE}" WHERE "version" = $1"#),
      vec![BASELINE.into()],
    )
    .await;

  assert_eq!(ran, 1);
  assert!(Migrator::get_pending_migrations(scratch.db()).await.unwrap().is_empty());
}

#[tokio::test]
async fn the_columns_carried_beyond_a_tables_first_shape_stand_after_its_row_id() {
  let scratch = Scratch::new().await;
  let columns = |table: &'static str| {
    let db = scratch.db();
    async move {
      store::all(
        db,
        "SELECT column_name::text AS name FROM information_schema.columns \
         WHERE table_schema = current_schema() AND table_name = $1 ORDER BY ordinal_position",
        vec![table.into()],
      )
      .await
      .unwrap()
      .iter()
      .map(|row| row.try_get::<String>("", "name").unwrap())
      .collect::<Vec<_>>()
    }
  };

  let roles = columns("roles").await;
  assert_eq!(
    roles[roles.len() - 9..],
    [
      "sessions",
      "description",
      "line",
      "provider_order",
      "fold_ratio",
      "active_provider",
      "active_model",
      "skills",
      "follows_asker"
    ]
  );
  let accounts = columns("claude_accounts").await;
  assert_eq!(
    accounts[accounts.len() - 9..],
    [
      "rowid",
      "owner_account",
      "endpoint_url",
      "key_file",
      "endpoint_models",
      "base_prompt",
      "node",
      "lendable",
      "owner_unit"
    ]
  );
  let settings = columns("supervisor_settings").await;
  assert_eq!(settings[settings.len() - 3..], ["saved_settings", "brain_on", "tools_allowed"]);
  let room_sessions = columns("room_sessions").await;
  assert_eq!(room_sessions[room_sessions.len() - 3..], ["rowid", "spent_calls", "spent_ms"]);
}

#[tokio::test]
async fn a_row_written_with_only_its_required_columns_takes_the_defaults_the_store_declares() {
  let scratch = Scratch::new().await;

  scratch
    .run(
      r#"INSERT INTO "repositories" ("name", "path", "created_at", "updated_at") VALUES ($1, $2, 0, 0)"#,
      vec!["worker-one".into(), "/checkout".into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ($1, $2)"#,
      vec!["rest_kind".into(), "day".into()],
    )
    .await;

  let row = store::one(
    scratch.db(),
    r#"SELECT "downgrade_heal", "support_count", "rowid" FROM "repositories" WHERE "name" = $1"#,
    vec!["worker-one".into()],
  )
  .await
  .unwrap()
  .unwrap();

  assert_eq!(row.try_get::<i64>("", "downgrade_heal").unwrap(), 1);
  assert_eq!(row.try_get::<Option<i64>>("", "support_count").unwrap(), None, "auto");
  assert!(row.try_get::<i64>("", "rowid").unwrap() > 0);
  assert_eq!(scratch.count(r#"SELECT COUNT(*) FROM "settings""#, vec![]).await, 1);
}

#[tokio::test]
async fn a_base_prompt_an_account_names_cannot_be_removed_and_a_missing_one_cannot_be_named() {
  let scratch = Scratch::new().await;
  let account = |id: &'static str, prompt: &'static str| {
    let db = scratch.db();
    async move {
      store::run(
        db,
        r#"INSERT INTO "claude_accounts" ("id", "name", "place", "credential_kind", "credential_path",
             "added_at", "endpoint_url", "key_file", "endpoint_models", "base_prompt")
           VALUES ($1, $1, 0, 'endpoint', '/home', 0, 'https://endpoint.test', '/key', '{"opus":"pro"}', $2)"#,
        vec![id.into(), prompt.into()],
      )
      .await
    }
  };
  let remove_kept =
    || store::run(scratch.db(), r#"DELETE FROM "base_prompts" WHERE "name" = 'kept'"#, vec![]);
  scratch
    .run(
      r#"INSERT INTO "base_prompts" ("name", "label", "text") VALUES ('kept', 'Kept', 'text')"#,
      vec![],
    )
    .await;

  account("endpoint", "kept").await.unwrap();
  assert!(account("stray", "missing").await.is_err());
  assert!(remove_kept().await.is_err());

  scratch.run(r#"UPDATE "claude_accounts" SET "base_prompt" = NULL"#, vec![]).await;
  remove_kept().await.unwrap();
}

const SCHEMA: &str = include_str!("../../crates/store/src/migration/baseline.sql");

// Every column and index of the store but the record's, in one line.
fn shape_sql() -> String {
  format!(
    "SELECT COALESCE((SELECT string_agg(table_name || '.' || column_name || ' ' || data_type || ' ' || \
     is_nullable || ' ' || COALESCE(column_default, ''), ', ' ORDER BY table_name, ordinal_position) \
     FROM information_schema.columns WHERE table_schema = current_schema() AND table_name <> '{MIGRATIONS_TABLE}'), '') \
     || ' / ' || COALESCE((SELECT string_agg(indexdef, ', ' ORDER BY indexname) FROM pg_indexes \
     WHERE schemaname = current_schema() AND tablename <> '{MIGRATIONS_TABLE}'), '') AS shape"
  )
}

async fn texts(db: &DatabaseConnection, sql: &str) -> Vec<String> {
  db.query_all(Statement::from_string(DbBackend::Postgres, sql.to_owned()))
    .await
    .unwrap()
    .iter()
    .map(|row| row.try_get_by_index::<String>(0).unwrap())
    .collect()
}

async fn shape(db: &DatabaseConnection) -> Vec<String> {
  texts(db, &shape_sql()).await
}

async fn recorded(db: &DatabaseConnection) -> Vec<String> {
  texts(db, &format!(r#"SELECT "version" FROM "{MIGRATIONS_TABLE}" ORDER BY "version""#)).await
}

// Every migration this build carries, the baseline first and the ones that ran after it.
fn every_run() -> Vec<String> {
  std::iter::once(BASELINE.to_owned())
    .chain(POST_BASELINE.iter().map(|name| (*name).to_owned()))
    .chain([OWNERSHIP.to_owned(), SEAT_UNITS.to_owned()])
    .collect()
}

// A blank database holding the baseline's schema and a row, recorded as an install from before 1.0.0 that ran
// the first `through` migrations of its last release.
async fn installed(through: usize) -> Scratch {
  let scratch = Scratch::blank().await;
  let db = scratch.db();

  db.execute_unprepared(SCHEMA).await.unwrap();
  db.execute_unprepared(&format!(
    r#"CREATE TABLE "{MIGRATIONS_TABLE}" ("version" varchar PRIMARY KEY, "applied_at" bigint NOT NULL)"#
  ))
  .await
  .unwrap();
  for version in &BEFORE[..through] {
    db.execute_unprepared(&format!(
      r#"INSERT INTO "{MIGRATIONS_TABLE}" ("version", "applied_at") VALUES ('{version}', 1)"#
    ))
    .await
    .unwrap();
  }
  scratch
    .run(r#"INSERT INTO "settings" ("key", "value") VALUES ('rest_kind', 'day')"#, vec![])
    .await;

  scratch
}

#[tokio::test]
async fn a_blank_database_gets_the_baseline_and_every_migration_made_since() {
  let migrated = Scratch::blank().await;
  assert!(store::migration::up(migrated.db()).await.unwrap());
  let laid = Scratch::blank().await;
  laid.db().execute_unprepared(SCHEMA).await.unwrap();
  laid.db().execute_unprepared(OWNERSHIP_SQL).await.unwrap();
  laid.db().execute_unprepared(SEAT_UNITS_SQL).await.unwrap();

  assert_eq!(shape(migrated.db()).await, shape(laid.db()).await);
  assert_eq!(recorded(migrated.db()).await, every_run());
  assert!(!store::migration::up(migrated.db()).await.unwrap());
}

#[tokio::test]
async fn an_install_at_the_last_release_before_the_baseline_only_has_its_record_rewritten_and_gains_the_migrations_since()
 {
  let scratch = installed(BEFORE.len()).await;
  let db = scratch.db();
  let current = {
    let fresh = Scratch::blank().await;
    store::migration::up(fresh.db()).await.unwrap();
    shape(fresh.db()).await
  };

  // The record is rewritten to name the baseline alone, touching no table; the migrations made since the
  // baseline then run onto the standing shape, which needs the renames and the new columns.
  assert!(!store::migration::up(db).await.unwrap());
  assert_eq!(recorded(db).await, every_run());
  assert_eq!(
    shape(db).await,
    current,
    "the carried migrations bring the laid baseline to this build's shape"
  );
  assert_eq!(texts(db, r#"SELECT "key" || '=' || "value" FROM "settings""#).await, ["rest_kind=day"]);

  // A second start finds every migration recorded and changes nothing.
  assert!(!store::migration::up(db).await.unwrap());
  assert_eq!(recorded(db).await, every_run());
  assert_eq!(shape(db).await, current);
}

#[tokio::test]
async fn an_install_short_of_the_last_release_before_the_baseline_is_refused() {
  let scratch = installed(3).await;
  let db = scratch.db();
  let before = shape(db).await;

  let refused = store::migration::up(db).await.unwrap_err().to_string();
  assert!(refused.contains(BEFORE[2]), "{refused}");
  assert_eq!(recorded(db).await, BEFORE[..3]);
  assert_eq!(shape(db).await, before);
}
