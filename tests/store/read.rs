//! A tool's read-only connection on the store, and the statements bound and run on a pool.

use sqlx::Row;
use store::{number, open_read};

use crate::support::scratch::Scratch;

#[tokio::test]
async fn a_read_connection_reads_numbers_however_they_were_stored() {
  let scratch = Scratch::new().await;

  scratch
    .run(
      r#"INSERT INTO "model_catalogue" ("name", "label", "file", "bytes", "sha256", "temperature", "added_at")
         VALUES ('entry', 'Entry', 'entry.gguf', 7, 'aa', 0.5, 1700000000)"#,
      vec![],
    )
    .await;

  let mut store = open_read(&scratch.address()).await.unwrap();
  let row =
    sqlx::query(r#"SELECT "label", "added_at", "temperature" FROM "model_catalogue" WHERE "name" = $1"#)
      .bind("entry")
      .fetch_one(&mut store)
      .await
      .unwrap();

  assert_eq!(row.try_get::<String, _>("label").unwrap(), "Entry");
  assert_eq!(number(&row, "added_at").unwrap(), Some(1_700_000_000.0));
  assert_eq!(number(&row, "temperature").unwrap(), Some(0.5));
}

#[tokio::test]
async fn a_read_connection_never_writes() {
  let scratch = Scratch::new().await;

  let mut store = open_read(&scratch.address()).await.unwrap();
  let written = sqlx::query(r#"INSERT INTO "settings" ("key", "value") VALUES ('rest_kind', 'day')"#)
    .execute(&mut store)
    .await;

  assert!(written.is_err());
  assert_eq!(scratch.count(r#"SELECT COUNT(*) FROM "settings""#, vec![]).await, 0);
}

#[tokio::test]
async fn a_statement_binds_every_number_at_its_widest_on_a_pool() {
  let scratch = Scratch::new().await;
  let db = store::connect(&scratch.address(), store::pool(2)).await.unwrap();

  let written = store::run(
    &db,
    r#"INSERT INTO "node_assignments" ("node", "kind", "count") VALUES ($1, $2, $3)"#,
    vec!["node-a".into(), "worker".into(), 3_i32.into()],
  )
  .await
  .unwrap();
  let row =
    store::one(&db, r#"SELECT "count" FROM "node_assignments" WHERE "count" = $1"#, vec![3_u16.into()])
      .await
      .unwrap()
      .unwrap();

  assert_eq!(written, 1);
  assert_eq!(row.try_get::<i64>("", "count").unwrap(), 3);
  assert!(
    store::all(&db, r#"SELECT 1 FROM "node_assignments" WHERE "count" > $1"#, vec![9_i64.into()])
      .await
      .unwrap()
      .is_empty()
  );
}
