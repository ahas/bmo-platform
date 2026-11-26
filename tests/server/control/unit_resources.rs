//! Who reaches the rows of agent management filed under an organisation unit (docs/api.md, 「Who a caller
//! acts for」, 「settings」 and 「runs」): each list answers a member the rows of their own account and those
//! of the units they head, heading read off the chain's `agency.unit-resources.manage` link asked in the
//! caller's name; the owner parameters narrow a list; a single row and a run stand as their agent stands; a
//! head files rows under the unit and a stranger is refused; agency's own settings read and write the chain's
//! platform link; and a provider the chain turns off below a person closes a run's supply walk before any
//! source of it is walked.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use agency_server::chain_settings;
use agency_server::control::{Front, router};
use agency_server::places::Ground;
use agency_server::supply::{self, Chosen, PERSONAL, Shortfall, Source};
use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, put};
use policy::organisation::ORGANISATION;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::support::grounds::{ground_on, seeded};
use crate::support::scratch::Scratch;

const TOKEN: &str = "secret";

/// The signed-in person every member call names.
const MEMBER: i64 = 7;

/// One section the stub grants: its name, its read and its write.
type Grant = (&'static str, bool, bool);

/// A value written onto a link through the stub, as the stub recorded the call.
#[derive(Clone, Debug, PartialEq)]
struct Recorded {
  kind: String,
  link: String,
  caller: String,
  value: Value,
}

/// The account service of a test: the console's sections, the chain's entries and values — a unit's entry
/// answered writable to the pairs it heads alone, and the platform's entry whole, with whatever the last
/// write left on it — the resolutions it is told, and the subtree of a unit it is told.
#[derive(Clone, Default)]
struct Stub {
  sections: Arc<Mutex<Vec<Grant>>>,
  heads: Arc<Mutex<Vec<(i64, i64)>>>,
  subtree: Arc<Mutex<Vec<i64>>>,
  resolved: Arc<Mutex<Vec<(String, Value)>>>,
  values: Arc<Mutex<Vec<(String, String, Value)>>>,
  writes: Arc<Mutex<Vec<Recorded>>>,
}

impl Stub {
  fn grant(&self, grants: &[Grant]) {
    *self.sections.lock().unwrap() = grants.to_vec();
  }

  /// The pairs the chain answers a unit's link to: who heads which unit.
  fn head(&self, heads: &[(i64, i64)]) {
    *self.heads.lock().unwrap() = heads.to_vec();
  }

  /// The units standing under the unit a subtree is asked for.
  fn under(&self, units: &[i64]) {
    *self.subtree.lock().unwrap() = units.to_vec();
  }

  fn resolve_to(&self, kind: &str, value: Value) {
    let mut held = self.resolved.lock().unwrap();
    held.retain(|(key, _)| key != kind);
    held.push((kind.to_owned(), value));
  }

  fn writes(&self) -> Vec<Recorded> {
    self.writes.lock().unwrap().clone()
  }
}

// The person a chain call was made in the name of.
fn caller(headers: &HeaderMap) -> i64 {
  headers
    .get("owner-key")
    .and_then(|value| value.to_str().ok())
    .and_then(|text| text.trim().parse().ok())
    .unwrap_or_default()
}

async fn sections(
  State(stub): State<Stub>,
  Path(user): Path<String>,
  headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
  assert_eq!(headers.get("owner-key").unwrap(), user.as_str());
  assert_eq!(headers.get("owner-role").unwrap(), "member");
  assert_eq!(headers.get("authorization").unwrap(), format!("Bearer {TOKEN}").as_str());

  let rows = stub
    .sections
    .lock()
    .unwrap()
    .iter()
    .map(|(section, read, write)| json!({ "section": section, "read": read, "write": write }))
    .collect::<Vec<_>>();

  (StatusCode::OK, Json(json!({ "sections": rows })))
}

// The chain answers a unit's entry to its writers alone — to the heads, as the test names them — and the
// platform's entry to anyone, with the value last written onto it.
async fn entry(
  State(stub): State<Stub>,
  Path((kind, link)): Path<(String, String)>,
  headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
  if let Some(unit) = link.strip_prefix("unit:") {
    let unit: i64 = unit.parse().unwrap_or_default();

    return if stub.heads.lock().unwrap().contains(&(caller(&headers), unit)) {
      (StatusCode::OK, Json(json!({ "writable": true })))
    } else {
      (
        StatusCode::FORBIDDEN,
        Json(json!({ "error": format!("the entry of {kind} at {link} is answered to its writers alone") })),
      )
    };
  }

  if link != "platform" {
    return (StatusCode::NOT_FOUND, Json(json!({ "error": "no such link" })));
  }

  let mut body = json!({ "writable": true });

  if let Some((_, _, value)) = stub
    .values
    .lock()
    .unwrap()
    .iter()
    .find(|(held, link, _)| *held == kind && link == "platform")
  {
    body["value"] = value.clone();
  }

  (StatusCode::OK, Json(body))
}

async fn write_value(
  State(stub): State<Stub>,
  Path((kind, link)): Path<(String, String)>,
  headers: HeaderMap,
  Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
  let value = body.get("value").cloned().unwrap_or(Value::Null);

  stub.writes.lock().unwrap().push(Recorded {
    kind: kind.clone(),
    link: link.clone(),
    caller: headers.get("owner-key").and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned(),
    value: value.clone(),
  });

  let mut held = stub.values.lock().unwrap();
  held.retain(|(key, at, _)| !(key == &kind && at == &link));

  if !value.is_null() {
    held.push((kind, link, value));
  }

  (StatusCode::OK, Json(json!({})))
}

async fn resolve(
  State(stub): State<Stub>,
  Path(kind): Path<String>,
  Query(_asked): Query<HashMap<String, String>>,
) -> Json<Value> {
  if let Some((_, value)) = stub.resolved.lock().unwrap().iter().find(|(key, _)| *key == kind) {
    return Json(json!({ "value": value.clone() }));
  }

  Json(json!({ "layers": [] }))
}

async fn declare() -> StatusCode {
  StatusCode::OK
}

async fn subtree(State(stub): State<Stub>, Path(_unit): Path<i64>) -> Json<Value> {
  let units = stub
    .subtree
    .lock()
    .unwrap()
    .iter()
    .map(|unit| json!({ "id": unit }))
    .collect::<Vec<_>>();

  Json(json!({ "units": units }))
}

// The account service on a loopback port of its own, answering as its stub says.
async fn account_service(stub: Stub) -> String {
  let app = Router::new()
    .route("/users/{user}/sections", get(sections))
    .route("/chain/kinds/{kind}/links/{link}", get(entry))
    .route("/chain/kinds/{kind}/links/{link}/value", put(write_value))
    .route("/chain/kinds/{kind}/resolve", get(resolve))
    .route("/chain/kinds/{kind}", put(declare))
    .route("/units/{unit}/subtree", get(subtree))
    .with_state(stub);
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  format!("http://127.0.0.1:{port}")
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

/// A served world: the stub account service, agency's own front on a port, and the ground both stand on.
async fn stand(scratch: &Scratch, home: &std::path::Path) -> (u16, Stub, Arc<Ground>) {
  let stub = Stub::default();
  let address = account_service(stub.clone()).await;
  let ground = ground(scratch, home, &address).await;

  (front(ground.clone()).await, stub, ground)
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

async fn member_get(port: u16, path: &str) -> (u16, Value) {
  call(port, reqwest::Method::GET, path, Some("7"), Some("member")).await
}

async fn keyless(port: u16, path: &str) -> (u16, Value) {
  call(port, reqwest::Method::GET, path, None, None).await
}

/// The rows of three sides: member 7's, unit 5's and the platform's, each a supervisor, a repository and a
/// role agent of the worker role.
async fn rows(scratch: &Scratch, home: &std::path::Path) {
  for (side, account, unit) in
    [("seven", Some(MEMBER), None), ("unit", None, Some(5i64)), ("platform", None, None)]
  {
    supervisor(scratch, &format!("sup-{side}"), account, unit).await;
    scratch
      .run(
        r#"INSERT INTO "repositories" ("name", "path", "owner_account", "owner_unit", "created_at", "updated_at")
           VALUES ($1, $2, $3, $4, 0, 0)"#,
        vec![
          format!("repo-{side}").into(),
          home.join(format!("repo-{side}")).to_string_lossy().into_owned().into(),
          account.into(),
          unit.into(),
        ],
      )
      .await;
    scratch
      .run(
        r#"INSERT INTO "role_agents" ("name", "role", "owner_account", "owner_unit")
           VALUES ($1, 'worker', $2, $3)"#,
        vec![format!("seat-{side}").into(), account.into(), unit.into()],
      )
      .await;
  }
}

/// A supervisor filed as the test says.
async fn supervisor(scratch: &Scratch, name: &str, account: Option<i64>, unit: Option<i64>) {
  scratch
    .run(
      r#"INSERT INTO "supervisors" ("name", "label", "session_scope", "turn_shape", "owner_account",
           "owner_unit", "created_at", "updated_at")
         VALUES ($1, $1, 'room', 'pieces', $2, $3, 0, 0)"#,
      vec![name.into(), account.into(), unit.into()],
    )
    .await;
}

/// A registration filed as the test says.
async fn registration(scratch: &Scratch, name: &str, account: Option<i64>, unit: Option<i64>) {
  scratch
    .run(
      r#"INSERT INTO "registered_models" ("name", "label", "model", "context_tokens", "answer_tokens",
           "thinking", "walked", "added_at", "owner_account", "owner_unit")
         VALUES ($1, $1, $1, 4096, 512, 0, 0, 0, $2, $3)"#,
      vec![name.into(), account.into(), unit.into()],
    )
    .await;
}

// Whether one row of a table stands under exactly these owners.
async fn filed_under(
  scratch: &Scratch,
  table: &str,
  name: &str,
  account: Option<i64>,
  unit: Option<i64>,
) -> bool {
  scratch
    .count(
      &format!(
        r#"SELECT COUNT(*) FROM "{table}" WHERE "name" = $1
           AND "owner_account" IS NOT DISTINCT FROM $2 AND "owner_unit" IS NOT DISTINCT FROM $3"#
      ),
      vec![name.into(), account.into(), unit.into()],
    )
    .await
    == 1
}

/// A run opened across the boundary by the deployment's own call, answered whole.
async fn open_run(port: u16, agent: &str, work_unit: &str) -> Value {
  let (status, body) = send(
    port,
    reqwest::Method::POST,
    "/runs",
    None,
    None,
    json!({ "agent": agent, "unit": work_unit, "depth": 0, "startedAt": 1_700_000_000_000_i64 }),
  )
  .await;

  assert_eq!(status, 201, "{body}");

  body
}

// The `items` of a list answering that field, those carrying the prefix, as a set.
fn shown(body: &Value, field: &str, prefix: &str) -> BTreeSet<String> {
  body["items"]
    .as_array()
    .expect("a list answers items")
    .iter()
    .filter_map(|one| one[field].as_str().filter(|name| name.starts_with(prefix)).map(str::to_owned))
    .collect()
}

fn names(listed: &[impl AsRef<str>]) -> BTreeSet<String> {
  listed.iter().map(|name| name.as_ref().to_owned()).collect()
}

// An owner id as a route answers it: a whole number, or a text of digits.
fn as_id(value: &Value) -> Option<i64> {
  match value {
    Value::Number(held) => held.as_i64(),
    Value::String(held) => held.trim().parse().ok(),
    _ => None,
  }
}

#[tokio::test]
async fn the_list_of_each_resource_answers_a_member_their_own_rows_and_their_unit_s_alone() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, stub, _ground) = stand(&scratch, home.path()).await;

  stub.grant(&[("supervisors", true, false)]);
  rows(&scratch, home.path()).await;

  let parts = [("/supervisors", "sup-"), ("/role-agents", "seat-"), ("/repositories", "repo-")];

  // A head of unit 5 sees its own account's rows and unit 5's, never the platform's.
  stub.head(&[(MEMBER, 5)]);

  for (path, prefix) in parts {
    let (status, body) = member_get(port, path).await;

    assert_eq!(status, 200, "{path}: {body}");
    assert_eq!(
      shown(&body, "name", prefix),
      names(&[format!("{prefix}seven"), format!("{prefix}unit")]),
      "a head of the unit reaches its own rows and the unit's: {path}: {body}"
    );
  }

  // Heading nothing, a member's list holds their own account's rows alone.
  stub.head(&[]);

  for (path, prefix) in parts {
    let (status, body) = member_get(port, path).await;

    assert_eq!(status, 200, "{path}: {body}");
    assert_eq!(
      shown(&body, "name", prefix),
      names(&[format!("{prefix}seven")]),
      "the unit's rows stand in no stranger's list: {path}: {body}"
    );
  }

  // The deployment's own call reaches every row.
  for (path, prefix) in parts {
    let (status, body) = keyless(port, path).await;

    assert_eq!(status, 200, "{path}: {body}");
    assert_eq!(
      shown(&body, "name", prefix),
      names(&[format!("{prefix}seven"), format!("{prefix}unit"), format!("{prefix}platform"),]),
      "the platform's call reaches every row: {path}: {body}"
    );
  }
}

#[tokio::test]
async fn the_owner_params_narrow_lists_and_a_member_names_only_what_they_reach() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, stub, _ground) = stand(&scratch, home.path()).await;

  stub.grant(&[("supervisors", true, false)]);

  for (side, account, unit) in [
    ("seven", Some(MEMBER), None),
    ("unit5", None, Some(5i64)),
    ("unit7", None, Some(7)),
    ("unit9", None, Some(9)),
    ("platform", None, None),
  ] {
    supervisor(&scratch, &format!("sup-{side}"), account, unit).await;
  }

  // Member 7 heads units 5 and 7; the account service answers 7 as standing under 5.
  stub.head(&[(MEMBER, 5), (MEMBER, 7)]);
  stub.under(&[5, 7]);

  let (status, body) = member_get(port, "/supervisors?unit=5").await;
  assert_eq!((status, shown(&body, "name", "sup-")), (200, names(&["sup-unit5"])), "{body}");

  let (status, body) = member_get(port, "/supervisors?ownerAccount=7").await;
  assert_eq!((status, shown(&body, "name", "sup-")), (200, names(&["sup-seven"])), "{body}");

  let (status, body) = member_get(port, "/supervisors?unit=5&unitSubtree=true").await;
  assert_eq!(
    (status, shown(&body, "name", "sup-")),
    (200, names(&["sup-unit5", "sup-unit7"])),
    "the unit and the units below it: {body}"
  );

  // A member names their own account and a unit they head alone; the axes never stand together, and every
  // value reads as its kind.
  for (path, asked) in [
    ("/supervisors?unit=9", 403u16),
    ("/supervisors?ownerAccount=8", 403),
    ("/supervisors?ownerAccount=7&unit=5", 400),
    ("/supervisors?unit=not-a-number", 400),
    ("/supervisors?unit=5&unitSubtree=false", 400),
  ] {
    let (status, body) = member_get(port, path).await;

    assert_eq!(status, asked, "{path}: {body}");

    if asked == 403 {
      assert_eq!(body["code"], "forbidden", "{path}: {body}");
    }
  }

  // The deployment's own call names any owner.
  let (status, body) = keyless(port, "/supervisors?unit=9").await;
  assert_eq!((status, shown(&body, "name", "sup-")), (200, names(&["sup-unit9"])), "{body}");
}

#[tokio::test]
async fn a_single_row_of_a_unit_the_reader_does_not_head_is_answered_as_none_stands_there() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, stub, _ground) = stand(&scratch, home.path()).await;

  stub.grant(&[("supervisors", true, false), ("models", true, false)]);
  supervisor(&scratch, "sup-gated", None, Some(5)).await;
  registration(&scratch, "gated", None, Some(5)).await;
  let run = open_run(port, "sup-gated", "work-gated").await;

  // Member 7 heads nothing: the unit's rows answer as none stood there.
  for path in ["/supervisors/sup-gated", "/models/registered::gated"] {
    let (status, body) = member_get(port, path).await;

    assert_eq!(status, 404, "{path}: {body}");
  }

  let (status, body) = member_get(port, &format!("/runs/{}", run["id"])).await;
  assert_eq!(status, 404, "the run the unit's agent opened: {body}");

  // The deployment's own call reads every row whole.
  let paths = [
    "/supervisors/sup-gated".to_owned(),
    "/models/registered::gated".to_owned(),
    format!("/runs/{}", run["id"]),
  ];

  for path in &paths {
    let (status, body) = keyless(port, path).await;

    assert_eq!(status, 200, "{path}: {body}");
  }
}

#[tokio::test]
async fn a_head_files_a_supervisor_a_seat_and_a_model_under_their_unit_and_a_stranger_is_refused() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, stub, _ground) = stand(&scratch, home.path()).await;

  stub.head(&[(MEMBER, 5)]);

  let (status, body) = send(
    port,
    reqwest::Method::POST,
    "/supervisors",
    Some("7"),
    Some("member"),
    json!({ "name": "sup-made", "label": "Made", "provider": "deepseek", "model": "deepseek-flash",
            "ownerUnit": 5 }),
  )
  .await;

  assert_eq!(status, 201, "a head files a supervisor: {body}");
  assert_eq!(as_id(&body["supervisor"]["ownerUnit"]), Some(5), "{body}");
  assert!(body["supervisor"]["ownerAccount"].is_null(), "{body}");
  assert!(filed_under(&scratch, "supervisors", "sup-made", None, Some(5)).await);

  let (status, body) = send(
    port,
    reqwest::Method::POST,
    "/role-agents",
    Some("7"),
    Some("member"),
    json!({ "role": "worker", "name": "worker-made", "ownerUnit": 5 }),
  )
  .await;

  assert_eq!(status, 201, "a head files a seat: {body}");
  assert_eq!(as_id(&body["ownerUnit"]), Some(5), "{body}");
  assert!(filed_under(&scratch, "role_agents", "worker-made", None, Some(5)).await);

  let (status, body) = send(
    port,
    reqwest::Method::POST,
    "/models",
    Some("7"),
    Some("member"),
    json!({ "key": "registered::made", "model": "made-model", "unit": 5 }),
  )
  .await;

  assert_eq!(status, 201, "a head files a registration: {body}");
  assert_eq!(as_id(&body["unit"]), Some(5), "{body}");
  assert!(filed_under(&scratch, "registered_models", "made", None, Some(5)).await);

  // A head changes a registration its unit owns.
  let (status, body) = send(
    port,
    reqwest::Method::PATCH,
    "/models/registered::made",
    Some("7"),
    Some("member"),
    json!({ "roleFiles": true }),
  )
  .await;

  assert_eq!(status, 200, "{body}");

  // A member heading nothing files nothing there and changes nothing of the unit's.
  stub.head(&[]);

  for (method, path, body) in [
    (
      reqwest::Method::POST,
      "/supervisors",
      json!({ "name": "sup-stranger", "label": "Stranger", "provider": "deepseek",
              "model": "deepseek-flash", "ownerUnit": 5 }),
    ),
    (
      reqwest::Method::POST,
      "/role-agents",
      json!({ "role": "worker", "name": "worker-stranger", "ownerUnit": 5 }),
    ),
    (
      reqwest::Method::POST,
      "/models",
      json!({ "key": "registered::stranger", "model": "stranger-model", "unit": 5 }),
    ),
    (reqwest::Method::PATCH, "/models/registered::made", json!({ "roleFiles": false })),
    (reqwest::Method::DELETE, "/models/registered::made", json!({})),
  ] {
    let (status, answer) = send(port, method.clone(), path, Some("7"), Some("member"), body).await;

    assert_eq!((status, &answer["code"]), (403, &json!("forbidden")), "{method} {path}: {answer}");
  }

  // A row carries one owner: a body naming both is refused whole. (The seat answers it 422, its route's
  // judgement of the body; the supervisor 400 under its `ownerUnit` field; a registration answers it only
  // where a change names both.)
  let (status, body) = send(
    port,
    reqwest::Method::POST,
    "/supervisors",
    Some("7"),
    Some("member"),
    json!({ "name": "sup-both", "label": "Both", "ownerAccount": 7, "ownerUnit": 5 }),
  )
  .await;

  assert_eq!(status, 400, "a supervisor: {body}");
  assert!(
    body["error"].as_str().unwrap_or_default().contains("never under both"),
    "a supervisor: {body}"
  );

  let (status, body) = send(
    port,
    reqwest::Method::POST,
    "/role-agents",
    Some("7"),
    Some("member"),
    json!({ "role": "worker", "name": "worker-both", "ownerAccount": 7, "ownerUnit": 5 }),
  )
  .await;

  assert_eq!(status, 422, "a seat: {body}");
  assert!(
    body["error"].as_str().unwrap_or_default().contains("never under both"),
    "a seat: {body}"
  );

  // And the account a registration belongs to stays the owner's to set: a member naming `account` is
  // refused before either axis is judged.
  let (status, body) = send(
    port,
    reqwest::Method::PATCH,
    "/models/registered::made",
    Some("7"),
    Some("member"),
    json!({ "account": 7, "unit": 6 }),
  )
  .await;

  assert_eq!((status, &body["code"]), (403, &json!("forbidden")), "a registration: {body}");
}

#[tokio::test]
async fn a_run_stands_under_its_agent_s_owners_and_the_list_narrows_by_them() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, stub, _ground) = stand(&scratch, home.path()).await;

  supervisor(&scratch, "sup-unit", None, Some(5)).await;
  supervisor(&scratch, "sup-plain", None, None).await;

  let unit_run = open_run(port, "sup-unit", "work-unit").await;
  let plain_run = open_run(port, "sup-plain", "work-plain").await;
  let other_run = open_run(port, "no-such-agent", "work-other").await;

  assert_eq!(as_id(&unit_run["ownerUnit"]), Some(5), "the run took its agent's unit: {unit_run}");
  assert!(unit_run["ownerAccount"].is_null(), "{unit_run}");
  assert!(
    plain_run["ownerUnit"].is_null() && plain_run["ownerAccount"].is_null(),
    "the platform's agent opens the platform's run: {plain_run}"
  );
  assert!(
    other_run["ownerUnit"].is_null() && other_run["ownerAccount"].is_null(),
    "an agent naming neither owner stands with the platform's: {other_run}"
  );

  // A head of unit 5: the run of the unit's agent alone.
  stub.head(&[(MEMBER, 5)]);

  let (status, body) = member_get(port, "/runs").await;
  assert_eq!((status, shown(&body, "agentName", "")), (200, names(&["sup-unit"])), "{body}");

  let (status, body) = member_get(port, "/runs?unit=5").await;
  assert_eq!((status, shown(&body, "agentName", "")), (200, names(&["sup-unit"])), "{body}");

  // Heading nothing: neither the unit's run nor the platform's stands in the list.
  stub.head(&[]);

  let (status, body) = member_get(port, "/runs").await;
  assert_eq!(status, 200, "{body}");
  assert!(shown(&body, "agentName", "").is_empty(), "{body}");

  // The deployment's own call reaches every run and names any owner.
  let (status, body) = keyless(port, "/runs").await;
  assert_eq!(
    (status, shown(&body, "agentName", "")),
    (200, names(&["no-such-agent", "sup-plain", "sup-unit"])),
    "{body}"
  );

  let (status, body) = keyless(port, "/runs?unit=5").await;
  assert_eq!((status, shown(&body, "agentName", "")), (200, names(&["sup-unit"])), "{body}");

  // The owner parameters stand in the runs' filters; a name outside them is refused.
  let (status, body) = keyless(port, "/runs?holder=7").await;
  assert_eq!(status, 400, "{body}");
}

#[tokio::test]
async fn the_settings_read_the_platform_link_and_seed_once_moves_a_row_onto_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, stub, ground) = stand(&scratch, home.path()).await;

  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value")
         VALUES ('users_administrator', '9'), ('deploy_flush_minutes', '30')
         ON CONFLICT ("key") DO UPDATE SET "value" = EXCLUDED."value""#,
      vec![],
    )
    .await;

  chain_settings::seed_once(&ground).await.unwrap();

  let flush = ORGANISATION.settings.deploy_flush_minutes.key.clone();

  assert_eq!(
    stub.writes(),
    vec![Recorded {
      kind: flush.clone(),
      link: "platform".to_owned(),
      caller: "9".to_owned(),
      value: json!(30),
    }],
    "the row moved onto the platform's link, in the administrator's name"
  );
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "settings" WHERE "key" = 'deploy_flush_minutes'"#, vec![])
      .await,
    0,
    "the row gave its value up"
  );

  let (status, body) = keyless(port, "/settings").await;
  assert_eq!(
    (status, &body["deployFlushMinutes"]),
    (200, &json!(30)),
    "the link's word is read: {body}"
  );

  let (status, body) = send(
    port,
    reqwest::Method::PATCH,
    "/settings",
    None,
    None,
    json!({ "deployFlushMinutes": 45 }),
  )
  .await;

  assert_eq!(
    (status, &body["deployFlushMinutes"]),
    (200, &json!(45)),
    "the write goes to the link: {body}"
  );
  assert_eq!(stub.writes().last().map(|one| one.value.clone()), Some(json!(45)));
  assert_eq!(
    stub.writes().last().map(|one| one.kind.clone()),
    Some(flush),
    "written onto the flush's own kind"
  );
}

// The first step of a person's run walking its order, from a hand-built choice: none where a source took
// the run, the closure where none did.
async fn walk(ground: &Ground) -> Option<supply::Closed> {
  let mut chosen = Chosen {
    run: "test-7".to_owned(),
    account: Some(MEMBER),
    provider: "deepseek".to_owned(),
    model: "deepseek-flash".to_owned(),
    source: Source::Personal,
    order: vec![PERSONAL.to_owned()],
    units: Vec::new(),
    step: 1,
  };

  supply::next_source(ground, &mut chosen, &[PERSONAL.to_owned()]).await.unwrap().err()
}

#[tokio::test]
async fn a_provider_turned_off_below_the_person_closes_the_walk() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let stub = Stub::default();

  stub.resolve_to("provider.deepseek.offered", json!(false));

  let address = account_service(stub.clone()).await;
  let ground = ground(&scratch, home.path(), &address).await;

  let closed = walk(&ground).await.expect("a turned-off provider leaves no source");

  assert_eq!(closed.why, Shortfall::Withheld);
  assert_eq!(closed.source, supply::PLATFORM);
  assert!(closed.reason.contains("deepseek"), "{}", closed.reason);

  // Where the chain says the provider stands, the walk itself goes on: the run closes now only for what no
  // source of it holds, not for the provider being off.
  stub.resolve_to("provider.deepseek.offered", json!(true));

  let closed = walk(&ground).await.expect("the person holds no personal source here");

  assert_ne!(closed.why, Shortfall::Withheld, "the walk itself said: {}", closed.sentence);
}

#[tokio::test]
async fn a_registration_filed_under_a_unit_is_no_platform_source() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());

  registration(&scratch, "unit-model", None, Some(5)).await;
  registration(&scratch, "account-model", Some(9), None).await;
  registration(&scratch, "free-model", None, None).await;

  assert!(
    !supply::platform_open(&ground, MEMBER, "registered", "unit-model").await.unwrap(),
    "a registration a unit owns is the unit's, no platform source"
  );
  assert!(
    !supply::platform_open(&ground, MEMBER, "registered", "account-model").await.unwrap(),
    "a registration an account owns is no platform source either"
  );
  assert!(
    supply::platform_open(&ground, MEMBER, "registered", "free-model").await.unwrap(),
    "the platform's own registration stands open"
  );
}

/// One line in the log, written by `agent` under `run`.
async fn line(scratch: &Scratch, agent: &str, run: &str, at: i64, text: &str) {
  scratch
    .run(
      r#"INSERT INTO "agent_log" ("agent", "run", "at", "kind", "text") VALUES ($1, $2, $3, 'text', $4)"#,
      vec![agent.into(), run.into(), at.into(), text.into()],
    )
    .await;
}

// The times the log lines and usage rows of these tests stand on: a fixed span, and a window wide enough to
// hold it whole.
const AT: i64 = 1_700_000_000_000;

fn texts(page: &Value) -> Vec<String> {
  page["items"]
    .as_array()
    .expect("a log page answers items")
    .iter()
    .filter_map(|one| one["text"].as_str().map(str::to_owned))
    .collect()
}

fn group_keys(page: &Value) -> BTreeSet<String> {
  page["groups"]
    .as_array()
    .expect("usage answers groups")
    .iter()
    .filter_map(|one| one["key"].as_str().map(str::to_owned))
    .collect()
}

fn group(page: &Value, key: &str) -> Value {
  page["groups"]
    .as_array()
    .expect("usage answers groups")
    .iter()
    .find(|one| one["key"].as_str() == Some(key))
    .cloned()
    .unwrap_or(Value::Null)
}

#[tokio::test]
async fn a_member_reads_the_log_of_the_agents_they_reach_alone() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, stub, _ground) = stand(&scratch, home.path()).await;

  supervisor(&scratch, "sup-seven", Some(MEMBER), None).await;
  supervisor(&scratch, "sup-unit", None, Some(5)).await;
  supervisor(&scratch, "sup-plain", None, None).await;
  line(&scratch, "sup-seven", "r-seven", AT, "mine").await;
  line(&scratch, "sup-unit", "r-unit", AT, "the unit's").await;
  line(&scratch, "sup-plain", "r-plain", AT, "the platform's").await;

  // A head of unit 5: their own lines and the unit's, and no line of the platform's agent.
  stub.head(&[(MEMBER, 5)]);

  let (status, body) = member_get(port, "/log-lines?runId=r-seven").await;
  assert_eq!((status, texts(&body)), (200, vec!["mine".to_owned()]), "{body}");

  let (status, body) = member_get(port, "/log-lines?runId=r-unit").await;
  assert_eq!((status, texts(&body)), (200, vec!["the unit's".to_owned()]), "{body}");

  let (status, body) = member_get(port, "/log-lines?runId=r-plain").await;
  assert_eq!(
    (status, texts(&body)),
    (200, Vec::<String>::new()),
    "the platform's log is none of theirs"
  );

  // An agent's own log page answers the same judgement, and an unreached agent as none standing.
  let (status, body) = member_get(port, "/log/agents/sup-unit/lines").await;
  assert_eq!((status, texts(&body)), (200, vec!["the unit's".to_owned()]), "{body}");

  let (status, body) = member_get(port, "/log/agents/sup-plain/lines").await;
  assert_eq!(status, 200, "{body}");
  assert!(body["items"].as_array().is_some_and(|items| items.is_empty()), "{body}");

  // Heading nothing: the unit's lines fall away, one's own stand.
  stub.head(&[]);

  let (_, body) = member_get(port, "/log-lines?runId=r-unit").await;
  assert!(texts(&body).is_empty(), "{body}");

  let (status, body) = member_get(port, "/log-lines?runId=r-seven").await;
  assert_eq!((status, texts(&body)), (200, vec!["mine".to_owned()]), "{body}");

  // The deployment's own call reads every line, and a stranger's page keeps the shape whole.
  let (status, body) = keyless(port, "/log-lines?runId=r-plain").await;
  assert_eq!((status, texts(&body)), (200, vec!["the platform's".to_owned()]), "{body}");
}

#[tokio::test]
async fn a_role_s_log_page_shows_a_member_only_their_own_agents_lines() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, _stub, _ground) = stand(&scratch, home.path()).await;

  scratch
    .run(
      r#"INSERT INTO "role_agents" ("name", "role", "owner_account") VALUES ('role-own', 'worker', $1)"#,
      vec![MEMBER.into()],
    )
    .await;
  scratch
    .run(r#"INSERT INTO "role_agents" ("name", "role") VALUES ('role-other', 'worker')"#, vec![])
    .await;
  line(&scratch, "role-own", "w1", AT, "hers").await;
  line(&scratch, "role-other", "w2", AT, "not hers").await;
  line(&scratch, "worker", "w3", AT, "the role's own").await;

  // The role's name carries no owner, so the page goes on over the agents of it the reader reaches alone.
  let (status, body) = member_get(port, "/log/agents/worker/lines").await;
  assert_eq!((status, texts(&body)), (200, vec!["hers".to_owned()]), "{body}");

  let (status, body) = keyless(port, "/log/agents/worker/lines").await;
  assert_eq!(status, 200, "{body}");
  assert_eq!(texts(&body).len(), 3, "{body}");
}

#[tokio::test]
async fn a_member_writes_a_note_only_into_a_log_they_reach() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, _stub, _ground) = stand(&scratch, home.path()).await;

  supervisor(&scratch, "sup-seven", Some(MEMBER), None).await;
  supervisor(&scratch, "sup-plain", None, None).await;

  let (status, body) = send(
    port,
    reqwest::Method::POST,
    "/log-lines",
    Some("7"),
    Some("member"),
    json!({ "agentName": "sup-plain", "runId": "r-plain", "text": "into another's log" }),
  )
  .await;
  assert_eq!(status, 403, "{body}");

  let (status, body) = send(
    port,
    reqwest::Method::POST,
    "/log-lines",
    Some("7"),
    Some("member"),
    json!({ "agentName": "sup-seven", "runId": "r-seven", "text": "into one's own log" }),
  )
  .await;
  assert_eq!(status, 201, "{body}");

  let (status, body) = member_get(port, "/log-lines?runId=r-seven").await;
  assert_eq!((status, texts(&body)), (200, vec!["into one's own log".to_owned()]), "{body}");
}

#[tokio::test]
async fn a_member_s_usage_stands_over_only_the_work_they_reach() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (port, stub, _ground) = stand(&scratch, home.path()).await;

  supervisor(&scratch, "sup-seven", Some(MEMBER), None).await;
  supervisor(&scratch, "sup-unit", None, Some(5)).await;
  supervisor(&scratch, "sup-plain", None, None).await;

  for agent in ["sup-seven", "sup-unit", "sup-plain"] {
    open_run(port, agent, &format!("w-{agent}")).await;
  }
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
                             "state", "started_at", "prompt", "item_agent", "model", "token_count")
         VALUES (70, 7, 1, 'sup-seven', 1, 'c7', 'done', $1, 'p', 'sup-seven', 'g-test', 100),
                (71, 7, 2, 'sup-plain', 2, 'cp', 'done', $1, 'p', 'sup-plain', 'g-test', 900)"#,
      vec![AT.into()],
    )
    .await;
  let window = "&after=0&before=4000000000000";

  // The head of unit 5: their own account's turns, the unit's under the platform-named key, and no others'.
  stub.head(&[(MEMBER, 5)]);

  let (status, body) = member_get(port, &format!("/usage?groupBy=account{window}")).await;
  assert_eq!(status, 200, "{body}");
  assert_eq!(group_keys(&body), names(&["7", "none"]), "{body}");
  assert_eq!(group(&body, "none")["turns"], 1, "the unit's run alone stands in it: {body}");

  let (status, body) = member_get(port, &format!("/usage?groupBy=model{window}")).await;
  assert_eq!(status, 200, "{body}");
  assert_eq!(group(&body, "g-test")["tokens"], 100, "only their own job is summed: {body}");
  assert!(group(&body, "g-test")["contextTokens"].is_null(), "no others' sessions show: {body}");

  // Heading nothing: the unit's run falls away too, the account's stands.
  stub.head(&[]);

  let (_, body) = member_get(port, &format!("/usage?groupBy=account{window}")).await;
  assert_eq!(group_keys(&body), names(&["7"]), "{body}");

  let (_, body) = member_get(port, &format!("/usage?groupBy=model{window}")).await;
  assert_eq!(group(&body, "g-test")["tokens"], 100, "{body}");

  // The deployment's own call stands over everything.
  let (_, body) = keyless(port, &format!("/usage?groupBy=account{window}")).await;
  assert_eq!(group(&body, "none")["turns"], 2, "the unit's and the platform's run together: {body}");

  let (_, body) = keyless(port, &format!("/usage?groupBy=model{window}")).await;
  assert_eq!(group(&body, "g-test")["tokens"], 1000, "{body}");
}
