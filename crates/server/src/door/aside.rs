//! Words offered to a run's working turn from outside it (docs/agent-control.md, 「Words that arrive while a
//! turn is working」): a session's push offers them under the run its turn carries, the door takes them at its
//! next look at the working program while it listens for them, and the push is answered with what became of
//! them. The words a run took are kept for any later run of the same turn until its session lets the run go.
//! The offers stand for as long as the process does, under the run's name.

use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex, MutexGuard};

use tokio::sync::oneshot;

use super::turn::AsideOutcome;

/// Words offered to a run, and where what became of them is answered.
pub struct Offered {
  pub words: String,
  answer: oneshot::Sender<AsideOutcome>,
}

#[derive(Default)]
struct Run {
  listening: bool,
  offered: VecDeque<Offered>,
  took: Vec<String>,
}

#[derive(Default)]
struct Asides {
  runs: HashMap<String, Run>,
}

static ASIDES: LazyLock<Mutex<Asides>> = LazyLock::new(Mutex::default);

fn asides() -> MutexGuard<'static, Asides> {
  ASIDES.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A run's door listening for offers while its turn works; every offer it did not take is answered refused
/// as it stops.
pub struct Listening {
  run: String,
}

/// The door of a run starting to listen for offers.
pub fn listen(run: &str) -> Listening {
  asides().runs.entry(run.to_owned()).or_default().listening = true;

  Listening {
    run: run.to_owned(),
  }
}

impl Listening {
  /// The offer standing first for the run, taken.
  pub fn take(&self) -> Option<Offered> {
    asides().runs.get_mut(&self.run)?.offered.pop_front()
  }

  /// What became of an offer taken answered, and its words kept for a later run of the turn where they went
  /// in.
  pub fn said(&self, offered: Offered, outcome: AsideOutcome) {
    if outcome == AsideOutcome::Delivered
      && let Some(run) = asides().runs.get_mut(&self.run)
    {
      run.took.push(offered.words);
    }

    let _ = offered.answer.send(outcome);
  }
}

impl Drop for Listening {
  fn drop(&mut self) {
    let mut asides = asides();
    let Some(run) = asides.runs.get_mut(&self.run) else { return };

    run.listening = false;

    for offered in run.offered.drain(..) {
      let _ = offered.answer.send(AsideOutcome::Refused);
    }

    if run.took.is_empty() {
      asides.runs.remove(&self.run);
    }
  }
}

/// An offer standing for a run's door, until a look takes it or the turn ends.
pub struct Offer {
  pub answer: oneshot::Receiver<AsideOutcome>,
}

/// Words offered to the run, where its door listens; none where it does not, so they did not go in.
pub fn offer(run: &str, words: &str) -> Option<Offer> {
  let mut asides = asides();
  let held = asides.runs.get_mut(run).filter(|held| held.listening)?;
  let (answer, answered) = oneshot::channel();

  held.offered.push_back(Offered {
    words: words.to_owned(),
    answer,
  });

  Some(Offer { answer: answered })
}

/// The prompt a later run of the same turn goes in with: the turn's own, and behind it every word the turn
/// took so far.
pub fn carried(run: &str, prompt: &str) -> String {
  let took = asides().runs.get(run).map(|run| run.took.clone()).unwrap_or_default();

  std::iter::once(prompt.to_owned()).chain(took).collect::<Vec<_>>().join("\n\n")
}

/// What the run's turn took let go, as its session lets the run go.
pub fn forget(run: &str) {
  let mut asides = asides();

  if asides.runs.get(run).is_some_and(|run| !run.listening) {
    asides.runs.remove(run);
  }
}
