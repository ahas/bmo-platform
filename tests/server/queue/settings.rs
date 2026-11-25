//! Agency's own settings rows: a value set, read, taken once, and the numbers a family of marks is kept for.

use agency_server::queue::settings::{forget, get, numbered, set, take};

use crate::support::scratch::Scratch;

#[tokio::test]
async fn a_value_is_set_read_taken_once_and_its_family_numbered() {
  let scratch = Scratch::new().await;
  let db = scratch.db();

  set(db, "job_stop:12", "first").await.unwrap();
  set(db, "job_stop:12", "asked").await.unwrap();
  set(db, "job_stop:3", "asked").await.unwrap();
  set(db, "job_stop_other", "x").await.unwrap();

  assert_eq!(get(db, "job_stop:12").await.unwrap().as_deref(), Some("asked"));
  assert_eq!(numbered(db, "job_stop:").await.unwrap(), [3, 12]);
  assert_eq!(take(db, "job_stop:3").await.unwrap().as_deref(), Some("asked"));
  assert_eq!(take(db, "job_stop:3").await.unwrap(), None);

  forget(db, "job_stop:12").await.unwrap();
  assert_eq!(get(db, "job_stop:12").await.unwrap(), None);
}
