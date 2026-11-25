//! The gateway's address and the status its launcher last wrote.

use harness_machine::gateway::*;
use policy::providers::registered::REGISTERED;
use serde_json::json;

#[test]
fn the_gateway_answers_where_the_process_names_or_else_where_the_policy_does() {
  assert_eq!(url(Some(" http://[::1]:9/// ")), "http://[::1]:9");
  assert_eq!(url(Some("  ")), REGISTERED.gateway.url);
  assert_eq!(url(None), REGISTERED.gateway.url);
}

#[test]
fn the_status_reads_what_the_launcher_wrote_and_fills_what_it_left_out() {
  let root = tempfile::tempdir().unwrap();
  let file = root.path().join("llm-gateway").join("status.json");

  assert_eq!(status(root.path()), None);

  std::fs::create_dir_all(file.parent().unwrap()).unwrap();
  std::fs::write(&file, "{\"serving\":\"t1\",\"draining\":1,\"at\":5}\n").unwrap();
  assert_eq!(
    serde_json::to_value(status(root.path())).unwrap(),
    json!({ "serving": "t1", "starting": null, "draining": 1, "at": 5 })
  );

  std::fs::write(&file, "5").unwrap();
  assert_eq!(
    serde_json::to_value(status(root.path())).unwrap(),
    json!({ "serving": null, "starting": null, "draining": 0, "at": 0 })
  );

  std::fs::write(&file, "not json").unwrap();
  assert_eq!(status(root.path()), None);
}
