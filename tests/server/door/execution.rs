use agency_server::door::execution::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[test]
fn a_root_is_a_task_s_own_name_or_one_of_its_own() {
  assert_eq!(execution_root("bmo-task-99"), "bmo-task-99");
  assert!(execution_root("bmo-word-w-3").starts_with("bmo-root-"));
  assert!(execution_root("bmo-run-17-plan-2").starts_with("bmo-root-"));
}

#[test]
fn a_child_s_row_is_its_parent_s_under_an_id_of_its_own() {
  let parent = Row {
    body: serde_json::from_value(json!({ "id": "bmo-execution-1", "root": "r", "provider": "claude-code", "panePath": true, "model": "m", "attempt": 2 }))
      .unwrap(),
  };
  let child = native_child(&parent, "toolu_1");
  let digest = format!("{:x}", Sha256::digest(br#"["bmo-execution-1","claude-code","toolu_1"]"#));

  assert_eq!(child.id(), format!("bmo-native-{digest}"));
  assert_eq!(child.text("parent"), Some("bmo-execution-1"));
  assert_eq!(child.body.get("model"), Some(&Value::Null));
  assert_eq!(child.body.get("attempt"), Some(&json!(2)));
  assert!(!child.body.contains_key("panePath"));
}
