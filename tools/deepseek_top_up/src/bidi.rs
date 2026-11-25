//! A WebDriver BiDi connection to the browser the call started: commands sent as JSON over its loopback
//! WebSocket, each answered by the reply carrying its id; the events the browser sends are read and let go.
//!
//! No maintained Rust client of WebDriver BiDi stands to build on, so the protocol's commands are written here as
//! the JSON objects the specification defines, over a maintained WebSocket client and serde_json.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

type Sink = futures_util::stream::SplitSink<
  tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
  Message,
>;
type Waiting = Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>;

/// A command the browser did not answer as asked: an error reply, no reply within the wait, or a connection gone.
/// What the browser said is not kept, since it may quote what a page held.
#[derive(Debug)]
pub struct Unanswered;

/// One session on the browser's WebDriver BiDi endpoint.
pub struct Connection {
  sink: tokio::sync::Mutex<Sink>,
  waiting: Waiting,
  next: AtomicU64,
  reader: JoinHandle<()>,
  /// How long a command is waited on.
  patience: Duration,
}

impl Connection {
  /// A session opened on the endpoint the browser announced, its commands each waited on for `patience`.
  pub async fn open(endpoint: &str, patience: Duration) -> Result<Self, String> {
    let address = format!("{}/session", endpoint.trim_end_matches('/'));
    let (socket, _) = tokio::time::timeout(patience, tokio_tungstenite::connect_async(address.as_str()))
      .await
      .map_err(|_| format!("its endpoint did not answer within {} seconds", patience.as_secs()))?
      .map_err(|why| format!("its endpoint did not answer: {why}"))?;
    let (sink, mut stream) = socket.split();
    let waiting: Waiting = Arc::default();
    let reader = tokio::spawn({
      let waiting = waiting.clone();

      async move {
        while let Some(Ok(message)) = stream.next().await {
          let Message::Text(text) = message else {
            continue;
          };
          let Ok(reply) = serde_json::from_str::<Value>(&text) else {
            continue;
          };

          if let Some(id) = reply["id"].as_u64()
            && let Some(sender) =
              waiting.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&id)
          {
            let _ = sender.send(reply);
          }
        }

        // The connection is gone: every command still waiting is let go unanswered.
        waiting.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clear();
      }
    });
    let connection = Self {
      sink: tokio::sync::Mutex::new(sink),
      waiting,
      next: AtomicU64::new(1),
      reader,
      patience,
    };

    connection
      .call("session.new", json!({ "capabilities": {} }))
      .await
      .map_err(|_| "its endpoint opened no session".to_owned())?;
    Ok(connection)
  }

  /// One command and its result.
  pub async fn call(&self, method: &str, params: Value) -> Result<Value, Unanswered> {
    let id = self.next.fetch_add(1, Ordering::Relaxed);
    let (sender, receiver) = oneshot::channel();

    self.waiting.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(id, sender);

    let sent = json!({ "id": id, "method": method, "params": params }).to_string();

    if self.sink.lock().await.send(Message::text(sent)).await.is_err() {
      self.waiting.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&id);
      return Err(Unanswered);
    }

    let reply = tokio::time::timeout(self.patience, receiver).await;

    self.waiting.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&id);

    match reply {
      Ok(Ok(mut reply)) if reply["type"] == "success" => Ok(reply["result"].take()),
      _ => Err(Unanswered),
    }
  }
}

impl Drop for Connection {
  fn drop(&mut self) {
    self.reader.abort();
  }
}
