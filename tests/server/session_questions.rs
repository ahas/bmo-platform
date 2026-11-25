//! A question a job's turn stops on, put to the turn's caller (docs/api.md, 「Sessions」, a question in the
//! middle of a turn): shown while it waits and published as `turn-question`; an allow, a leave and a refusal
//! — with the caller's words, or the fixed paragraph where it gave none — handed to the door as they were
//! given; gone once answered, withdrawn or cleared, a later answer to it refused as naming nothing; a session
//! of a scope that puts no question to its caller taking no answer; and a turn that hears no words refusing
//! them.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::door::confirm::{ConfirmAnswer, Confirmation, refusal_words};
use agency_server::herdr_backend::{HerdrSessions, hears_words};
use agency_server::places::{Ground, Places};
use agency_server::seeds::deployment;
use agency_server::session_questions::{Questions, TurnQuestion};
use agent_session::{
  Answer, Apart, Asks, Conversation, FaultKind, Mapping, Question, Resume, Scope, SessionId, Sessions,
  Start, WorkParts,
};
use harness_providers::Machine;
use serde_json::json;
use tokio::sync::oneshot;

use crate::support::rows::supervisor;
use crate::support::scratch::Scratch;

fn permission() -> Confirmation {
  Confirmation::Permission {
    text: "rm -f $S/*.md".into(),
    amendable: true,
  }
}

fn session() -> SessionId {
  SessionId("worker-a@worker".into())
}

fn answered(receiver: oneshot::Receiver<Result<ConfirmAnswer, String>>) -> ConfirmAnswer {
  let mut receiver = receiver;

  receiver
    .try_recv()
    .expect("the answer was handed to the door")
    .expect("an answer, not a refusal to ask")
}

#[test]
fn a_question_put_to_the_caller_is_shown_until_its_answer_is_handed_to_the_door() {
  let questions = Questions::default();
  let (question, asking) = questions.put(&session(), &permission());

  assert_eq!(
    question.asks,
    Asks::Permission {
      text: "rm -f $S/*.md".into(),
      amendable: true
    }
  );
  assert_eq!(questions.shown(&session()), Some(question.clone()));

  questions.answer(&session(), &question.id, Answer::Allow).unwrap();

  assert_eq!(answered(asking.answer), ConfirmAnswer::Allow);
  assert_eq!(questions.shown(&session()), None, "an answered question stands no more");
  assert_eq!(
    questions.answer(&session(), &question.id, Answer::Allow).unwrap_err().kind,
    FaultKind::Unknown,
    "a question answered once takes no second answer"
  );
}

#[test]
fn a_leave_and_a_refusal_reach_the_door_as_the_caller_gave_them() {
  let questions = Questions::default();
  let (left, leaving) = questions.put(&session(), &permission());

  questions.answer(&session(), &left.id, Answer::Leave).unwrap();
  assert_eq!(answered(leaving.answer), ConfirmAnswer::Leave);

  let (refused, refusing) = questions.put(&session(), &permission());

  questions
    .answer(
      &session(),
      &refused.id,
      Answer::Refuse {
        words: Some("Write the removal as a script in your scratchpad.".into()),
      },
    )
    .unwrap();
  assert_eq!(
    answered(refusing.answer),
    ConfirmAnswer::Refuse("Write the removal as a script in your scratchpad.".into())
  );

  let unknown = Confirmation::Unknown {
    screen: "Pick a theme".into(),
  };
  let (bare, refusing) = questions.put(&session(), &unknown);

  assert_eq!(
    bare.asks,
    Asks::Unknown {
      screen: "Pick a theme".into()
    }
  );
  assert_ne!(bare.id, refused.id, "every question is known by an id of its own");

  questions.answer(&session(), &bare.id, Answer::Refuse { words: None }).unwrap();
  assert_eq!(answered(refusing.answer), ConfirmAnswer::Refuse(refusal_words(&unknown, None)));
}

#[test]
fn a_question_the_screen_moved_on_from_or_the_turn_let_go_takes_no_answer() {
  let questions = Questions::default();
  let (first, asking) = questions.put(&session(), &permission());

  asking.withdraw.cancel();

  assert_eq!(questions.shown(&session()), None);
  assert_eq!(
    questions.answer(&session(), &first.id, Answer::Allow).unwrap_err().kind,
    FaultKind::Unknown
  );

  let (second, _asking) = questions.put(&session(), &permission());

  assert_eq!(
    questions.answer(&session(), "q-0", Answer::Allow).unwrap_err().kind,
    FaultKind::Unknown,
    "an id that is not the one standing names nothing"
  );
  assert_eq!(
    questions
      .answer(&SessionId("worker-b@worker".into()), &second.id, Answer::Allow)
      .unwrap_err()
      .kind,
    FaultKind::Unknown,
    "a question stands on its own session alone"
  );

  questions.clear(&session());

  assert_eq!(questions.shown(&session()), None);
  assert_eq!(
    questions.answer(&session(), &second.id, Answer::Allow).unwrap_err().kind,
    FaultKind::Unknown
  );
}

#[test]
fn a_turn_question_names_its_session_its_run_and_the_question_as_the_caller_reads_it() {
  let published = TurnQuestion {
    session: "worker-a@worker".into(),
    run: "bmo-job-41".into(),
    question: Question {
      id: "q-1".into(),
      asks: Asks::Permission {
        text: "rm -f $S/*.md".into(),
        amendable: true,
      },
    },
  };

  assert_eq!(
    json!(published),
    json!({
      "session": "worker-a@worker",
      "run": "bmo-job-41",
      "question": { "id": "q-1", "kind": "permission", "text": "rm -f $S/*.md", "amendable": true },
    })
  );
}

fn work(hears: bool) -> WorkParts {
  WorkParts {
    provider: "claude-code".into(),
    model: "opus".into(),
    mapping: Mapping {
      model: "claude-opus-5".into(),
      effort: None,
      tier: None,
      answer_tokens: None,
    },
    resume: Resume::Fresh,
    cwd: "/tmp/run".into(),
    name: "bmo-job-41".into(),
    title: "a turn".into(),
    chat: -5,
    unit: None,
    user: None,
    work_key: "job-worker".into(),
    hears_words: hears,
    variables: BTreeMap::new(),
  }
}

#[test]
fn a_job_turn_that_hears_no_words_takes_none_and_every_other_turn_hears_them() {
  assert!(!hears_words(Some(&work(false))));
  assert!(hears_words(Some(&work(true))));
  assert!(hears_words(None), "a turn with no work parts hears the words pushed into it");
}

#[tokio::test]
async fn a_session_whose_turns_put_no_question_to_their_caller_takes_no_answer() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let at = |path: std::path::PathBuf| path.to_string_lossy().into_owned();
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), at(home.path().to_path_buf())),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
    ],
    home.path().join("data"),
  );

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let sessions = HerdrSessions::new(Places::new(Ground::new(&service, &machine, None), Arc::default()));
  supervisor(
    scratch.db(),
    "supervisor",
    &[("session_scope", "room".into()), ("turn_shape", "pieces".into())],
  )
  .await;

  let conversation = sessions
    .start(Start {
      agent: "supervisor".into(),
      scope: Scope::Conversation(Conversation {
        provider: "claude-code".into(),
        model: "claude-sonnet-5".into(),
        cwd: "/tmp/run".into(),
      }),
    })
    .await
    .unwrap();

  assert_eq!(
    sessions.answer(&conversation.id, "q-1", Answer::Allow).await.unwrap_err().kind,
    FaultKind::Unknown,
    "a conversation puts no question to its caller"
  );

  let apart = sessions
    .start(Start {
      agent: "branch-a".into(),
      scope: Scope::Apart(Apart {
        provider: "claude-code".into(),
        model: "claude-sonnet-5".into(),
        cwd: "/tmp/run".into(),
        todo: None,
        role: None,
        ..Apart::default()
      }),
    })
    .await
    .unwrap();

  assert_eq!(
    sessions.answer(&apart.id, "q-1", Answer::Allow).await.unwrap_err().kind,
    FaultKind::Unknown,
    "a thrown-away session's turn that stands on no question takes no answer"
  );
  assert_eq!(sessions.read_state(&apart.id, None).await.unwrap().question, None);
}
