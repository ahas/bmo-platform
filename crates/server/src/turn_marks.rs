//! The room turn each supervisor is carrying now (docs/api.md, 「assignments and jobs」): the room it answers
//! and whether every new message in it is the owner's, as the receiver marked the turn. An item the agent files
//! while it carries the turn is filed under that room, and as the owner's ask where the turn is privileged.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// What an agent's turn is about as agency reads it, never as the model writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
  pub chat_id: i64,
  pub privileged: bool,
}

// Each agent's turn now, by the agent, with the run it goes out as.
static CARRIED: LazyLock<Mutex<HashMap<String, (String, Mark)>>> = LazyLock::new(Mutex::default);

fn carried() -> std::sync::MutexGuard<'static, HashMap<String, (String, Mark)>> {
  CARRIED.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// An agent's room turn begun as `run`.
pub fn carrying(agent: &str, run: &str, mark: Mark) {
  carried().insert(agent.to_owned(), (run.to_owned(), mark));
}

/// An agent's turn `run` ended; a newer turn of the agent's stands.
pub fn ended(agent: &str, run: &str) {
  let mut carried = carried();

  if carried.get(agent).is_some_and(|(going, _)| going == run) {
    carried.remove(agent);
  }
}

/// The room turn an agent is carrying now, where it carries one.
pub fn of(agent: &str) -> Option<Mark> {
  carried().get(agent).map(|(_, mark)| *mark)
}
