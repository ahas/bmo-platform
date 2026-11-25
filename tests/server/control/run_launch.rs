//! A run's runner launched in the shape the search unit sends: left in its agent's host under the run's unit
//! with the program as the caller named it, and a runner the host did not start closing its run as an error,
//! on a running server and a host standing in for the data root's.

use std::path::Path;
use std::time::{Duration, Instant};

use agent_host::disk::{private_dir, publish};
use agent_host::envelope::{Cause, Outcome, Started};
use agent_host::identity::{ProcessIdentity, process_birth};
use agent_host::layout::Layout;
use serde_json::{Value, json};

use crate::support::scratch::Scratch;
use crate::support::standing;

async fn get(port: u16, token: &str, path: &str) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
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

/// The data root's host as the server finds it: this process holds the claim, and each pointer put in the
/// inbox is answered in turn, started where its answer is true and refused as a launch error where it is
/// false. It answers what each pointer and its envelope asked, once every answer is given or its bound runs
/// out.
fn stand_in_host(data: &Path, answers: Vec<bool>) -> tokio::task::JoinHandle<Vec<(Value, Value)>> {
  let layout = Layout::new(data);
  let own = ProcessIdentity::own();

  private_dir(&layout.inbox()).unwrap();
  publish(&layout.claim(), &own).unwrap();

  tokio::task::spawn_blocking(move || {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut asked = vec![];

    while asked.len() < answers.len() && Instant::now() < deadline {
      let mut pointers: Vec<_> = std::fs::read_dir(layout.inbox())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_none_or(|staged| staged != "new"))
        .collect();

      pointers.sort();

      for pointer in pointers {
        let pointed: Value = serde_json::from_slice(&std::fs::read(&pointer).unwrap()).unwrap();
        let envelope = Path::new(pointed["envelope"].as_str().unwrap()).to_owned();
        let command: Value =
          serde_json::from_slice(&std::fs::read(envelope.join("command.json")).unwrap()).unwrap();

        std::fs::remove_file(&pointer).unwrap();

        if answers[asked.len()] {
          let pid = std::process::id() as i32;
          let started = Started {
            pid,
            birth: process_birth(pid).unwrap_or_default(),
            supervisor: own.clone(),
          };

          publish(&envelope.join("started.json"), &started).unwrap();
        } else {
          let refused = Outcome {
            exit_code: 1,
            cause: Cause::LaunchError,
          };

          publish(&envelope.join("outcome.json"), &refused).unwrap();
        }

        asked.push((pointed, command));
      }

      std::thread::sleep(Duration::from_millis(10));
    }

    asked
  })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_s_runner_is_left_under_its_unit_and_one_the_host_refused_closes_its_run() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let host = stand_in_host(&data, vec![true, false]);
  let mut ids = vec![];

  for unit in ["search-unit-1", "search-unit-2"] {
    let opening =
      json!({ "agent": "helper", "unit": unit, "depth": 0, "started_at": 4_000, "search_id": 40 });
    let (status, opened) = post(port, token, "/runs", opening).await;

    assert_eq!(status, 201, "{opened}");
    ids.push(opened["id"].as_i64().unwrap());
  }

  let launch = json!({
    "title": "search 40",
    "agent": "helper",
    "command": {
      "program": "/opt/runner",
      "args": ["search", "run", "40"],
      "cwd": "/srv/code",
      "environment": { "RUN_ID": "40" },
    },
  });
  let started = format!("/runs/{}", ids[0]);
  let refused = format!("/runs/{}", ids[1]);
  let mut first = launch.clone();
  first["runId"] = json!(ids[0]);
  let mut second = launch.clone();
  second["runId"] = json!(ids[1]);

  // A runner the host started leaves its run open for the runner to close, its execution rooted on the unit.
  assert_eq!(post(port, token, "/executions", first.clone()).await.0, 201);
  assert_eq!(get(port, token, &started).await.1["outcome"], Value::Null);
  assert_eq!(
    scratch
      .count(
        r#"SELECT count(*) FROM "agent_executions"
           WHERE "body"::jsonb ->> 'root' = 'search-unit-1' AND "body"::jsonb ->> 'state' = 'running'"#,
        vec![],
      )
      .await,
    1
  );

  // A runner the host did not start closes its run as an error and says why.
  let (status, said) = post(port, token, "/executions", second.clone()).await;

  assert_eq!(status, 503, "{said}");
  assert!(said["error"].as_str().unwrap().contains("launch-error"), "{said}");

  let (_, closed) = get(port, token, &refused).await;

  assert_eq!(closed["outcome"], "error");
  assert!(closed["endedAt"].as_i64().is_some(), "{closed}");

  let asked = host.await.unwrap();

  assert_eq!(asked.len(), 2);
  assert_eq!(asked[0].0["agent"], "helper");
  assert_eq!(asked[0].1["title"], "search 40");
  assert_eq!(
    asked[0].1["command"],
    json!({
      "args": ["/opt/runner", "search", "run", "40"],
      "cwd": "/srv/code",
      "env": { "RUN_ID": "40" },
      "stdin": null,
    })
  );

  // A closed run starts nothing, a run with no row is nothing held, and a launch with no program is refused.
  assert_eq!(post(port, token, "/executions", second).await.0, 409);
  let mut missing = launch.clone();
  missing["runId"] = json!(ids[1] + 1_000);
  assert_eq!(post(port, token, "/executions", missing).await.1["code"], "not-found");

  let mut nameless = first.clone();

  nameless["command"]["program"] = json!(" ");
  assert_eq!(post(port, token, "/executions", nameless).await.0, 400);
  assert_eq!(post(port, token, "/executions", json!({ "title": "t" })).await.0, 400);
}
