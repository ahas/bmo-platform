//! What a job's run asks of its repository's runtime, asked of a running server in the shapes the job unit
//! sends: the bench made ready with a job before's dev-server request taken back, the servers swept, the job
//! documents read with the trailer's marks filled and a support checkout's handoff instruction its
//! repository's, the files of a change that touch the prefix, and a session handed to another holder.

use serde_json::{Value, json};

use crate::support::rows::repository;
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

async fn patch(port: u16, token: &str, path: &str, body: Value) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .patch(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .json(&body)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

async fn delete(port: u16, token: &str, path: &str) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .delete(format!("http://127.0.0.1:{port}{path}"))
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

#[tokio::test(flavor = "multi_thread")]
async fn a_running_server_answers_what_a_job_s_run_asks_of_its_worker() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";
  let bench = data.join("workbench").join("helper");

  std::fs::create_dir_all(&bench).unwrap();
  std::fs::write(bench.join("dev-server.wanted"), "").unwrap();
  // A repository naming no prompt folder, and one whose row names a folder holding a handoff instruction —
  // found by the row's folder, not by the repository's name — with a support checkout.
  repository(scratch.db(), "helper", &home.path().join("helper").to_string_lossy(), &[]).await;
  repository(
    scratch.db(),
    "kept-apart",
    &home.path().join("supervisor").to_string_lossy(),
    &[("support_count", 1i64.into()), ("prompts", "pieces".into())],
  )
  .await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  // The bench made ready: the workbench stands, and a job before's request for its dev server is taken back.
  assert_eq!(patch(port, token, "/repositories/helper", json!({ "bench": true })).await.0, 200);
  assert!(bench.is_dir() && !bench.join("dev-server.wanted").exists());

  // Nothing listening is swept.
  assert_eq!(delete(port, token, "/repositories/helper/servers").await, (200, json!({ "lines": [] })));

  // The job documents: the trailer with its paths filled in, and a handoff
  // instruction only where the repository's folder holds one, a support checkout reading its repository's.
  let (status, documents) = get(port, token, "/repositories/helper/definition").await;
  let trailer = documents["trailer"].as_str().unwrap_or_default();

  assert_eq!(status, 200, "{documents}");
  assert!(documents.get("graph").is_none(), "no graph text is served: {documents}");
  assert!(!trailer.contains("{{"), "every mark of the trailer is filled: {trailer}");
  assert!(trailer.contains("plain-writing.md"), "{trailer}");
  assert_eq!(documents["handoff"], Value::Null);

  let (_, support) = get(port, token, "/repositories/kept-apart-w2/definition").await;

  assert!(support["handoff"].as_str().is_some_and(|text| !text.is_empty()), "{support}");

  // Which of a change's files a worker session's prefix is built from, in the order given.
  assert_eq!(
    post(
      port,
      token,
      "/prefix-checks",
      json!({ "workerName": "helper", "files": ["src/main.rs", "prompts/agents/worker/plugin/skills/a.md", "CLAUDE.md"] }),
    )
    .await,
    (200, json!({ "touched": ["prompts/agents/worker/plugin/skills/a.md", "CLAUDE.md"] }))
  );

  // There is no worker scope: a repository's `@worker` address is no session to hand over.
  let (status, session) =
    patch(port, token, "/sessions/helper@worker", json!({ "holder": "runner" })).await;

  assert_eq!(status, 404, "{session}");
}
