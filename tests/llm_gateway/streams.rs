//! The block a program's streamed answer is writing, as the gateway counts it (docs/own-llm.md, 「The
//! gateway」): a block's start opens it under the session, its deltas add their bytes, and its end lets go.

use std::collections::HashMap;
use std::sync::Mutex;

use llm_gateway::streams::{Block, apply};
use serde_json::json;

// "a block's start opens it, its deltas add the bytes they write, and its end or the message's lets go of it"
#[test]
fn a_block_is_held_from_its_start_to_its_end_with_the_bytes_its_deltas_wrote() {
  let open = Mutex::new(HashMap::new());
  let held = |session: &str| open.lock().unwrap().get(session).cloned();

  apply(&open, "s-1", &json!({ "type": "message_start", "message": { "id": "m" } }));
  assert_eq!(held("s-1"), None, "no block before one starts");

  apply(
    &open,
    "s-1",
    &json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "thinking" } }),
  );
  apply(
    &open,
    "s-1",
    &json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "thinking_delta", "thinking": "abcd" } }),
  );
  apply(
    &open,
    "s-1",
    &json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "signature_delta", "signature": "zzzz" } }),
  );
  assert_eq!(
    held("s-1"),
    Some(Block {
      index: 0,
      kind: "thinking",
      bytes: 4
    }),
    "a signature writes no words"
  );

  apply(&open, "s-1", &json!({ "type": "content_block_stop", "index": 0 }));
  assert_eq!(held("s-1"), None);

  apply(
    &open,
    "s-1",
    &json!({ "type": "content_block_start", "index": 1, "content_block": { "type": "tool_use" } }),
  );
  apply(
    &open,
    "s-1",
    &json!({ "type": "content_block_delta", "index": 1, "delta": { "type": "input_json_delta", "partial_json": "{\"a\":" } }),
  );
  apply(
    &open,
    "s-2",
    &json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "text" } }),
  );
  apply(
    &open,
    "s-2",
    &json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": "hello" } }),
  );
  assert_eq!(
    held("s-1"),
    Some(Block {
      index: 1,
      kind: "tool",
      bytes: 5
    })
  );
  assert_eq!(
    held("s-2"),
    Some(Block {
      index: 0,
      kind: "text",
      bytes: 5
    }),
    "each session its own block"
  );

  apply(&open, "s-1", &json!({ "type": "message_stop" }));
  assert_eq!(held("s-1"), None);
  assert!(held("s-2").is_some());
}
