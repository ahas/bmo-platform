//! The worker ask: a slot of the worker role asked under the source `ask` and waited for however long the line
//! takes, one run of the role at the task's folder on the road the slot carries, its words and the files it
//! changed answered and the slot finished; a repository's checkout outside the owner's turn and a checkout on
//! a node are refused (docs/tools.md, 「The worker ask」).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use tool_call::account_service::AccountService;
use tool_call::{Control, Session};
use worker_ask_tool::{Api, AskInput, ask_with};

#[derive(Default)]
struct Asked {
  slot: Option<Value>,
  admissions: usize,
  started: Option<Value>,
  turned: Option<(String, Value)>,
  finished: Vec<Value>,
}

#[derive(Clone)]
struct Stub {
  folder: String,
  // How many admissions answer the slot not let in before one lets it in.
  waits: usize,
  asked: Arc<Mutex<Asked>>,
}

fn stub(folder: &std::path::Path, waits: usize) -> Stub {
  Stub {
    folder: folder.to_string_lossy().into_owned(),
    waits,
    asked: Arc::default(),
  }
}

fn slot(state: &str) -> Value {
  json!({ "token": "t1", "state": state, "role": "worker", "provider": "deepseek", "model": "deepseek-flash" })
}

async fn repositories() -> Json<Value> {
  Json(json!({ "items": [
    { "name": "bmo", "path": "/work/bmo", "nodeName": null,
      "checkouts": [{ "name": "bmo-w1", "path": "/work/bmo-w1", "nodeName": null },
                    { "name": "bmo@n3", "path": "/work/bmo-n3", "nodeName": "n3" }] },
  ], "more": false }))
}

async fn ask_slot(State(stub): State<Stub>, Json(body): Json<Value>) -> (StatusCode, Json<Value>) {
  stub.asked.lock().unwrap().slot = Some(body);

  (StatusCode::CREATED, Json(slot("waiting")))
}

async fn move_slot(State(stub): State<Stub>, Json(body): Json<Value>) -> Json<Value> {
  let mut asked = stub.asked.lock().unwrap();

  match body["state"].as_str() {
    Some("bound") => Json(slot("active")),
    Some("finished") => {
      asked.finished.push(body);
      Json(json!({ "held": true }))
    }
    _ => {
      asked.admissions += 1;

      if asked.admissions > stub.waits {
        Json(json!({ "request": slot("active"), "demand": null }))
      } else {
        Json(json!({ "request": null, "demand": null }))
      }
    }
  }
}

async fn read_slot() -> Json<Value> {
  Json(slot("waiting"))
}

async fn start(State(stub): State<Stub>, Json(body): Json<Value>) -> (StatusCode, Json<Value>) {
  stub.asked.lock().unwrap().started = Some(body);

  (StatusCode::CREATED, Json(json!({ "id": "worker@run:1" })))
}

async fn turn(Path(id): Path<String>, State(stub): State<Stub>, Json(body): Json<Value>) -> Json<Value> {
  stub.asked.lock().unwrap().turned = Some((id.clone(), body));

  Json(json!({ "answer": format!("Done in {} via {id}.", stub.folder), "state": "ended" }))
}

async fn lines(State(stub): State<Stub>) -> Json<Value> {
  Json(json!({ "lines": [
    { "kind": "bashEditDiff", "path": format!("{}/a.rs", stub.folder) },
    { "kind": "edit", "path": format!("{}/b.rs", stub.folder) },
    { "kind": "edit", "path": "/elsewhere/c.rs" }
  ] }))
}

async fn server(stub: Stub) -> Api {
  let app = Router::new()
    .route("/repositories", get(repositories))
    .route("/slot-requests", post(ask_slot))
    .route("/slot-requests/{token}", patch(move_slot).get(read_slot))
    .route("/sessions", post(start))
    .route("/sessions/{id}/turns", post(turn))
    .route("/log/agents/{name}/lines", get(lines))
    .with_state(stub);
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  Api {
    control: Control { port, token: None },
    pace: Duration::from_millis(10),
  }
}

fn session(root: &std::path::Path) -> Session {
  Session {
    variables: BTreeMap::from([
      ("BMO_DATA_DIR".to_owned(), root.to_string_lossy().into_owned()),
      ("BMO_TURN_AGENT".to_owned(), "bmo".to_owned()),
    ]),
    ..Session::default()
  }
}

fn asked(words: &str, folder: &std::path::Path) -> AskInput {
  AskInput {
    words: words.into(),
    folder: Some(folder.to_string_lossy().into_owned()),
    app: None,
  }
}

#[tokio::test]
async fn a_slot_waits_in_line_and_the_worker_run_answers_with_its_words_and_the_files_it_changed() {
  let home = tempfile::tempdir().unwrap();
  let folder = home.path().join("scratch");
  std::fs::create_dir_all(&folder).unwrap();
  let stub = stub(&folder, 3);
  let held = stub.asked.clone();
  let api = server(stub).await;
  let answer = ask_with(&api, &session(home.path()), asked("add a file", &folder)).await;
  let answer = serde_json::to_string(&answer).unwrap();

  assert!(answer.contains("Done in"), "{answer}");
  assert!(answer.contains("via worker@run:1"), "{answer}");
  assert!(answer.contains("a.rs") && answer.contains("b.rs"), "{answer}");
  assert!(!answer.contains("c.rs"), "a file outside the folder is not the run's: {answer}");

  let held = held.lock().unwrap();
  let slot = held.slot.clone().unwrap();

  assert_eq!(slot["role"], "worker");
  assert_eq!(slot["source"], "ask");
  assert_eq!(slot["askedBy"], "bmo");
  assert_eq!(slot["path"], folder.display().to_string());
  assert_eq!(held.admissions, 4, "it asked for its admission as long as the line held it");

  let started = held.started.clone().unwrap();

  assert_eq!(started["scope"], "role");
  assert_eq!(started["slot"], "t1", "the run starts on the slot it was let in on");
  assert_eq!(started["cwd"], folder.display().to_string());
  assert!(
    started.get("provider").is_none() && started.get("options").is_none(),
    "the road is the slot's"
  );

  let (session, body) = held.turned.clone().unwrap();

  assert_eq!(session, "worker@run:1");
  assert!(
    body.get("model").is_none() && body.get("mapping").is_none(),
    "the turn's road is the session's"
  );
  assert_eq!(held.finished, [json!({ "state": "finished", "outcome": "answered", "cancel": false })]);
}

#[tokio::test]
async fn a_repository_checkout_outside_the_owners_turn_and_one_on_a_node_are_refused() {
  let home = tempfile::tempdir().unwrap();
  let stub = stub(home.path(), 0);
  let held = stub.asked.clone();
  let api = server(stub).await;

  for (folder, said) in [("/work/bmo-w1", "owner's turn"), ("/work/bmo-n3", "on n3")] {
    let answer = serde_json::to_string(
      &ask_with(&api, &session(home.path()), asked("do it", std::path::Path::new(folder))).await,
    )
    .unwrap();

    assert!(answer.contains(said), "{folder}: {answer}");
  }

  assert!(held.lock().unwrap().slot.is_none(), "nothing was asked");
}

// "an app names a personal app of the person the session acts for: the run works in that app's folder of 'my apps'
// and its words are led by the make-app guide; an app and a folder together, and an app under a record naming no
// account, are refused"
#[tokio::test]
async fn an_app_s_run_works_in_its_folder_of_my_apps_led_by_the_guide() {
  let home = tempfile::tempdir().unwrap();
  let folder = home.path().join("my-apps").join("weekly_summary");
  let stub = stub(&folder, 0);
  let held = stub.asked.clone();
  let api = server(stub).await;
  let app = |folder: Option<&str>, app: &str| AskInput {
    words: "sum up the week's notes".into(),
    folder: folder.map(str::to_owned),
    app: Some(app.to_owned()),
  };
  let owned = Session {
    owner_account: Some("8".to_owned()),
    ..session(home.path())
  };
  let refused = |answer: tool_call::Answer| serde_json::to_string(&answer).unwrap();

  assert!(
    refused(ask_with(&api, &owned, app(Some("/work/x"), "weekly_summary")).await).contains("not both")
  );
  assert!(
    refused(ask_with(&api, &session(home.path()), app(None, "weekly_summary")).await)
      .contains("acts for no person")
  );
  assert!(refused(ask_with(&api, &owned, app(None, "../up")).await).contains("no app's name"));
  assert!(held.lock().unwrap().slot.is_none(), "nothing was asked");

  let answer = refused(ask_with(&api, &owned, app(None, "weekly_summary")).await);

  assert!(answer.contains("Done in"), "{answer}");

  let held = held.lock().unwrap();

  assert_eq!(held.slot.clone().unwrap()["path"], folder.display().to_string());
  assert_eq!(held.started.clone().unwrap()["cwd"], folder.display().to_string());

  let prompt = held.turned.clone().unwrap().1["prompt"].as_str().unwrap().to_owned();

  assert!(prompt.contains("the personal app weekly_summary"), "{prompt}");
  assert!(prompt.contains("sum up the week's notes"), "{prompt}");
  assert!(prompt.contains("## The worker run's guide"), "the guide leads the words: {prompt}");
}

#[tokio::test]
async fn empty_words_are_refused_before_a_slot_is_asked() {
  let home = tempfile::tempdir().unwrap();
  let stub = stub(home.path(), 0);
  let held = stub.asked.clone();
  let api = server(stub).await;
  let answer =
    serde_json::to_string(&ask_with(&api, &session(home.path()), asked("  ", home.path())).await).unwrap();

  assert!(answer.contains("words must not be empty"), "{answer}");
  assert!(held.lock().unwrap().slot.is_none());
}

#[test]
fn the_account_service_is_read_from_the_environment_or_the_web_unit_s_env_file() {
  let root = tempfile::tempdir().unwrap();

  std::fs::create_dir_all(root.path().join("env")).unwrap();
  std::fs::write(
    root.path().join("env").join("web.env"),
    "WEB_ADDRESS=http://127.0.0.1:4/\nWEB_TOKEN=from-file\n",
  )
  .unwrap();

  assert_eq!(
    AccountService::read(root.path(), |_| None),
    AccountService {
      address: "http://127.0.0.1:4".into(),
      token: "from-file".into(),
    }
  );

  let token = root.path().join("web.token");

  std::fs::write(&token, "in-a-file\n").unwrap();

  let token = token.to_string_lossy().into_owned();
  let named = AccountService::read(root.path(), |name| match name {
    "WEB_ADDRESS" => Some("http://127.0.0.1:5".into()),
    "WEB_TOKEN_FILE" => Some(token.clone()),
    _ => None,
  });

  assert_eq!(named.address, "http://127.0.0.1:5");
  assert_eq!(named.token, "from-file", "a token by value in the env file stands before a file named");
}

#[tokio::test]
async fn agency_is_asked_where_the_data_roots_env_file_names_it() {
  let agency = super::agency_named_by_its_file().await;
  let api = Api::read(agency.root.path(), |_| None);
  let input = AskInput {
    words: "fix the typo".to_owned(),
    folder: Some("/work/elsewhere".to_owned()),
    app: None,
  };

  ask_with(&api, &session(agency.root.path()), input).await;

  agency.assert_asked_under_the_files_token();
}
