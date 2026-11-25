//! A JSON value that keeps its keys in the order they were written.

use agency_server::lines::ordered::*;

#[test]
fn an_object_keeps_its_keys_where_they_were_written() {
  let read: Ordered =
    serde_json::from_str(r#"{"type":"stdio","command":"npx","args":["-y"],"env":{}}"#).unwrap();

  assert_eq!(
    serde_json::to_string(&read).unwrap(),
    r#"{"type":"stdio","command":"npx","args":["-y"],"env":{}}"#
  );
  assert_eq!(
    serde_json::to_string(&Ordered::map([
      ("b", Ordered::Null),
      ("a", Ordered::Flag(true)),
      ("b", Ordered::text("x"))
    ]))
    .unwrap(),
    r#"{"b":"x","a":true}"#
  );
}
