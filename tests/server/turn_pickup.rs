//! A turn in flight across agency's own restart, picked up again (docs/sessions.md, 「A restart hands the
//! session over」): a job whose turn ended in its program's record after the process that sent it went away
//! ends with its report rather than stopped, a chat turn's end reaches its caller under its own id with its
//! run closed, a turn its program is still working on is waited on, and one no record names ends failed; a
//! job's turn adopted at start stands as seen, its slot kept while it goes and given back as it ends; a job's
//! turn on a thrown-away session is read from that session's place, which the sweep at start keeps while the
//! turn goes on and which is ended with the turn, while a job's turn no record names is reaped as before; a
//! turn on a Claude Code pane, which keeps no record of agency's, is read from the place's run, the hooks'
//! calls and the session's transcript, a job's and a role session's alike; and a job's turn sent on
//! a thrown-away session is kept in flight while it goes, whatever run the worker's own place holds, so the
//! start after its process went away adopts the job from the row its launch wrote; a job's turn in a pane
//! of its own, which no place stands for, keeps its row through every periodic pass while its round goes
//! on and is adopted at start through its execution's liveness row, ending with its round; and a job's turn
//! read in its hooks' place has the pane of its own it went out in closed once its end is handed back.

use std::collections::HashMap;
use std::sync::Arc;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::control::work::reap_stale;
use agency_server::herdr_backend::TurnEnded;
use agency_server::lines::place::Place;
use agency_server::now_ms;
use agency_server::order_run::dispatch;
use agency_server::places::Ground;
use agency_server::turn_pickup::*;
use harness_providers::Machine;
use serde_json::json;

use crate::support::grounds::{deepseek_balance, job_slot, seeded, slot_now};
use crate::support::scratch::Scratch;

fn ground(scratch: &Scratch, home: &std::path::Path) -> Arc<Ground> {
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), home.to_string_lossy().into_owned()),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
    ],
    home.join("data"),
  );

  Arc::new(Ground::new(&service, &machine, None))
}

// A line of a pane program's record, as the terminal client appends it.
fn append(place: &Place, line: serde_json::Value) {
  use std::io::Write as _;

  std::fs::create_dir_all(&place.path).unwrap();

  let mut file = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(place.path.join("loop.jsonl"))
    .unwrap();

  writeln!(file, "{line}").unwrap();
}

fn started(place: &Place, id: &str, name: &str) {
  append(
    place,
    json!({ "kind": "start", "id": id, "name": name, "title": "a turn", "at": 1, "typed": false }),
  );
}

fn answered(place: &Place, id: &str, text: &str) {
  append(
    place,
    json!({ "kind": "answer", "id": id, "text": text, "model": "[\"deepseek\",\"deepseek-flash\"]",
            "usage": { "inputTokens": 10, "outputTokens": 5, "cacheReadTokens": 0 } }),
  );
  append(place, json!({ "kind": "end", "id": id }));
}

fn in_flight(
  run: &str,
  agent: &str,
  turn: i64,
  chat_id: Option<i64>,
  ledger: Option<i64>,
  job: Option<i64>,
) -> InFlight {
  InFlight {
    session: format!("{agent}@worker"),
    turn,
    run: run.to_owned(),
    key: None,
    agent: agent.to_owned(),
    chat_id,
    ledger,
    job,
    started_at: now_ms() - 1_000,
  }
}

#[test]
fn a_kept_turn_stands_as_its_programs_record_says() {
  let data = tempfile::tempdir().unwrap();
  let place = Place::of(data.path(), "agent-a");
  let turn = in_flight("chat:5:1", "agent-a", 1, Some(5), None, None);

  assert_eq!(standing(&place, &turn), Standing::Lost, "no record names it");

  std::fs::create_dir_all(place.path.join("inbox")).unwrap();
  std::fs::write(
    place.path.join("inbox/000-t.json"),
    json!({ "id": "t", "name": "chat:5:1" }).to_string(),
  )
  .unwrap();
  assert_eq!(standing(&place, &turn), Standing::Going, "it waits in the inbox");

  std::fs::remove_file(place.path.join("inbox/000-t.json")).unwrap();
  started(&place, "t", "chat:5:1");
  assert_eq!(standing(&place, &turn), Standing::Going, "the program works on it");

  append(&place, json!({ "kind": "progress", "id": "t", "phase": "tool" }));
  answered(&place, "t", "the answer");

  match standing(&place, &turn) {
    Standing::Ended(Came::Answered(reply)) => {
      assert_eq!(reply.text, "the answer");
      assert_eq!(reply.mode, "deepseek");
    }
    other => panic!("{other:?}"),
  }

  // A later turn under another name does not hide this one's end.
  started(&place, "u", "chat:5:2");
  assert!(matches!(standing(&place, &turn), Standing::Ended(Came::Answered(_))));
}

#[test]
fn a_turn_put_in_after_others_under_the_same_name_stands_as_its_own_lines_say() {
  let data = tempfile::tempdir().unwrap();
  let place = Place::of(data.path(), "search-9-rounds");

  started(&place, "a", "research-9");
  answered(&place, "a", "the first round");
  assert!(matches!(
    standing_after(&place, "research-9", 0),
    Standing::Ended(Came::Answered(reply)) if reply.text == "the first round"
  ));
  assert_eq!(standing_after(&place, "research-9", 1), Standing::Lost, "the second never went in");

  std::fs::create_dir_all(place.path.join("inbox")).unwrap();
  std::fs::write(
    place.path.join("inbox/000-b.json"),
    json!({ "id": "b", "name": "research-9" }).to_string(),
  )
  .unwrap();
  assert_eq!(standing_after(&place, "research-9", 1), Standing::Going, "it waits in the inbox");
  assert_eq!(standing_after(&place, "research-9", 2), Standing::Lost, "no third went in");

  std::fs::remove_file(place.path.join("inbox/000-b.json")).unwrap();
  started(&place, "b", "research-9");
  assert_eq!(standing_after(&place, "research-9", 1), Standing::Going, "the program works on it");

  answered(&place, "b", "the final round");
  assert!(matches!(
    standing_after(&place, "research-9", 1),
    Standing::Ended(Came::Answered(reply)) if reply.text == "the final round"
  ));
}

#[tokio::test]
async fn a_job_whose_turn_ended_after_the_restart_ends_with_its_report_and_a_chat_turn_reaches_its_caller()
{
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let old = now_ms() - 20 * 60 * 1000;
  let worker = Place::of(&ground.roots.data, "worker-a");
  let chat = Place::of(&ground.roots.data, "agent-a");
  let mut ended = ground.events.subscribe::<TurnEnded>();

  crate::support::rows::repository(scratch.db(), "worker-a", "/checkouts/worker-a", &[]).await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "run_id", "run_name", "room_id", "state", "started_at", "prompt")
         VALUES (1, 0, 'harness', 60, 'worker-a', 600, 'job-6', 7, 'running', $1, 'six')"#,
      vec![old.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "runs" ("agent", "work_unit", "depth", "started_at", "id") VALUES
           ('worker-a', 'job-6', 0, $1, 600), ('agent-a', 'chat:5:1', 0, $1, 601)"#,
      vec![old.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at",
           "changed_at")
         VALUES (60, 1, 'harness', 'worker-a', 'running', $1, $1)"#,
      vec![now_ms().into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "agent_executions" ("id", "body") VALUES ('job-execution', $1)"#,
      vec![
        json!({
          "id": "job-execution", "agent": "worker-a", "line": "worker", "provider": "deepseek",
          "state": "running", "startedAt": old, "endedAt": null,
          "inboxTurn": { "id": "j", "name": "job-6" },
        })
        .to_string()
        .into(),
      ],
    )
    .await;

  let job_id = store::one(scratch.db(), r#"SELECT "id" FROM "jobs" WHERE "run_name" = 'job-6'"#, vec![])
    .await
    .unwrap()
    .unwrap()
    .try_get::<i64>("", "id")
    .unwrap();

  keep(scratch.db(), &in_flight("job-6", "worker-a", 41, Some(7), None, Some(job_id))).await;
  keep(
    scratch.db(),
    &InFlight {
      session: "agent-a".into(),
      ..in_flight("chat:5:1", "agent-a", 42, Some(5), Some(601), None)
    },
  )
  .await;
  started(&worker, "j", "job-6");
  answered(&worker, "j", "The job's report.");
  started(&chat, "c", "chat:5:1");
  answered(&chat, "c", "The room's answer.");

  // The pass that ends a job left running leaves one whose turn is kept to the pickup.
  assert_eq!(reap_stale(&ground).await.unwrap(), 0);
  assert_eq!(at_start(&ground).await.unwrap(), 2);

  let job = store::one(
    scratch.db(),
    r#"SELECT "state", "outcome", "report" FROM "jobs" WHERE "id" = $1"#,
    vec![job_id.into()],
  )
  .await
  .unwrap()
  .unwrap();

  // An answered turn puts the job under its review with its report (docs/job-review.md).
  assert_eq!(job.try_get::<String>("", "state").unwrap(), "reviewing");
  assert_eq!(job.try_get::<Option<String>>("", "outcome").unwrap(), None);
  assert_eq!(job.try_get::<String>("", "report").unwrap(), "The job's report.");

  let run = |id: i64| {
    let db = scratch.db();
    async move {
      store::one(db, r#"SELECT "finished_at", "outcome" FROM "runs" WHERE "id" = $1"#, vec![id.into()])
        .await
        .unwrap()
        .unwrap()
    }
  };

  assert_eq!(run(600).await.try_get::<String>("", "outcome").unwrap(), "answered");
  assert_eq!(run(601).await.try_get::<String>("", "outcome").unwrap(), "answered");

  let execution = store::one(
    scratch.db(),
    r#"SELECT "body" FROM "agent_executions" WHERE "id" = 'job-execution'"#,
    vec![],
  )
  .await
  .unwrap()
  .unwrap()
  .try_get::<String>("", "body")
  .unwrap();

  assert!(
    execution.contains("\"endedAt\":") && !execution.contains("\"endedAt\":null"),
    "{execution}"
  );

  let mut ends = vec![];

  while let Some(end) = ended.ready() {
    ends.push((end.session, end.end.turn, end.end.answer));
  }

  ends.sort();
  assert_eq!(
    ends,
    [
      ("agent-a".to_owned(), 42, Some("The room's answer.".to_owned())),
      ("worker-a@worker".to_owned(), 41, Some("The job's report.".to_owned())),
    ]
  );
  assert!(
    in_flight_turns(&scratch).await.is_empty(),
    "no turn stays kept once its end is handed back"
  );
}

// "a turn that waited for its node's link ends failed at the start, its reason saying so, and its mark goes with it"
#[tokio::test]
async fn a_turn_that_waited_for_its_node_s_link_ends_failed_at_the_start_saying_so_and_its_mark_goes_with_it()
 {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let mut ended = ground.events.subscribe::<TurnEnded>();

  keep(
    scratch.db(),
    &InFlight {
      session: "u7".into(),
      ..in_flight("chat:7:1", "u7", 71, Some(7), None, None)
    },
  )
  .await;
  mark_node_wait(scratch.db(), "chat:7:1", Some("pc-7")).await;

  assert_eq!(at_start(&ground).await.unwrap(), 1);

  let lost = ended.next().await.unwrap();

  assert_eq!((lost.session.as_str(), lost.end.turn), ("u7", 71));
  assert!(lost.end.answer.is_none(), "{lost:?}");
  assert!(
    lost.end.reason.as_deref().is_some_and(|why| why.contains("waited for pc-7's link")),
    "{lost:?}"
  );
  assert!(in_flight_turns(&scratch).await.is_empty());
  assert_eq!(node_waited(scratch.db(), "chat:7:1").await, None);
}

// "a stop from the shell confirms a turn gone only once agency no longer keeps it"
#[tokio::test]
async fn a_stop_from_the_shell_confirms_a_turn_gone_only_once_agency_no_longer_keeps_it() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let gone = |run| agency_server::verbs::run_stop::turn_gone(db, run, std::time::Duration::ZERO);

  assert!(gone("chat:7:2").await, "a turn not kept is gone at once");

  keep(db, &in_flight("chat:7:2", "u7", 72, Some(7), None, None)).await;

  let looking = {
    let db = db.clone();

    tokio::spawn(async move {
      agency_server::verbs::run_stop::turn_gone(&db, "chat:7:2", std::time::Duration::ZERO).await
    })
  };

  // The stop looks again and again while the turn is kept, and confirms nothing meanwhile.
  for _ in 0..3 {
    assert!(kept(db, "chat:7:2").await);
  }

  assert!(!looking.is_finished(), "a turn still kept is not confirmed gone");

  forget(db, "chat:7:2", "it ended").await;

  assert!(looking.await.unwrap());
}

async fn in_flight_turns(scratch: &Scratch) -> Vec<InFlight> {
  agency_server::turn_pickup::in_flight(scratch.db()).await.unwrap()
}

#[tokio::test]
async fn a_turn_still_working_is_waited_on_and_one_no_record_names_ends_failed() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let chat = Place::of(&ground.roots.data, "agent-b");
  let mut ended = ground.events.subscribe::<TurnEnded>();
  let mut program = std::process::Command::new("sleep").arg("30").spawn().unwrap();
  let identity = agent_host::identity::ProcessIdentity::of(program.id() as i32).unwrap();

  std::fs::create_dir_all(&chat.path).unwrap();
  std::fs::write(
    chat.path.join("running.json"),
    json!({ "identity": "one", "pid": identity.pid, "birth": identity.birth, "loop": identity,
            "starts": 1, "startedAt": now_ms(), "recent": [] })
    .to_string(),
  )
  .unwrap();
  keep(
    scratch.db(),
    &InFlight {
      session: "agent-b".into(),
      ..in_flight("chat:5:3", "agent-b", 43, Some(5), None, None)
    },
  )
  .await;
  keep(
    scratch.db(),
    &InFlight {
      session: "agent-c".into(),
      ..in_flight("chat:5:4", "agent-c", 44, Some(5), None, None)
    },
  )
  .await;
  started(&chat, "w", "chat:5:3");

  assert_eq!(at_start(&ground).await.unwrap(), 2);

  let lost = ended.next().await.unwrap();

  assert_eq!((lost.session.as_str(), lost.end.turn), ("agent-c", 44));
  assert!(lost.end.answer.is_none() && lost.end.reason.is_some(), "{lost:?}");

  answered(&chat, "w", "Worth the wait.");

  let waited = tokio::time::timeout(std::time::Duration::from_secs(10), ended.next())
    .await
    .unwrap()
    .unwrap();

  assert_eq!((waited.session.as_str(), waited.end.turn), ("agent-b", 43));
  assert_eq!(waited.end.answer.as_deref(), Some("Worth the wait."));

  let _ = program.kill();
  let _ = program.wait();
}

#[tokio::test]
async fn a_turn_whose_program_died_is_closed_and_its_room_freed() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let place = Place::of(&ground.roots.data, "agent-dead");
  let turn = in_flight("chat:7:1", "agent-dead", 7, Some(7), None, None);
  let mut ended = ground.events.subscribe::<TurnEnded>();

  std::fs::create_dir_all(&place.path).unwrap();
  std::fs::write(
    place.path.join("run.json"),
    json!({ "name": "chat:7:1", "title": "a turn", "person": false,
            "startedAt": now_ms() - 1_000, "endedAt": null })
    .to_string(),
  )
  .unwrap();
  let _ = store::run(
    scratch.db(),
    r#"INSERT INTO "agent_executions" ("id", "body") VALUES ('dead-execution', $1)"#,
    vec![
      json!({
        "id": "dead-execution", "agent": "agent-dead", "line": "chat", "provider": "deepseek",
        "startedAt": now_ms() - 1_000, "endedAt": null,
        "inboxTurn": { "name": "chat:7:1" },
        "resident": { "place": place.path.to_string_lossy(), "pid": 99999999, "birth": "gone" }
      })
      .to_string()
      .into(),
    ],
  )
  .await;
  keep(scratch.db(), &turn).await;

  assert_eq!(reap_dead(&ground).await, 1);
  assert!(in_flight_turns(&scratch).await.is_empty(), "the room's turn is freed");
  let run = agency_server::door::place_run::read_run(&place.path).unwrap();
  assert!(run.ended_at.is_some(), "the run carries its end");
  let end = ended.ready().expect("the turn's end is published");
  assert_eq!(end.session, "agent-dead@worker");
  assert!(matches!(end.end.outcome, agent_session::Outcome::Failed));
  assert!(end.end.reason.unwrap_or_default().contains("died silent"));
}

// A turn this process still carries is its own end's to close: its walk goes from one road to the next under
// the same run, and the place's run ends between them — a failed request ending the first road's turn — while
// the turn goes on (docs/sessions.md, 「A turn whose program dies」).
#[tokio::test]
async fn a_turn_this_process_carries_is_left_to_its_own_end_though_its_place_s_run_ended() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let place = Place::of(&ground.roots.data, "agent-walking");
  let turn = in_flight("chat:8:1", "agent-walking", 8, Some(8), None, None);
  let mut ended = ground.events.subscribe::<TurnEnded>();

  agency_server::door::place_run::begin_run(
    &place.path,
    "chat:8:1",
    "a turn",
    now_ms() - 1_000,
    Default::default(),
  )
  .unwrap();
  agency_server::door::place_run::record_call(
    &place.path,
    agency_server::lines::place::FAILURE_KIND,
    &json!({ "hook_event_name": "StopFailure", "error": "server_error",
             "last_assistant_message": "API Error: 503 Service is too busy" }),
    now_ms(),
    false,
    None,
  )
  .unwrap();
  assert!(agency_server::door::place_run::read_run(&place.path).unwrap().ended_at.is_some());
  keep(scratch.db(), &turn).await;

  let carrying = Carrying::of("chat:8:1");

  assert_eq!(reap_dead(&ground).await, 0);
  assert!(kept(scratch.db(), "chat:8:1").await, "the turn going on stays kept");
  assert!(ended.ready().is_none(), "nothing is handed back for it");

  // Once its end has gone, a run left ended in its place is no longer kept.
  drop(carrying);
  assert_eq!(reap_dead(&ground).await, 0);
  assert!(!kept(scratch.db(), "chat:8:1").await);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lease_of_a_job_s_turn_adopted_at_start_stands_as_seen_and_stays_the_job_s_when_the_turn_ends() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = Arc::new(seeded(scratch.db(), home.path()).await);
  let worker = Place::of(&ground.roots.data, "worker-a");
  let mut program = std::process::Command::new("sleep").arg("30").spawn().unwrap();
  let identity = agent_host::identity::ProcessIdentity::of(program.id() as i32).unwrap();

  deepseek_balance(&ground, 5.0).await;
  crate::support::rows::repository(scratch.db(), "worker-a", "/checkouts/worker-a", &[]).await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "run_name", "room_id", "state", "started_at", "prompt")
         VALUES (31, 1, 0, 'harness', 0, 'worker-a', 'job-31', 7, 'running', $1, 'adopted')"#,
      vec![(now_ms() - 60_000).into()],
    )
    .await;

  // The process before held the job's slot, bound to its run, and left its turn going in the program.
  let held = job_slot(&ground, 31, true).await;

  std::fs::create_dir_all(&worker.path).unwrap();
  std::fs::write(
    worker.path.join("running.json"),
    json!({ "identity": "one", "pid": identity.pid, "birth": identity.birth, "loop": identity,
            "starts": 1, "startedAt": now_ms(), "recent": [] })
    .to_string(),
  )
  .unwrap();
  keep(scratch.db(), &in_flight("job-31", "worker-a", 51, Some(7), None, Some(31))).await;
  started(&worker, "j", "job-31");

  assert_eq!(at_start(&ground).await.unwrap(), 1);

  // Adopted, its run stands as seen, and the job keeps its slot while the turn goes on.
  let leases = dispatch::bound_leases(&ground.db).await.unwrap();

  assert_eq!(leases, vec![(held.token.clone(), "job-31".to_owned(), true)]);
  assert_eq!(slot_now(&ground, &held.token).await.state, "active");

  // The turn ends in the program's record: its report goes to the job's review, and the job keeps its agent's
  // seat while it is reviewed, since its agent answers for it until it ends (docs/role-agents.md).
  answered(&worker, "j", "The adopted job's report.");

  let mut state = String::new();

  for _ in 0..80 {
    let job = store::one(scratch.db(), r#"SELECT "state" FROM "jobs" WHERE "id" = 31"#, vec![])
      .await
      .unwrap()
      .unwrap();

    state = job.try_get::<String>("", "state").unwrap();

    if state != "running" {
      break;
    }

    tokio::time::sleep(std::time::Duration::from_millis(125)).await;
  }

  assert_eq!(state, "reviewing");

  let given = slot_now(&ground, &held.token).await;

  assert_eq!((given.state.as_str(), given.ended_by.as_deref()), ("active", None), "{given:?}");

  let _ = program.kill();
  let _ = program.wait();
}

// A job's row a test stands, running since `started_at` under the run `job-<id>` in worker-a's checkout.
async fn running_job(scratch: &Scratch, id: i64, started_at: i64) {
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "run_name", "room_id", "state", "started_at", "prompt")
         VALUES ($1, 1, 0, 'harness', 0, 'worker-a', $2, 7, 'running', $3, 'apart')"#,
      vec![id.into(), format!("job-{id}").into(), started_at.into()],
    )
    .await;
}

async fn job_state(scratch: &Scratch, id: i64) -> (String, Option<String>) {
  let row =
    store::one(scratch.db(), r#"SELECT "state", "report" FROM "jobs" WHERE "id" = $1"#, vec![id.into()])
      .await
      .unwrap()
      .unwrap();

  (row.try_get("", "state").unwrap(), row.try_get("", "report").unwrap())
}

// A thrown-away session's place as the process before left it, marked as the session's own.
fn apart_place(ground: &Ground, session: &str) -> (String, Place) {
  let id = agent_session::SessionId(session.to_owned());
  let name = agency_server::apart_places::place_name(&id);

  agency_server::apart_places::mark(ground, &name, &id, None).unwrap();

  (name.clone(), Place::of(&ground.roots.data, &name))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_s_turn_on_a_thrown_away_session_is_adopted_from_its_place_and_its_place_ends_with_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = Arc::new(seeded(scratch.db(), home.path()).await);
  let data = ground.roots.data.clone();
  let mut program = std::process::Command::new("sleep").arg("30").spawn().unwrap();
  let identity = agent_host::identity::ProcessIdentity::of(program.id() as i32).unwrap();

  deepseek_balance(&ground, 5.0).await;
  crate::support::rows::repository(scratch.db(), "worker-a", "/checkouts/worker-a", &[]).await;
  running_job(&scratch, 41, now_ms() - 20 * 60 * 1000).await;

  // The process before held the job's slot, bound to its run, and left its turn going in the program of
  // the session's own place; another session's place it left carries no turn.
  let held = job_slot(&ground, 41, true).await;
  let (adopted, place) = apart_place(&ground, "worker-a@run:13");
  let (idle, _) = apart_place(&ground, "worker-a@run:12");

  std::fs::write(
    place.path.join("running.json"),
    json!({ "identity": "one", "pid": identity.pid, "birth": identity.birth, "loop": identity,
            "starts": 1, "startedAt": now_ms(), "recent": [] })
    .to_string(),
  )
  .unwrap();
  keep(
    scratch.db(),
    &InFlight {
      session: "worker-a@run:13".into(),
      ..in_flight("job-41", "worker-a", 61, Some(7), None, Some(41))
    },
  )
  .await;
  started(&place, "j", "job-41");

  assert_eq!(place_name_of(&in_flight_turns(&scratch).await[0]), adopted);
  assert_eq!(at_start(&ground).await.unwrap(), 1);

  // The sweep at start ends the place carrying no turn and keeps the adopted one.
  assert_eq!(agency_server::apart_places::sweep(&ground).await, 1);
  assert!(!agency_server::apart_places::is_apart_place(&data, &idle), "the idle place is ended");
  assert!(
    agency_server::apart_places::is_apart_place(&data, &adopted),
    "the adopted place is kept"
  );

  // The pass ending a job left running leaves the adopted one: it stays running with its slot, seen.
  assert_eq!(reap_stale(&ground).await.unwrap(), 0);
  assert_eq!(job_state(&scratch, 41).await.0, "running");
  assert_eq!(
    dispatch::bound_leases(&ground.db).await.unwrap(),
    vec![(held.token.clone(), "job-41".to_owned(), true)]
  );
  assert_eq!(slot_now(&ground, &held.token).await.state, "active");

  // The turn ends in its place's record: the job ends through its own end path, its slot is given back
  // and the session's place is ended with it.
  answered(&place, "j", "The thrown-away session's report.");

  for _ in 0..80 {
    if !agency_server::apart_places::is_apart_place(&data, &adopted) {
      break;
    }

    tokio::time::sleep(std::time::Duration::from_millis(125)).await;
  }

  assert!(
    !agency_server::apart_places::is_apart_place(&data, &adopted),
    "the place ends with its turn"
  );
  assert_eq!(
    job_state(&scratch, 41).await,
    ("reviewing".to_owned(), Some("The thrown-away session's report.".to_owned()))
  );

  // Under review the job keeps its agent's seat.
  let given = slot_now(&ground, &held.token).await;

  assert_eq!((given.state.as_str(), given.ended_by.as_deref()), ("active", None), "{given:?}");
  assert!(in_flight_turns(&scratch).await.is_empty());

  let _ = program.kill();
  let _ = program.wait();
}

#[tokio::test]
async fn a_job_s_turn_on_a_thrown_away_session_no_record_names_is_reaped_and_its_place_ended() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = Arc::new(seeded(scratch.db(), home.path()).await);
  let data = ground.roots.data.clone();

  crate::support::rows::repository(scratch.db(), "worker-a", "/checkouts/worker-a", &[]).await;
  running_job(&scratch, 42, now_ms() - 20 * 60 * 1000).await;

  let (name, _) = apart_place(&ground, "worker-a@run:14");

  keep(
    scratch.db(),
    &InFlight {
      session: "worker-a@run:14".into(),
      ..in_flight("job-42", "worker-a", 62, Some(7), None, Some(42))
    },
  )
  .await;

  assert_eq!(at_start(&ground).await.unwrap(), 1);
  assert!(in_flight_turns(&scratch).await.is_empty(), "the lost job's turn is left to the reap");
  assert_eq!(agency_server::apart_places::sweep(&ground).await, 1);
  assert!(!agency_server::apart_places::is_apart_place(&data, &name), "its place is ended");
  assert_eq!(reap_stale(&ground).await.unwrap(), 1);

  let (state, report) = job_state(&scratch, 42).await;

  assert_eq!(state, "stopped");
  assert!(report.unwrap_or_default().starts_with(agency_server::control::work::REAPED));
}

// A moment as a Claude Code transcript stamps its lines.
fn stamp(at: i64) -> String {
  chrono::DateTime::from_timestamp_millis(at).unwrap().to_rfc3339()
}

// Lines appended to a Claude Code session's transcript.
fn transcribe(transcript: &std::path::Path, lines: &[serde_json::Value]) {
  use std::io::Write as _;

  let mut file = std::fs::OpenOptions::new().create(true).append(true).open(transcript).unwrap();

  for line in lines {
    writeln!(file, "{line}").unwrap();
  }
}

// A Claude Code pane's place as the process before left it with a turn going: the place's run naming the
// turn, the program standing in it, the hooks' start and submit, and the session's transcript holding the
// prompt. Answers the transcript's path and the hooks' payload.
fn claude_code_turn(
  place: &Place,
  run: &str,
  began: i64,
  program: u32,
) -> (std::path::PathBuf, serde_json::Value) {
  use agency_server::door::place_run::{begin_run, record_call};

  let identity = agent_host::identity::ProcessIdentity::of(program as i32).unwrap();
  let transcript = place.path.join("session-s.jsonl");
  let payload = json!({ "session_id": "s", "transcript_path": transcript });

  begin_run(&place.path, run, "a turn", began, Default::default()).unwrap();
  std::fs::write(
    place.path.join("running.json"),
    json!({ "identity": "one", "pid": identity.pid, "birth": identity.birth, "loop": identity,
            "starts": 1, "startedAt": began, "recent": [], "paneId": "w1:p2" })
    .to_string(),
  )
  .unwrap();
  record_call(&place.path, "start", &payload, began + 1, false, None).unwrap();
  record_call(&place.path, "submit", &payload, began + 2, false, None).unwrap();
  transcribe(
    &transcript,
    &[json!({ "type": "user", "timestamp": stamp(began + 3), "message": { "content": "the work" } })],
  );

  (transcript, payload)
}

// The turn's answer written to the transcript and its stop recorded by the hooks, which closes the run.
fn claude_code_answers(
  place: &Place,
  transcript: &std::path::Path,
  payload: &serde_json::Value,
  text: &str,
) {
  let at = now_ms();

  transcribe(
    transcript,
    &[
      json!({ "type": "assistant", "timestamp": stamp(at),
              "message": { "id": "m1", "model": "deepseek-flash",
                           "content": [{ "type": "text", "text": text }] } }),
      json!({ "type": "system", "subtype": "turn_duration", "timestamp": stamp(at + 1) }),
    ],
  );
  agency_server::door::place_run::record_call(&place.path, "stop", payload, at + 2, false, None).unwrap();
}

#[test]
fn a_claude_code_pane_s_turn_stands_as_its_place_s_run_and_hooks_say() {
  let data = tempfile::tempdir().unwrap();
  let place = Place::of(data.path(), "apart-worker-a-run-20");
  let turn = InFlight {
    session: "worker-a@run:20".into(),
    ..in_flight("job-20", "worker-a", 1, None, None, Some(20))
  };
  let began = now_ms() - 5_000;
  let transcript = place.path.join("session-s.jsonl");
  let payload = json!({ "session_id": "s", "transcript_path": transcript });

  std::fs::create_dir_all(&place.path).unwrap();
  assert_eq!(standing(&place, &turn), Standing::Lost, "no run names it");

  agency_server::door::place_run::begin_run(&place.path, "job-20", "a turn", began, Default::default())
    .unwrap();
  assert_eq!(standing(&place, &turn), Standing::Lost, "the program never took it");

  agency_server::door::place_run::record_call(&place.path, "submit", &payload, began + 1, false, None)
    .unwrap();
  assert_eq!(standing(&place, &turn), Standing::Going, "taken and not closed");

  claude_code_answers(&place, &transcript, &payload, "The report.");

  match standing(&place, &turn) {
    Standing::Ended(Came::Answered(reply)) => {
      assert_eq!(reply.text, "The report.");
      assert_eq!(reply.session_id.as_deref(), Some("s"));
    }
    other => panic!("{other:?}"),
  }

  let other = InFlight {
    run: "job-21".into(),
    ..turn.clone()
  };

  assert_eq!(standing(&place, &other), Standing::Lost, "the place's run is another turn's");
}

#[test]
fn a_supervisor_s_turn_on_a_node_is_read_from_the_transcript_the_link_brought_back() {
  let data = tempfile::tempdir().unwrap();
  let place = Place::of(data.path(), "a-supervisor");
  let turn = InFlight {
    session: "a-supervisor@-5".into(),
    ..in_flight("chat:-5:1", "a-supervisor", 1, Some(-5), None, None)
  };
  let began = now_ms() - 5_000;
  let config = data.path().join("env").join("a-login");
  let folder = data.path().join("supervisor-homes").join("a-supervisor");
  // The hooks run here through the home bridge with the program's own payload, which names the node's path.
  let on_node = "/node-root/data/env/a-login/projects/-node-root-folder/s.jsonl";
  let payload = json!({ "session_id": "s", "transcript_path": on_node });
  let mirrored =
    harness_providers::claude_code::ClaudeCode::records_under(&config, &folder, "s").swap_remove(0);

  std::fs::create_dir_all(mirrored.parent().unwrap()).unwrap();
  std::fs::create_dir_all(&place.path).unwrap();
  agency_server::watches::links::mark_place_on(&place.path, Some("pc-1"), &folder.to_string_lossy())
    .unwrap();
  std::fs::write(
    place.path.join("program.json"),
    json!({ "identity": "one", "restart": null, "command": {
      "args": [], "cwd": folder, "env": { "CLAUDE_CONFIG_DIR": config } } })
    .to_string(),
  )
  .unwrap();
  agency_server::door::place_run::begin_run(&place.path, "chat:-5:1", "a turn", began, Default::default())
    .unwrap();
  agency_server::door::place_run::record_call(&place.path, "submit", &payload, began + 1, false, None)
    .unwrap();
  assert_eq!(
    transcript_here(&place, on_node, "s"),
    mirrored,
    "a node's path is read where the link lands it"
  );

  claude_code_answers(&place, &mirrored, &payload, "The room's answer.");

  match standing(&place, &turn) {
    Standing::Ended(Came::Answered(reply)) => assert_eq!(reply.text, "The room's answer."),
    other => panic!("the answer the program wrote on the node is handed back: {other:?}"),
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_s_turn_on_a_claude_code_pane_is_adopted_across_the_start_and_ends_through_the_job_s_end() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = Arc::new(seeded(scratch.db(), home.path()).await);
  let data = ground.roots.data.clone();
  let mut program = std::process::Command::new("sleep").arg("30").spawn().unwrap();

  deepseek_balance(&ground, 5.0).await;
  crate::support::rows::repository(scratch.db(), "worker-a", "/checkouts/worker-a", &[]).await;
  running_job(&scratch, 43, now_ms() - 20 * 60 * 1000).await;

  let held = job_slot(&ground, 43, true).await;
  let (name, place) = apart_place(&ground, "worker-a@run:16");
  let (transcript, payload) = claude_code_turn(&place, "job-43", now_ms() - 60_000, program.id());

  // The door's row of the turn, named by its liveness row, which the last process left open.
  scratch
    .run(
      r#"INSERT INTO "agent_executions" ("id", "body") VALUES ('door-43', $1)"#,
      vec![
        json!({ "id": "door-43", "agent": name, "line": "worker", "provider": "deepseek",
                "state": "running", "startedAt": now_ms() - 60_000, "endedAt": null, "paneId": "w1:p2",
                "resident": { "place": place.path, "pid": program.id() } })
        .to_string()
        .into(),
      ],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "turn_liveness" ("execution", "run", "line", "session", "phase", "opened_at",
           "written_at", "ending", "ended_at")
         VALUES ('door-43', 'job-43', 'worker', 's', 'working', $1, $1, 'restarted', $1)"#,
      vec![now_ms().into()],
    )
    .await;
  keep(
    scratch.db(),
    &InFlight {
      session: "worker-a@run:16".into(),
      ..in_flight("job-43", "worker-a", 63, Some(7), None, Some(43))
    },
  )
  .await;

  assert_eq!(at_start(&ground).await.unwrap(), 1);
  assert_eq!(agency_server::apart_places::sweep(&ground).await, 0, "the adopted place is kept");
  assert!(agency_server::apart_places::is_apart_place(&data, &name));
  assert_eq!(agency_server::queue::reconcile::pass(scratch.db(), &data).await.unwrap(), 0);
  assert_eq!(reap_stale(&ground).await.unwrap(), 0);
  assert_eq!(job_state(&scratch, 43).await.0, "running", "the job goes on across the start");
  assert_eq!(slot_now(&ground, &held.token).await.state, "active");

  // A sub-turn of the round ends, its stop kept from closing the run since the round still owes a turn, a
  // word is submitted after it, and every pass runs again and again: the turn stays kept and the job running.
  let mid = now_ms();

  agency_server::door::place_run::record_call(&place.path, "stop", &payload, mid, true, None).unwrap();
  agency_server::door::place_run::record_call(&place.path, "submit", &payload, mid + 1, false, None)
    .unwrap();

  for _ in 0..3 {
    every_pass(&ground).await;
    assert!(kept_now(&scratch, "job-43").await, "no pass lets go of a turn whose round goes on");
  }

  assert_eq!(job_state(&scratch, 43).await.0, "running");
  assert!(agency_server::apart_places::is_apart_place(&data, &name));

  // The program answers and its hooks close the run: the job ends with the answer as its report.
  claude_code_answers(&place, &transcript, &payload, "The Claude Code job's report.");

  for _ in 0..80 {
    if !agency_server::apart_places::is_apart_place(&data, &name) {
      break;
    }

    tokio::time::sleep(std::time::Duration::from_millis(125)).await;
  }

  assert!(
    !agency_server::apart_places::is_apart_place(&data, &name),
    "the place ends with its turn"
  );
  assert_eq!(
    job_state(&scratch, 43).await,
    ("reviewing".to_owned(), Some("The Claude Code job's report.".to_owned()))
  );
  assert_eq!(
    slot_now(&ground, &held.token).await.state,
    "active",
    "the job keeps its agent's seat under review"
  );

  let door =
    store::one(scratch.db(), r#"SELECT "body" FROM "agent_executions" WHERE "id" = 'door-43'"#, vec![])
      .await
      .unwrap()
      .unwrap()
      .try_get::<String>("", "body")
      .unwrap();

  assert!(!door.contains("\"endedAt\":null"), "the door's row is closed with the turn: {door}");

  let _ = program.kill();
  let _ = program.wait();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_role_session_s_turn_on_a_claude_code_pane_is_adopted_and_its_end_reaches_its_session() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = Arc::new(seeded(scratch.db(), home.path()).await);
  let data = ground.roots.data.clone();
  let mut program = std::process::Command::new("sleep").arg("30").spawn().unwrap();
  let mut ended = ground.events.subscribe::<TurnEnded>();
  let (name, place) = apart_place(&ground, "role-a@run:1");
  let (transcript, payload) = claude_code_turn(&place, "role:9", now_ms() - 60_000, program.id());

  keep(
    scratch.db(),
    &InFlight {
      session: "role-a@run:1".into(),
      ..in_flight("role:9", "role-a", 71, None, None, None)
    },
  )
  .await;

  assert_eq!(at_start(&ground).await.unwrap(), 1);
  assert_eq!(agency_server::apart_places::sweep(&ground).await, 0);
  assert!(ended.ready().is_none(), "nothing ends while the turn goes on");

  claude_code_answers(&place, &transcript, &payload, "Done.");

  let end = tokio::time::timeout(std::time::Duration::from_secs(10), ended.next())
    .await
    .unwrap()
    .unwrap();

  assert_eq!((end.session.as_str(), end.end.turn), ("role-a@run:1", 71));
  assert_eq!(end.end.answer.as_deref(), Some("Done."));

  for _ in 0..80 {
    if !agency_server::apart_places::is_apart_place(&data, &name) {
      break;
    }

    tokio::time::sleep(std::time::Duration::from_millis(125)).await;
  }

  assert!(
    !agency_server::apart_places::is_apart_place(&data, &name),
    "the place ends with its turn"
  );

  let _ = program.kill();
  let _ = program.wait();
}

// A job's turn's work parts as the job line names them, under the job's run name.
fn job_work(cwd: &str, run: &str) -> agent_session::WorkParts {
  agent_session::WorkParts {
    provider: "claude-code".into(),
    model: "claude-sonnet-5".into(),
    mapping: agent_session::Mapping {
      model: "claude-sonnet-5".into(),
      effort: None,
      tier: None,
      answer_tokens: None,
    },
    resume: agent_session::Resume::Fresh,
    cwd: cwd.into(),
    name: run.into(),
    title: "a job".into(),
    chat: 0,
    unit: None,
    user: None,
    work_key: "worker".into(),
    hears_words: false,
    variables: Default::default(),
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_s_turn_sent_is_kept_in_flight_while_it_goes_and_the_pickup_adopts_the_row_its_launch_wrote()
{
  use agency_server::herdr_backend::HerdrSessions;
  use agency_server::places::Places;
  use agent_session::{Apart, JobMark, Scope, SessionId, Sessions, Start, Turn, TurnContext};

  use crate::integration::common;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let line = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&line, 5.0).await;
  crate::support::rows::repository(scratch.db(), "worker", "/checkouts/worker", &[]).await;

  let config = home.path().join("config");

  std::fs::create_dir_all(config.join("herdr")).unwrap();

  let herdr = common::herdr::HerdrModel::stand(config.join("herdr").join("herdr.sock"));
  let at = |path: &std::path::Path| path.to_string_lossy().into_owned();
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), at(home.path())),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
      ("XDG_CONFIG_HOME".to_owned(), at(&config)),
      ("BMO_DEPLOY_DIR".to_owned(), env!("CARGO_MANIFEST_DIR").to_owned()),
    ],
    home.path().join("data"),
  );
  let service = common::service(&scratch, &[("AGENCY_SOURCE_DIR", at(&home.path().join("source")))]);
  let places = Places::new(
    Ground::new(&service, &machine, Some(herdr::Herdr::there(herdr.socket.clone()))),
    Arc::default(),
  );
  let ground = places.ground.clone();
  let sessions = Arc::new(HerdrSessions::new(places));

  // The worker's own place holds the run of an earlier job, long ended, which says nothing of a turn on a
  // thrown-away session.
  agency_server::door::place_run::begin_run(
    &Place::of(&ground.roots.data, "worker").path,
    "job-70",
    "an earlier job",
    now_ms() - 3_600_000,
    Default::default(),
  )
  .unwrap();
  agency_server::door::place_run::end_run(
    &Place::of(&ground.roots.data, "worker").path,
    now_ms() - 3_000_000,
  )
  .unwrap();

  // A job's turn as the job line sends it, on a thrown-away session of its role, on the provider named.
  let turn_of = async |job: i64, provider: &str| {
    let started = sessions
      .start(Start {
        agent: "worker".into(),
        scope: Scope::Apart(Apart {
          provider: provider.into(),
          model: "claude-sonnet-5".into(),
          cwd: at(home.path()),
          role: Some("worker".into()),
          ..Apart::default()
        }),
      })
      .await
      .unwrap();
    let mut work = job_work(&at(home.path()), &format!("job-{job}"));

    work.provider = provider.into();

    let turn = Turn {
      session: started.id.clone(),
      prompt: "work".into(),
      context: TurnContext {
        job: Some(JobMark { job, item: Some(1) }),
        work: Some(work),
        ..TurnContext::default()
      },
    };

    (started.id, turn)
  };

  // A job's turn that ends: kept while it went, as its start's event says, and no longer kept once ended.
  let mut begun = ground.events.subscribe::<agency_server::herdr_backend::TurnStarted>();
  let (ended_session, turn) = turn_of(79, "codex").await;
  let ended = tokio::time::timeout(std::time::Duration::from_secs(20), sessions.send_turn(turn)).await;

  assert!(ended.is_ok(), "the turn on a road that answers nothing ends");
  sessions.end(&ended_session).await.unwrap();
  assert_eq!(begun.ready().map(|started| started.run).as_deref(), Some("job-79"), "it went out");
  assert!(!kept_now(&scratch, "job-79").await, "a turn that ended is no longer kept");

  // A job's turn that goes on: its row stands under its run with the job, the session and no room, and the
  // queue's pass leaves it there although the worker's own place holds another run.
  running_job(&scratch, 78, now_ms() - 20 * 60 * 1000).await;

  let held = job_slot(&ground, 78, true).await;
  let (session, turn) = turn_of(78, "claude-code").await;
  let sending = sessions.clone();
  let going = tokio::spawn(async move { sending.send_turn(turn).await });

  // The session carries the turn a step before the store keeps it, so the wait is for the row.
  for _ in 0..600 {
    if kept_now(&scratch, "job-78").await || going.is_finished() {
      break;
    }

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
  }

  assert!(!going.is_finished(), "the job's turn goes on");

  let kept = in_flight_turns(&scratch).await;
  let row = kept.iter().find(|turn| turn.run == "job-78").expect("the job's turn is kept in flight");

  assert_eq!((row.job, row.chat_id, row.session.as_str()), (Some(78), None, session.0.as_str()));
  assert_eq!(row.agent, "worker");
  assert_eq!(reap_dead(&ground).await, 0);
  assert!(kept_now(&scratch, "job-78").await, "the queue's pass leaves a running job's turn kept");

  // The process carrying it goes away under it, leaving the row its launch wrote; the next start adopts the
  // job from that row, its session's place standing as a Claude Code pane's with the turn going.
  going.abort();
  let _ = going.await;

  let left = in_flight_turns(&scratch).await;
  let row = left
    .iter()
    .find(|turn| turn.run == "job-78")
    .expect("the launch's row outlives its process");
  let mut program = std::process::Command::new("sleep").arg("30").spawn().unwrap();
  let name = place_name_of(row);

  agency_server::apart_places::mark(&ground, &name, &SessionId(row.session.clone()), None).unwrap();

  let place = Place::of(&ground.roots.data, &name);
  let (transcript, payload) = claude_code_turn(&place, "job-78", now_ms() - 60_000, program.id());

  assert_eq!(at_start(&ground).await.unwrap(), 1);
  assert_eq!(agency_server::apart_places::sweep(&ground).await, 0, "the adopted place is kept");
  assert!(agency_server::apart_places::is_apart_place(&ground.roots.data, &name));
  assert_eq!(reap_stale(&ground).await.unwrap(), 0);
  assert_eq!(job_state(&scratch, 78).await.0, "running", "the job goes on across the start");
  assert_eq!(slot_now(&ground, &held.token).await.state, "active");

  claude_code_answers(&place, &transcript, &payload, "The relaunched job's report.");

  for _ in 0..80 {
    if job_state(&scratch, 78).await.0 != "running" && !kept_now(&scratch, "job-78").await {
      break;
    }

    tokio::time::sleep(std::time::Duration::from_millis(125)).await;
  }

  assert_eq!(
    job_state(&scratch, 78).await,
    ("reviewing".to_owned(), Some("The relaunched job's report.".to_owned()))
  );
  assert!(!kept_now(&scratch, "job-78").await);

  let _ = program.kill();
  let _ = program.wait();
}

async fn kept_now(scratch: &Scratch, run: &str) -> bool {
  agency_server::turn_pickup::kept(scratch.db(), run).await
}

// How many lines of a run's log hold exactly this text.
async fn logged_count(scratch: &Scratch, run: &str, text: &str) -> i64 {
  scratch
    .count(
      r#"SELECT COUNT(*) AS "n" FROM "agent_log" WHERE "run" = $1 AND "text" = $2"#,
      vec![run.into(), text.into()],
    )
    .await
}

async fn logged(scratch: &Scratch, run: &str, text: &str) -> bool {
  logged_count(scratch, run, text).await > 0
}

// Every pass that runs on a clock over what turns leave, run once: the executions' reconciliation, the
// reap of dead turns, the reap of jobs left running, the run sweep, the jobs' leases and the slots' line.
async fn every_pass(ground: &Ground) {
  let running = agency_server::queue::running::Running::new(ground.db.clone(), &ground.machine(), None);

  agency_server::queue::reconcile::pass(&ground.db, &ground.roots.data).await.unwrap();
  reap_dead(ground).await;
  reap_stale(ground).await.unwrap();
  agency_server::run_sweep::pass(ground).await.unwrap();
  agency_server::job_leases::pass(&ground.db, ground.vars()).await.unwrap();
  dispatch::reclaim_bound(&ground.db, &ground.events, &running).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_s_turn_in_a_pane_of_its_own_stays_kept_through_every_pass_and_is_adopted_through_its_liveness()
 {
  use agency_server::herdr_backend::HerdrSessions;
  use agency_server::places::Places;
  use agent_session::{Apart, JobMark, Scope, Sessions, Start, Turn, TurnContext};

  use crate::integration::common;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let line = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&line, 5.0).await;
  crate::support::rows::repository(scratch.db(), "worker", "/checkouts/worker", &[]).await;

  let config = home.path().join("config");

  std::fs::create_dir_all(config.join("herdr")).unwrap();

  let herdr = common::herdr::HerdrModel::stand(config.join("herdr").join("herdr.sock"));
  let at = |path: &std::path::Path| path.to_string_lossy().into_owned();
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), at(home.path())),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
      ("XDG_CONFIG_HOME".to_owned(), at(&config)),
      ("BMO_DEPLOY_DIR".to_owned(), env!("CARGO_MANIFEST_DIR").to_owned()),
    ],
    home.path().join("data"),
  );
  let service = common::service(&scratch, &[("AGENCY_SOURCE_DIR", at(&home.path().join("source")))]);
  let places = Places::new(
    Ground::new(&service, &machine, Some(herdr::Herdr::there(herdr.socket.clone()))),
    Arc::default(),
  );
  let ground = places.ground.clone();
  let sessions = Arc::new(HerdrSessions::new(places.clone()));

  // A job's turn as the job line sends it on Claude Code's own provider: a pane of its own, no place.
  running_job(&scratch, 80, now_ms() - 20 * 60 * 1000).await;

  let held = job_slot(&ground, 80, true).await;
  let started = sessions
    .start(Start {
      agent: "worker".into(),
      scope: Scope::Apart(Apart {
        provider: "claude-code".into(),
        model: "claude-sonnet-5".into(),
        cwd: at(home.path()),
        role: Some("worker".into()),
        ..Apart::default()
      }),
    })
    .await
    .unwrap();
  let turn = Turn {
    session: started.id.clone(),
    prompt: "work".into(),
    context: TurnContext {
      job: Some(JobMark {
        job: 80,
        item: Some(1),
      }),
      work: Some(job_work(&at(home.path()), "job-80")),
      ..TurnContext::default()
    },
  };
  let sending = sessions.clone();
  let going = tokio::spawn(async move { sending.send_turn(turn).await });

  // The session carries the turn a step before the store keeps it, so the wait is for the row.
  for _ in 0..600 {
    if kept_now(&scratch, "job-80").await || going.is_finished() {
      break;
    }

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
  }

  assert!(!going.is_finished(), "the job's turn goes on");
  assert!(kept_now(&scratch, "job-80").await, "the launch keeps the job's turn in flight");

  // The pane as the door leaves it: the program in a pane of its own, the execution the run opened and its
  // liveness row under the run's name, and the session's transcript with a first turn ended and a word
  // still queued, so the round goes on.
  let mut program = std::process::Command::new("sleep").arg("30").spawn().unwrap();
  let identity = agent_host::identity::ProcessIdentity::of(program.id() as i32).unwrap();
  let (_, pane) = herdr.workspace("a job", serde_json::Value::Null, Some(&["claude"]));
  let began = now_ms() - 60_000;
  let transcript = home.path().join("session-80.jsonl");

  scratch
    .run(
      r#"INSERT INTO "agent_executions" ("id", "body") VALUES ('own-80', $1)"#,
      vec![
        json!({ "id": "own-80", "agent": null, "line": "worker", "provider": "claude-code",
                "parent": "job-80", "state": "running", "startedAt": began, "endedAt": null,
                "paneId": pane, "panePath": true, "sessionId": "session-80" })
        .to_string()
        .into(),
      ],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "turn_liveness" ("execution", "run", "line", "session", "phase", "pane", "program_pid",
           "program_birth", "records", "opened_at", "written_at")
         VALUES ('own-80', 'job-80', 'worker', 'session-80', 'working', $1, $2, $3, $4, $5, $5)"#,
      vec![
        pane.clone().into(),
        i64::from(identity.pid).into(),
        identity.birth.clone().into(),
        json!([transcript]).to_string().into(),
        began.into(),
      ],
    )
    .await;
  transcribe(
    &transcript,
    &[
      json!({ "type": "user", "timestamp": stamp(began + 10), "message": { "content": "the work" } }),
      json!({ "type": "assistant", "timestamp": stamp(began + 20),
              "message": { "id": "m1", "content": [{ "type": "text", "text": "Started; a check runs." }] } }),
      json!({ "type": "queue-operation", "operation": "enqueue", "timestamp": stamp(began + 30) }),
      json!({ "type": "system", "subtype": "turn_duration", "timestamp": stamp(began + 40) }),
    ],
  );

  // The process before left the first assistant line filed, as a live turn's writer does.
  scratch
    .run(
      r#"INSERT INTO "agent_log" ("agent", "run", "at", "kind", "text", "job_id", "source_id", "source_block")
         VALUES ('worker', 'job-80', $1, 'text', 'Started; a check runs.', 80, 'm1', 0)"#,
      vec![(began + 20).into()],
    )
    .await;

  // Every pass, again and again, leaves the turn of a round still going kept, and its job running.
  for _ in 0..3 {
    every_pass(&ground).await;
    assert!(kept_now(&scratch, "job-80").await, "no pass lets go of a turn whose round goes on");
  }

  assert_eq!(job_state(&scratch, 80).await.0, "running");

  // The process carrying it goes away; the next start adopts the job through its execution's liveness row.
  going.abort();
  let _ = going.await;

  let row = in_flight_turns(&scratch).await.into_iter().find(|turn| turn.run == "job-80");

  assert!(row.is_some(), "the launch's row outlives its process");
  assert_eq!(
    own_pane(scratch.db(), "job-80").await.map(|found| (found.execution, found.pane)),
    Some(("own-80".to_owned(), Some(pane.clone())))
  );
  assert_eq!(at_start(&ground).await.unwrap(), 1);

  // The next process holds no thrown-away session of its own, and still answers the carried one's state by
  // its id while the turn goes on (docs/sessions.md, 「A restart hands the session over」).
  let restarted = HerdrSessions::new(places.clone());

  assert!(
    restarted.held_scope(&started.id).await.is_err(),
    "a fresh process holds no thrown-away session of its own"
  );
  assert!(
    restarted.read_state(&started.id, None).await.is_ok(),
    "the carried session's state answers by its id"
  );

  // A word said to the job's session, which this process does not hold, takes the job's road and is never
  // refused as ended: it goes into the picked-up turn while its program reads working, at once where the turn
  // already listens and otherwise on the pass that offers kept words again (docs/agent-control.md, 「Words for a
  // running job」).
  scratch
    .run(
      r#"UPDATE "jobs" SET "session_id" = $1 WHERE "id" = 80"#,
      vec![started.id.0.clone().into()],
    )
    .await;
  herdr.agent_status(&pane, "working");

  let word = "check the other branch too";
  let ask = agency_server::words::asked(json!({ "words": word })).unwrap();
  let said = agency_server::words::to_session(ground.clone(), Arc::new(restarted), &started.id.0, ask)
    .await
    .expect("words to a running job's session are taken");

  assert_eq!(said.job_id, Some(80));
  assert!(
    matches!(serde_json::to_value(said.said).unwrap().as_str(), Some("delivered" | "kept")),
    "{said:?}"
  );

  let typed = |herdr: &common::herdr::HerdrModel| {
    herdr.prompts().iter().any(|(target, text)| target == &pane && text == word)
  };

  for _ in 0..600 {
    if typed(&herdr) {
      break;
    }

    agency_server::job_words::offer_kept(scratch.db(), &crate::support::gates::none()).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
  }

  assert!(typed(&herdr), "the word went into the picked-up turn: {:?}", herdr.prompts());

  let unhanded = || async {
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "job_words" WHERE "job_id" = 80 AND "handed_at" IS NULL"#,
        vec![],
      )
      .await
  };

  for _ in 0..600 {
    if unhanded().await == 0 {
      break;
    }

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
  }

  assert_eq!(unhanded().await, 0, "a word the turn took is no longer kept for its next turn");

  // A note added to the job's item while it runs reaches the same turn: the job service's event pass reads its
  // `note.created`, keeps it for the job and hands it to the picked-up turn behind the paragraph naming it a
  // note of the item (docs/agent-control.md, 「Words for a running job」).
  let (peers, vars) = crate::support::peers::stand().await;
  let note = "Also update the operator's guide.";

  agency_server::queue::settings::set(scratch.db(), agency_server::watches::job_events::CURSOR_KEY, "10")
    .await
    .unwrap();
  peers.lock().unwrap().events = vec![json!({
    "id": 11, "kind": "note.created", "at": 11,
    "data": { "id": 5, "itemId": 1, "text": note, "actorName": "chat-agent", "labels": {}, "createdAt": now_ms() },
  })];
  agency_server::watches::job_events::pass(scratch.db(), &vars, &crate::support::gates::none())
    .await
    .unwrap();

  let noted = |herdr: &common::herdr::HerdrModel| {
    herdr.prompts().iter().any(|(target, text)| {
      target == &pane && text.contains("item #1 by chat-agent") && text.ends_with(note)
    })
  };

  for _ in 0..600 {
    if noted(&herdr) {
      break;
    }

    agency_server::job_words::offer_kept(scratch.db(), &crate::support::gates::none()).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
  }

  assert!(noted(&herdr), "the note went into the picked-up turn: {:?}", herdr.prompts());

  for _ in 0..600 {
    if unhanded().await == 0 {
      break;
    }

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
  }

  assert_eq!(unhanded().await, 0, "a note the turn took is no longer kept for its next turn");

  // The job's report ends with what it was told while it ran: the word and the note, each as said.
  let report = agency_server::words::report(scratch.db(), 80)
    .await
    .unwrap()
    .expect("a report of words");

  assert!(report.contains(word) && report.contains(note), "{report}");

  for _ in 0..3 {
    every_pass(&ground).await;
    assert!(
      kept_now(&scratch, "job-80").await,
      "the adopted turn stays kept while its round goes on"
    );
  }

  assert_eq!(job_state(&scratch, 80).await.0, "running", "the job goes on across the start");
  assert_eq!(slot_now(&ground, &held.token).await.state, "active");

  // What the turn writes while it is carried is filed as it goes, so the log a screen follows moves rather
  // than standing at the line the restart left, and the line the process before filed is not filed again.
  transcribe(
    &transcript,
    &[json!({ "type": "assistant", "timestamp": stamp(now_ms()),
              "message": { "id": "m-going", "content": [{ "type": "text", "text": "A look under way." }] } })],
  );

  for _ in 0..80 {
    if logged(&scratch, "job-80", "A look under way.").await {
      break;
    }

    tokio::time::sleep(std::time::Duration::from_millis(125)).await;
  }

  assert!(
    logged(&scratch, "job-80", "A look under way.").await,
    "the picked-up turn's log follows it as it goes"
  );
  assert_eq!(
    logged_count(&scratch, "job-80", "Started; a check runs.").await,
    1,
    "a line the process before filed is not filed again"
  );
  assert!(kept_now(&scratch, "job-80").await, "the log follows while the turn still runs");

  // The queued word is taken and answered, and the round ends: the job ends with its report.
  let now = now_ms();

  transcribe(
    &transcript,
    &[
      json!({ "type": "queue-operation", "operation": "dequeue", "timestamp": stamp(now) }),
      json!({ "type": "user", "timestamp": stamp(now + 1), "message": { "content": "the check ended" } }),
      json!({ "type": "assistant", "timestamp": stamp(now + 2),
              "message": { "id": "m2", "content": [{ "type": "text", "text": "The final report." }] } }),
      json!({ "type": "system", "subtype": "turn_duration", "timestamp": stamp(now + 3) }),
    ],
  );

  // The job's end is written as its turn's end is handed back, and the turn is let go after it.
  for _ in 0..80 {
    if job_state(&scratch, 80).await.0 != "running" && !kept_now(&scratch, "job-80").await {
      break;
    }

    tokio::time::sleep(std::time::Duration::from_millis(125)).await;
  }

  let (state, report) = job_state(&scratch, 80).await;

  assert_eq!(state, "reviewing");
  assert!(report.unwrap_or_default().contains("The final report."));
  assert!(!kept_now(&scratch, "job-80").await);

  for _ in 0..40 {
    if !herdr.panes().iter().any(|one| one.id == pane) {
      break;
    }

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
  }

  assert!(
    !herdr.panes().iter().any(|one| one.id == pane),
    "the pane of its own is closed with the turn"
  );

  let _ = program.kill();
  let _ = program.wait();
}

// A pane of its own a kept turn went out in, its program this test's own process, which stands while the test
// runs, and its transcript the one given.
fn own_pane_of(transcript: &std::path::Path, started_at: i64) -> OwnPane {
  let identity = agent_host::identity::ProcessIdentity::of(std::process::id() as i32).unwrap();

  OwnPane {
    execution: "own".into(),
    pane: None,
    transcript: transcript.to_path_buf(),
    session: "s".into(),
    program: Some(agency_server::door::liveness::Program {
      pid: identity.pid,
      birth: Some(identity.birth),
    }),
    started_at,
    provider: "claude-code".into(),
    model: "claude-sonnet-5".into(),
    effort: None,
    role: None,
    line: None,
    cwd: None,
  }
}

#[test]
fn a_turn_in_a_pane_of_its_own_its_program_never_took_is_lost_however_long_the_program_stands() {
  let home = tempfile::tempdir().unwrap();
  let transcript = home.path().join("session.jsonl");
  let began = now_ms() - 60_000;
  let pane = own_pane_of(&transcript, began);

  // No transcript at all: the program never took the words.
  assert_eq!(standing_in_pane(&pane), Standing::Lost);

  // Words of an earlier turn before the execution began say nothing of this one.
  transcribe(
    &transcript,
    &[
      json!({ "type": "user", "timestamp": stamp(began - 10_000), "message": { "content": "an older turn" } }),
    ],
  );
  assert_eq!(standing_in_pane(&pane), Standing::Lost);

  // Once the words stand in the transcript since the execution began, the turn goes on while its program does.
  transcribe(
    &transcript,
    &[json!({ "type": "user", "timestamp": stamp(began + 10), "message": { "content": "the work" } })],
  );
  assert_eq!(standing_in_pane(&pane), Standing::Going);
}

#[tokio::test]
async fn an_adopted_turn_no_job_holds_is_cut_at_the_bound_of_an_adopted_turn() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let bound = policy::resident_turn::RESIDENT_TURN.adopted_bound_minutes as i64 * 60_000;
  let mut turn = in_flight("voyager-run", "voyager", 1, Some(7), None, None);

  assert_eq!(over_bound(&ground, &turn, turn.started_at + bound - 1).await, None);

  let cut = over_bound(&ground, &turn, turn.started_at + bound).await.expect("cut at the bound");

  assert!(cut.contains(&(bound / 60_000).to_string()), "the reason names the bound: {cut}");

  // A job's turn is held to the job's own bound, which is no shorter here, and marks its job stopped.
  turn.job = Some(5);
  running_job(&scratch, 5, now_ms()).await;
  assert_eq!(over_bound(&ground, &turn, turn.started_at + bound - 1).await, None);
}

#[tokio::test]
async fn a_turn_its_pane_of_its_own_never_took_is_ended_at_start_and_a_job_s_is_launched_again() {
  use agency_server::control::work::{RESUMED, resume_launches};

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let began = now_ms() - 20 * 60 * 1000;

  crate::support::rows::repository(scratch.db(), "worker-a", "/checkouts/worker-a", &[]).await;

  // Two turns in panes of their own whose programs never took their words: a research run's, which no job
  // holds, and a job's. Each program stands — this process — and each transcript holds nothing since its
  // execution began.
  let identity = agent_host::identity::ProcessIdentity::of(std::process::id() as i32).unwrap();

  for (run, agent, job) in [("research-1", "voyager", None), ("job-7191", "worker-a", Some(7191))] {
    let execution = format!("own-{run}");
    let transcript = home.path().join(format!("{run}.jsonl"));

    scratch
      .run(
        r#"INSERT INTO "agent_executions" ("id", "body") VALUES ($1, $2)"#,
        vec![
          execution.clone().into(),
          json!({ "id": execution, "agent": null, "line": "worker", "provider": "claude-code",
                  "parent": run, "state": "running", "startedAt": began, "endedAt": null })
          .to_string()
          .into(),
        ],
      )
      .await;
    scratch
      .run(
        r#"INSERT INTO "turn_liveness" ("execution", "run", "line", "session", "phase", "program_pid",
             "program_birth", "records", "opened_at", "written_at")
           VALUES ($1, $2, 'worker', 's', 'working', $3, $4, $5, $6, $6)"#,
        vec![
          execution.into(),
          run.into(),
          i64::from(identity.pid).into(),
          identity.birth.clone().into(),
          json!([transcript]).to_string().into(),
          began.into(),
        ],
      )
      .await;

    keep(scratch.db(), &in_flight(run, agent, 1, Some(7), None, job)).await;
  }

  running_job(&scratch, 7191, began).await;
  scratch
    .run(r#"UPDATE "jobs" SET "turn_words" = 'the work' WHERE "id" = 7191"#, vec![])
    .await;

  let mut ended = ground.events.subscribe::<TurnEnded>();

  assert_eq!(at_start(&ground).await.unwrap(), 2);

  // The research run's turn ends failed for its caller, and the job's is let go: neither is kept, and each
  // execution the turn opened is closed.
  assert!(!kept_now(&scratch, "research-1").await);
  assert!(!kept_now(&scratch, "job-7191").await);
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) AS "n" FROM "agent_executions" WHERE ("body"::jsonb)->>'endedAt' IS NULL"#,
        vec![],
      )
      .await,
    0
  );

  let end = ended.ready().expect("the research run's end is published");

  assert_eq!(end.session, "voyager@worker");
  assert!(end.end.reason.unwrap_or_default().contains("no record its program keeps names it"));
  assert!(ended.ready().is_none(), "the job's turn publishes no end: its job goes on");

  // The job is launched again from the words it keeps, carried before the pass that ends a job left running
  // reads it, so that pass leaves it. This ground stands no places, so the launch itself ends failed saying so.
  assert_eq!(resume_launches(&ground, None).await, 1);
  assert!(logged(&scratch, "job-7191", RESUMED).await);
  reap_stale(&ground).await.unwrap();

  for _ in 0..100 {
    if job_state(&scratch, 7191).await.0 != "running" {
      break;
    }

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
  }

  let (state, report) = job_state(&scratch, 7191).await;
  let report = report.unwrap_or_default();

  assert_eq!(state, "stopped");
  assert!(report.contains("agency stands no places"), "the launch ran again: {report}");
  assert!(
    !report.contains(agency_server::control::work::REAPED),
    "the job was not reaped: {report}"
  );
}

// The place a run in a pane of its own keeps for its hooks outlives a restart while that run is a turn kept in
// flight, so its stops stay held for the rest of the turn, and is ended with that turn's end; one whose run
// is no turn in flight is ended by the sweep (docs/agent-control.md, 「What a resident turn reads」).
#[tokio::test]
async fn a_hooks_place_of_a_turn_kept_in_flight_outlives_the_sweep_and_ends_with_its_turn() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let data = ground.roots.data.clone();
  let (going, going_place) = apart_place(&ground, "worker-a@3c1e0b7a-0d5e-4a4f-9a54-6f2d8f1b2c01");
  let (left, left_place) = apart_place(&ground, "worker-a@9b7d2e44-51f0-4c3a-8e1b-0a6c5d4e3f02");

  for (place, run) in [(&going_place, "job-51"), (&left_place, "job-50")] {
    agency_server::door::place_run::begin_run(&place.path, run, "a job", now_ms(), Default::default())
      .unwrap();
  }
  keep(scratch.db(), &in_flight("job-51", "worker-a", 71, None, None, Some(51))).await;

  assert_eq!(agency_server::apart_places::sweep(&ground).await, 1);
  assert!(
    agency_server::apart_places::is_apart_place(&data, &going),
    "the running turn's place is kept"
  );
  assert!(
    !agency_server::apart_places::is_apart_place(&data, &left),
    "the ended run's place is ended"
  );

  agency_server::apart_places::end_run_places(&ground, "job-51", "worker-a").await;

  assert!(!going_place.path.exists(), "the place ends with its turn");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_s_turn_read_in_its_hooks_place_has_its_pane_of_its_own_closed_once_its_end_is_handed_back() {
  use crate::integration::common::herdr::HerdrModel;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let mut ground = seeded(scratch.db(), home.path()).await;
  let herdr = HerdrModel::stand(home.path().join("herdr.sock"));

  ground.herdr = Some(herdr::Herdr::there(herdr.socket.clone()));

  let ground = Arc::new(ground);
  let mut program = std::process::Command::new("sleep").arg("30").spawn().unwrap();

  deepseek_balance(&ground, 5.0).await;
  crate::support::rows::repository(scratch.db(), "worker-a", "/checkouts/worker-a", &[]).await;
  running_job(&scratch, 44, now_ms() - 20 * 60 * 1000).await;

  // The job's turn as the worker line sends it: its program in a pane of its own, its hooks writing into a place
  // of the run's own, where the program wrote its end while no process carried the turn.
  let (_, place) = apart_place(&ground, "worker-a@run:17");
  let (transcript, payload) = claude_code_turn(&place, "job-44", now_ms() - 60_000, program.id());
  let (_, pane) = herdr.workspace("a job", serde_json::Value::Null, Some(&["program"]));

  scratch
    .run(
      r#"INSERT INTO "agent_executions" ("id", "body") VALUES ('own-44', $1)"#,
      vec![
        json!({ "id": "own-44", "line": "worker", "provider": "claude-code", "state": "running",
                "startedAt": now_ms() - 60_000, "endedAt": null, "paneId": pane })
        .to_string()
        .into(),
      ],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "turn_liveness" ("execution", "run", "line", "session", "phase", "pane", "records",
           "opened_at", "written_at")
         VALUES ('own-44', 'job-44', 'worker', 's', 'working', $1, $2, $3, $3)"#,
      vec![pane.clone().into(), json!([transcript]).to_string().into(), now_ms().into()],
    )
    .await;
  keep(
    scratch.db(),
    &InFlight {
      session: "worker-a@run:17".into(),
      ..in_flight("job-44", "worker-a", 64, None, None, Some(44))
    },
  )
  .await;
  claude_code_answers(&place, &transcript, &payload, "The job's report.");

  assert_eq!(at_start(&ground).await.unwrap(), 1);
  assert!(
    !herdr.panes().iter().any(|one| one.id == pane),
    "the pane of its own is closed with the turn read in its place"
  );

  let _ = program.kill();
  let _ = program.wait();
}
