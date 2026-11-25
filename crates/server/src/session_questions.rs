//! The questions a session's turn stands on, put to the turn's caller (docs/api.md, 「Sessions」, a question
//! in the middle of a turn): one a session at most, each known by an id of its own, shown on the session's
//! state and published as `turn-question` while it waits, answered through the session contract, and gone
//! once it is answered, withdrawn because the screen moved on, or the turn ends. Agency judges none of
//! them: the answer is the caller's, and it is handed to the door as it was given.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use agent_session::{Answer, Asks, Fault, FaultKind, Question, SessionId};
use serde::Serialize;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::door::confirm::{Asking, ConfirmAnswer, Confirmation, refusal_words};
use crate::events::Event;

/// A turn stopped on a question put to its caller: the session, the run and the question.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TurnQuestion {
  pub session: String,
  pub run: String,
  pub question: Question,
}

impl Event for TurnQuestion {
  const NAME: &'static str = "question.created";
}

// A question waiting for its caller's answer: what it asks, where the answer goes, and the token the door
// cancels once the screen stops asking it.
struct Standing {
  question: Question,
  asked: Confirmation,
  answer: oneshot::Sender<Result<ConfirmAnswer, String>>,
  withdrawn: CancellationToken,
}

/// The questions every session's turn stands on now.
#[derive(Default)]
pub struct Questions {
  standing: Mutex<HashMap<SessionId, Standing>>,
  numbered: AtomicU64,
}

/// A question as the session contract shows it.
pub fn asks_of(asked: &Confirmation) -> Asks {
  match asked {
    Confirmation::Permission { text, amendable } => Asks::Permission {
      text: text.clone(),
      amendable: *amendable,
    },
    Confirmation::Unknown { screen } => Asks::Unknown {
      screen: screen.clone(),
    },
  }
}

/// The caller's answer as the door types it: a refusal carries the caller's words, or where it gave none
/// the paragraph a refused command is told.
pub fn confirm_answer(asked: &Confirmation, answer: Answer) -> ConfirmAnswer {
  match answer {
    Answer::Allow => ConfirmAnswer::Allow,
    Answer::Leave => ConfirmAnswer::Leave,
    Answer::Refuse { words } => ConfirmAnswer::Refuse(
      words
        .map(|words| words.trim().to_owned())
        .filter(|words| !words.is_empty())
        .unwrap_or_else(|| refusal_words(asked, None)),
    ),
  }
}

impl Questions {
  fn standing(&self) -> std::sync::MutexGuard<'_, HashMap<SessionId, Standing>> {
    self.standing.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  /// A question the session's turn stopped on, put to its caller: the question as it is shown, and the
  /// asking the door waits on. A question standing before it on the session is let go.
  pub fn put(&self, session: &SessionId, asked: &Confirmation) -> (Question, Asking) {
    let (answer, answered) = oneshot::channel();
    let withdrawn = CancellationToken::new();
    let question = Question {
      id: format!("q-{}", self.numbered.fetch_add(1, Ordering::Relaxed) + 1),
      asks: asks_of(asked),
    };

    self.standing().insert(
      session.clone(),
      Standing {
        question: question.clone(),
        asked: asked.clone(),
        answer,
        withdrawn: withdrawn.clone(),
      },
    );

    (
      question,
      Asking {
        answer: answered,
        withdraw: withdrawn,
      },
    )
  }

  /// The question the session's turn stands on now, none where it stands on none or the screen moved on
  /// from it.
  pub fn shown(&self, session: &SessionId) -> Option<Question> {
    let mut standing = self.standing();

    if standing.get(session).is_some_and(|one| one.withdrawn.is_cancelled()) {
      standing.remove(session);
    }

    standing.get(session).map(|one| one.question.clone())
  }

  /// The caller's answer to the question named, handed to the door; a question the session's turn does not
  /// stand on now is refused.
  pub fn answer(&self, session: &SessionId, id: &str, answer: Answer) -> Result<(), Fault> {
    let unknown = || Fault::new(FaultKind::Unknown, format!("{session} stands on no question {id} now"));
    let mut standing = self.standing();
    let named = standing
      .get(session)
      .is_some_and(|one| one.question.id == id && !one.withdrawn.is_cancelled());

    if !named {
      return Err(unknown());
    }

    let one = standing.remove(session).ok_or_else(unknown)?;

    one.answer.send(Ok(confirm_answer(&one.asked, answer))).map_err(|_| unknown())
  }

  /// Whatever the session's turn stood on, let go as the turn ends.
  pub fn clear(&self, session: &SessionId) {
    self.standing().remove(session);
  }
}
