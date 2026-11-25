//! A cancellation covers the run asked for, its descendants and what is rooted in it, is taken back
//! once the work it was raised over has ended, and is swept once its grace has passed.

use agency_server::queue::cancels::{MARKER_GRACE, covered, swept, take_back};
use agency_server::queue::running::EXECUTION_CANCEL_PREFIX;
use serde_json::json;

use crate::support::scratch::Scratch;

#[test]
fn a_cancellation_covers_its_descendants_and_what_is_rooted_in_it_and_nothing_else() {
  let rows = [
    json!({ "id": "child", "parent": "run" }),
    json!({ "id": "grandchild", "parent": "child" }),
    json!({ "id": "rooted", "root": "run" }),
    json!({ "id": "other", "parent": "elsewhere", "root": "place" }),
  ];

  assert_eq!(
    covered(&rows, "run").into_iter().collect::<Vec<_>>(),
    ["child", "grandchild", "rooted", "run"]
  );
}

#[tokio::test]
async fn taking_a_cancellation_back_drops_the_markers_it_raised_and_no_other() {
  let scratch = Scratch::new().await;

  for (id, body) in [
    ("child", json!({ "id": "child", "parent": "run" })),
    ("other", json!({ "id": "other" })),
  ] {
    scratch
      .run(
        r#"INSERT INTO "agent_executions" ("id", "body") VALUES ($1, $2)"#,
        vec![id.into(), body.to_string().into()],
      )
      .await;
  }

  for name in ["run", "child", "other"] {
    scratch
      .run(
        r#"INSERT INTO "settings" ("key", "value") VALUES ($1, '1')"#,
        vec![format!("{EXECUTION_CANCEL_PREFIX}{name}").into()],
      )
      .await;
  }

  let mut dropped = take_back(scratch.db(), "run").await.unwrap();

  dropped.sort();
  assert_eq!(dropped, ["child", "run"]);
  assert_eq!(scratch.count(r#"SELECT COUNT(*) FROM "settings""#, vec![]).await, 1);
}

#[tokio::test]
async fn the_sweep_takes_every_marker_past_its_grace_and_leaves_a_younger_one_and_every_other_setting() {
  let scratch = Scratch::new().await;
  let now = 10_000_000_000_i64;
  let grace = i64::try_from(MARKER_GRACE.as_millis()).unwrap();

  for (key, raised) in [
    (format!("{EXECUTION_CANCEL_PREFIX}old"), now - grace - 1),
    (format!("{EXECUTION_CANCEL_PREFIX}young"), now - grace + 1),
    // A key the prefix's underscore would match were it read as any letter.
    ("executionXcancel:old".to_owned(), now - grace - 1),
    ("reset_point:agent".to_owned(), 1),
  ] {
    scratch
      .run(
        r#"INSERT INTO "settings" ("key", "value") VALUES ($1, $2)"#,
        vec![key.into(), raised.to_string().into()],
      )
      .await;
  }

  scratch
    .run(r#"INSERT INTO "settings" ("key", "value") VALUES ('note', 'not a number')"#, vec![])
    .await;

  assert_eq!(swept(scratch.db(), now).await.unwrap(), 1);

  let old = format!("{EXECUTION_CANCEL_PREFIX}old");
  let left = scratch
    .count(r#"SELECT COUNT(*) FROM "settings" WHERE "key" = $1"#, vec![old.into()])
    .await;

  assert_eq!(left, 0, "the marker past its grace is taken");
  assert_eq!(scratch.count(r#"SELECT COUNT(*) FROM "settings""#, vec![]).await, 4, "nothing else is");
}
