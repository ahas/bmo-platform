//! A job whose turn had not gone out when agency restarted (docs/api.md, 「assignments and jobs」): the start
//! launches again every job still `starting`, every `running` job that keeps the words of a turn that never came
//! back, and every `reviewing` job whose review's turn was still taking its checkout back, and leaves the rest to
//! the pass that ends a job left running; a job's slot request still waiting in the line is taken up again
//! rather than asked anew; and a work order whose job service drops the connection or answers an error is asked
//! again, the job held meanwhile, never read as none, and the review it opens is opened once (docs/job-review.md,
//! 「Stages」).

use std::sync::{Arc, Mutex};

use agency_server::Service;
use agency_server::config::Config;
use agency_server::control::work::{REAPED, RESUMED, job_seat, reap_stale, resume_launches};
use agency_server::now_ms;
use agency_server::order_run::dispatch::{self, Asked, Wanted};
use agency_server::places::Ground;
use axum::Router;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use harness_providers::Machine;
use serde_json::json;
use tokio::net::TcpListener;

use crate::support::grounds::seeded;
use crate::support::rows::repository;
use crate::support::scratch::Scratch;

// A ground on a scratch store with the variables given. A test ground reaches no peer, so one that calls a stand-in
// is not marked as one.
fn ground(scratch: &Scratch, home: &std::path::Path, more: &[(&str, String)]) -> Arc<Ground> {
  let service = Service::new(scratch.db().clone(), Config::read(|_| None).unwrap());
  let mut env = vec![
    ("HOME".to_owned(), home.to_string_lossy().into_owned()),
    ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
  ];

  env.extend(more.iter().map(|(name, value)| ((*name).to_owned(), value.clone())));

  Arc::new(Ground::new(&service, &Machine::with_env(env, home.join("data")), None))
}

// A test ground, which tells no one and reaches no peer.
fn test_ground(scratch: &Scratch, home: &std::path::Path) -> Arc<Ground> {
  ground(scratch, home, &[("BMO_TEST_GROUND", "1".to_owned())])
}

async fn state_of(scratch: &Scratch, id: i64) -> (String, String) {
  let row =
    store::one(scratch.db(), r#"SELECT "state", "report" FROM "jobs" WHERE "id" = $1"#, vec![id.into()])
      .await
      .unwrap()
      .unwrap();

  (
    row.try_get("", "state").unwrap(),
    row.try_get::<Option<String>>("", "report").unwrap().unwrap_or_default(),
  )
}

async fn resumed_in_log(scratch: &Scratch, id: i64) -> bool {
  scratch
    .count(
      r#"SELECT COUNT(*) AS "n" FROM "agent_log" WHERE "run" = $1 AND "text" = $2"#,
      vec![format!("job-{id}").into(), RESUMED.into()],
    )
    .await
    > 0
}

#[tokio::test]
async fn the_start_launches_again_every_job_whose_turn_had_not_gone_out_and_leaves_the_rest_to_the_reap() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = test_ground(&scratch, home.path());
  let old = now_ms() - 20 * 60 * 1000;

  repository(scratch.db(), "bmo-cli", "/checkouts/bmo-cli", &[]).await;

  // The jobs are numbered apart from every other test's, since the jobs this process carries are one set for
  // the whole test binary. 7101 waits for its first turn; 7102 keeps the words of a turn that never came back; 7103 keeps none; 7104 is under
  // review with the turn its review asked for still taking its checkout back; 7105 keeps words but its turn is
  // kept in flight for the pickup; 7106 is under review with no turn asked for.
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "run_name", "room_id", "state", "started_at", "prompt", "turn_words", "report", "review_turn")
         VALUES
           (7101, 1, 0, 'harness', 0, 'bmo-cli', NULL, 7, 'starting', $1, 'one', NULL, NULL, NULL),
           (7102, 2, 0, 'harness', 0, 'bmo-cli', 'job-7102', 7, 'running', $1, 'two', 'the work', NULL, 'work'),
           (7103, 3, 0, 'harness', 0, 'bmo-cli', 'job-7103', 7, 'running', $1, 'three', NULL, NULL, 'work'),
           (7104, 4, 0, 'harness', 0, 'bmo-cli', 'job-7104', 7, 'reviewing', $1, 'four', 'stage two', 'stage one done',
              'work'),
           (7105, 5, 0, 'harness', 0, 'bmo-cli', 'job-7105', 7, 'running', $1, 'five', 'the work', NULL, 'work'),
           (7106, 6, 0, 'harness', 0, 'bmo-cli', 'job-7106', 7, 'reviewing', $1, 'six', 'stale', 'a report', NULL)"#,
      vec![old.into()],
    )
    .await;
  agency_server::turn_pickup::keep(
    scratch.db(),
    &agency_server::turn_pickup::InFlight {
      session: "bmo-cli~1".into(),
      turn: 1,
      key: None,
      run: "job-7105".into(),
      agent: "bmo-cli".into(),
      chat_id: Some(7),
      ledger: None,
      job: Some(7105),
      started_at: old,
    },
  )
  .await;

  assert_eq!(resume_launches(&ground, None).await, 3);
  reap_stale(&ground).await.unwrap();

  for id in [7101, 7102, 7104] {
    assert!(resumed_in_log(&scratch, id).await, "job {id} says it was launched again");
  }

  for id in [7103, 7105, 7106] {
    assert!(!resumed_in_log(&scratch, id).await, "job {id} is not launched again");
  }

  // The job that kept nothing is reaped as before; the one the pickup holds and the one under review stand.
  let (state, report) = state_of(&scratch, 7103).await;

  assert_eq!(state, "stopped");
  assert!(report.contains(REAPED));
  assert_eq!(state_of(&scratch, 7105).await.0, "running");
  assert_eq!(state_of(&scratch, 7106).await.0, "reviewing");

  // Each job launched again goes out as its launch would have; this ground stands no places, so each ends
  // failed saying so rather than reaped. Each launch is a task of its own, which a loaded machine may run
  // late, so the wait is bounded wide.
  for id in [7101, 7102, 7104] {
    for _ in 0..1500 {
      if matches!(state_of(&scratch, id).await.0.as_str(), "stopped" | "ended") {
        break;
      }

      tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let (state, report) = state_of(&scratch, id).await;

    assert_eq!(state, "stopped", "job {id}");
    assert!(report.contains("agency stands no places"), "job {id} went out again: {report}");
  }
}

#[tokio::test]
async fn a_job_s_slot_request_still_waiting_in_the_line_is_taken_up_again_rather_than_asked_anew() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  repository(scratch.db(), "worker", "/checkouts/worker", &[]).await;

  let asked = |key: &str| Asked {
    request_key: key.to_owned(),
    owner: format!("{}77", dispatch::JOB_OWNER),
    wanted: Wanted {
      role: "worker",
      path: Some("/checkouts/worker"),
      fixed: None,
      agent: None,
    },
    ..Asked::default()
  };

  // The launch of the process before asked, and its request waits in the line as that process went away.
  let waiting =
    dispatch::enqueue(&ground.db, &ground.events, &asked("job-77-1-1"), dispatch::JOB, async |_, _| Ok(()))
      .await
      .unwrap();

  assert_eq!(waiting.state, "waiting");

  // The launch again asks under a key of its own, and is let in on the request that waited.
  let seat = job_seat(&ground, 77, &asked("job-77-2-1")).await.unwrap().expect("let in");

  assert_eq!(seat.token, waiting.token, "the request that waited is the one let in");
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) AS "n" FROM "role_slot_requests" WHERE "owner" = $1"#,
        vec![format!("{}77", dispatch::JOB_OWNER).into()],
      )
      .await,
    1,
    "no second request is asked"
  );
}

// What the stand-in job service saw: the connections it dropped unanswered, the work order's reads it answered,
// the job's wait as each answered read found it, and the review's entries.
#[derive(Default)]
struct Seen {
  cut: usize,
  reads: usize,
  held: Vec<Option<String>>,
  entries: Vec<String>,
}

#[tokio::test(flavor = "multi_thread")]
async fn a_work_order_whose_service_drops_the_connection_is_asked_again_and_the_review_is_opened_once() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let seen: Arc<Mutex<Seen>> = Arc::default();
  let kept = seen.clone();
  let db = scratch.db().clone();
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let app = Router::new().fallback(move |request: Request| {
    let kept = kept.clone();
    let db = db.clone();

    async move {
      let path = request.uri().path().to_owned();

      if path == "/items/3/work-order" {
        let held = store::one(&db, r#"SELECT "held_for" FROM "jobs" WHERE "id" = 30"#, vec![])
          .await
          .unwrap()
          .and_then(|row| row.try_get::<Option<String>>("", "held_for").ok().flatten());
        let reads = {
          let mut seen = kept.lock().unwrap();

          seen.reads += 1;
          seen.held.push(held);
          seen.reads
        };

        // The job service answers the first read that reaches it with an error.
        if reads < 2 {
          return (StatusCode::SERVICE_UNAVAILABLE, "not now").into_response();
        }

        return axum::Json(json!({ "steps": ["find the cause", "fix it"] })).into_response();
      }

      if request.method() == axum::http::Method::POST && path == "/items/3/reviews" {
        let body = axum::body::to_bytes(request.into_body(), 1 << 20).await.unwrap();
        let entry: serde_json::Value = serde_json::from_slice(&body).unwrap();

        kept.lock().unwrap().entries.push(entry["kind"].as_str().unwrap_or_default().to_owned());

        return (StatusCode::CREATED, axum::Json(json!({}))).into_response();
      }

      axum::Json(json!({ "items": [] })).into_response()
    }
  });

  // The job service goes away under the first call made of it: the connection is taken, the request read, and the
  // connection dropped with no answer. Every later connection is served by the routes above.
  let cutting = seen.clone();

  tokio::spawn(async move {
    let mut first = true;

    loop {
      let (mut stream, _) = listener.accept().await.unwrap();

      if std::mem::take(&mut first) {
        let mut head = [0u8; 4096];
        let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut head).await;

        cutting.lock().unwrap().cut += 1;
        drop(stream);
        continue;
      }

      let app = app.clone();

      tokio::spawn(async move {
        let _ = hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new())
          .serve_connection(
            hyper_util::rt::TokioIo::new(stream),
            hyper_util::service::TowerToHyperService::new(app),
          )
          .await;
      });
    }
  });

  let ground = ground(
    &scratch,
    home.path(),
    &[
      ("BMO_JOB_URL", format!("http://127.0.0.1:{port}")),
      ("BMO_JOB_TOKEN", "job-token".to_owned()),
      (agency_server::job_review::WORK_ORDER_PAUSE, "5".to_owned()),
    ],
  );

  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "room_id", "state", "started_at", "prompt")
         VALUES (30, 3, 0, 'harness', 0, 'bmo-cli', 7, 'starting', $1, 'the item')"#,
      vec![now_ms().into()],
    )
    .await;

  let words = agency_server::job_review::opened(&ground, 30).await.expect("the review opens");

  assert!(words.contains("Do stage 1 alone now: find the cause"), "{words}");

  {
    let seen = seen.lock().unwrap();

    assert_eq!(seen.cut, 1, "the first read's connection was dropped unanswered");
    assert_eq!(
      seen.reads, 2,
      "the read is asked again after the cut and after the error, until it answers"
    );
    assert!(
      seen
        .held
        .iter()
        .all(|held| held.as_deref().is_some_and(|held| held.contains("work order"))),
      "the job names the wait from the cut on: {:?}",
      seen.held
    );
    assert_eq!(seen.entries, vec!["opened".to_owned()]);
  }

  let held = store::one(
    scratch.db(),
    r#"SELECT "held_for", "review_stages" FROM "jobs" WHERE "id" = 30"#,
    vec![],
  )
  .await
  .unwrap()
  .unwrap();

  assert_eq!(held.try_get::<Option<String>>("", "held_for").unwrap(), None, "the wait is over");
  assert_eq!(
    held.try_get::<Option<String>>("", "review_stages").unwrap().as_deref(),
    Some(r#"["find the cause","fix it"]"#)
  );

  // A launch again after a restart opens the review no second time: the same words, no read, no entry.
  assert_eq!(agency_server::job_review::opened(&ground, 30).await.as_deref(), Some(words.as_str()));

  let seen = seen.lock().unwrap();

  assert_eq!((seen.cut, seen.reads), (1, 2));
  assert_eq!(seen.entries.len(), 1);
}
