//! A support checkout grown at a launch (docs/worker-slots.md, 「Growing a checkout at a launch」): a repository set
//! to `auto` whose checkouts are all held grows the next one for the item, while an agent of the item's is left
//! free and the machine has room, and is refused once either runs out; the job's launch makes its worktree.

use agency_server::places::checkouts::ensure_support_checkout;
use serde_json::{Value, json};

use crate::support::git::{at, git};
use crate::support::grounds::ground_on;
use crate::support::rows::{repository, seats};
use crate::support::scratch::Scratch;
use crate::support::standing;

const TOKEN: &str = "secret";

async fn assign(port: u16, item_id: i64) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .post(format!("http://127.0.0.1:{port}/assignments"))
    .bearer_auth(TOKEN)
    .json(&json!({
      "itemId": item_id,
      "holderName": "harness",
      "assigneeName": "helper",
      "item": {
        "title": "A harmless item",
        "brief": "Answer that nothing is to be done.",
        "notes": [],
        "labels": { "model": "claude-opus-5", "role": "worker" },
      },
    }))
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

// A repository `helper` set to `auto` at a path under `home`, and three agents of the worker role on the item's
// model.
async fn helper(scratch: &Scratch, home: &std::path::Path) {
  repository(scratch.db(), "helper", &home.join("helper").to_string_lossy(), &[]).await;
  seats(
    scratch.db(),
    "worker",
    &[
      ("claude-code", "claude-opus-5"),
      ("claude-code", "claude-opus-5"),
      ("claude-code", "claude-opus-5"),
    ],
  )
  .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn every_checkout_held_grows_the_next_while_an_agent_is_left_free() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");

  std::fs::create_dir_all(&data).unwrap();
  helper(&scratch, home.path()).await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), TOKEN);
  let mut taken = vec![];

  for item in [61, 62, 63] {
    let (status, held) = assign(server.port, item).await;

    assert_eq!(status, 201, "{held}");
    taken.push(held["workerName"].as_str().unwrap_or_default().to_owned());
  }

  assert_eq!(taken, ["helper", "helper-w2", "helper-w3"]);

  // Three agents, each spoken for by a hold no job has started on yet: the fourth launch waits for one.
  let (status, refused) = assign(server.port, 64).await;

  assert_eq!((status, refused["code"].as_str()), (409, Some("agent-busy")), "{refused}");
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "support_checkouts" WHERE "repository" = 'helper'"#, vec![])
      .await,
    2,
    "a refused launch records no checkout"
  );
}

// A grown checkout is only a record until its job launches: the launch makes the worktree at the slot's path,
// detached at the base checkout's main, before the job's program is stood in it.
#[tokio::test(flavor = "multi_thread")]
async fn a_grown_checkout_is_made_a_worktree_of_main_when_its_job_launches() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let base = home.path().join("helper");

  std::fs::create_dir_all(&data).unwrap();
  git(&["init", "-q", "-b", "main", &at(&base)]);
  git(&["-C", &at(&base), "commit", "-q", "--allow-empty", "-m", "start"]);
  helper(&scratch, home.path()).await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), TOKEN);

  assign(server.port, 81).await;

  let (status, grown) = assign(server.port, 82).await;
  let grown = grown["workerName"].as_str().unwrap_or_default().to_owned();
  let slot = home.path().join(&grown);

  assert_eq!(status, 201, "{grown}");
  assert!(!slot.exists(), "the grown checkout waits for its launch");

  ensure_support_checkout(&ground_on(scratch.db().clone(), home.path()), &grown)
    .await
    .unwrap();

  assert_eq!(
    git(&["-C", &at(&slot), "rev-parse", "HEAD"]),
    git(&["-C", &at(&base), "rev-parse", "main"])
  );
  assert_eq!(git(&["-C", &at(&slot), "branch", "--show-current"]), "", "detached");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_machine_with_no_room_grows_nothing_and_says_why() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");

  std::fs::create_dir_all(&data).unwrap();
  helper(&scratch, home.path()).await;
  // One job at work here, and the owner's count of one.
  scratch
    .run(
      r#"INSERT INTO "agent_executions" ("id", "body") VALUES ('e1', $1)"#,
      vec![json!({ "id": "e1", "agent": "helper", "endedAt": null }).to_string().into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('machine_limit:launch_jobs', '1')"#,
      vec![],
    )
    .await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), TOKEN);
  let (status, held) = assign(server.port, 71).await;

  assert_eq!((status, held["workerName"].as_str()), (201, Some("helper")), "{held}");

  let (status, refused) = assign(server.port, 72).await;

  assert_eq!((status, refused["code"].as_str()), (422, Some("no-worker-free")), "{refused}");
  assert!(refused["error"].as_str().unwrap_or_default().contains("jobs 1 >= 1"), "{refused}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_routes_answer_and_take_auto_or_a_count_from_zero_to_the_most() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");

  std::fs::create_dir_all(&data).unwrap();
  helper(&scratch, home.path()).await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), TOKEN);
  let change = async |count: Value| {
    let answer = reqwest::Client::new()
      .patch(format!("http://127.0.0.1:{}/repositories/helper", server.port))
      .bearer_auth(TOKEN)
      .json(&json!({ "supportCount": count }))
      .send()
      .await
      .unwrap();

    (answer.status().as_u16(), answer.json::<Value>().await.unwrap_or(Value::Null))
  };

  for (asked, status, answered) in [
    (json!(3), 200, json!(3)),
    (json!("auto"), 200, json!("auto")),
    (json!(9), 400, json!("auto")),
    (json!("many"), 400, json!("auto")),
  ] {
    let (got, body) = change(asked.clone()).await;

    assert_eq!(got, status, "{asked}: {body}");

    let read = reqwest::Client::new()
      .get(format!("http://127.0.0.1:{}/repositories/helper", server.port))
      .bearer_auth(TOKEN)
      .send()
      .await
      .unwrap()
      .json::<Value>()
      .await
      .unwrap();

    assert_eq!(read["supportCount"], answered, "{asked}");
  }
}
