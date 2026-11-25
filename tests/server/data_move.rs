//! The data move's mark: a store without it reads as not moved, and one holding it as moved.

use agency_server::data_move::{MOVED, moved};

use crate::support::scratch::Scratch;

#[tokio::test]
async fn the_move_is_done_once_its_mark_is_held() {
  let scratch = Scratch::new().await;

  assert!(!moved(scratch.db()).await.unwrap());

  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ($1, '1700000000000')"#,
      vec![MOVED.into()],
    )
    .await;

  assert!(moved(scratch.db()).await.unwrap());
}
