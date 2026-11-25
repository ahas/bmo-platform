//! A stand-in for the job service and the receiver in one, as the queue's notices reach them: items by their
//! number, the queues, the leases that stand, the events past a cursor and the window's oldest, and every call
//! heard; the questions asked, each told as `question.created` and settled by a reply as the service writes it;
//! the receiver answers each instruction with an id of its own; and the paths that fail while a test holds them
//! failing, and the calls whose answer is lost after they landed; and the label values put to it, each key's taken.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::IntoResponse;
use serde_json::{Value, json};
use tokio::net::TcpListener;

/// One call the stand-in heard.
#[derive(Clone, Debug)]
pub struct Heard {
  pub method: Method,
  pub path: String,
  pub query: String,
  pub body: Value,
  /// The bearer token the call carried, where it carried one.
  pub bearer: Option<String>,
}

/// What the stand-in holds; a test sets it and reads the calls back.
#[derive(Default)]
pub struct Held {
  pub items: BTreeMap<i64, Value>,
  pub queues: Vec<Value>,
  /// The leases that stand and are renewed; any other is gone.
  pub leases: BTreeSet<i64>,
  /// The lease a take answers, where one is taken; a conflict where none.
  pub next_lease: Option<i64>,
  pub events: Vec<Value>,
  /// The oldest event the window holds; a cursor below it is answered 410.
  pub window_from: i64,
  pub calls: Vec<Heard>,
  pub instructions: i64,
  /// The paths answered 503 while they stand here: a peer restarting under the call.
  pub failing: BTreeSet<String>,
  /// Each item's notes, oldest first, as the job service keeps them.
  pub notes: BTreeMap<i64, Vec<Value>>,
  /// The questions asked, oldest first, as the job service keeps them.
  pub questions: Vec<Value>,
  /// The calls, as `<method> <path>`, that land but whose answer is lost: answered 503 after they are made.
  pub lost: BTreeSet<String>,
}

pub type Peers = Arc<Mutex<Held>>;

impl Held {
  /// The bodies of the instructions queued, in order.
  pub fn instructed(&self) -> Vec<Value> {
    self.posted("/instructions")
  }

  fn posted(&self, path: &str) -> Vec<Value> {
    self
      .calls
      .iter()
      .filter(|call| call.method == Method::POST && call.path == path)
      .map(|call| call.body.clone())
      .collect()
  }

  /// A note added to an item, as the job service keeps it: its id, its text, its writer, its labels and the
  /// moment it was written.
  pub fn note(&mut self, item: i64, text: &str, labels: Value) {
    let id = self.notes.values().map(Vec::len).sum::<usize>() as i64 + 1;
    let note = json!({ "id": id, "itemId": item, "text": text, "actorName": "tester", "labels": labels,
                       "createdAt": chrono::Utc::now().timestamp_millis() });

    self.notes.entry(item).or_default().push(note);
  }

  // The event told next, numbered past every event held.
  fn told(&mut self, kind: &str, data: Value) {
    let id = self.events.iter().filter_map(|event| event["id"].as_i64()).max().unwrap_or(0) + 1;

    self.events.push(json!({ "id": id, "kind": kind, "at": id, "data": data }));
  }

  /// A question asked, kept open and told as the job service tells it.
  fn ask(&mut self, asked: &Value) -> Value {
    let question = json!({
      "id": self.questions.len() as i64 + 1,
      "itemId": asked["itemId"],
      "askerName": asked["askerName"],
      "text": asked["text"],
      "choices": asked["choices"],
      "state": "open",
      "toldByAsker": asked["toldByAsker"].as_bool().unwrap_or(false),
    });

    self.questions.push(question.clone());
    self.told("question.created", question.clone());

    question
  }

  /// A person's settling reply to a question, which answers it, told as the job service tells it.
  pub fn settle(&mut self, question: i64, choice: &str) {
    if let Some(asked) = self.questions.iter_mut().find(|asked| asked["id"] == question) {
      asked["state"] = json!("answered");
    }

    self.told(
      "reply.created",
      json!({ "id": question, "questionId": question, "choice": choice, "text": choice, "settles": true }),
    );
  }

  /// The label values put, each as its key and the values put, in order.
  pub fn label_values(&self) -> Vec<(String, Value)> {
    self
      .calls
      .iter()
      .filter(|call| call.method == Method::PUT && call.path.starts_with("/label-values/"))
      .map(|call| (call.path.trim_start_matches("/label-values/").to_owned(), call.body["values"].clone()))
      .collect()
  }

  /// The instructions withdrawn, by their id.
  pub fn withdrawn(&self) -> Vec<String> {
    self
      .calls
      .iter()
      .filter(|call| call.method == Method::DELETE && call.path.starts_with("/instructions/"))
      .map(|call| call.path.trim_start_matches("/instructions/").to_owned())
      .collect()
  }
}

fn query_of(uri: &Uri) -> BTreeMap<String, String> {
  uri
    .query()
    .unwrap_or_default()
    .split('&')
    .filter_map(|pair| pair.split_once('='))
    .map(|(key, value)| (key.to_owned(), value.replace("%2C", ",").replace("%2c", ",")))
    .collect()
}

async fn answer(
  State(peers): State<Peers>,
  method: Method,
  uri: Uri,
  headers: HeaderMap,
  body: Bytes,
) -> axum::response::Response {
  let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
  let mut held = peers.lock().unwrap();
  let query = query_of(&uri);

  held.calls.push(Heard {
    method: method.clone(),
    path: uri.path().to_owned(),
    query: uri.query().unwrap_or_default().to_owned(),
    body: body.clone(),
    bearer: headers
      .get("authorization")
      .and_then(|value| value.to_str().ok())
      .and_then(|value| value.strip_prefix("Bearer "))
      .map(str::to_owned),
  });

  let path = uri.path().to_owned();

  if held.failing.contains(&path) {
    return StatusCode::SERVICE_UNAVAILABLE.into_response();
  }

  let answered = match (method.as_str(), path.as_str()) {
    ("POST", "/instructions") => {
      held.instructions += 1;
      (StatusCode::CREATED, axum::Json(json!({ "id": held.instructions }))).into_response()
    }
    ("DELETE", path) if path.starts_with("/instructions/") => StatusCode::NO_CONTENT.into_response(),
    ("PUT", path) if path.starts_with("/label-values/") => axum::Json(body.clone()).into_response(),
    ("GET", "/queues") => {
      axum::Json(json!({ "items": held.queues.clone(), "more": false })).into_response()
    }
    ("GET", "/items") => {
      let states: Vec<&str> =
        query.get("state").map(|states| states.split(',').collect()).unwrap_or_default();
      let items: Vec<Value> = held
        .items
        .values()
        .filter(|item| states.is_empty() || states.contains(&item["state"].as_str().unwrap_or_default()))
        .cloned()
        .collect();

      axum::Json(json!({ "items": items, "more": false })).into_response()
    }
    ("POST", path) if path.starts_with("/items/") && path.ends_with("/notes") => {
      let item: i64 = path
        .trim_start_matches("/items/")
        .trim_end_matches("/notes")
        .parse()
        .unwrap_or_default();
      let labels = body["labels"].clone();

      held.note(item, body["text"].as_str().unwrap_or_default(), labels);
      (StatusCode::CREATED, axum::Json(held.notes[&item].last().cloned())).into_response()
    }
    ("GET", path) if path.starts_with("/items/") && path.ends_with("/notes") => {
      let item: i64 = path
        .trim_start_matches("/items/")
        .trim_end_matches("/notes")
        .parse()
        .unwrap_or_default();
      let notes = held.notes.get(&item).cloned().unwrap_or_default();

      axum::Json(json!({ "items": notes, "more": false })).into_response()
    }
    ("GET", path) if path.starts_with("/items/") => {
      let id: i64 = path.trim_start_matches("/items/").parse().unwrap_or_default();

      match held.items.get(&id) {
        Some(item) => axum::Json(item.clone()).into_response(),
        None => (StatusCode::NOT_FOUND, axum::Json(json!({ "code": "not_found" }))).into_response(),
      }
    }
    ("PATCH", path) if path.starts_with("/leases/") => {
      let id: i64 = path.trim_start_matches("/leases/").parse().unwrap_or_default();

      if held.leases.contains(&id) {
        axum::Json(json!({ "id": id })).into_response()
      } else {
        (StatusCode::GONE, axum::Json(json!({ "code": "gone" }))).into_response()
      }
    }
    ("POST", "/leases") => match held.next_lease {
      Some(lease) => (StatusCode::CREATED, axum::Json(json!({ "id": lease }))).into_response(),
      None => (StatusCode::CONFLICT, axum::Json(json!({ "code": "conflict" }))).into_response(),
    },
    ("GET", "/events") => {
      let after: i64 = query.get("after").and_then(|after| after.parse().ok()).unwrap_or(0);

      if after > 0 && after < held.window_from {
        return (StatusCode::GONE, axum::Json(json!({ "code": "gone" }))).into_response();
      }

      let kinds: Vec<&str> = query.get("kinds").map(|kinds| kinds.split(',').collect()).unwrap_or_default();
      let events: Vec<Value> = held
        .events
        .iter()
        .filter(|event| event["id"].as_i64().unwrap_or_default() > after)
        .filter(|event| event["id"].as_i64().unwrap_or_default() >= held.window_from)
        .filter(|event| kinds.is_empty() || kinds.contains(&event["kind"].as_str().unwrap_or_default()))
        .cloned()
        .collect();

      axum::Json(json!({ "items": events, "more": false })).into_response()
    }
    ("POST", "/questions") => (StatusCode::CREATED, axum::Json(held.ask(&body))).into_response(),
    ("GET", path) if path.starts_with("/questions/") => {
      let id: i64 = path.trim_start_matches("/questions/").parse().unwrap_or_default();

      match held.questions.iter().find(|asked| asked["id"] == id) {
        Some(question) => axum::Json(question.clone()).into_response(),
        None => (StatusCode::NOT_FOUND, axum::Json(json!({ "code": "not_found" }))).into_response(),
      }
    }
    ("GET", "/questions") => {
      let questions: Vec<Value> = held
        .questions
        .iter()
        .filter(|asked| {
          query.get("itemId").is_none_or(|item| item.parse().ok() == asked["itemId"].as_i64())
        })
        .filter(|asked| query.get("state").is_none_or(|state| asked["state"] == state.as_str()))
        .cloned()
        .collect();

      axum::Json(json!({ "items": questions, "more": false })).into_response()
    }
    _ => StatusCode::NOT_FOUND.into_response(),
  };

  if held.lost.contains(&format!("{method} {path}")) {
    return StatusCode::SERVICE_UNAVAILABLE.into_response();
  }

  answered
}

/// The stand-in served, with the settings that name it as both the job service and the receiver.
pub async fn stand() -> (Peers, BTreeMap<String, String>) {
  let peers: Peers = Arc::default();
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());
  let app = axum::Router::new().fallback(answer).with_state(peers.clone());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  let vars = [
    ("BMO_JOB_URL".to_owned(), address.clone()),
    ("BMO_JOB_TOKEN".to_owned(), "job-token".to_owned()),
    ("BMO_JOB_CHAT_TOKEN".to_owned(), "supervisors-token".to_owned()),
    ("BMO_RECEIVER_URL".to_owned(), address),
    ("BMO_RECEIVER_TOKEN".to_owned(), "receiver-token".to_owned()),
  ]
  .into();

  (peers, vars)
}

/// An item as the job service answers it.
pub fn item(id: i64, state: &str, extra: Value) -> Value {
  let mut item = json!({ "id": id, "title": format!("item {id}"), "state": state, "queueName": "todo" });

  if let (Some(item), Some(extra)) = (item.as_object_mut(), extra.as_object()) {
    item.extend(extra.clone());
  }

  item
}
