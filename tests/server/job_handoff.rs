//! The handoff a job's session leaves (docs/sessions.md, 「The handoff a job's session leaves」; docs/api.md,
//! 「assignments and jobs」): what a session's answer gives, the run log standing in for a session that cannot
//! be asked, a prompt opened on a handoff with the item's title kept first; and a stopped job's answer to the
//! stop words noted on its item as its handoff, which the item's next job opens on, on another agent's launch
//! as much as the same one's; and, carried on a backend of the test's own over the session contract, a running
//! job whose model changed under its review asks the session it leaves on that session's own road, the new
//! session's first turn opening on what it wrote, and a stopped job's relaunch sending its first turn on the
//! handoff the stopped session wrote.

use std::sync::Arc;
use std::time::Duration;

use agency_server::control::work::{handed_over, launched};
use agency_server::control::{Front, router};
use agency_server::door::execution::cancelled;
use agency_server::job_handoff::{Handoff, Source, ahead, answered, from_log};
use agency_server::job_review::{Resumed, Turn as ReviewTurn};
use agency_server::lines::place::Place;
use agency_server::now_ms;
use agency_server::run_log::lines::Line;
use agency_server::run_log::{RunLog, Target};
use agency_server::turn_pickup::{InFlight, at_start, keep};
use agent_session::Scope;
use policy::job_handoff::JOB_HANDOFF;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::support::grounds::seeded_with;
use crate::support::program;
use crate::support::rows::repository;
use crate::support::scratch::Scratch;
use crate::support::stand_in::StandIn;

const TOKEN: &str = "secret";

#[test]
fn a_session_s_answer_is_its_handoff_unless_it_answered_nothing_or_none() {
  assert_eq!(answered(None), None);
  assert_eq!(answered(Some("  \n")), None);
  assert_eq!(answered(Some(" None. ")), None);
  assert_eq!(
    answered(Some("\nThe parser is fixed; its tests pass.\n")),
    Some(Handoff {
      text: "The parser is fixed; its tests pass.".to_owned(),
      source: Source::Session,
    })
  );
}

#[test]
fn a_prompt_opened_on_a_handoff_keeps_the_item_s_title_first() {
  let opened = ahead("The parser is fixed.", "# An item\n\n## Instruction\n\nDo the work.");

  assert!(opened.starts_with("# An item\n\n## The last session's handoff\n"), "{opened}");
  assert!(
    opened.find("The parser is fixed.").unwrap() < opened.find("## Instruction").unwrap(),
    "{opened}"
  );
  assert!(opened.ends_with("## Instruction\n\nDo the work."), "{opened}");

  // A prompt that opens on anything but a title is opened on the handoff whole.
  let continued = ahead("The parser is fixed.", "This job already ran on deepseek.\n\n# An item");

  assert!(continued.starts_with("## The last session's handoff\n"), "{continued}");
  assert!(continued.ends_with("This job already ran on deepseek.\n\n# An item"), "{continued}");
}

#[tokio::test]
async fn a_session_that_cannot_be_asked_leaves_its_run_log_s_last_lines_oldest_first() {
  let scratch = Scratch::new().await;
  let mut log = RunLog::new(Target {
    agent: "helper".to_owned(),
    run: "job-7".to_owned(),
    chat_id: None,
    job_id: Some(7),
  });

  for said in ["Read docs/area.md.", "Edited src/area.rs.", "The area tests fail on a missing field."] {
    log.write(scratch.db(), Line::note(said.to_owned())).await;
  }

  let handoff = from_log(scratch.db(), 7, &JOB_HANDOFF.why.road_stopped).await;

  assert_eq!(handoff.source, Source::Log);
  assert!(handoff.text.contains(&JOB_HANDOFF.why.road_stopped), "{}", handoff.text);

  let read = handoff.text.find("Read docs/area.md.").expect("the first line");
  let edited = handoff.text.find("Edited src/area.rs.").expect("the second line");
  let failing = handoff.text.find("The area tests fail on a missing field.").expect("the last line");

  assert!(read < edited && edited < failing, "{}", handoff.text);
}

// The control front on the ground, on a loopback port of its own.
async fn front(ground: Arc<agency_server::places::Ground>) -> u16 {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let app = router(Front::new(TOKEN.to_owned(), ground).unwrap());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  port
}

async fn call(port: u16, method: reqwest::Method, path: &str, body: Option<Value>) -> (u16, Value) {
  let mut request = reqwest::Client::new()
    .request(method, format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(TOKEN);

  if let Some(body) = body {
    request = request.json(&body);
  }

  let answer = request.send().await.unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_job_s_answer_is_noted_as_its_handoff_and_the_item_s_next_job_opens_on_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (peers, vars) = crate::support::peers::stand().await;
  let ground = Arc::new(seeded_with(scratch.db(), home.path(), &vars).await);
  let port = front(ground.clone()).await;
  let place = Place::of(&ground.roots.data, "worker-01");
  let handoff = "Done: the parser reads the new field. Left: its tests in tests/area.rs, uncommitted.";

  peers.lock().unwrap().leases.insert(1);
  peers
    .lock()
    .unwrap()
    .items
    .insert(42, crate::support::peers::item(42, "open", json!({})));
  repository(scratch.db(), "helper", &home.path().join("helper").to_string_lossy(), &[]).await;
  std::fs::create_dir_all(home.path().join("helper")).unwrap();

  // A running job of item 42 whose turn goes on in its program, picked up as agency's start picks it up. The
  // program stands until the test ends, so the pickup waits on the turn's record however late the stop comes.
  let _program = program::running(&place);
  let now = now_ms();

  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at",
           "changed_at")
         VALUES (8, 42, 'harness', 'helper', 'running', $1, $1)"#,
      vec![now.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "run_name", "state", "started_at", "prompt")
         VALUES (8, 42, 1, 'harness', 8, 'helper', 'job-8', 'running', $1, '# item 42')"#,
      vec![now.into()],
    )
    .await;
  keep(
    scratch.db(),
    &InFlight {
      session: "worker-01@worker".into(),
      turn: 1,
      run: "job-8".into(),
      key: None,
      agent: "worker-01".into(),
      chat_id: None,
      ledger: None,
      job: Some(8),
      started_at: now - 1_000,
    },
  )
  .await;

  let append = |line: Value| {
    use std::io::Write as _;

    std::fs::create_dir_all(&place.path).unwrap();
    let mut file = std::fs::OpenOptions::new()
      .create(true)
      .append(true)
      .open(place.path.join("loop.jsonl"))
      .unwrap();

    writeln!(file, "{line}").unwrap();
  };

  append(
    json!({ "kind": "start", "id": "j", "name": "job-8", "title": "a turn", "at": 1, "typed": false }),
  );
  at_start(&ground).await.unwrap();

  // The program answers the stop words with its handoff once the stop reaches it, and ends.
  let db = scratch.db().clone();
  let program = tokio::spawn({
    let file = place.path.join("loop.jsonl");

    async move {
      for _ in 0..200 {
        if cancelled(&db, &["job-8"]).await.unwrap() {
          use std::io::Write as _;

          let mut file = std::fs::OpenOptions::new().append(true).open(&file).unwrap();

          writeln!(file, "{}", json!({ "kind": "answer", "id": "j", "text": handoff })).unwrap();
          writeln!(file, "{}", json!({ "kind": "end", "id": "j" })).unwrap();
          return true;
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
      }

      false
    }
  });
  let (status, stopped) = call(
    port,
    reqwest::Method::PATCH,
    "/jobs/8",
    Some(json!({ "state": "stopped", "report": "Stopped for the bench." })),
  )
  .await;

  assert!(program.await.unwrap(), "the stop reached the job's turn");
  assert_eq!((status, stopped["state"].as_str()), (200, Some("stopped")), "{stopped}");

  // Its answer stands on the item as a note of the handoff's own kind.
  let notes = peers.lock().unwrap().notes.get(&42).cloned().unwrap_or_default();

  assert_eq!(notes.len(), 1, "{notes:?}");
  assert_eq!(notes[0]["text"], handoff);
  assert_eq!(notes[0]["labels"], json!({ "kind": "handoff", "job": "8", "source": "session" }));

  // The item's next job, launched afresh, opens on it: the title first, the handoff ahead of the rest, and the
  // handoff no note of its own below.
  let (status, assignment) = call(
    port,
    reqwest::Method::POST,
    "/assignments",
    Some(json!({ "itemId": 42, "holderName": "harness", "assigneeName": "helper",
                 "item": { "title": "item 42", "brief": "Carry on.", "notes": [] } })),
  )
  .await;

  assert_eq!(status, 201, "{assignment}");

  let (status, created) = call(
    port,
    reqwest::Method::POST,
    "/jobs",
    Some(json!({
      "assignmentId": assignment["id"],
      "itemId": 42,
      "leaseId": 1,
      "holderName": "harness",
      "item": { "title": "item 42", "brief": "Carry on.", "notes": notes, "labels": { "agent": "worker-01" } },
    })),
  )
  .await;

  assert_eq!(status, 201, "{created}");

  let (_, prompt) =
    call(port, reqwest::Method::GET, &format!("/jobs/{}/prompt", created["id"]), None).await;
  let prompt = prompt["text"].as_str().unwrap_or_default();

  assert!(prompt.starts_with("# item 42\n\n## The last session's handoff\n"), "{prompt}");
  assert!(prompt.find(handoff).unwrap() < prompt.find("Carry on.").unwrap(), "{prompt}");
  assert_eq!(prompt.matches(handoff).count(), 1, "{prompt}");
  assert!(!prompt.contains("## Notes"), "{prompt}");

  let _ = call(
    port,
    reqwest::Method::PATCH,
    &format!("/jobs/{}", created["id"]),
    Some(json!({ "state": "stopped", "report": "The test is over." })),
  )
  .await;
}

// A job of the item in the base checkout `helper`, running on agent `worker-01` with the prompt given.
async fn job_on(scratch: &Scratch, home: &std::path::Path, id: i64, item: i64, prompt: &str) {
  let now = now_ms();

  repository(scratch.db(), "helper", &home.join("helper").to_string_lossy(), &[]).await;
  std::fs::create_dir_all(home.join("helper")).unwrap();
  // A git checkout on no item's branch, so the prompt carries no preamble bundle above the handoff.
  crate::support::git::git(&["-C", &crate::support::git::at(&home.join("helper")), "init", "--quiet"]);
  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at",
           "changed_at")
         VALUES ($1, $2, 'harness', 'helper', 'running', $3, $3)"#,
      vec![id.into(), item.into(), now.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "state", "started_at", "prompt", "item_agent")
         VALUES ($1, $2, 1, 'harness', $1, 'helper', 'running', $3, $4, 'worker-01')"#,
      vec![id.into(), item.into(), now.into(), prompt.to_owned().into()],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_job_whose_model_changed_opens_its_next_session_on_the_handoff_its_last_session_wrote() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (peers, vars) = crate::support::peers::stand().await;
  let ground = seeded_with(scratch.db(), home.path(), &vars).await;
  let handoff =
    "Done: stage one, the parser reads the new field. Found: the fixture lacks it. Left: stage two.";

  // The job's last turn answered on Claude Code's model; its agent's road now goes out on DeepSeek's.
  job_on(&scratch, home.path(), 9, 90, "# item 90\n\nDo the work.").await;
  scratch
    .run(
      r#"UPDATE "jobs" SET "state" = 'reviewing', "review_session" = $1 WHERE "id" = 9"#,
      vec![
        json!({ "provider": "claude-code", "model": "claude-sonnet-5", "session": "left-1" })
          .to_string()
          .into(),
      ],
    )
    .await;

  let sessions = StandIn::answering(&[(handoff, "left-1"), ("Stage two is done.", "next-1")]);
  let left = Resumed {
    provider: "claude-code".to_owned(),
    model: "claude-sonnet-5".to_owned(),
    session: "left-1".to_owned(),
    effort: None,
    prefix: None,
  };
  let ended = launched(
    &ground,
    Some(sessions.clone()),
    9,
    Some(ReviewTurn {
      words: "Do stage two now.".to_owned(),
      resume: Some(left),
    }),
  )
  .await
  .unwrap()
  .expect("the turn's end");

  assert_eq!(
    ended.2.as_deref().map(|report| report.starts_with("Stage two is done.")),
    Some(true),
    "{ended:?}"
  );

  // The session left is asked first, on its own road and resumed by its id; the new road's session opens on
  // what it wrote, ahead of the job's prompt and the review's words.
  let turns = sessions.turns();

  assert_eq!(turns.len(), 2, "{turns:?}");
  assert_eq!((turns[0].0.as_str(), turns[0].1.as_deref()), ("claude-code", Some("left-1")));
  assert_eq!(turns[0].2, JOB_HANDOFF.instruction);
  assert_ne!(turns[1].0, "claude-code", "the next turn goes out on the road the model changed to");
  assert_eq!(turns[1].1, None, "the next road opens a session of its own");
  assert!(turns[1].2.starts_with("# item 90\n\n## The last session's handoff\n"), "{}", turns[1].2);
  assert!(
    turns[1].2.find(handoff).unwrap() < turns[1].2.find("Do stage two now.").unwrap(),
    "{}",
    turns[1].2
  );

  // The handoff stands on the item, written by the session itself.
  let notes = peers.lock().unwrap().notes.get(&90).cloned().unwrap_or_default();

  assert_eq!(notes.len(), 1, "{notes:?}");
  assert_eq!(
    (notes[0]["text"].as_str(), &notes[0]["labels"]["source"]),
    (Some(handoff), &json!("session"))
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_job_s_relaunch_sends_its_first_turn_on_the_handoff_the_stopped_session_wrote() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (peers, vars) = crate::support::peers::stand().await;
  let ground = Arc::new(seeded_with(scratch.db(), home.path(), &vars).await);
  let port = front(ground.clone()).await;
  let handoff = "Done: the parser. Doing: its tests in tests/area.rs, uncommitted. Left: the docs.";

  peers.lock().unwrap().leases.insert(1);

  // The job that was stopped, its session's answer to the stop words standing on the item as its handoff.
  job_on(&scratch, home.path(), 11, 110, "# item 110\n\nDo the work.").await;
  scratch
    .run(
      r#"UPDATE "jobs" SET "state" = 'stopped', "ended_at" = $1 WHERE "id" = 11"#,
      vec![now_ms().into()],
    )
    .await;
  scratch
    .run(r#"UPDATE "assignments" SET "state" = 'released' WHERE "id" = 11"#, vec![])
    .await;
  peers
    .lock()
    .unwrap()
    .note(110, handoff, json!({ "kind": "handoff", "job": "11", "source": "session" }));

  // The item relaunched: the job's prompt is composed on the handoff.
  let (status, assignment) = call(
    port,
    reqwest::Method::POST,
    "/assignments",
    Some(json!({ "itemId": 110, "holderName": "harness", "assigneeName": "helper",
                 "item": { "title": "item 110", "brief": "Carry on.", "notes": [] } })),
  )
  .await;

  assert_eq!(status, 201, "{assignment}");

  let (status, created) = call(
    port,
    reqwest::Method::POST,
    "/jobs",
    Some(json!({ "assignmentId": assignment["id"], "itemId": 110, "leaseId": 1, "holderName": "harness",
                 "item": { "title": "item 110", "brief": "Carry on.", "notes": [], "labels": { "agent": "worker-01" } } })),
  )
  .await;

  assert_eq!(status, 201, "{created}");

  // Its first turn carried on the test's own backend: the new session's first words open on the handoff.
  let prompt: String = store::one(
    scratch.db(),
    r#"SELECT "prompt" FROM "jobs" WHERE "id" = $1"#,
    vec![created["id"].as_i64().unwrap().into()],
  )
  .await
  .unwrap()
  .unwrap()
  .try_get("", "prompt")
  .unwrap();

  job_on_prompt(&scratch, 12, 110, &prompt).await;

  let sessions = StandIn::answering(&[("Carried on.", "fresh-1")]);

  launched(&ground, Some(sessions.clone()), 12, None)
    .await
    .unwrap()
    .expect("the turn's end");

  let turns = sessions.turns();

  assert_eq!(turns.len(), 1, "{turns:?}");
  assert_eq!(turns[0].1, None, "the relaunch opens a session of its own");
  assert!(
    turns[0].2.starts_with("# item 110\n\n## The last session's handoff\n"),
    "{}",
    turns[0].2
  );
  assert!(
    turns[0].2.find(handoff).unwrap() < turns[0].2.find("Carry on.").unwrap(),
    "{}",
    turns[0].2
  );
}

// A second job of the same item and checkout, written with the prompt given.
async fn job_on_prompt(scratch: &Scratch, id: i64, item: i64, prompt: &str) {
  let now = now_ms();

  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at",
           "changed_at")
         VALUES ($1, $2, 'harness', 'helper', 'running', $3, $3)"#,
      vec![id.into(), item.into(), now.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "state", "started_at", "prompt", "item_agent")
         VALUES ($1, $2, 1, 'harness', $1, 'helper', 'starting', $3, $4, 'worker-01')"#,
      vec![id.into(), item.into(), now.into(), prompt.to_owned().into()],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_review_turn_whose_session_record_is_gone_opens_on_the_run_log_and_notes_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (peers, vars) = crate::support::peers::stand().await;
  let ground = seeded_with(scratch.db(), home.path(), &vars).await;
  let mut log = RunLog::new(Target {
    agent: "helper".to_owned(),
    run: "job-13".to_owned(),
    chat_id: None,
    job_id: Some(13),
  });

  log
    .write(scratch.db(), Line::note("Stage one: the parser reads the new field.".to_owned()))
    .await;

  // The job's last turn answered on the road its next turn takes, but its session's record is gone: the turn
  // answers on a session of its own.
  job_on(&scratch, home.path(), 13, 130, "# item 130\n\nDo the work.").await;

  let road =
    agency_server::road::for_job(&ground, Some("worker"), Some("helper"), None, &[], Some("harness"))
      .await
      .unwrap();
  let (taken, _) = road.going().expect("a road");
  let left = Resumed {
    provider: taken.provider.clone(),
    model: taken.model.clone(),
    session: "gone-1".to_owned(),
    effort: None,
    prefix: None,
  };
  let sessions = StandIn::answering(&[("Stage two is done.", "fresh-2")]);

  launched(
    &ground,
    Some(sessions.clone()),
    13,
    Some(ReviewTurn {
      words: "Do stage two now.".to_owned(),
      resume: Some(left),
    }),
  )
  .await
  .unwrap()
  .expect("the turn's end");

  // The turn asks to continue the session, and carries the run log as the handoff for where it cannot.
  let started = sessions.started.lock().unwrap().clone();
  let Scope::Apart(apart) = &started[0].scope else { panic!("an apart session") };

  assert_eq!(apart.resume.as_deref(), Some("gone-1"));
  assert!(
    apart
      .resume_else
      .as_deref()
      .unwrap()
      .starts_with("# item 130\n\n## The last session's handoff\n")
  );
  assert!(
    apart
      .resume_else
      .as_deref()
      .unwrap()
      .contains("Stage one: the parser reads the new field.")
  );

  // It answered on another session, so the run log's handoff is noted on the item.
  let notes = peers.lock().unwrap().notes.get(&130).cloned().unwrap_or_default();

  assert_eq!(notes.len(), 1, "{notes:?}");
  assert_eq!(notes[0]["labels"]["source"], "log");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_handed_over_as_its_job_ends_is_asked_for_its_handoff_first() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (peers, vars) = crate::support::peers::stand().await;
  let ground = seeded_with(scratch.db(), home.path(), &vars).await;
  let handoff = "Done: the whole item, committed as abc123. Nothing is left.";

  job_on(&scratch, home.path(), 14, 140, "# item 140\n\nDo the work.").await;
  scratch
    .run(
      r#"UPDATE "jobs" SET "state" = 'ended', "session_id" = 'worker-01@apart:5', "review_session" = $1
         WHERE "id" = 14"#,
      vec![
        json!({ "provider": "claude-code", "model": "claude-sonnet-5", "session": "kept-1" })
          .to_string()
          .into(),
      ],
    )
    .await;

  let sessions = StandIn::answering(&[(handoff, "kept-1")]);

  handed_over(&ground, sessions.as_ref(), "worker-01@apart:5").await;

  let turns = sessions.turns();

  assert_eq!(turns.len(), 1, "{turns:?}");
  assert_eq!((turns[0].0.as_str(), turns[0].1.as_deref()), ("claude-code", Some("kept-1")));
  assert_eq!(turns[0].2, JOB_HANDOFF.instruction);

  let notes = peers.lock().unwrap().notes.get(&140).cloned().unwrap_or_default();

  assert_eq!(notes.len(), 1, "{notes:?}");
  assert_eq!((notes[0]["text"].as_str(), &notes[0]["labels"]["job"]), (Some(handoff), &json!("14")));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_review_turn_whose_prefix_moved_mid_job_asks_its_session_for_the_handoff_before_a_fresh_one_opens()
 {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (peers, vars) = crate::support::peers::stand().await;
  let ground = seeded_with(scratch.db(), home.path(), &vars).await;
  let handoff = "Done: stage one. Found: the fixture lacks the field. Left: stage two, in tests/area.rs.";

  // The job's last turn answered on the road its next turn takes, but under a command whose prefix has moved
  // since: the role's prompt, its tools or its thinking changed mid-job.
  job_on(&scratch, home.path(), 15, 150, "# item 150\n\nDo the work.").await;

  let road =
    agency_server::road::for_job(&ground, Some("worker"), Some("helper"), None, &[], Some("harness"))
      .await
      .unwrap();
  let (taken, _) = road.going().expect("a road");
  let left = Resumed {
    provider: taken.provider.clone(),
    model: taken.model.clone(),
    session: "left-1".to_owned(),
    effort: None,
    prefix: Some("the prefix it was opened under".to_owned()),
  };
  let sessions = StandIn::answering(&[(handoff, "left-1"), ("Stage two is done.", "fresh-1")]);

  *sessions.prefix.lock().unwrap() = Some("the prefix every command stands under now".to_owned());

  launched(
    &ground,
    Some(sessions.clone()),
    15,
    Some(ReviewTurn {
      words: "Do stage two now.".to_owned(),
      resume: Some(left),
    }),
  )
  .await
  .unwrap()
  .expect("the turn's end");

  // The turn named its session and was put back before anything went in; the session it leaves is then asked for
  // its handoff, resumed by its id, and the turn goes out on a fresh session opening on that handoff.
  let turns = sessions.turns();

  assert_eq!(turns.len(), 3, "{turns:?}");
  assert_eq!(turns[0].1.as_deref(), Some("left-1"), "the turn first names the job's own session");
  assert_eq!(
    (turns[1].1.as_deref(), turns[1].2.as_str()),
    (Some("left-1"), JOB_HANDOFF.instruction.as_str())
  );
  assert_eq!(turns[2].1, None, "the turn then opens a fresh session");
  assert!(
    turns[2].2.starts_with("# item 150\n\n## The last session's handoff\n"),
    "{}",
    turns[2].2
  );
  assert!(
    turns[2].2.find(handoff).unwrap() < turns[2].2.find("Do stage two now.").unwrap(),
    "{}",
    turns[2].2
  );

  // The handoff stands on the item before the fresh turn went out, written by the session itself.
  let notes = peers.lock().unwrap().notes.get(&150).cloned().unwrap_or_default();

  assert_eq!(notes.len(), 1, "{notes:?}");
  assert_eq!(
    (notes[0]["text"].as_str(), &notes[0]["labels"]["source"]),
    (Some(handoff), &json!("session"))
  );

  // The session the fresh turn answered on is kept with the prefix it went out under.
  let row = store::one(scratch.db(), r#"SELECT "review_session" FROM "jobs" WHERE "id" = 15"#, vec![])
    .await
    .unwrap()
    .expect("the job's row");
  let kept: Value = serde_json::from_str(&row.try_get::<String>("", "review_session").unwrap()).unwrap();

  assert_eq!(kept["session"], "fresh-1");
  assert_eq!(kept["prefix"], "the prefix every command stands under now");
}
