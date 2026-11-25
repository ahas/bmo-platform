//! What a reader of an account's windows declares of itself and does on its clock (docs/usage-windows.md,
//! 「Where readings come from」): the provider it reads, how often it passes, how long its reading stands,
//! and the pass itself. Each reader answers for its own provider; nothing keeps a table of them.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::Duration;

use crate::windows::Keeping;

/// A pass on its way: every reading it took written, every failure said.
pub type Passing<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// A reader of one provider's windows on a clock.
pub trait Reader: Send + Sync {
  /// The provider whose windows it reads, as a slot names it.
  fn mode(&self) -> &str;
  /// How often it passes.
  fn cadence(&self) -> Duration;
  /// How long a reading of its provider's windows stands after it was seen.
  fn fresh(&self) -> Duration;
  /// One pass at `now_ms`, in epoch milliseconds: each reading taken replaces its slot's last, and a slot
  /// that could not be read keeps what it had.
  fn pass<'a>(&'a self, keeping: &'a Keeping, now_ms: i64) -> Passing<'a>;
}

/// The failure last said of each thing a reader reads, so a failure is said once when it starts and its
/// recovery once when it ends.
#[derive(Default)]
pub struct Said(Mutex<HashMap<String, String>>);

impl Said {
  /// What a reading of `what` came to, said where it differs from what was said of it last: a failure on
  /// the error output with its reason, a recovery on the standard output. Answers the line it said, where
  /// it said one.
  pub fn outcome(&self, what: &str, outcome: &Result<(), String>) -> Option<String> {
    let mut said = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

    match outcome {
      Ok(()) => said.remove(what).map(|_| {
        let line = format!("{what} read again");

        println!("{line}");
        line
      }),
      Err(failure) => {
        let line = (said.get(what) != Some(failure)).then(|| {
          let line = format!("{what} could not be read: {failure}");

          eprintln!("{line}");
          line
        });

        said.insert(what.to_owned(), failure.clone());
        line
      }
    }
  }
}
