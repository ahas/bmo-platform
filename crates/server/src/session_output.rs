//! What a session's turn has written of its answer so far (docs/api.md, 「Sessions」, the output read):
//! every text block the program's answers wrote in the turn's record, in order, joined as the turn's
//! words, kept from the turn's start past its end until the session's next turn begins or the session
//! ends, so a turn stopped from outside still leaves what it wrote. An attempt that goes out again on
//! another login or a fresh session begins its words again. Each turn's words are kept under its id, and
//! beside them what the turn waits for while it waits (docs/api.md, 「sessions」, a running turn's wait).

use std::collections::HashMap;
use std::sync::Mutex;

use agent_session::{Output, SessionId, Waiting};

// One turn: the run it goes out as, whether it still goes, the text blocks it wrote and what it waits for.
struct Written {
  turn: i64,
  run: String,
  running: bool,
  texts: Vec<String>,
  waiting: Option<Waiting>,
}

/// Where one turn's written text goes while it runs, for a watch that reads the program's record.
#[derive(Clone, Copy)]
pub struct OutputWatch<'a> {
  pub outputs: &'a Outputs,
  pub session: &'a SessionId,
  pub turn: i64,
}

impl OutputWatch<'_> {
  /// The text blocks the program wrote since it was last read.
  pub fn wrote(&self, texts: &[String]) {
    self.outputs.wrote(self.session, self.turn, texts);
  }

  /// What the turn waits for now, or that it waits no longer.
  pub fn waiting(&self, waiting: Option<Waiting>) {
    self.outputs.waiting(self.session, self.turn, waiting);
  }
}

/// What every session's turns have written, by the turn's id.
#[derive(Default)]
pub struct Outputs {
  written: Mutex<HashMap<(SessionId, i64), Written>>,
  // The latest turn begun in each session, read where a caller names no turn.
  last: Mutex<HashMap<SessionId, i64>>,
}

impl Outputs {
  fn written(&self) -> std::sync::MutexGuard<'_, HashMap<(SessionId, i64), Written>> {
    self.written.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  fn latest(&self) -> std::sync::MutexGuard<'_, HashMap<SessionId, i64>> {
    self.last.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  /// A session's turn begun under its id and run, in place of the turns of its that have ended.
  pub fn begun(&self, session: &SessionId, turn: i64, run: &str) {
    let mut written = self.written();

    written.retain(|(held, _), one| held != session || one.running);
    written.insert(
      (session.clone(), turn),
      Written {
        turn,
        run: run.to_owned(),
        running: true,
        texts: Vec::new(),
        waiting: None,
      },
    );
    self.latest().insert(session.clone(), turn);
  }

  /// An attempt of the turn going out, its words begun again.
  pub fn attempted(&self, session: &SessionId, turn: i64) {
    if let Some(written) = self.written().get_mut(&(session.clone(), turn)) {
      written.texts.clear();
    }
  }

  /// The text blocks the turn's program wrote since it was last read.
  pub fn wrote(&self, session: &SessionId, turn: i64, texts: &[String]) {
    if let Some(written) = self.written().get_mut(&(session.clone(), turn)) {
      written.texts.extend_from_slice(texts);
    }
  }

  /// The whole of what a turn has written so far, read afresh from its program's record, in place of what was
  /// shown before: a turn picked up at the server's start, read off the record on every look.
  pub fn shown(&self, session: &SessionId, turn: i64, text: String) {
    if let Some(written) = self.written().get_mut(&(session.clone(), turn))
      && !text.is_empty()
    {
      written.texts = vec![text];
    }
  }

  /// What the turn waits for now, or that it waits no longer.
  pub fn waiting(&self, session: &SessionId, turn: i64, waiting: Option<Waiting>) {
    if let Some(written) = self.written().get_mut(&(session.clone(), turn)) {
      written.waiting = waiting;
    }
  }

  /// The turn ended, what it wrote kept; an ended turn waits for nothing.
  pub fn ended(&self, session: &SessionId, turn: i64) {
    if let Some(written) = self.written().get_mut(&(session.clone(), turn)) {
      written.running = false;
      written.waiting = None;
    }
  }

  /// The session gone, and what its turns wrote with it.
  pub fn forget(&self, session: &SessionId) {
    self.written().retain(|(held, _), _| held != session);
    self.latest().remove(session);
  }

  /// What the turn named has written, or the session's latest turn where none is named; none where the
  /// session carried no turn of that id.
  pub fn read(&self, session: &SessionId, turn: Option<i64>) -> Option<Output> {
    let turn = turn.or_else(|| self.latest().get(session).copied())?;

    self.written().get(&(session.clone(), turn)).map(|written| Output {
      turn: written.turn,
      run: written.run.clone(),
      running: written.running,
      text: written.texts.join("\n\n"),
      waiting: written.waiting.clone(),
    })
  }
}
