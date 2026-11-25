//! One BMO's line to another (docs/relays.md): the prompt marking the sender and the refusal, the verdict
//! a receiving BMO's end comes to, and the record both sides read.

use std::sync::Mutex;

use agency_server::conversational::{Scope as TurnScope, Settings, Shape};
use agency_server::relays::{self, REFUSAL_MARKER, Verdict};
use agent_session::{
  Fault, FaultKind, Outcome, Scope, Session as SessionRow, SessionId, Sessions, Start, State, Turn, TurnEnd,
};

use crate::support::scratch::Scratch;

/// A backend that keeps what it was started on and handed, and answers every turn with one ending.
#[derive(Default)]
struct FakeSessions {
  started: Mutex<Vec<Start>>,
  sent: Mutex<Vec<Turn>>,
  end: Mutex<Option<TurnEnd>>,
}

impl Sessions for FakeSessions {
  async fn start(&self, request: Start) -> Result<SessionRow, Fault> {
    let id = match &request.scope {
      Scope::Room(room) => format!("{}@{room}", request.agent),
      _ => request.agent.clone(),
    };

    self.started.lock().unwrap().push(request.clone());

    Ok(SessionRow {
      id: SessionId(id),
      agent: request.agent,
      scope: request.scope,
    })
  }

  async fn switch(&self, _: &SessionId, _: Option<&str>, _: Option<&str>) -> Result<SessionRow, Fault> {
    Err(Fault::new(FaultKind::Refused, "a fake session does not switch"))
  }

  async fn send_turn(&self, turn: Turn) -> Result<TurnEnd, Fault> {
    self.sent.lock().unwrap().push(turn);

    Ok(
      self
        .end
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| TurnEnd::answered("run", "an answer")),
    )
  }

  async fn read_state(&self, _: &SessionId, _: Option<&str>) -> Result<State, Fault> {
    Err(Fault::new(FaultKind::Refused, "a fake session has no state"))
  }

  async fn interrupt(&self, _: &SessionId, _: i64) -> Result<(), Fault> {
    Ok(())
  }

  async fn fold(&self, _: &SessionId) -> Result<(), Fault> {
    Ok(())
  }

  async fn end(&self, _: &SessionId) -> Result<(), Fault> {
    Ok(())
  }

  async fn list(&self) -> Result<Vec<SessionRow>, Fault> {
    Ok(Vec::new())
  }
}

fn settings(scope: TurnScope) -> Settings {
  Settings {
    scope,
    shape: Shape::Pieces,
  }
}

#[tokio::test]
async fn a_line_goes_into_the_receiving_bmos_own_room_session_as_an_unprivileged_turn() {
  let sessions = FakeSessions::default();
  let served = relays::deliver(&sessions, "beta", &settings(TurnScope::Room), -5, "the line".into())
    .await
    .unwrap();

  assert_eq!(served.outcome, Outcome::Answered);

  let started = sessions.started.lock().unwrap();

  assert_eq!(started.len(), 1);
  assert_eq!(started[0].agent, "beta");
  assert_eq!(started[0].scope, Scope::Room("-5".into()));

  let sent = sessions.sent.lock().unwrap();

  assert_eq!(sent.len(), 1);
  assert_eq!(sent[0].session, SessionId("beta@-5".into()));
  assert_eq!(sent[0].prompt, "the line");
  assert_eq!(sent[0].context.room.as_ref().map(|room| room.chat_id), Some(-5));
  assert!(!sent[0].context.privileged, "a relay is never the person's");
}

#[tokio::test]
async fn a_line_to_a_supervisor_across_rooms_goes_into_its_one_session() {
  let sessions = FakeSessions::default();

  relays::deliver(&sessions, "beta", &settings(TurnScope::Agent), -5, "the line".into())
    .await
    .unwrap();

  let started = sessions.started.lock().unwrap();

  assert_eq!(started[0].scope, Scope::Agent);
  assert_eq!(sessions.sent.lock().unwrap()[0].session, SessionId("beta".into()));
}

#[test]
fn the_prompt_names_the_sender_and_says_the_words_are_the_bmos_not_the_persons() {
  let prompt = relays::prompt("alpha", "  the line  ");

  assert!(prompt.contains("Relayed from another BMO, alpha."), "{prompt}");
  assert!(prompt.contains("never your person's"), "{prompt}");
  assert!(prompt.contains(REFUSAL_MARKER), "{prompt}");
  assert!(prompt.ends_with("the line"), "{prompt}");
}

#[test]
fn an_answered_turn_is_delivered_and_the_refusal_marker_is_a_refusal() {
  assert_eq!(relays::verdict(&TurnEnd::answered("run", "an answer")), Verdict::Delivered);
  assert_eq!(
    relays::verdict(&TurnEnd::answered("run", format!("  {REFUSAL_MARKER}\n"))),
    Verdict::Refused
  );
}

#[test]
fn a_blocked_or_failed_turn_is_a_failure_with_its_reason() {
  let mut failed = TurnEnd::of("run", Outcome::Failed);
  failed.reason = Some("the engine ended it".into());

  assert_eq!(
    relays::verdict(&TurnEnd::of("run", Outcome::Blocked)),
    Verdict::Failed("the receiving BMO is waiting on a person".into())
  );
  assert_eq!(relays::verdict(&failed), Verdict::Failed("the engine ended it".into()));
  assert_eq!(
    relays::verdict(&TurnEnd::of("run", Outcome::Stopped)),
    Verdict::Failed("the receiving BMO's turn was stopped".into())
  );
}

#[tokio::test]
async fn a_line_is_recorded_and_read_by_both_sides() {
  let scratch = Scratch::new().await;
  let db = scratch.db();

  let sent =
    relays::record(db, "alpha", "beta", -5, relays::HANDED, "hello", "delivered", Some("hi"), None, 1_000)
      .await
      .unwrap();

  assert_eq!(sent.from, "alpha");
  assert_eq!(sent.to, "beta");
  assert_eq!(sent.room, -5);
  assert_eq!(sent.kind, relays::HANDED);
  assert_eq!(sent.state, "delivered");
  assert_eq!(sent.answer.as_deref(), Some("hi"));

  let refused = relays::record(
    db,
    "alpha",
    "beta",
    -5,
    relays::OPENED,
    "again",
    "refused",
    Some(REFUSAL_MARKER),
    Some("would not take it"),
    2_000,
  )
  .await
  .unwrap();

  assert_eq!(refused.state, "refused");
  assert_eq!(refused.kind, relays::OPENED);

  let for_alpha = relays::list(db, "alpha", 50).await.unwrap();
  let for_beta = relays::list(db, "beta", 50).await.unwrap();

  assert_eq!(for_alpha.len(), 2);
  assert_eq!(for_beta.len(), 2);
  assert_eq!(for_alpha[0].created_at, 2_000, "the latest first");
  assert_eq!(for_alpha[0].id, refused.id);
  assert!(relays::list(db, "gamma", 50).await.unwrap().is_empty());
}
