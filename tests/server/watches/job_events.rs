//! The job service's events told to an item's orderer (docs/agent-control.md, 「An item's change is told to its
//! orderer」): an item closed by somebody other than its orderer, a run's question asked and withdrawn once a
//! reply settles it, a question past its due time told to the question's orderer, each once; a job's question the
//! job service put to the owner once the supervisor's minute passed told to its orderer once, by the change that
//! moved it; a question settled
//! on its recommendation told to its orderer and one a person or the supervisor settled not; and a pass
//! with no cursor, or one past the window, telling nothing of what it passed but the commits still waiting to go
//! live; a commit waiting for its deployment told once for each commit, never again on another change, a start or
//! the sender's retelling; a question past its due time taken again owing its parked job's notice again; and a
//! note added to the item of a running job kept for that job before the cursor moves past it, and offered to its
//! working turn; and an item's title asked of the gate as the item is raised or changed.

use agency_server::outbox::owed;
use agency_server::queue::settings;
use agency_server::watches::job_events::{CURSOR_KEY, DEPLOY_OWED, QUESTION_OWED};
use serde_json::{Value, json};

use crate::support::peers::{item, stand};
use crate::support::rows::supervisor;
use crate::support::scratch::Scratch;

// The pass with no gate deployed, so a word for a job goes as written.
async fn pass(
  db: &sea_orm::DatabaseConnection,
  vars: &std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
  agency_server::watches::job_events::pass(db, vars, &crate::support::gates::none()).await
}

fn event(id: i64, kind: &str, data: Value) -> Value {
  json!({ "id": id, "kind": kind, "at": id, "data": data })
}

fn closed(id: i64, item_id: i64, by: Option<&str>) -> Value {
  event(
    id,
    "item.changed",
    item(item_id, "done", json!({ "previousState": "leased", "changedBy": by, "raisedBy": 70 })),
  )
}

#[tokio::test]
async fn a_pass_with_no_cursor_tells_nothing_it_passed_and_keeps_the_last_number() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  peers.lock().unwrap().events = vec![closed(3, 71, Some("someone")), closed(4, 72, Some("someone"))];

  pass(db, &vars).await.unwrap();

  assert!(peers.lock().unwrap().instructed().is_empty());
  assert_eq!(settings::get(db, CURSOR_KEY).await.unwrap().as_deref(), Some("4"));

  let asked = peers
    .lock()
    .unwrap()
    .calls
    .iter()
    .find(|call| call.path == "/events")
    .unwrap()
    .query
    .clone();

  assert!(
    asked.contains("kinds=item.created%2Citem.changed%2Cnote.created%2Cquestion.created"),
    "{asked}"
  );
  assert!(!asked.contains("after="), "no cursor reads from the oldest: {asked}");

  // An empty window starts the cursor at nothing, so what comes next is told.
  let scratch = Scratch::new().await;
  let (_, empty) = stand().await;

  pass(scratch.db(), &empty).await.unwrap();
  assert_eq!(settings::get(scratch.db(), CURSOR_KEY).await.unwrap().as_deref(), Some("0"));
}

#[tokio::test]
async fn an_item_closed_by_somebody_else_is_told_once_to_its_orderer_and_its_own_close_is_not() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  supervisor(db, "chat", &[]).await;
  settings::set(db, CURSOR_KEY, "10").await.unwrap();

  {
    let mut held = peers.lock().unwrap();

    held
      .items
      .insert(70, item(70, "done", json!({ "scope": "-700", "requesterName": "agent-a" })));
    held.events = vec![
      closed(11, 71, Some("someone")),
      closed(12, 72, Some("agent-a")),
      closed(13, 73, Some("chat")),
      event(
        14,
        "item.changed",
        item(74, "held", json!({ "previousState": "open", "changedBy": "x" })),
      ),
      event(15, "item.changed", item(75, "done", json!({ "previousState": null, "changedBy": "x" }))),
    ];
  }

  pass(db, &vars).await.unwrap();
  pass(db, &vars).await.unwrap();

  let told = peers.lock().unwrap().instructed();

  assert_eq!(told.len(), 1, "only the close by somebody else, once: {told:?}");
  assert_eq!(
    (&told[0]["kind"], &told[0]["events"], &told[0]["room"], &told[0]["sort"]),
    (&json!("item-closed:71"), &json!(["item-closed:#71"]), &json!(-700), &json!("job"))
  );
  assert!(told[0]["said"]["order"].as_str().unwrap().contains("someone"));
  assert_eq!(settings::get(db, CURSOR_KEY).await.unwrap().as_deref(), Some("15"));
}

#[tokio::test]
async fn a_question_is_told_to_its_orderer_withdrawn_once_settled_and_told_to_its_orderer_again_when_due() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  settings::set(db, CURSOR_KEY, "20").await.unwrap();

  {
    let mut held = peers.lock().unwrap();

    held.items.insert(80, item(80, "done", json!({ "scope": "-800" })));
    held.items.insert(81, item(81, "leased", json!({ "labels": { "raised-by": "80" } })));
    held.events = vec![
      event(
        21,
        "question.created",
        json!({ "id": 5, "itemId": 81, "text": "which one?", "choices": ["a", "b"] }),
      ),
      event(
        22,
        "question.created",
        json!({ "id": 6, "scope": "-900", "text": "on its own", "choices": [] }),
      ),
      event(
        22,
        "question.created",
        json!({ "id": 7, "itemId": 81, "text": "put by its asker", "choices": ["a", "b"], "toldByAsker": true }),
      ),
      event(
        23,
        "question.due",
        json!({ "id": 6, "scope": "-900", "text": "on its own", "choices": [] }),
      ),
      event(
        24,
        "reply.created",
        json!({ "id": 1, "questionId": 5, "settles": false, "text": "a word" }),
      ),
    ];
  }

  pass(db, &vars).await.unwrap();

  {
    let held = peers.lock().unwrap();
    let told = held.instructed();

    assert_eq!(told.len(), 3, "{told:?}");
    assert_eq!(
      (&told[0]["kind"], &told[0]["events"], &told[0]["room"]),
      (&json!("question-awaits-answer:5"), &json!(["question-awaits-answer:#5"]), &json!(-800)),
      "the question's item's orderer, up the chain"
    );
    assert!(told[0]["said"]["order"].as_str().unwrap().contains("which one?"));
    assert_eq!(told[1]["room"], json!(-900), "a question on its own: the room its scope names");
    assert_eq!(
      (&told[2]["kind"], &told[2]["events"], &told[2]["room"], &told[2]["sort"]),
      (
        &json!("job-question-due:6"),
        &json!(["job-question-due:#6"]),
        &json!(-900),
        &json!("job")
      ),
      "a question past its due time: an instruction to the question's orderer"
    );
    assert!(held.withdrawn().is_empty(), "a word that settles nothing withdraws nothing");
  }

  let question = owed(db, &format!("{QUESTION_OWED}5")).await.unwrap().remove("").unwrap();

  assert!(question.told_at.is_some(), "the question's row kept, told");

  peers.lock().unwrap().events.push(event(
    25,
    "reply.created",
    json!({ "id": 2, "questionId": 5, "settles": true, "choice": "a" }),
  ));
  pass(db, &vars).await.unwrap();
  assert!(owed(db, &format!("{QUESTION_OWED}5")).await.unwrap().is_empty(), "its row let go");

  let held = peers.lock().unwrap();

  assert_eq!(held.withdrawn(), ["1"], "the settled question's instruction withdrawn");
  assert_eq!(held.instructed().len(), 3, "nothing told twice");
}

#[tokio::test]
async fn a_question_the_job_service_put_to_the_owner_is_told_to_its_orderer_once_and_a_supervisor_s_move_not()
 {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  settings::set(db, CURSOR_KEY, "50").await.unwrap();

  {
    let mut held = peers.lock().unwrap();

    held.items.insert(90, item(90, "leased", json!({ "scope": "-950" })));
    let handed = |at: i64| {
      json!({ "id": 11, "itemId": 90, "text": "deploy the docs unit?", "choices": [], "addressee": "person",
              "readdressedBy": "due time", "readdressedAt": at })
    };
    held.events = vec![
      // The supervisor's own move: it tells the owner itself.
      event(
        51,
        "question.changed",
        json!({ "id": 10, "itemId": 90, "text": "x", "choices": ["a", "b"], "addressee": "person",
                "readdressedBy": "chat", "readdressedAt": 51 }),
      ),
      event(52, "question.changed", handed(52)),
      // A later change of the same question, a talk pausing it: told no more.
      event(53, "question.changed", handed(52)),
    ];
  }

  pass(db, &vars).await.unwrap();

  let told = peers.lock().unwrap().instructed();

  assert_eq!(told.len(), 1, "{told:?}");
  assert_eq!(
    (&told[0]["kind"], &told[0]["events"], &told[0]["room"]),
    (
      &json!("question-awaits-answer:11"),
      &json!(["question-awaits-answer:#11"]),
      &json!(-950)
    )
  );
  let order = told[0]["said"]["order"].as_str().unwrap();
  assert!(
    order.contains("minute") && order.contains("deploy the docs unit?") && order.contains("an open answer"),
    "{order}"
  );
  assert!(
    owed(db, &format!("{QUESTION_OWED}11")).await.unwrap().contains_key(""),
    "withdrawn once settled, as an asked one"
  );
}

#[tokio::test]
async fn a_question_settled_on_its_recommendation_owes_its_orderer_one_instruction_and_a_person_s_settle_none()
 {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  settings::set(db, CURSOR_KEY, "90").await.unwrap();

  {
    let mut held = peers.lock().unwrap();

    held.items.insert(95, item(95, "leased", json!({ "scope": "-950" })));
    held.questions.push(json!({ "id": 40, "itemId": 95, "state": "answered" }));
    held.questions.push(json!({ "id": 41, "itemId": 96, "state": "answered" }));
    held.events = vec![
      event(
        91,
        "reply.created",
        json!({
          "id": 3, "questionId": 40, "settles": true, "choice": "b", "text": "b", "actorName": "due time",
          "source": "recommendation", "why": "b keeps the contract"
        }),
      ),
      event(
        92,
        "reply.created",
        json!({
          "id": 4, "questionId": 41, "settles": true, "choice": null, "text": "write it fresh",
          "actorName": "due time", "source": "recommendation", "why": "nothing to keep"
        }),
      ),
      event(
        93,
        "reply.created",
        json!({ "id": 5, "questionId": 42, "settles": true, "choice": "a", "actorName": "tester", "source": "person" }),
      ),
      event(
        94,
        "reply.created",
        json!({ "id": 6, "questionId": 43, "settles": true, "choice": "a", "actorName": "qa-admin", "source": "supervisor" }),
      ),
    ];
  }

  pass(db, &vars).await.unwrap();

  let held = peers.lock().unwrap();
  let told = held.instructed();

  assert_eq!(
    told.len(),
    2,
    "one for each settle on a recommendation, none for a person's or a supervisor's"
  );
  assert_eq!(
    (&told[0]["kind"], &told[0]["events"], &told[0]["room"], &told[0]["sort"]),
    (
      &json!("job-question-settled:40"),
      &json!(["job-question-settled:#40"]),
      &json!(-950),
      &json!("job")
    ),
    "in the room of the question's item's orderer"
  );
  assert_eq!(
    (&told[1]["kind"], &told[1]["room"]),
    (&json!("job-question-settled:41"), &Value::Null),
    "an item the job service does not answer for: the owner's private room"
  );
}

#[tokio::test]
async fn a_cursor_past_the_window_reads_again_from_the_oldest_telling_nothing_it_passed() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  settings::set(db, CURSOR_KEY, "5").await.unwrap();

  {
    let mut held = peers.lock().unwrap();

    held.window_from = 30;
    held.events = vec![closed(30, 71, Some("someone")), closed(31, 72, Some("someone"))];
  }

  pass(db, &vars).await.unwrap();

  assert!(peers.lock().unwrap().instructed().is_empty());
  assert_eq!(settings::get(db, CURSOR_KEY).await.unwrap().as_deref(), Some("31"));
}

#[tokio::test]
async fn an_event_whose_notice_could_not_be_owed_holds_the_cursor_before_it_until_it_is() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  settings::set(db, CURSOR_KEY, "40").await.unwrap();

  {
    let mut held = peers.lock().unwrap();

    held.items.insert(82, item(82, "leased", json!({ "scope": "-820" })));
    // The item the second question stands on cannot be read: its orderer is not found.
    held.failing.insert("/items/83".to_owned());
    held.events = vec![
      event(41, "question.created", json!({ "id": 8, "itemId": 82, "text": "first", "choices": [] })),
      event(
        42,
        "question.created",
        json!({ "id": 9, "itemId": 83, "text": "second", "choices": [] }),
      ),
      closed(43, 84, Some("someone")),
    ];
  }

  assert!(pass(db, &vars).await.is_err());
  assert_eq!(settings::get(db, CURSOR_KEY).await.unwrap().as_deref(), Some("41"));
  assert_eq!(peers.lock().unwrap().instructed().len(), 1, "only the event before it told");

  peers.lock().unwrap().failing.clear();
  pass(db, &vars).await.unwrap();

  let kinds: Vec<Value> =
    peers.lock().unwrap().instructed().iter().map(|told| told["kind"].clone()).collect();

  assert_eq!(
    kinds,
    [
      json!("question-awaits-answer:8"),
      json!("question-awaits-answer:9"),
      json!("item-closed:84")
    ]
  );
  assert_eq!(settings::get(db, CURSOR_KEY).await.unwrap().as_deref(), Some("43"));
}

#[tokio::test]
async fn a_commit_waiting_to_go_live_stays_owed_until_its_item_no_longer_waits() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;
  let waiting = |id: i64, commit: &str| {
    event(
      id,
      "item.changed",
      item(
        90,
        "leased",
        json!({ "previousState": "leased", "labels": { "close-phase": "waiting-live", "close-commit": commit } }),
      ),
    )
  };

  settings::set(db, CURSOR_KEY, "50").await.unwrap();
  peers.lock().unwrap().events = vec![waiting(51, "abc"), waiting(52, "abc"), waiting(53, "def")];
  pass(db, &vars).await.unwrap();

  let owed = agency_server::outbox::owed(db, DEPLOY_OWED).await.unwrap();

  assert_eq!(owed["90"].mark, "def", "owed once a commit, and afresh for the next");
  assert_eq!(peers.lock().unwrap().instructed().len(), 2);

  peers.lock().unwrap().events.push(event(
    54,
    "item.changed",
    item(90, "leased", json!({ "previousState": "leased", "labels": {} })),
  ));
  pass(db, &vars).await.unwrap();

  assert!(agency_server::outbox::owed(db, DEPLOY_OWED).await.unwrap().is_empty());
  assert_eq!(peers.lock().unwrap().withdrawn(), ["2"], "the copy told last withdrawn");
}

#[tokio::test]
async fn a_merged_commit_waiting_for_its_batch_deployment_is_owed_like_one_waiting_to_go_live() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  settings::set(db, CURSOR_KEY, "60").await.unwrap();
  peers.lock().unwrap().events = vec![event(
    61,
    "item.changed",
    item(
      91,
      "leased",
      json!({ "previousState": "leased", "labels": { "close-phase": "waiting-batch", "close-commit": "abc" } }),
    ),
  )];
  pass(db, &vars).await.unwrap();

  assert_eq!(agency_server::outbox::owed(db, DEPLOY_OWED).await.unwrap()["91"].mark, "abc");
  assert_eq!(peers.lock().unwrap().instructed().len(), 1);
}

#[tokio::test]
async fn work_closed_in_two_repositories_is_owed_for_every_waiting_pair_until_none_waits() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;
  let changed = |id: i64, labels: Value| {
    event(
      id,
      "item.changed",
      item(95, "leased", json!({ "previousState": "leased", "labels": labels })),
    )
  };

  settings::set(db, CURSOR_KEY, "70").await.unwrap();
  peers.lock().unwrap().events = vec![changed(
    71,
    json!({ "close-phase": "waiting-batch", "close-commit": "def",
            "close-phase:web": "waiting-batch", "close-commit:web": "abc",
            "close-phase:drive": "waiting-batch", "close-commit:drive": "def" }),
  )];
  pass(db, &vars).await.unwrap();

  assert_eq!(owed(db, DEPLOY_OWED).await.unwrap()["95"].mark, "drive=def,web=abc");

  let told = peers.lock().unwrap().instructed();
  let said = told[0]["said"]["order"].as_str().unwrap();

  for part in ["commit abc into web's main", "commit def into drive's main"] {
    assert!(said.contains(part), "{part} in {said}");
  }

  // The web batch went live and its pair was taken off: the drive commit alone is owed afresh.
  peers.lock().unwrap().events.push(changed(
    72,
    json!({ "close-phase": "waiting-batch", "close-commit": "def",
            "close-phase:drive": "waiting-batch", "close-commit:drive": "def" }),
  ));
  pass(db, &vars).await.unwrap();

  assert_eq!(owed(db, DEPLOY_OWED).await.unwrap()["95"].mark, "drive=def");
  assert_eq!(peers.lock().unwrap().instructed().len(), 2);

  // The drive batch went live too: nothing waits, so the notice is let go.
  peers.lock().unwrap().events.push(changed(73, json!({})));
  pass(db, &vars).await.unwrap();

  assert!(owed(db, DEPLOY_OWED).await.unwrap().is_empty());
}

fn deploy_waiting(id: i64, item_id: i64, commit: &str) -> Value {
  event(
    id,
    "item.changed",
    item(
      item_id,
      "leased",
      json!({ "previousState": "leased", "labels": { "close-phase": "waiting-batch", "close-commit": commit } }),
    ),
  )
}

#[tokio::test]
async fn a_pass_past_the_window_owes_the_commits_its_stretch_left_waiting_and_lets_go_those_that_went_live()
{
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  settings::set(db, CURSOR_KEY, "10").await.unwrap();
  peers.lock().unwrap().events = vec![deploy_waiting(11, 92, "abc")];
  pass(db, &vars).await.unwrap();
  assert_eq!(owed(db, DEPLOY_OWED).await.unwrap()["92"].mark, "abc");

  // The cursor falls out of the window: 92 went live, 93 began to wait, 94 waited and went live, and a close by
  // somebody else is passed untold.
  {
    let mut held = peers.lock().unwrap();

    held.window_from = 20;
    held.events = vec![
      event(
        20,
        "item.changed",
        item(92, "done", json!({ "previousState": "leased", "changedBy": "someone" })),
      ),
      deploy_waiting(21, 93, "x1"),
      deploy_waiting(22, 94, "y1"),
      event(
        23,
        "item.changed",
        item(94, "done", json!({ "previousState": "leased", "changedBy": "someone" })),
      ),
    ];
  }

  pass(db, &vars).await.unwrap();

  let owed = owed(db, DEPLOY_OWED).await.unwrap();
  let kinds: Vec<Value> =
    peers.lock().unwrap().instructed().iter().map(|told| told["kind"].clone()).collect();

  assert_eq!(owed.keys().collect::<Vec<_>>(), ["93"], "{owed:?}");
  assert_eq!(kinds, [json!("item-awaits-deploy:92"), json!("item-awaits-deploy:93")]);
  assert_eq!(peers.lock().unwrap().withdrawn(), ["1"], "the copy of the commit gone live withdrawn");
  assert_eq!(settings::get(db, CURSOR_KEY).await.unwrap().as_deref(), Some("23"));
}

#[tokio::test]
async fn a_question_past_its_due_time_taken_again_owes_its_parked_job_s_notice_again() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  peers.lock().unwrap().items.insert(95, item(95, "leased", json!({ "scope": "-950" })));
  // The process before parked the job and stopped before its notice's row was written.
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "started_at", "prompt", "review_check", "review_question", "review_parked")
         VALUES (5, 95, 1, 'runner', 1, 'w1', 'reviewing', 1000, 'the work', 'promises', 31, true)"#,
      vec![],
    )
    .await;
  settings::set(db, CURSOR_KEY, "70").await.unwrap();
  peers.lock().unwrap().events =
    vec![event(71, "question.due", json!({ "id": 31, "itemId": 95, "text": "which?" }))];

  pass(db, &vars).await.unwrap();

  let told: Vec<(Value, Value)> = peers
    .lock()
    .unwrap()
    .instructed()
    .iter()
    .map(|told| (told["kind"].clone(), told["room"].clone()))
    .collect();

  assert_eq!(
    told,
    [
      (json!("job-review-parked:95"), json!(-950)),
      (json!("job-question-due:31"), json!(-950))
    ],
    "the parked job's notice owed again, beside the question's own"
  );
}

#[tokio::test]
async fn a_note_added_to_a_running_job_s_item_is_kept_before_the_cursor_moves_and_reaches_its_turn() {
  use agency_server::door::aside;
  use agency_server::door::turn::AsideOutcome;
  use agency_server::turn_pickup::{InFlight, keep};

  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  // A job running on item 8, its prompt composed with the item's notes up to the third, and its turn kept in
  // flight with the door listening for words under its run.
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "item_agent", "run_name", "session_id", "room_id", "state", "started_at", "prompt", "notes_through")
         VALUES (64, 8, 9, 'harness', 10, 'worker', 'coder-01', 'job-64', 'worker@run:5', 5, 'running', $1,
           'work', 3)"#,
      vec![(agency_server::now_ms() - 60_000).into()],
    )
    .await;
  keep(
    db,
    &InFlight {
      session: "worker@run:5".into(),
      turn: 1,
      run: "job-64".into(),
      key: None,
      agent: "worker".into(),
      chat_id: Some(5),
      ledger: None,
      job: Some(64),
      started_at: agency_server::now_ms(),
    },
  )
  .await;

  let listening = aside::listen("job-64");
  let note = |event_id: i64, note_id: i64, item_id: i64, text: &str| {
    event(
      event_id,
      "note.created",
      json!({ "id": note_id, "itemId": item_id, "text": text, "actorName": "chat-agent", "labels": {},
              "createdAt": agency_server::now_ms() }),
    )
  };

  settings::set(db, CURSOR_KEY, "10").await.unwrap();
  peers.lock().unwrap().events = vec![
    note(11, 5, 8, "Also cover the console's screen."),
    note(12, 6, 99, "A note on an item no job runs."),
  ];

  pass(db, &vars).await.unwrap();

  // The note is kept for the job by the time the cursor stands past it; the other is nobody's.
  assert_eq!(settings::get(db, CURSOR_KEY).await.unwrap().as_deref(), Some("12"));
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "job_words" WHERE "job_id" = 64 AND "note_id" = 5"#, vec![])
      .await,
    1
  );
  assert_eq!(
    scratch.count(r#"SELECT COUNT(*) FROM "job_words" WHERE "note_id" = 6"#, vec![]).await,
    0
  );
  assert!(peers.lock().unwrap().instructed().is_empty(), "a note tells nobody");

  // The turn's door takes it at its next look, behind the paragraph naming it a note of the item.
  let offered = tokio::time::timeout(std::time::Duration::from_secs(30), async {
    loop {
      if let Some(offered) = listening.take() {
        break offered;
      }

      tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
  })
  .await
  .expect("the note was offered to the job's turn");

  assert!(offered.words.contains("item #8 by chat-agent"), "{}", offered.words);
  assert!(offered.words.ends_with("Also cover the console's screen."), "{}", offered.words);
  listening.said(offered, AsideOutcome::Delivered);

  // Taken by the turn, it is handed and no later turn carries it again; the event read again keeps nothing new.
  for _ in 0..3000 {
    if scratch
      .count(
        r#"SELECT COUNT(*) FROM "job_words" WHERE "job_id" = 64 AND "handed_at" IS NOT NULL"#,
        vec![],
      )
      .await
      == 1
    {
      break;
    }

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
  }

  assert_eq!(
    agency_server::job_words::carried(db, &agency_server::peers::gate::NoGate, 64)
      .await
      .text,
    None
  );

  settings::set(db, CURSOR_KEY, "10").await.unwrap();
  pass(db, &vars).await.unwrap();
  assert_eq!(
    scratch.count(r#"SELECT COUNT(*) FROM "job_words" WHERE "job_id" = 64"#, vec![]).await,
    1
  );
}

#[tokio::test]
async fn items_waiting_for_their_deployment_are_told_once_each_and_not_again_on_the_next_change() {
  use agency_server::outbox::{restarted, send_due};

  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  settings::set(db, CURSOR_KEY, "80").await.unwrap();
  peers.lock().unwrap().events = vec![
    deploy_waiting(81, 101, "c1"),
    deploy_waiting(82, 102, "c2"),
    deploy_waiting(83, 103, "c3"),
  ];
  pass(db, &vars).await.unwrap();
  assert_eq!(peers.lock().unwrap().instructed().len(), 3, "each waiting item told once");

  // Another item of the queue changes, the waiting items' own events come again with the same commits, and the
  // sender's retelling and a start come due: nothing is told again.
  peers.lock().unwrap().events.extend([
    event(
      84,
      "item.changed",
      item(104, "leased", json!({ "previousState": "open", "changedBy": "x" })),
    ),
    deploy_waiting(85, 101, "c1"),
    deploy_waiting(86, 102, "c2"),
    deploy_waiting(87, 103, "c3"),
  ]);
  pass(db, &vars).await.unwrap();
  restarted(db).await.unwrap();
  assert_eq!(send_due(db, &vars).await.unwrap(), 0, "a kept notice is due no more");

  let owed = owed(db, DEPLOY_OWED).await.unwrap();

  assert_eq!(peers.lock().unwrap().instructed().len(), 3, "told once each, not again");
  assert_eq!(owed.keys().collect::<Vec<_>>(), ["101", "102", "103"], "kept while they wait");

  // A new commit after a send-back is a new state, told again.
  peers.lock().unwrap().events.push(deploy_waiting(88, 102, "c2b"));
  pass(db, &vars).await.unwrap();

  let kinds: Vec<Value> =
    peers.lock().unwrap().instructed().iter().map(|told| told["kind"].clone()).collect();

  assert_eq!(kinds.len(), 4, "{kinds:?}");
  assert_eq!(kinds[3], json!("item-awaits-deploy:102"));
}

#[tokio::test]
async fn an_item_s_title_is_asked_of_the_gate_as_it_is_raised_or_its_fields_change_and_not_as_its_state_moves()
 {
  use std::sync::Arc;

  use agency_server::peers::gate::Gate;

  use crate::support::gates::Translator;

  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;
  let translator = Arc::new(Translator::default());
  let gate: Arc<dyn Gate> = translator.clone();
  let titled = |id: i64, title: &str, previous: Option<&str>| {
    item(id, "open", json!({ "title": title, "previousState": previous, "changedBy": "someone" }))
  };

  settings::set(db, CURSOR_KEY, "2").await.unwrap();
  peers.lock().unwrap().events = vec![
    event(3, "item.created", titled(81, "Ουρά εργασιών", None)),
    event(4, "item.changed", titled(82, "Νέος τίτλος", None)),
    event(5, "item.changed", titled(83, "Κατάσταση", Some("leased"))),
    event(6, "item.created", titled(84, "Repair the queue", None)),
  ];

  agency_server::watches::job_events::pass(db, &vars, &gate).await.unwrap();

  // The titles are asked in tasks of their own, which run once the pass lets the runtime go.
  for _ in 0..100 {
    if translator.calls().len() >= 2 {
      break;
    }

    tokio::task::yield_now().await;
  }

  let mut asked: Vec<String> =
    translator.calls().into_iter().flat_map(|(_, originals)| originals).collect();

  asked.sort();
  assert_eq!(asked, ["Νέος τίτλος", "Ουρά εργασιών"], "a moved state and a Latin title ask nothing");
}
