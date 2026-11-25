//! A peer that goes away under the call: a front on a port of its own before a stand-in, which passes each
//! connection through to it, cuts each one once its request is in, or holds each one open unanswered, as the
//! test sets it. The front goes with its guard, every connection it holds with it.

#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// What the front does with a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
  /// Passed through to the stand-in behind it.
  Pass,
  /// Read, then closed with no answer: a peer restarting under the call.
  Cut,
  /// Read, then held open with no answer until the front goes.
  Hold,
}

struct Shared {
  mode: Mutex<Mode>,
  held: Mutex<Vec<TcpStream>>,
  accepted: AtomicUsize,
  heard: Notify,
}

/// The front standing, and the address a caller reaches it at.
pub struct Front {
  pub address: String,
  shared: Arc<Shared>,
  task: JoinHandle<()>,
}

impl Front {
  /// A front before `target`, an `http://host:port` address, passing every connection through.
  pub async fn before(target: &str) -> Front {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let target = target.trim_start_matches("http://").to_owned();
    let shared = Arc::new(Shared {
      mode: Mutex::new(Mode::Pass),
      held: Mutex::new(Vec::new()),
      accepted: AtomicUsize::new(0),
      heard: Notify::new(),
    });
    let serving = shared.clone();
    let task = tokio::spawn(async move {
      while let Ok((mut stream, _)) = listener.accept().await {
        let mode = *serving.mode.lock().unwrap();

        serving.accepted.fetch_add(1, Ordering::SeqCst);

        match mode {
          Mode::Pass => {
            let target = target.clone();

            tokio::spawn(async move {
              if let Ok(mut behind) = TcpStream::connect(&target).await {
                let _ = tokio::io::copy_bidirectional(&mut stream, &mut behind).await;
              }
            });
          }
          Mode::Cut | Mode::Hold => {
            let mut request = vec![0; 65536];
            let _ = stream.read(&mut request).await;

            if mode == Mode::Hold {
              serving.held.lock().unwrap().push(stream);
            }

            serving.heard.notify_one();
          }
        }
      }
    });

    Front {
      address,
      shared,
      task,
    }
  }

  /// What the front does with the connections that come next.
  pub fn set(&self, mode: Mode) {
    *self.shared.mode.lock().unwrap() = mode;
  }

  /// How many connections the front has taken.
  pub fn accepted(&self) -> usize {
    self.shared.accepted.load(Ordering::SeqCst)
  }

  /// Once a connection was cut or held since the last such wait.
  pub async fn heard(&self) {
    self.shared.heard.notified().await;
  }
}

impl Drop for Front {
  fn drop(&mut self) {
    self.task.abort();
    self.shared.held.lock().unwrap().clear();
  }
}
