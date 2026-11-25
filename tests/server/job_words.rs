//! Words for a running job (docs/agent-control.md, 「Words for a running job」): a job's session id, its current
//! one or an earlier turn's, addresses the job while it runs and is refused as ended only once it has ended; a
//! word said while a turn of the job works goes into that turn; and a note added to the job's item is kept for
//! its next turn, carried there, and never lost to a turn that did not carry it; and a server gives its sessions
//! numbers past every one the store names, so an id never names two sessions across restarts.

use std::future::Future;
use std::time::Duration;

use serde_json::{Value, json};

use agency_server::door::aside;
use agency_server::door::turn::AsideOutcome;
use agency_server::job_words::{self, Note};
use agency_server::now_ms;
use agency_server::peers::gate::NoGate;
use agency_server::turn_pickup::{InFlight, keep};
use agency_server::words;

use crate::support::gates;
use crate::support::scratch::Scratch;
use crate::support::standing;

// A job written as its launch writes it, standing in `state`.
async fn job(
  scratch: &Scratch,
  id: i64,
  item: i64,
  state: &str,
  session: &str,
  notes_through: Option<i64>,
) {
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "item_agent", "run_name", "session_id", "room_id", "state", "started_at", "prompt", "notes_through")
         VALUES ($1, $2, 9, 'harness', 10, 'worker', 'coder-01', $3, $4, 5, $5, $6, 'work', $7)"#,
      vec![
        id.into(),
        item.into(),
        format!("job-{id}").into(),
        session.into(),
        state.into(),
        (now_ms() - 60_000).into(),
        notes_through.into(),
      ],
    )
    .await;
}

// Waited on until `done` answers true, for as long as a slow machine may take and no longer.
async fn until<F: Future<Output = bool>>(mut done: impl FnMut() -> F) {
  tokio::time::timeout(Duration::from_secs(30), async {
    while !done().await {
      tokio::time::sleep(Duration::from_millis(10)).await;
    }
  })
  .await
  .expect("the condition came about");
}

async fn post(port: u16, token: &str, path: &str, body: Value) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .post(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .json(&body)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_s_sessions_take_words_while_it_runs_and_refuse_them_only_once_it_has_ended() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  // Under review between stages: its last turn's session is gone, and an earlier stage's session was another.
  job(&scratch, 61, 7, "reviewing", "worker@run:9", Some(0)).await;
  // Running, its turn in a session this server never held, as after agency's restart.
  job(&scratch, 66, 11, "running", "worker@run:3", Some(0)).await;
  scratch
    .run(
      r#"INSERT INTO "job_sessions" ("session_id", "job_id", "started_at") VALUES ('worker@run:4', 61, 1)"#,
      vec![],
    )
    .await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  for path in ["/sessions/worker@run:9/words", "/sessions/worker@run:4/words", "/jobs/61/words"] {
    let (status, said) = post(port, token, path, json!({ "words": "more" })).await;

    assert_eq!(status, 201, "{path}: {said}");
    assert_eq!(
      (said["said"].clone(), said["jobId"].clone()),
      (json!("kept"), json!(61)),
      "{path}: {said}"
    );
  }

  // A running job's session is no longer refused as ended.
  let (status, said) = post(port, token, "/sessions/worker@run:3/words", json!({ "words": "more" })).await;

  assert_eq!((status, said["jobId"].clone()), (201, json!(66)), "{said}");
  assert_ne!(said["code"], json!("run-ended"), "{said}");

  // The job's holder may say to it, and a supervisor that did not ask for it may not.
  assert_eq!(
    post(port, token, "/jobs/61/words", json!({ "words": "more", "by": "harness" })).await.1["said"],
    json!("kept")
  );
  assert_eq!(
    post(port, token, "/jobs/61/words", json!({ "words": "more", "by": "supervisor-b" }))
      .await
      .1["code"],
    json!("not-the-sender")
  );
  assert_eq!(
    post(port, token, "/jobs/999/words", json!({ "words": "more" })).await.1["code"],
    json!("unknown-job")
  );

  // Every word said was kept for the job's next turn, which carries them all.
  let kept = job_words::carried(scratch.db(), &NoGate, 61).await;

  assert_eq!(kept.behind("stage 2").matches("more").count(), 4, "{kept:?}");

  // Once the job has ended, its sessions refuse words as ended.
  scratch.run(r#"UPDATE "jobs" SET "state" = 'ended' WHERE "id" = 61"#, vec![]).await;

  for path in ["/sessions/worker@run:9/words", "/jobs/61/words"] {
    let (status, said) = post(port, token, path, json!({ "words": "more" })).await;

    assert_eq!((status, said["code"].clone()), (410, json!("run-ended")), "{path}: {said}");
  }
}

#[tokio::test]
async fn a_word_said_to_a_running_job_goes_into_the_turn_it_works_now_and_waits_for_the_next_otherwise() {
  let scratch = Scratch::new().await;
  let db = scratch.db().clone();

  job(&scratch, 62, 7, "running", "worker@run:3", Some(0)).await;
  // The job's turn in flight, as a turn this process sent or one it picked up across a restart keeps it.
  keep(
    &db,
    &InFlight {
      session: "worker@run:3".into(),
      turn: 1,
      run: "job-62".into(),
      key: None,
      agent: "worker".into(),
      chat_id: Some(5),
      ledger: None,
      job: Some(62),
      started_at: now_ms(),
    },
  )
  .await;

  let running = job_words::named(&db, 62).await.unwrap().unwrap();
  let listening = aside::listen("job-62");
  let said = tokio::spawn({
    let (db, running) = (db.clone(), running.clone());

    async move { job_words::say(&db, &NoGate, &running, "check the other branch too", "owner", None).await }
  });

  // The turn's door takes the offer at its look.
  let mut offered = None;

  until(|| {
    offered = offered.take().or_else(|| listening.take());

    std::future::ready(offered.is_some())
  })
  .await;

  let offered = offered.unwrap();

  assert_eq!(offered.words, "check the other branch too");
  listening.said(offered, AsideOutcome::Delivered);

  let said = said.await.unwrap().unwrap().unwrap();

  assert_eq!(said.outcome, Some(AsideOutcome::Delivered));
  assert_eq!(
    job_words::carried(&db, &NoGate, 62).await.text,
    None,
    "a word the turn took is not carried again"
  );

  // A turn that ends before taking a word leaves it for the job's next turn.
  drop(listening);

  let said = job_words::say(&db, &NoGate, &running, "and update the document", "owner", None)
    .await
    .unwrap()
    .unwrap();

  assert_eq!(said.outcome, None);
  assert!(
    job_words::carried(&db, &NoGate, 62)
      .await
      .behind("stage 3")
      .ends_with("and update the document"),
    "the kept word follows the next turn's own prompt"
  );
}

#[tokio::test]
async fn a_note_added_to_a_running_job_s_item_reaches_its_next_turn_and_is_never_lost() {
  let scratch = Scratch::new().await;
  let db = scratch.db().clone();
  let note = |id: i64, actor: &str, text: &str| {
    Note::of(&json!({ "id": id, "itemId": 8, "actorName": actor, "text": text, "createdAt": now_ms() }))
      .unwrap()
  };

  // Its prompt carried the item's notes up to the third.
  job(&scratch, 63, 8, "reviewing", "worker@run:5", Some(3)).await;

  // Its prompt's own note, its own turn's notes, the job service's records and a handoff, which the turn that
  // goes on opens with already, are not handed back to it.
  let handoff = Note::of(&json!({ "id": 9, "itemId": 8, "actorName": "agency", "text": "a handoff",
                                  "labels": { "kind": "handoff", "job": "63", "source": "log" },
                                  "createdAt": now_ms() }))
  .unwrap();

  for passed in [
    note(2, "harness", "old"),
    note(6, "worker", "mine"),
    note(7, "coder-01", "mine"),
    note(8, "bmo-job", "record"),
    handoff,
  ] {
    job_words::note_added(&db, &gates::none(), passed).await.unwrap();
  }

  // A note the orderer added while the job runs is kept for it, once however often its event is read.
  job_words::note_added(&db, &gates::none(), note(5, "harness", "Also cover the console's screen."))
    .await
    .unwrap();
  job_words::note_added(&db, &gates::none(), note(5, "harness", "Also cover the console's screen."))
    .await
    .unwrap();

  until(|| {
    let db = db.clone();

    async move { words::report(&db, 63).await.unwrap().is_some_and(|report| report.contains("kept")) }
  })
  .await;

  let kept = job_words::carried(&db, &NoGate, 63).await;
  let prompt = kept.behind("Do stage 2 now.");

  assert!(prompt.starts_with("Do stage 2 now."), "{prompt}");
  assert!(prompt.contains("item #8 by harness"), "{prompt}");
  assert_eq!(prompt.matches("Also cover the console's screen.").count(), 1, "{prompt}");
  assert!(
    !prompt.contains("old")
      && !prompt.contains("mine")
      && !prompt.contains("record")
      && !prompt.contains("a handoff"),
    "{prompt}"
  );

  // A turn that did not go out to its end leaves the note for the next, which carries it again.
  kept.left(&db).await;

  let kept = job_words::carried(&db, &NoGate, 63).await;

  assert!(kept.behind("").contains("Also cover the console's screen."));
  kept.taken(&db, 63, "job-63").await;
  assert_eq!(job_words::carried(&db, &NoGate, 63).await.text, None, "a carried note is handed once");

  // The job's report says what it was told while it ran.
  let report = words::report(&db, 63).await.unwrap().unwrap();

  assert!(report.contains("Also cover the console's screen."), "{report}");
}

#[tokio::test]
async fn a_server_numbers_its_sessions_past_every_number_a_process_before_it_gave() {
  use std::collections::HashMap;
  use std::sync::Arc;

  use agency_server::Service;
  use agency_server::config::Config;
  use agency_server::herdr_backend::HerdrSessions;
  use agency_server::places::{Ground, Places};
  use agent_session::{Apart, Scope, Sessions, Start};
  use harness_providers::Machine;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  // Processes before this one gave a job's session the number 41 and an earlier turn of it 57.
  job(&scratch, 65, 9, "ended", "worker@run:41", None).await;
  scratch
    .run(
      r#"INSERT INTO "job_sessions" ("session_id", "job_id", "started_at") VALUES ('worker@run:57', 65, 1)"#,
      vec![],
    )
    .await;

  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), home.path().to_string_lossy().into_owned()),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
    ],
    home.path().join("data"),
  );
  let sessions = HerdrSessions::new(Places::new(Ground::new(&service, &machine, None), Arc::default()));

  sessions.number_past_kept().await;

  let started = sessions
    .start(Start {
      agent: "worker".into(),
      scope: Scope::Apart(Apart {
        provider: "claude-code".into(),
        model: "claude-sonnet-5".into(),
        cwd: home.path().to_string_lossy().into_owned(),
        role: Some("worker".into()),
        ..Apart::default()
      }),
    })
    .await
    .unwrap();

  assert_eq!(started.id.0, "worker@run:58", "no id a process before gave is given again");
}

#[tokio::test]
async fn a_word_in_another_language_is_kept_as_said_and_goes_into_the_turn_and_the_next_one_in_english() {
  let scratch = Scratch::new().await;
  let db = scratch.db().clone();
  let greek = "Έλεγξε και τον άλλο κλάδο.";
  let later = "Ενημέρωσε και το έγγραφο.";
  let gate = std::sync::Arc::new(gates::Translator::reading(&[
    (greek, "Check the other branch too."),
    (later, "Update the document too."),
  ]));

  job(&scratch, 64, 9, "running", "worker@run:4", Some(0)).await;
  keep(
    &db,
    &InFlight {
      session: "worker@run:4".into(),
      turn: 1,
      run: "job-64".into(),
      key: None,
      agent: "worker".into(),
      chat_id: Some(5),
      ledger: None,
      job: Some(64),
      started_at: now_ms(),
    },
  )
  .await;

  let running = job_words::named(&db, 64).await.unwrap().unwrap();
  let listening = aside::listen("job-64");
  let said = tokio::spawn({
    let (db, running, gate) = (db.clone(), running.clone(), gate.clone());

    async move { job_words::say(&db, gate.as_ref(), &running, greek, "owner", None).await }
  });
  let mut offered = None;

  until(|| {
    offered = offered.take().or_else(|| listening.take());

    std::future::ready(offered.is_some())
  })
  .await;

  let offered = offered.unwrap();

  assert_eq!(offered.words, "Check the other branch too.");
  listening.said(offered, AsideOutcome::Delivered);
  said.await.unwrap().unwrap().unwrap();

  // A word no turn took is carried by the next in English too, and the store keeps both as said.
  drop(listening);
  job_words::say(&db, gate.as_ref(), &running, later, "owner", None)
    .await
    .unwrap()
    .unwrap();

  let carried = job_words::carried(&db, gate.as_ref(), 64).await.behind("stage 2");

  assert!(carried.ends_with("Update the document too."), "{carried}");
  assert!(!carried.contains(later), "{carried}");

  let kept: Vec<String> =
    store::all(&db, r#"SELECT "text" FROM "job_words" WHERE "job_id" = 64 ORDER BY "id""#, vec![])
      .await
      .unwrap()
      .iter()
      .map(|row| row.try_get("", "text").unwrap())
      .collect();

  assert_eq!(kept, [greek, later]);
}
