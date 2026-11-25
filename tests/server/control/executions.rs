//! The executions asked of a running server in the shapes the job unit reads them: a run tree listed whole
//! and open only, one read by its id, each covered by the cancellation standing over the tree's top, and that
//! cancellation taken back on its own.

use reqwest::Method;
use serde_json::{Value, json};

use crate::support::scratch::Scratch;
use crate::support::standing;

async fn ask(port: u16, token: &str, method: Method, path: &str) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .request(method, format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

fn ids(listed: &Value) -> Vec<&str> {
  listed
    .as_array()
    .unwrap()
    .iter()
    .map(|execution| execution["id"].as_str().unwrap())
    .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_server_lists_reads_and_takes_back_the_cancels_of_a_run_tree() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let rows = [
    json!({ "id": "exec-top", "parent": "unit-a", "root": "place-a", "line": "pane", "endedAt": null }),
    json!({
      "id": "exec-child", "parent": "exec-top", "root": "place-a", "line": "native-child",
      "sharedPane": true, "endedAt": null,
    }),
    json!({
      "id": "exec-done", "parent": "exec-child", "root": "place-a", "line": "native-child",
      "startedAt": 1000, "endedAt": 2000, "outcome": { "state": "done" }, "provider": "codex",
    }),
    json!({ "id": "exec-other", "parent": "unit-b", "root": "place-b", "endedAt": null, "outcome": "cut" }),
  ];

  for row in &rows {
    scratch
      .run(
        r#"INSERT INTO "agent_executions" ("id", "body") VALUES ($1, $2)"#,
        vec![row["id"].as_str().unwrap().into(), row.to_string().into()],
      )
      .await;
  }

  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('execution_cancel:exec-top', '1'),
         ('execution_cancel:exec-child', '1')"#,
      vec![],
    )
    .await;

  // Listed the latest first, open only where asked; the tree under the cancelled top reads as cancelled.
  let (status, open) = ask(port, token, Method::GET, "/executions?open=true").await;

  assert_eq!(
    (status, ids(&open["items"])),
    (200, vec!["exec-other", "exec-child", "exec-top"]),
    "{open}"
  );

  let (status, every) = ask(port, token, Method::GET, "/executions?open=false").await;

  assert_eq!(status, 200, "{every}");
  assert_eq!(ids(&every["items"]), ["exec-other", "exec-done", "exec-child", "exec-top"]);
  assert_eq!(
    every["items"][1],
    json!({
      "id": "exec-done", "parent": "exec-child", "root": "place-a", "line": "native-child",
      "sharedPane": false, "startedAt": 1000, "endedAt": 2000, "outcome": { "state": "done" },
      "cancelled": true, "details": { "provider": "codex" },
    })
  );
  assert_eq!(
    every["items"][0],
    json!({
      "id": "exec-other", "parent": "unit-b", "root": "place-b", "line": null, "sharedPane": false,
      "startedAt": null, "endedAt": null, "outcome": null, "cancelled": false,
      "details": { "outcome": "cut" },
    })
  );

  // One by its id, and nothing held for an id no row has.
  let (status, child) = ask(port, token, Method::GET, "/executions/exec-child").await;

  assert_eq!(
    (status, &child["sharedPane"], &child["cancelled"], &child["details"]),
    (200, &json!(true), &json!(true), &json!({})),
    "{child}"
  );

  let (status, absent) = ask(port, token, Method::GET, "/executions/nobody").await;

  assert_eq!((status, absent["code"].as_str()), (404, Some("not-found")), "{absent}");

  // The top's cancellations taken back with those of everything it owns, and none left to take again.
  let (status, taken) = ask(port, token, Method::PATCH, "/executions/exec-top").await;
  assert_eq!((status, &taken["dropped"]), (200, &json!(["exec-child", "exec-top"])));
  assert_eq!(
    ask(port, token, Method::GET, "/executions/exec-done").await.1["cancelled"],
    json!(false)
  );
  let (status, taken) = ask(port, token, Method::PATCH, "/executions/exec-top").await;
  assert_eq!((status, &taken["dropped"]), (200, &json!([])));
}
