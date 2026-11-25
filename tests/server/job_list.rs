//! The jobs' list (docs/api.md, 「assignments and jobs」): the newest first, paged by a job's id before or after
//! it with `more` saying whether a page lies beyond, narrowed by each filter alone and by every one named at once,
//! and a query naming no filter of its, or a value that does not read, refused.

use serde_json::Value;

use crate::support::scratch::Scratch;
use crate::support::standing;

const TOKEN: &str = "secret";

// Six jobs, oldest first: (item, holder, worker, room, state). The running ones begin now, so the server's
// start leaves them running.
const JOBS: [(i64, &str, &str, i64, &str); 6] = [
  (11, "harness-a", "worker-a", -5, "ended"),
  (12, "harness-a", "worker-b", -6, "stopped"),
  (11, "harness-b", "worker-a", -5, "running"),
  (13, "harness-b", "worker-b", -6, "ended"),
  (14, "harness-a", "worker-a", -6, "starting"),
  (11, "harness-a", "worker-b", -5, "ended"),
];

// A server over a store holding the six jobs, and its port; the jobs' ids are 1 to 6, oldest first.
async fn served(scratch: &Scratch, home: &tempfile::TempDir) -> standing::Standing {
  let data = home.path().join("data");
  let now = agency_server::now_ms();

  std::fs::create_dir_all(&data).unwrap();

  for (at, (item, holder, worker, room, state)) in JOBS.iter().enumerate() {
    scratch
      .run(
        r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "room_id",
                               "state", "started_at", "prompt")
           VALUES ($1, $2, 1, $3, 1, $4, $5, $6, $7, 'work')"#,
        vec![
          (at as i64 + 1).into(),
          (*item).into(),
          (*holder).into(),
          (*worker).into(),
          (*room).into(),
          (*state).into(),
          now.into(),
        ],
      )
      .await;
  }

  standing::stand_server(home.path(), &data, &scratch.address(), TOKEN)
}

async fn get(port: u16, path: &str) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(TOKEN)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

async fn ids(port: u16, path: &str) -> Vec<i64> {
  let (status, page) = get(port, path).await;

  assert_eq!(status, 200, "{path}: {page}");
  page["items"]
    .as_array()
    .unwrap()
    .iter()
    .map(|job| job["id"].as_i64().unwrap())
    .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_newest_jobs_come_first_and_pages_go_back_by_before_and_forward_by_after() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let server = served(&scratch, &home).await;
  let port = server.port;

  assert_eq!(ids(port, "/jobs").await, [6, 5, 4, 3, 2, 1]);

  let (_, page) = get(port, "/jobs?limit=2").await;

  assert_eq!(page["more"], true);
  assert_eq!(ids(port, "/jobs?limit=2").await, [6, 5]);
  assert_eq!(ids(port, "/jobs?before=5&limit=2").await, [4, 3]);
  assert_eq!(
    ids(port, "/jobs?after=2&limit=2").await,
    [4, 3],
    "the jobs nearest after it, newest first"
  );
  assert_eq!(get(port, "/jobs?before=3&limit=2").await.1["more"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn running_narrows_to_the_jobs_starting_or_running_or_to_every_other() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let server = served(&scratch, &home).await;

  assert_eq!(ids(server.port, "/jobs?running=true").await, [5, 3]);
  assert_eq!(ids(server.port, "/jobs?running=false").await, [6, 4, 2, 1]);
}

#[tokio::test(flavor = "multi_thread")]
async fn state_narrows_to_one_state_or_several_comma_separated() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let server = served(&scratch, &home).await;

  assert_eq!(ids(server.port, "/jobs?state=ended").await, [6, 4, 1]);
  assert_eq!(ids(server.port, "/jobs?state=running,starting").await, [5, 3]);
}

#[tokio::test(flavor = "multi_thread")]
async fn item_id_narrows_to_one_item_s_jobs() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let server = served(&scratch, &home).await;

  assert_eq!(ids(server.port, "/jobs?itemId=11").await, [6, 3, 1]);
}

#[tokio::test(flavor = "multi_thread")]
async fn room_id_narrows_to_one_room_s_jobs() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let server = served(&scratch, &home).await;

  assert_eq!(ids(server.port, "/jobs?roomId=-6").await, [5, 4, 2]);
}

#[tokio::test(flavor = "multi_thread")]
async fn worker_name_narrows_to_one_worker_s_jobs() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let server = served(&scratch, &home).await;

  assert_eq!(ids(server.port, "/jobs?workerName=worker-a").await, [5, 3, 1]);
}

#[tokio::test(flavor = "multi_thread")]
async fn holder_name_narrows_to_one_holder_s_jobs() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let server = served(&scratch, &home).await;

  assert_eq!(ids(server.port, "/jobs?holderName=harness-b").await, [4, 3]);
}

#[tokio::test(flavor = "multi_thread")]
async fn every_filter_named_holds_at_once_and_one_not_taken_or_not_read_is_refused() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let server = served(&scratch, &home).await;
  let port = server.port;

  assert_eq!(ids(port, "/jobs?itemId=11&workerName=worker-b&roomId=-5&running=false").await, [6]);

  for (path, field) in [
    ("/jobs?holder=harness-a", "holder"),
    ("/jobs?running=yes", "running"),
    ("/jobs?itemId=eleven", "itemId"),
    ("/jobs?limit=0", "limit"),
  ] {
    let (status, refused) = get(port, path).await;

    assert_eq!(status, 400, "{path}: {refused}");
    assert!(refused["error"].as_str().unwrap_or_default().contains(field), "{path}: {refused}");
  }
}
