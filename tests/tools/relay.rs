//! The relay and chat verbs (docs/relays.md): the sender read from the turn, never typed; the line handed
//! to agency with the room and the person; the record's answer returned, a chat's answer with it, and a
//! refusal or a failure answered as a failed call. A call standing in no agent's turn reaches nothing.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{Value, json};
use tool_call::{Control, Session};

use relay_tool::{Api, RelayInput, send_with};

#[derive(Default)]
struct Asked {
  asked: Vec<(String, Value)>,
}

#[derive(Clone)]
struct Stub {
  status: StatusCode,
  answer: Value,
  asked: Arc<Mutex<Asked>>,
}

async fn take(
  State(stub): State<Stub>,
  path: axum::extract::Path<String>,
  Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
  stub.asked.lock().unwrap().asked.push((path.0, body));

  (stub.status, Json(stub.answer.clone()))
}

async fn server(stub: Stub) -> Api {
  let app = Router::new().route("/{one}", post(take)).with_state(stub);
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  Api {
    control: Control {
      port,
      token: Some("test-token".to_owned()),
    },
  }
}

fn stub(status: StatusCode, answer: Value) -> Stub {
  Stub {
    status,
    answer,
    asked: Arc::default(),
  }
}

fn session(agent: Option<&str>, room: Option<i64>) -> Session {
  let mut variables = BTreeMap::new();

  if let Some(agent) = agent {
    variables.insert("BMO_TURN_AGENT".to_owned(), agent.to_owned());
  }

  if let Some(room) = room {
    variables.insert("BMO_TURN_CHAT".to_owned(), room.to_string());
  }

  Session {
    variables,
    ..Session::default()
  }
}

fn input(to: &str, text: &str, room: Option<i64>) -> RelayInput {
  RelayInput {
    to: to.to_owned(),
    text: text.to_owned(),
    room,
    person: Some(7),
  }
}

fn delivered() -> Value {
  json!({
    "relay": { "id": 1, "from": "a", "to": "b", "room": -5, "kind": "relay", "line": "hello",
                "state": "delivered", "answer": null, "reason": null, "createdAt": 1000 },
    "answer": null,
  })
}

#[tokio::test]
async fn a_relay_carries_the_turns_agent_and_the_room_it_names() {
  let asked = stub(StatusCode::CREATED, delivered());
  let seen = asked.asked.clone();
  let api = server(asked).await;
  let answer = send_with(&api, &session(Some("a"), None), input("b", " hello ", Some(-7)), false)
    .await
    .unwrap();

  assert!(!answer.is_error);
  assert!(answer.plain().contains("Relayed to b"), "{}", answer.plain());

  let asked = seen.lock().unwrap();

  assert_eq!(asked.asked.len(), 1);
  assert_eq!(asked.asked[0].0, "relays");
  assert_eq!(
    asked.asked[0].1,
    json!({ "from": "a", "to": "b", "text": "hello", "room": -7, "person": 7 })
  );
}

#[tokio::test]
async fn a_relay_with_no_room_is_said_in_the_turns_own() {
  let asked = stub(StatusCode::CREATED, delivered());
  let seen = asked.asked.clone();
  let api = server(asked).await;

  send_with(&api, &session(Some("a"), Some(-9)), input("b", "hello", None), false)
    .await
    .unwrap();

  assert_eq!(seen.lock().unwrap().asked[0].1["room"], json!(-9));
}

#[tokio::test]
async fn a_chat_answers_with_the_receiving_bmos_answer() {
  let asked = stub(
    StatusCode::CREATED,
    json!({
      "relay": { "id": 2, "from": "a", "to": "b", "room": -5, "kind": "chat", "line": "hello",
                  "state": "delivered", "answer": "hello back", "reason": null, "createdAt": 1000 },
      "answer": "hello back",
    }),
  );
  let seen = asked.asked.clone();
  let api = server(asked).await;
  let answer = send_with(&api, &session(Some("a"), None), input("b", "hello", Some(-5)), true)
    .await
    .unwrap();

  assert!(!answer.is_error);
  assert!(answer.plain().contains("hello back"), "{}", answer.plain());
  assert_eq!(seen.lock().unwrap().asked[0].0, "chats");
}

#[tokio::test]
async fn a_chat_names_the_persons_room_or_reaches_nothing() {
  let asked = stub(StatusCode::CREATED, delivered());
  let seen = asked.asked.clone();
  let api = server(asked).await;
  let answer = send_with(&api, &session(Some("a"), None), input("b", "hello", None), true)
    .await
    .unwrap();

  assert!(answer.is_error);
  assert!(answer.plain().contains("room"), "{}", answer.plain());
  assert!(seen.lock().unwrap().asked.is_empty());
}

#[tokio::test]
async fn a_refusal_is_a_failed_call_naming_the_receiving_bmo() {
  let asked = stub(
    StatusCode::CONFLICT,
    json!({ "code": "relay-refused", "error": "b refused the line: the receiving BMO would not take it" }),
  );
  let api = server(asked).await;
  let answer = send_with(&api, &session(Some("a"), None), input("b", "hello", Some(-5)), false)
    .await
    .unwrap();

  assert!(answer.is_error);
  assert!(answer.plain().contains("b refused the line"), "{}", answer.plain());
}

#[tokio::test]
async fn a_call_in_no_agents_turn_reaches_nothing() {
  let asked = stub(StatusCode::CREATED, delivered());
  let seen = asked.asked.clone();
  let api = server(asked).await;
  let answer = send_with(&api, &session(None, Some(-5)), input("b", "hello", None), false)
    .await
    .unwrap();

  assert!(answer.is_error);
  assert!(answer.plain().contains("no agent's turn"), "{}", answer.plain());
  assert!(seen.lock().unwrap().asked.is_empty());
}

#[tokio::test]
async fn agency_is_asked_where_the_data_roots_env_file_names_it() {
  let agency = super::agency_named_by_its_file().await;
  let api = Api::read(agency.root.path(), |_| None);

  send_with(&api, &session(Some("a"), None), input("b", "hello", Some(-5)), false)
    .await
    .unwrap();

  agency.assert_asked_under_the_files_token();
}
