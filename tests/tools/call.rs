//! Agency's side of the tool host's call protocol: what a request reads as, the turn read from its record,
//! what a call is answered with, and the tree the tools read `prompts/` from.

use std::collections::BTreeMap;

use serde_json::json;
use tool_call::{Answer, Asked, Session, Turn, prompts_tree, read_handed};

fn request(asked: serde_json::Value) -> String {
  let mut said = json!({
    "protocol": 1,
    "session": {
      "holderName": "chat",
      "namespaces": ["bmo"],
      "capabilities": ["room.tools"],
      "grants": [],
      "variables": { "BMO_DATA_DIR": "/d" },
      "labels": { "line": "chat", "archive": "postgres:///s" }
    },
    "callback": "/run/c.sock",
  });

  said.as_object_mut().unwrap().extend(asked.as_object().unwrap().clone());
  said.to_string()
}

#[test]
fn a_request_reads_as_the_host_writes_it_and_a_field_no_tool_reads_is_passed_over() {
  let call = read_handed(
    request(json!({ "kind": "call", "entry": "bmo/worker_ask", "arguments": { "name": "a" } })).as_bytes(),
  )
  .unwrap();

  assert_eq!(
    call.asked,
    Asked::Call {
      entry: "bmo/worker_ask".into(),
      arguments: json!({ "name": "a" })
    }
  );
  assert_eq!(call.session.line(), "chat");
  assert_eq!(call.session.archive(), "postgres:///s");
  assert_eq!(call.session.data_root(), std::path::PathBuf::from("/d"));
  assert!(call.session.capabilities.contains("room.tools"));

  let read =
    read_handed(request(json!({ "kind": "resources_read", "uri": "bmo://procedure/x" })).as_bytes())
      .unwrap();

  assert_eq!(
    read.asked,
    Asked::ResourcesRead {
      uri: "bmo://procedure/x".into()
    }
  );
  assert_eq!(
    read_handed(request(json!({ "kind": "resources_list" })).as_bytes()).unwrap().asked,
    Asked::ResourcesList
  );
  assert_eq!(
    read_handed(request(json!({ "kind": "claim", "entry": "web/x", "address": "https://x" })).as_bytes())
      .unwrap()
      .asked,
    Asked::Other
  );
}

#[test]
fn a_request_in_another_protocol_or_that_is_no_request_is_refused() {
  let other = request(json!({ "kind": "resources_list" })).replace("\"protocol\":1", "\"protocol\":2");

  assert_eq!(
    read_handed(other.as_bytes()).unwrap_err(),
    "the request speaks protocol 2, and this tool speaks 1"
  );
  assert!(read_handed("[]".as_bytes()).unwrap_err().starts_with("the request does not read"));
}

#[test]
fn an_answer_is_written_as_the_protocol_writes_a_tool_result() {
  assert_eq!(
    serde_json::to_value(Answer::text("read")).unwrap(),
    json!({ "content": [{ "type": "text", "text": "read" }], "isError": false })
  );

  let refused = Answer::failure("no such thing");

  assert!(refused.is_error);
  assert_eq!(refused.plain(), "ERROR: no such thing");
}

#[test]
fn the_turn_is_the_run_its_place_names_over_the_records_own_and_none_for_a_persons() {
  let place = tempfile::tempdir().unwrap();
  let session = Session {
    variables: BTreeMap::from([
      ("BMO_TURN_AGENT".to_owned(), "supervisor".to_owned()),
      ("BMO_TURN_BEGAN".to_owned(), "1000".to_owned()),
      ("BMO_TURN_CHAT".to_owned(), "-5".to_owned()),
    ]),
    labels: tool_call::Labels {
      place: Some(place.path().to_path_buf()),
      ..tool_call::Labels::default()
    },
    ..Session::default()
  };

  assert_eq!(
    Turn::of(&session),
    Turn {
      agent: Some("supervisor".into()),
      began: Some(1000),
      chat: Some(-5),
      via: None
    }
  );

  std::fs::write(
    place.path().join("run.json"),
    json!({ "env": { "BMO_TURN_CHAT": "-7", "BMO_TURN_VIA": " console " } }).to_string(),
  )
  .unwrap();

  assert_eq!((Turn::of(&session).chat, Turn::of(&session).via), (Some(-7), Some("console".into())));

  std::fs::write(
    place.path().join("run.json"),
    json!({ "person": true, "env": { "BMO_TURN_CHAT": "-7" } }).to_string(),
  )
  .unwrap();

  assert_eq!(Turn::of(&session).chat, Some(-5));
}

#[test]
fn the_tree_is_the_nearest_folder_above_holding_prompts() {
  let tree = tempfile::tempdir().unwrap();
  let placed = tree.path().join("tools").join("procedures").join(".placed");

  std::fs::create_dir_all(tree.path().join("prompts").join("agents")).unwrap();
  std::fs::create_dir_all(&placed).unwrap();

  assert_eq!(prompts_tree(&placed), tree.path());
}

#[test]
fn a_prompts_folder_of_another_shape_is_passed_by() {
  // The tool host runs a tool from its copy under the data root, whose own `prompts/` holds the personas.
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let copy = data.join("tool-host").join("tools").join("worker_ask").join("1-a");
  let agency = home.path().join("agency");

  std::fs::create_dir_all(data.join("prompts").join("persona")).unwrap();
  std::fs::create_dir_all(&copy).unwrap();
  std::fs::create_dir_all(agency.join("prompts").join("agents").join("chat")).unwrap();
  std::fs::create_dir_all(agency.join("tools").join("worker_ask")).unwrap();

  assert_ne!(prompts_tree(&copy), data, "the data root's personas are no prompt tree of agency's");
  assert_eq!(prompts_tree(&agency.join("tools").join("worker_ask")), agency);
}
