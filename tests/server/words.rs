//! Words said to a run or a repository's worker (docs/agent-control.md, 「Words that arrive while a turn is
//! working」 and 「Words said to a worker」): the body's bounds, who may say, a run that has ended, a run at
//! rest, a repository no store holds, and a checkout another turn is holding; and what a job's report makes
//! of the words its run's log kept.

use serde_json::{Value, json};

use agency_server::words;
use agency_server::worker_session::held::scope_of;

use crate::support::rows::repository;
use crate::support::scratch::Scratch;
use crate::support::standing;

async fn call(
  port: u16,
  token: &str,
  method: reqwest::Method,
  path: &str,
  body: Option<Value>,
) -> (u16, Value) {
  let mut request = reqwest::Client::new()
    .request(method, format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token);

  if let Some(body) = body {
    request = request.json(&body);
  }

  let answer = request.send().await.unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

async fn get(port: u16, token: &str, path: &str) -> (u16, Value) {
  call(port, token, reqwest::Method::GET, path, None).await
}

async fn post(port: u16, token: &str, path: &str, body: Value) -> (u16, Value) {
  call(port, token, reqwest::Method::POST, path, Some(body)).await
}

#[test]
fn a_word_is_one_text_or_one_file_at_least_and_within_the_policys_bounds() {
  let owner = words::asked(json!({ "words": "  more  " })).unwrap();

  assert_eq!((words::sender(&owner).as_str(), owner.words.as_str()), ("owner", "  more  "));
  assert_eq!(words::sender(&words::asked(json!({ "text": "x", "by": " sup " })).unwrap()), "sup");
  assert!(words::asked(json!({ "words": "   " })).is_err());
  assert!(words::asked(json!({ "words": "x", "files": [{ "name": "a.png", "data": "AAAA" }] })).is_ok());
  assert!(words::asked(json!({ "files": [{ "name": "a.png", "data": "AAAA" }] })).is_ok());
  assert!(words::asked(json!({ "files": [{ "name": "  ", "data": "AAAA" }] })).is_err());

  let many = json!({
    "files": (0..11).map(|at| json!({ "name": format!("a{at}"), "data": "AAAA" })).collect::<Vec<_>>(),
  });

  assert!(words::asked(many).is_err(), "a word carries the policy's count of files at most");
}

#[tokio::test]
async fn a_jobs_report_carries_the_words_said_into_its_turn_and_what_became_of_them() {
  let scratch = Scratch::new().await;

  for (kind, text) in [
    ("prompt", "run the tests before you report"),
    ("note", "The turn took the words."),
    ("prompt", "and say what changed"),
    ("note", "The turn ended without taking the words."),
  ] {
    scratch
      .run(
        r#"INSERT INTO "agent_log" ("agent", "run", "at", "kind", "name", "text", "job_id")
           VALUES ('checkout', 'job-7', 1, $1, 'word', $2, 7)"#,
        vec![kind.into(), text.into()],
      )
      .await;
  }

  let report = words::report(scratch.db(), 7).await.unwrap().unwrap();

  assert!(
    report.contains("run the tests before you report\n  The turn took the words.")
      && report.contains("and say what changed\n  The turn ended without taking the words."),
    "{report}"
  );
  assert!(
    words::with_words(scratch.db(), 7, Some("the answer".into()))
      .await
      .unwrap()
      .unwrap()
      .starts_with("the answer\n\nWords said into this job's turn")
  );
  assert_eq!(words::with_words(scratch.db(), 8, None).await.unwrap(), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_runs_session_takes_words_from_the_owner_and_the_supervisor_that_asked_for_it_alone() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  // The helper's agents stand on Claude Code, the road the run below is asked on.
  crate::support::rows::seats(scratch.db(), "helper", &[("claude-code", "claude-haiku-4-5-20251001")])
    .await;

  let (_, roles) = get(port, token, "/roles/helper").await;
  let entry = roles["agents"]
    .as_array()
    .unwrap()
    .iter()
    .find(|entry| entry["provider"] == "claude-code")
    .cloned()
    .unwrap_or_else(|| panic!("the helper role holds no agent on claude-code: {roles}"));

  // A slot let in, asked for by a supervisor, and a role's session started on it.
  let (status, slot) = post(
    port,
    token,
    "/slot-requests",
    json!({
      "role": "helper",
      "requestKey": "words:run",
      "owner": "words-run",
      "askedBy": "supervisor-a",
      "provider": entry["provider"],
      "model": entry["model"],
    }),
  )
  .await;

  assert_eq!(status, 201, "{slot}");

  let seat = slot["token"].as_str().unwrap().to_owned();
  let (status, admission) =
    call(port, token, reqwest::Method::PATCH, &format!("/slot-requests/{seat}"), Some(json!({}))).await;

  assert_eq!((status, admission["state"].as_str()), (200, Some("active")), "{admission}");

  let (status, started) = post(
    port,
    token,
    "/sessions",
    json!({ "scope": "role", "agentName": "helper", "slot": seat }),
  )
  .await;

  assert_eq!(status, 201, "{started}");

  let session = started["id"].as_str().unwrap().to_owned();
  let path = format!("/sessions/{session}/words");

  // A supervisor that did not ask for the run may not say to it; the one that did, and the owner, may.
  assert_eq!(
    post(port, token, &path, json!({ "words": "more", "by": "supervisor-b" })).await.1["code"],
    json!("not-the-sender")
  );
  assert_eq!(
    post(port, token, &path, json!({ "words": "more", "by": "supervisor-a" })).await.1["code"],
    json!("no-turn")
  );
  assert_eq!(post(port, token, &path, json!({ "words": "more" })).await.1["code"], json!("no-turn"));

  // An empty word is refused before the run is read at all.
  assert_eq!(post(port, token, &path, json!({ "words": " " })).await.0, 400);

  // Once the slot is given back the run has ended, and a word said to it reaches no turn.
  call(port, token, reqwest::Method::DELETE, &format!("/sessions/{session}"), None).await;

  while get(port, token, &format!("/sessions/{session}")).await.0 != 404 {
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
  }

  assert_eq!(post(port, token, &path, json!({ "words": "more" })).await.1["code"], json!("run-ended"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_repository_s_words_are_the_owners_and_wait_while_another_turn_holds_the_worker() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";
  let checkout = home.path().join("helper");

  std::fs::create_dir_all(&data).unwrap();
  std::fs::create_dir_all(&checkout).unwrap();
  repository(scratch.db(), "helper", &checkout.to_string_lossy(), &[]).await;

  // Another turn at the worker holds the lock its own word's turn would take, as a job's turn does.
  let held = agency_server::places::locks::try_hold(&agency_server::places::locks::agent_lock_path(
    &data,
    &scope_of("helper"),
  ))
  .expect("the worker's lock is free");

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let path = "/repositories/helper/words";

  // The worker's agents stand on Claude Code, the road a word's own turn at the worker stands on.
  crate::support::rows::seats(scratch.db(), "worker", &[("claude-code", "claude-opus-5")]).await;

  assert_eq!(
    post(port, token, "/repositories/nobody/words", json!({ "words": "more" })).await.1["code"],
    json!("unknown-repository")
  );
  assert_eq!(
    post(port, token, path, json!({ "words": "more", "by": "supervisor-a" })).await.1["code"],
    json!("not-the-sender")
  );

  let (status, said) = post(port, token, path, json!({ "words": "more" })).await;

  assert_eq!(status, 201, "{said}");
  assert_eq!(said["said"], json!("refused"), "{said}");
  assert!(
    said["reason"].as_str().unwrap_or_default().contains("another turn works at helper"),
    "{said}"
  );

  drop(held);
}
