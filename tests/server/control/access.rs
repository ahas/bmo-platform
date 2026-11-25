//! Who may read or change agent management (docs/api.md, 「Who a caller acts for」): a member's five kinds of
//! route asked with the console's permission for the section of each, read and write apart, and the agent list
//! read under any of the five; the administrator and a caller carrying no key unchecked; and a permission that
//! could not be read refused, never guessed. A member's write reaches the rows of their own account alone, the
//! roles none, and work stands only on a repository of its holder's side.

use std::sync::{Arc, Mutex};

use agency_server::control::{Front, router};
use agency_server::places::Ground;
use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::support::grounds::seeded;
use crate::support::scratch::Scratch;

const TOKEN: &str = "secret";

/// The five kinds of route agent management is read and changed through, each with its console section.
const PARTS: [(&str, &str); 5] = [
  ("/supervisors", "supervisors"),
  ("/roles", "roles"),
  ("/repositories", "repositories"),
  ("/models", "models"),
  ("/model-catalogue", "model-files"),
];

/// The kinds the agent list is read from.
const AGENT_LIST: [&str; 3] = ["/supervisors", "/roles", "/repositories"];

/// One section a stub grants: its name, its read and its write.
type Grant = (&'static str, bool, bool);

#[derive(Clone)]
struct Stub {
  grants: Arc<Mutex<Vec<Grant>>>,
  status: StatusCode,
  asked: Arc<Mutex<usize>>,
}

async fn sections(
  State(stub): State<Stub>,
  Path(user): Path<String>,
  headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
  *stub.asked.lock().unwrap() += 1;

  assert_eq!(headers.get("owner-key").unwrap(), user.as_str());
  assert_eq!(headers.get("owner-role").unwrap(), "member");
  assert_eq!(headers.get("authorization").unwrap(), format!("Bearer {TOKEN}").as_str());

  // The retired `agents` section stands granted beside every answer, so a gate still reading it passes a call
  // the tests expect refused.
  let rows = [("live", true, true), ("agents", true, true)]
    .into_iter()
    .chain(stub.grants.lock().unwrap().iter().copied())
    .map(|(section, read, write)| json!({ "section": section, "read": read, "write": write }))
    .collect::<Vec<_>>();

  (stub.status, Json(json!({ "sections": rows })))
}

/// What a test holds of its account service: the sections it grants, which the test may change between calls,
/// and how often it was asked.
struct Granting {
  grants: Arc<Mutex<Vec<Grant>>>,
  asked: Arc<Mutex<usize>>,
}

impl Granting {
  fn grant(&self, grants: &[Grant]) {
    *self.grants.lock().unwrap() = grants.to_vec();
  }

  fn asked(&self) -> usize {
    *self.asked.lock().unwrap()
  }
}

// The account service on a loopback port of its own, answering one user's sections as the test says.
async fn account_service(grants: &[Grant], status: StatusCode) -> (String, Granting) {
  let granting = Granting {
    grants: Arc::new(Mutex::new(grants.to_vec())),
    asked: Arc::new(Mutex::new(0)),
  };
  let state = Stub {
    grants: granting.grants.clone(),
    status,
    asked: granting.asked.clone(),
  };
  let app = Router::new().route("/users/{user}/sections", get(sections)).with_state(state);
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  (format!("http://127.0.0.1:{port}"), granting)
}

// A ground seeded as the deployment seeds it, with the account service named in the data root's env file.
async fn ground(scratch: &Scratch, home: &std::path::Path, address: &str) -> Arc<Ground> {
  let served = seeded(scratch.db(), home).await;
  let env = home.join("data/env");

  std::fs::create_dir_all(&env).unwrap();
  std::fs::write(env.join("web.env"), format!("WEB_ADDRESS={address}\nWEB_TOKEN={TOKEN}\n")).unwrap();

  Arc::new(served)
}

async fn front(ground: Arc<Ground>) -> u16 {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let app = router(Front::new(TOKEN.to_owned(), ground).unwrap());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  port
}

async fn call(
  port: u16,
  method: reqwest::Method,
  path: &str,
  key: Option<&str>,
  role: Option<&str>,
) -> (u16, Value) {
  let mut request = reqwest::Client::new()
    .request(method, format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(TOKEN);

  if let Some(key) = key {
    request = request.header("Owner-Key", key);
  }

  if let Some(role) = role {
    request = request.header("Owner-Role", role);
  }

  let answer = request.send().await.unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

// The front on a fresh ground whose account service grants a member what the test says.
async fn granted(scratch: &Scratch, home: &std::path::Path, grants: &[Grant]) -> (u16, Granting) {
  let (address, granting) = account_service(grants, StatusCode::OK).await;

  (front(ground(scratch, home, &address).await).await, granting)
}

async fn member(port: u16, method: reqwest::Method, path: &str) -> (u16, Value) {
  call(port, method, path, Some("7"), Some("member")).await
}

#[tokio::test]
async fn a_member_reads_each_kind_under_its_own_section_and_the_models_under_no_other() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, granting) = granted(&scratch, home.path(), &[]).await;

  for (path, section) in PARTS {
    granting.grant(&[(section, true, false)]);

    let (status, body) = member(port, reqwest::Method::GET, path).await;

    assert_eq!(status, 200, "{section} reads {path}: {body}");

    for (other, _) in PARTS.into_iter().filter(|(other, _)| *other != path && !AGENT_LIST.contains(other)) {
      let (status, body) = member(port, reqwest::Method::GET, other).await;

      assert_eq!(status, 403, "{section} does not read {other}: {body}");
      assert!(body["error"].as_str().unwrap().contains("read"), "{body}");
    }
  }
}

#[tokio::test]
async fn the_agent_list_is_read_under_any_part_and_under_none_is_refused() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, granting) = granted(&scratch, home.path(), &[("model-files", true, false)]).await;

  for path in AGENT_LIST {
    let (status, body) = member(port, reqwest::Method::GET, path).await;

    assert_eq!(status, 200, "{path}: {body}");
  }

  granting.grant(&[("rooms", true, true)]);

  let (status, body) = member(port, reqwest::Method::GET, "/supervisors").await;

  assert_eq!(status, 403, "{body}");
  assert!(body["error"].as_str().unwrap().contains("any section of agent management"), "{body}");
}

#[tokio::test]
async fn a_change_needs_its_own_sections_write() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let mut grants: Vec<Grant> = PARTS.iter().map(|(_, section)| (*section, true, false)).collect();

  grants.push(("roles", true, true));

  let (port, granting) = granted(&scratch, home.path(), &grants).await;
  let (status, body) = member(port, reqwest::Method::POST, "/role-agents").await;

  assert_ne!(status, 403, "the roles write lets the change reach the route: {body}");

  for (path, section) in PARTS.into_iter().filter(|(path, _)| *path != "/roles") {
    let (status, body) = member(port, reqwest::Method::POST, path).await;

    assert_eq!(status, 403, "{path}: {body}");
    assert!(
      body["error"]
        .as_str()
        .unwrap()
        .contains(&format!("write access to the {section} section")),
      "{body}"
    );
  }

  assert_eq!(granting.asked(), PARTS.len());
}

#[tokio::test]
async fn who_owns_a_registration_is_not_a_member_s_to_set_even_with_the_models_write() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, _granting) = granted(&scratch, home.path(), &[("models", true, true)]).await;
  let answer = reqwest::Client::new()
    .patch(format!("http://127.0.0.1:{port}/models/registered::made"))
    .bearer_auth(TOKEN)
    .header("Owner-Key", "7")
    .header("Owner-Role", "member")
    .json(&json!({ "account": 7 }))
    .send()
    .await
    .unwrap();
  let status = answer.status().as_u16();
  let body: Value = answer.json().await.unwrap();

  assert_eq!((status, &body["code"]), (403, &json!("forbidden")), "{body}");
}

#[tokio::test]
async fn where_a_registration_is_served_is_not_a_member_s_to_set_even_with_the_models_write() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, _granting) = granted(&scratch, home.path(), &[("models", true, true)]).await;
  let answer = reqwest::Client::new()
    .put(format!("http://127.0.0.1:{port}/models/registered::made/addresses/n1"))
    .bearer_auth(TOKEN)
    .header("Owner-Key", "7")
    .header("Owner-Role", "member")
    .json(&json!({ "url": "http://[::1]:9/v1/chat/completions", "keyFile": "made-key" }))
    .send()
    .await
    .unwrap();
  let status = answer.status().as_u16();
  let body: Value = answer.json().await.unwrap();

  assert_eq!((status, &body["code"]), (403, &json!("forbidden")), "{body}");
}

#[tokio::test]
async fn the_administrator_and_a_caller_carrying_no_key_are_not_checked() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, granting) = granted(&scratch, home.path(), &[]).await;

  let (administrator, _) =
    call(port, reqwest::Method::POST, "/roles", Some("7"), Some("administrator")).await;
  let (nobody, _) = call(port, reqwest::Method::POST, "/roles", None, None).await;

  assert_ne!(administrator, 403, "the owner keeps every access");
  assert_ne!(nobody, 403, "a unit's own call keeps the route's present behaviour");
  assert_eq!(granting.asked(), 0, "neither is asked of the account service");
}

#[tokio::test]
async fn a_permission_that_cannot_be_read_is_no_access() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (address, _) = account_service(&[], StatusCode::INTERNAL_SERVER_ERROR).await;
  let port = front(ground(&scratch, home.path(), &address).await).await;
  let (status, body) = member(port, reqwest::Method::GET, "/supervisors").await;

  assert_eq!(status, 503, "{body}");
  assert_eq!(body["code"], "access-unchecked");
}

/// A member's write of every part a test of rows changes.
const ROW_WRITES: [Grant; 3] =
  [("supervisors", true, true), ("repositories", true, true), ("roles", true, true)];

/// The rows of three sides a test of ownership changes: member 7's, member 8's and the platform's, each a
/// supervisor, a repository and a role agent of the worker role.
async fn rows(scratch: &Scratch, home: &std::path::Path) {
  for (side, account) in [("seven", Some(7i64)), ("eight", Some(8)), ("platform", None)] {
    scratch
      .run(
        r#"INSERT INTO "supervisors" ("name", "label", "session_scope", "turn_shape", "owner_account",
             "created_at", "updated_at")
           VALUES ($1, $1, 'room', 'pieces', $2, 0, 0)"#,
        vec![format!("sup-{side}").into(), account.into()],
      )
      .await;
    scratch
      .run(
        r#"INSERT INTO "repositories" ("name", "path", "owner_account", "created_at", "updated_at")
           VALUES ($1, $2, $3, 0, 0)"#,
        vec![
          format!("repo-{side}").into(),
          home.join(format!("repo-{side}")).to_string_lossy().into_owned().into(),
          account.into(),
        ],
      )
      .await;
    scratch
      .run(
        r#"INSERT INTO "role_agents" ("name", "role", "owner_account") VALUES ($1, 'worker', $2)"#,
        vec![format!("worker-{side}").into(), account.into()],
      )
      .await;
  }
}

async fn send(
  port: u16,
  method: reqwest::Method,
  path: &str,
  key: Option<&str>,
  role: Option<&str>,
  body: Value,
) -> (u16, Value) {
  let mut request = reqwest::Client::new()
    .request(method, format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(TOKEN)
    .json(&body);

  if let Some(key) = key {
    request = request.header("Owner-Key", key);
  }

  if let Some(role) = role {
    request = request.header("Owner-Role", role);
  }

  let answer = request.send().await.unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

/// A harmless change of each kind of row a member may own, by the path that names it.
fn changes(side: &str) -> [(String, Value); 3] {
  [
    (format!("/supervisors/sup-{side}"), json!({ "label": "Renamed" })),
    (format!("/repositories/repo-{side}"), json!({ "label": "Renamed" })),
    (format!("/role-agents/worker-{side}"), json!({ "enabled": false })),
  ]
}

#[tokio::test]
async fn a_member_changes_the_supervisors_repositories_and_role_agents_of_their_own_account_alone() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, _granting) = granted(&scratch, home.path(), &ROW_WRITES).await;

  rows(&scratch, home.path()).await;

  for (path, body) in changes("seven") {
    let (status, answer) = send(port, reqwest::Method::PATCH, &path, Some("7"), Some("member"), body).await;

    assert_eq!(status, 200, "member 7 changes its own {path}: {answer}");
  }

  for (side, whose) in [("eight", "another account"), ("platform", "the platform's")] {
    for (path, body) in changes(side) {
      let (status, answer) =
        send(port, reqwest::Method::PATCH, &path, Some("7"), Some("member"), body).await;

      assert_eq!((status, &answer["code"]), (403, &json!("forbidden")), "{path}: {answer}");
      assert!(answer["error"].as_str().unwrap().contains(whose), "{path}: {answer}");
    }
  }

  for path in ["/supervisors/sup-eight", "/repositories/repo-platform", "/role-agents/worker-eight"] {
    let (status, answer) = member(port, reqwest::Method::DELETE, path).await;

    assert_eq!(status, 403, "{path} is not removed: {answer}");
  }

  let left = scratch
    .count(
      r#"SELECT COUNT(*) FROM "supervisors" WHERE "name" = 'sup-eight' AND "label" = 'sup-eight'"#,
      vec![],
    )
    .await;

  assert_eq!(left, 1, "another account's supervisor stands as it stood");
}

#[tokio::test]
async fn the_platform_s_rows_and_every_account_s_are_changed_by_the_administrator_and_the_keyless_caller() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, granting) = granted(&scratch, home.path(), &[]).await;

  rows(&scratch, home.path()).await;

  for side in ["platform", "eight"] {
    for (path, body) in changes(side) {
      let (administrator, answer) =
        send(port, reqwest::Method::PATCH, &path, Some("7"), Some("administrator"), body.clone()).await;

      assert_eq!(administrator, 200, "the administrator changes {path}: {answer}");

      let (keyless, answer) = send(port, reqwest::Method::PATCH, &path, None, None, body).await;

      assert_eq!(keyless, 200, "the deployment's own call changes {path}: {answer}");
    }
  }

  assert_eq!(granting.asked(), 0, "neither is asked of the account service");
}

#[tokio::test]
async fn a_member_adds_under_their_own_account_and_never_under_another_s() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, _granting) = granted(&scratch, home.path(), &ROW_WRITES).await;
  let as_member = |path: &'static str, body: Value| {
    send(port, reqwest::Method::POST, path, Some("7"), Some("member"), body)
  };

  let (status, agent) = as_member("/role-agents", json!({ "role": "worker", "name": "worker-mine" })).await;

  assert_eq!((status, &agent["ownerAccount"]), (201, &json!(7)), "{agent}");

  let path = home.path().join("mine").to_string_lossy().into_owned();
  let (status, repository) = as_member("/repositories", json!({ "name": "mine", "path": path })).await;

  assert_eq!((status, &repository["ownerAccount"]), (201, &json!(7)), "{repository}");

  for (path, body) in [
    ("/role-agents", json!({ "role": "worker", "name": "worker-theirs", "ownerAccount": 8 })),
    ("/repositories", json!({ "name": "theirs", "path": path, "ownerAccount": 8 })),
    ("/supervisors", json!({ "name": "theirs", "label": "Theirs", "ownerAccount": 8 })),
  ] {
    let (status, answer) = as_member(path, body).await;

    assert_eq!((status, &answer["code"]), (403, &json!("forbidden")), "{path}: {answer}");
    assert!(answer["error"].as_str().unwrap().contains("under another account"), "{path}: {answer}");
  }

  // A member's change cannot move its own agent to another account.
  let (status, answer) = send(
    port,
    reqwest::Method::PATCH,
    "/role-agents/worker-mine",
    Some("7"),
    Some("member"),
    json!({ "ownerAccount": "8" }),
  )
  .await;

  assert_eq!(status, 403, "{answer}");
}

#[tokio::test]
async fn the_roles_are_the_platform_s_and_a_role_s_agents_stand_under_the_roles_section() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, granting) = granted(&scratch, home.path(), &ROW_WRITES).await;

  rows(&scratch, home.path()).await;

  for (method, path) in [
    (reqwest::Method::POST, "/roles"),
    (reqwest::Method::PATCH, "/roles/worker"),
    (reqwest::Method::DELETE, "/roles/worker"),
  ] {
    let (status, answer) = member(port, method, path).await;

    assert_eq!((status, &answer["code"]), (403, &json!("forbidden")), "{path}: {answer}");
    assert!(answer["error"].as_str().unwrap().contains("platform's presets"), "{path}: {answer}");
  }

  let (administrator, answer) = send(
    port,
    reqwest::Method::PATCH,
    "/roles/worker",
    Some("7"),
    Some("administrator"),
    json!({ "description": "Works an item." }),
  )
  .await;

  assert_eq!(administrator, 200, "{answer}");

  // Without the roles write, a member changes no agent of a role, even their own.
  granting.grant(&[("supervisors", true, true), ("roles", true, false)]);

  let (status, answer) = send(
    port,
    reqwest::Method::PATCH,
    "/role-agents/worker-seven",
    Some("7"),
    Some("member"),
    json!({ "enabled": false }),
  )
  .await;

  assert_eq!(status, 403, "{answer}");
  assert!(
    answer["error"].as_str().unwrap().contains("write access to the roles section"),
    "{answer}"
  );

  let (status, answer) = member(port, reqwest::Method::GET, "/role-agents").await;

  assert_eq!(status, 200, "the agents are read under the roles read: {answer}");
}

#[tokio::test]
async fn work_stands_only_on_a_repository_of_its_holder_s_side() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, _granting) = granted(&scratch, home.path(), &[]).await;

  rows(&scratch, home.path()).await;

  let assign = |id: i64, holder: &str, assignee: &str| json!({ "itemId": id, "holderName": holder, "assigneeName": assignee, "item": { "title": "t", "labels": {} } });
  let ask = |body: Value| send(port, reqwest::Method::POST, "/assignments", None, None, body);

  for (id, holder, repository) in [
    (71, "sup-eight", "repo-seven"),
    (72, "sup-seven", "repo-platform"),
    (73, "sup-platform", "repo-seven"),
    (74, "worker-eight", "repo-seven"),
  ] {
    let (status, answer) = ask(assign(id, holder, repository)).await;

    assert_eq!(
      (status, &answer["code"]),
      (403, &json!("foreign-repository")),
      "{holder} on {repository}: {answer}"
    );
  }

  let (status, own) = ask(assign(75, "sup-seven", "repo-seven")).await;

  assert_eq!(status, 201, "{own}");

  let (status, answer) = ask(assign(76, "sup-platform", "repo-platform")).await;

  assert_eq!(status, 201, "the platform's supervisor on the platform's repository: {answer}");

  // A launch lending another account's repository beside its own is refused, nothing written.
  let item =
    json!({ "title": "t", "labels": { "agent": "worker-seven", "repositories": "repo-seven,repo-eight" } });
  let (status, answer) = send(
    port,
    reqwest::Method::POST,
    "/jobs",
    None,
    None,
    json!({ "assignmentId": own["id"], "itemId": 75, "leaseId": 1, "holderName": "sup-seven", "item": item }),
  )
  .await;

  assert_eq!((status, &answer["code"]), (403, &json!("foreign-repository")), "{answer}");
  assert_eq!(scratch.count(r#"SELECT COUNT(*) FROM "jobs""#, vec![]).await, 0);

  // A supervisor carrying a turn files no item on another account's repository.
  agency_server::turn_marks::carrying(
    "sup-eight",
    "run-foreign",
    agency_server::turn_marks::Mark {
      chat_id: -8,
      privileged: true,
    },
  );

  let filing = |repository: &str| json!({ "agentName": "sup-eight", "title": "t", "brief": "b", "queueName": "todo", "repository": repository });
  let (status, answer) =
    send(port, reqwest::Method::POST, "/item-filings", None, None, filing("repo-seven")).await;

  assert_eq!((status, &answer["code"]), (403, &json!("foreign-repository")), "{answer}");

  let (status, answer) =
    send(port, reqwest::Method::POST, "/item-filings", None, None, filing("repo-eight")).await;

  assert_ne!(
    answer["code"], "foreign-repository",
    "its own repository passes the check: {status} {answer}"
  );

  agency_server::turn_marks::ended("sup-eight", "run-foreign");
}

#[tokio::test]
async fn a_unit_s_repository_asks_no_section_and_is_judged_by_the_chain() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  // The member holds the repositories' read and no write.
  let (port, granting) = granted(&scratch, home.path(), &[("repositories", true, false)]).await;

  scratch
    .run(
      r#"INSERT INTO "repositories" ("name", "path", "owner_unit", "created_at", "updated_at")
         VALUES ('repo-unit', $1, 40, 0, 0)"#,
      vec![home.path().join("repo-unit").to_string_lossy().into_owned().into()],
    )
    .await;
  rows(&scratch, home.path()).await;

  // A unit's repository, changed or added under the unit, goes past the section to the chain's check, which this
  // stand-in account service answers no head of the unit.
  for (method, path, body) in [
    (reqwest::Method::PATCH, "/repositories/repo-unit", json!({ "label": "Renamed" })),
    (
      reqwest::Method::POST,
      "/repositories",
      json!({ "name": "added-unit", "path": "/nowhere", "ownerUnit": 40 }),
    ),
  ] {
    let (status, answer) = send(port, method.clone(), path, Some("7"), Some("member"), body).await;
    let said = answer["error"].as_str().unwrap_or_default();

    assert!(!said.contains("section"), "{method} {path}: {status} {answer}");
    assert!(
      ["access-unchecked", "forbidden"].contains(&answer["code"].as_str().unwrap_or_default()),
      "{method} {path}: {status} {answer}"
    );
  }

  // The member's own repository and an add under no unit still ask the section's write.
  for (method, path, body) in [
    (reqwest::Method::PATCH, "/repositories/repo-seven", json!({ "label": "Renamed" })),
    (
      reqwest::Method::POST,
      "/repositories",
      json!({ "name": "added-own", "path": "/nowhere" }),
    ),
  ] {
    let (status, answer) = send(port, method.clone(), path, Some("7"), Some("member"), body).await;

    assert_eq!(status, 403, "{method} {path}: {answer}");
    assert!(
      answer["error"].as_str().unwrap().contains("write access to the repositories section"),
      "{answer}"
    );
  }

  assert_eq!(granting.asked(), 2);
}
