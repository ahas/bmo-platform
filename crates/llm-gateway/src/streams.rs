//! The block a program's streamed answer is writing, counted as its bytes pass the front (docs/own-llm.md,
//! 「The gateway」): for each session that names itself on its request, the block open in the answer, its kind
//! and the bytes its deltas have written so far. A block is held only while it is open; nothing is written
//! down, and the bytes are never weighed as tokens.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use sse_stream::SseStream;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

/// The header a program's request names its session by: Claude Code's own.
pub const SESSION_HEADER: &str = "x-claude-code-session-id";

/// The block a session's answer is writing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Block {
  /// Its place in the answer.
  pub index: u64,
  /// `thinking`, `text` or `tool`.
  pub kind: &'static str,
  /// The bytes its deltas have written so far.
  pub bytes: u64,
}

/// The open block of every session whose answer is streaming now.
#[derive(Clone, Default)]
pub struct Streams {
  open: Arc<Mutex<HashMap<String, Block>>>,
}

impl Streams {
  /// The block the session's answer is writing, where one is open.
  pub fn open(&self, session: &str) -> Option<Block> {
    self.open.lock().unwrap().get(session).cloned()
  }

  /// Where the bytes of a session's streamed answer are handed as they pass, to be read for its blocks; the
  /// session lets go of its block once the sender is dropped, whatever the answer had come to.
  pub fn counter(&self, session: String) -> UnboundedSender<Bytes> {
    let (feed, mut bytes) = unbounded_channel::<Bytes>();
    let open = Arc::clone(&self.open);

    tokio::spawn(async move {
      let passed = futures_util::stream::poll_fn(move |seen| {
        bytes.poll_recv(seen).map(|got| got.map(Ok::<Bytes, Infallible>))
      });
      let mut events = std::pin::pin!(SseStream::from_bytes_stream(passed));

      while let Some(event) = events.next().await {
        let Ok(event) = event else { continue };
        let Some(data) = event.data.as_deref().and_then(|data| serde_json::from_str::<Value>(data).ok())
        else {
          continue;
        };

        apply(&open, &session, &data);
      }

      open.lock().unwrap().remove(&session);
    });

    feed
  }
}

// The kind a block's start names, as a reader counts it.
fn kind_of(block: &Value) -> &'static str {
  match block.get("type").and_then(Value::as_str) {
    Some("thinking" | "redacted_thinking") => "thinking",
    Some("tool_use" | "server_tool_use") => "tool",
    _ => "text",
  }
}

// The bytes a delta writes into its block.
fn written(delta: &Value) -> u64 {
  ["text", "thinking", "partial_json"]
    .iter()
    .find_map(|field| delta.get(*field).and_then(Value::as_str))
    .map_or(0, |words| words.len() as u64)
}

/// One event of a Messages stream against the session's open block: a block's start opens it, a delta adds its
/// bytes, and a block's end or the message's lets go of it.
pub fn apply(open: &Mutex<HashMap<String, Block>>, session: &str, event: &Value) {
  let mut open = open.lock().unwrap();
  let index = event.get("index").and_then(Value::as_u64).unwrap_or(0);

  match event.get("type").and_then(Value::as_str) {
    Some("content_block_start") => {
      let kind = event.get("content_block").map_or("text", kind_of);

      open.insert(
        session.to_owned(),
        Block {
          index,
          kind,
          bytes: 0,
        },
      );
    }
    Some("content_block_delta") => {
      if let (Some(block), Some(delta)) = (open.get_mut(session), event.get("delta"))
        && block.index == index
      {
        block.bytes += written(delta);
      }
    }
    Some("content_block_stop" | "message_stop") => {
      open.remove(session);
    }
    _ => {}
  }
}
