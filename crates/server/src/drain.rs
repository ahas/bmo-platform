//! The server draining on its stop (docs/sessions.md, 「A restart hands the session over」; docs/api.md,
//! 「health」, the drain): one mark, set once at the server's `SIGTERM`, that every part holding a caller
//! reads. While it stands the front takes no new call, an event stream ends with its `stopping` frame, a
//! turn's send still waiting is answered so its caller re-attaches, and the calls in hand are given the
//! policy's span before the process exits.

use std::sync::Arc;
use std::time::Duration;

use policy::drain::DRAIN;
use tokio::sync::watch;

/// The code a call answered because the server drains carries, and the name of a stream's last frame.
pub const STOPPING: &str = "stopping";

/// Whether the server drains, and the moment it began to.
#[derive(Clone)]
pub struct Drain(Arc<watch::Sender<bool>>);

impl Default for Drain {
  fn default() -> Self {
    Self(Arc::new(watch::Sender::new(false)))
  }
}

impl Drain {
  /// The drain begun; every wait on it ends now, and every one begun later ends at once.
  pub fn begin(&self) {
    self.0.send_replace(true);
  }

  /// Whether the drain has begun.
  pub fn draining(&self) -> bool {
    *self.0.borrow()
  }

  /// Ends once the drain has begun, at once where it already has.
  pub async fn begun(&self) {
    let mut seen = self.0.subscribe();

    // The sender lives as long as the mark, so the wait ends only on the drain itself.
    let _ = seen.wait_for(|draining| *draining).await;
  }

  /// Ends once the drain has run for the span the calls in hand are given.
  pub async fn spent(&self) {
    self.begun().await;
    tokio::time::sleep(short_calls()).await;
  }
}

/// The drain begun on the process's `SIGTERM` or `SIGINT`, the stops a service manager and a shell send;
/// refused where they cannot be listened for, which leaves the stop to end the process where it stands.
pub fn on_stop(drain: Drain) -> std::io::Result<()> {
  use tokio::signal::unix::{SignalKind, signal};

  let mut terminate = signal(SignalKind::terminate())?;
  let mut interrupt = signal(SignalKind::interrupt())?;

  tokio::spawn(async move {
    let named = tokio::select! {
      _ = terminate.recv() => "SIGTERM",
      _ = interrupt.recv() => "SIGINT",
    };

    println!("agency: {named}: draining, no new call is taken");
    drain.begin();
  });

  Ok(())
}

/// How long the calls in hand at the stop may go on before the server exits.
pub fn short_calls() -> Duration {
  Duration::from_secs(DRAIN.short_calls_seconds)
}

/// What a caller refused while the server drains is told to wait before it asks again, in seconds.
pub fn retry_after() -> u64 {
  DRAIN.retry_after_seconds
}
