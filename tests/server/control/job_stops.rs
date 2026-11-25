//! A job stopped (docs/api.md, 「assignments and jobs」): the supervisor's stop only on a turn that is the owner's,
//! refused for an agent carrying no turn and for anyone else's turn; a stop cutting the job's turn short and
//! answering once that turn has ended, the job stopped with the stop's reason; and the repository's checkout
//! read as held by the job while its turn goes on, and free, taking the next item, once the stop has answered;
//! and the job's slot given back with its stop, so the next job of its role is let in.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::control::{Front, router};
use agency_server::door::execution::cancelled;
use agency_server::lines::place::Place;
use agency_server::now_ms;
use agency_server::order_run::dispatch;
use agency_server::places::Ground;
use agency_server::turn_marks::{self, Mark};
use agency_server::turn_pickup::{InFlight, at_start, keep};
use harness_providers::Machine;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::support::grounds::{deepseek_balance, job_slot, seeded, slot_now};
use crate::support::program;
use crate::support::rows::repository;
use crate::support::scratch::Scratch;

const TOKEN: &str = "secret";

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

// The server's control front on the ground, served on a loopback port of its own.
async fn front(ground: Arc<Ground>) -> u16 {
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

// A repository's checkout with one running job on an item, its assignment running; the job's turn under `run`
// where it names one.
async fn running(scratch: &Scratch, worker: &str, job: i64, item: i64, run: Option<&str>) {
  let now = now_ms();

  repository(scratch.db(), worker, &format!("/checkouts/{worker}"), &[]).await;
  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at",
           "changed_at")
         VALUES ($1, $2, 'harness', $3, 'running', $4, $4)"#,
      vec![job.into(), item.into(), worker.into(), now.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "run_name", "state", "started_at", "prompt")
         VALUES ($1, $2, 0, 'harness', $1, $3, $4, 'running', $5, 'work')"#,
      vec![job.into(), item.into(), worker.into(), run.map(str::to_owned).into(), now.into()],
    )
    .await;
}

// The job holding a repository's base checkout, as the repository's view answers it; null where none does.
async fn holding(port: u16, repository: &str) -> Value {
  let (_, view) = call(port, reqwest::Method::GET, &format!("/repositories/{repository}"), None).await;

  view["checkouts"][0]["job"].clone()
}

async fn job_stop(port: u16, body: Value) -> (u16, Value) {
  call(port, reqwest::Method::POST, "/job-stops", Some(body)).await
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_owner_s_turn_stops_a_job_and_the_stop_names_exactly_one_job() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let port = front(ground(&scratch, home.path())).await;
  let agent = "stopping-chat-agent";
  let stop = |reason: &str| json!({ "agentName": agent, "itemId": 41, "reason": reason });

  running(&scratch, "worker-a", 7, 41, None).await;

  // An agent carrying no turn stops nothing.
  let (status, refused) = job_stop(port, stop("no turn")).await;

  assert_eq!((status, refused["code"].as_str()), (409, Some("no-turn")), "{refused}");

  // A turn not every message of which is the owner's stops nothing either, and the job runs on.
  turn_marks::carrying(
    agent,
    "run-1",
    Mark {
      chat_id: -5,
      privileged: false,
    },
  );

  let (status, refused) = job_stop(port, stop("someone in the room")).await;

  assert_eq!((status, refused["code"].as_str()), (409, Some("not-owner")), "{refused}");
  assert_eq!(call(port, reqwest::Method::GET, "/jobs/7", None).await.1["state"], "running");

  // The owner's turn stops it; a stop naming no job, or two, and an item with no running job are refused.
  turn_marks::carrying(
    agent,
    "run-2",
    Mark {
      chat_id: -5,
      privileged: true,
    },
  );
  turn_marks::ended(agent, "run-1");

  let (status, refused) = job_stop(port, json!({ "agentName": agent, "reason": "which one" })).await;

  assert_eq!((status, refused["code"].as_str()), (400, Some("bad-request")), "{refused}");

  let (status, refused) =
    job_stop(port, json!({ "agentName": agent, "jobId": 7, "itemId": 41, "reason": "both" })).await;

  assert_eq!((status, refused["code"].as_str()), (400, Some("bad-request")), "{refused}");

  let (status, refused) =
    job_stop(port, json!({ "agentName": agent, "itemId": 99, "reason": "none" })).await;

  assert_eq!((status, refused["code"].as_str()), (404, Some("no-job")), "{refused}");

  let (status, stopped) = job_stop(port, stop("The owner called it off.")).await;

  assert_eq!(status, 200, "{stopped}");
  assert_eq!(
    (&stopped["state"], &stopped["outcome"], &stopped["report"]),
    (&json!("stopped"), &json!("stopped"), &json!("The owner called it off."))
  );
  assert_eq!(call(port, reqwest::Method::GET, "/assignments/7", None).await.1["state"], "released");

  let (status, refused) =
    job_stop(port, json!({ "agentName": agent, "jobId": 7, "reason": "again" })).await;

  assert_eq!(status, 409, "a job already stopped is refused: {refused}");
  turn_marks::ended(agent, "run-2");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_job_s_turn_is_cut_short_first_and_its_worker_takes_the_next_item() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let port = front(ground.clone()).await;
  let place = Place::of(&ground.roots.data, "worker-a");
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

  // The job's turn goes on in the worker's program across agency's restart, picked up and waited on.
  let _program = program::running(&place);
  running(&scratch, "worker-a", 8, 42, Some("job-8")).await;
  keep(
    scratch.db(),
    &InFlight {
      session: "worker-a@worker".into(),
      turn: 1,
      run: "job-8".into(),
      key: None,
      agent: "worker-a".into(),
      chat_id: None,
      ledger: None,
      job: Some(8),
      started_at: now_ms() - 1_000,
    },
  )
  .await;
  append(
    json!({ "kind": "start", "id": "j", "name": "job-8", "title": "a turn", "at": 1, "typed": false }),
  );
  at_start(&ground).await.unwrap();

  assert_eq!(holding(port, "worker-a").await["jobId"], json!(8));

  // The program: its turn goes on until the stop's cancellation reaches it, and is then cut short.
  let db = scratch.db().clone();
  let program = tokio::spawn({
    let place = place.path.clone();
    async move {
      for _ in 0..200 {
        if cancelled(&db, &["job-8"]).await.unwrap() {
          use std::io::Write as _;

          let mut file = std::fs::OpenOptions::new().append(true).open(place.join("loop.jsonl")).unwrap();

          writeln!(file, "{}", json!({ "kind": "interrupted", "id": "j" })).unwrap();
          writeln!(file, "{}", json!({ "kind": "end", "id": "j" })).unwrap();
          return true;
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
      }

      false
    }
  });

  // The stop answers only once the turn has ended, the job stopped with the stop's reason, not the turn's.
  let (status, stopped) = call(
    port,
    reqwest::Method::PATCH,
    "/jobs/8",
    Some(json!({ "state": "stopped", "report": "Stopped for the bench." })),
  )
  .await;

  assert!(program.await.unwrap(), "the stop reached the job's turn");
  assert_eq!(status, 200, "{stopped}");
  assert_eq!(
    (&stopped["state"], &stopped["outcome"], &stopped["report"]),
    (&json!("stopped"), &json!("stopped"), &json!("Stopped for the bench."))
  );
  assert!(
    !cancelled(scratch.db(), &["job-8"]).await.unwrap(),
    "the stop takes its cancellation back"
  );

  // Its checkout reads free and is held for the next item at once.
  assert_eq!(holding(port, "worker-a").await, Value::Null);

  let (status, assignment) = call(
    port,
    reqwest::Method::POST,
    "/assignments",
    Some(json!({ "itemId": 43, "holderName": "harness", "assigneeName": "worker-a",
                 "item": { "title": "The next item", "brief": "Carry on.", "notes": [] } })),
  )
  .await;

  assert_eq!(status, 201, "{assignment}");
  assert_eq!(assignment["workerName"], "worker-a", "{assignment}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_stopped_through_the_route_gives_its_slot_back_and_the_next_waiting_job_is_let_in() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = Arc::new(seeded(scratch.db(), home.path()).await);
  let port = front(ground.clone()).await;

  deepseek_balance(&ground, 5.0).await;
  scratch
    .run(r#"UPDATE "roles" SET "max_concurrent" = 1 WHERE "name" = 'worker'"#, vec![])
    .await;
  crate::support::rows::reseated(scratch.db()).await;
  running(&scratch, "worker-a", 9, 44, Some("job-9")).await;

  // The job holds the worker role's one slot, its lease bound to its run, and the next job waits for it.
  let held = job_slot(&ground, 9, true).await;
  let next = job_slot(&ground, 10, false).await;

  assert_eq!(held.child_id.as_deref(), Some("job-9"));

  let (status, stopped) = call(
    port,
    reqwest::Method::PATCH,
    "/jobs/9",
    Some(json!({ "state": "stopped", "report": "Stopped for the line." })),
  )
  .await;

  assert_eq!(status, 200, "{stopped}");
  assert_eq!(stopped["state"], "stopped");

  // The job's end gave its slot back as its owner's, with the reason the end carries.
  let given = slot_now(&ground, &held.token).await;

  assert_eq!(
    (given.state.as_str(), given.ended_by.as_deref()),
    ("released", Some(dispatch::BY_OWNER)),
    "{given:?}"
  );
  assert!(
    given.outcome.as_deref().is_some_and(|why| why.contains("Stopped for the line.")),
    "{given:?}"
  );

  // The next waiting job of the role is let in at its next ask.
  assert!(
    dispatch::acquire(&ground, &slot_now(&ground, &next.token).await)
      .await
      .unwrap()
      .is_some()
  );
}
