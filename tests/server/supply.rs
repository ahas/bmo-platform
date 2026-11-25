//! A person's supply (docs/supply.md): the source a run takes as it starts, its reservations and their
//! settlement, the meter that extends and hands them on, the supply hook's kept registration, the gateway's call
//! keys, and the usage the ledger answers by person and by source. What the modules share stands here: a supply
//! consumer on the loopback that answers each ask as its test scripts it, the agency front on a port of its own,
//! and a person's run as it asks.

mod call_keys;
mod choice;
mod contract;
mod kept_hook;
mod meter;
mod organisation;
mod settlements;
mod told;
mod usage;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agency_server::control::{Front, router};
use agency_server::hooks::{Remote, register_remote};
use agency_server::places::Ground;
use agency_server::supply::hook::SupplyHook;
use agency_server::supply::{Asked, Choice, Chosen, Kind, Source, choose};
use axum::Json;
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// The person the runs are for.
pub const PERSON: i64 = 7;
/// The agent the person's runs go out as.
pub const AGENT: &str = "worker-seven";
/// The token the front is called under.
pub const TOKEN: &str = "secret";

/// A job's run of the person's on a road, as it asks as it starts.
pub fn asked(account: Option<i64>, provider: &'static str) -> Asked<'static> {
  Asked {
    kind: Kind::Job,
    id: 41,
    agent: AGENT,
    account,
    provider,
    model: "deepseek-flash",
    job: Some(41),
    supervisor: None,
    room: None,
  }
}

/// One of the person's own keys for a road.
pub async fn personal_key(db: &sea_orm::DatabaseConnection, road: &str) {
  store::run(
    db,
    r#"INSERT INTO "personal_supply" ("user_id", "road", "secret", "place", "added_at") VALUES ($1, $2, 'key', 1, 0)"#,
    vec![PERSON.into(), road.into()],
  )
  .await
  .unwrap();
}

/// A person's run gone out on the first source of an order of platform sources, the consumer scripted to
/// accept a reservation of each at its amount, `r-<source>`, expiring in an hour.
pub async fn reserved(ground: &Ground, consumer: &Consumer, sources: &[(&str, f64)]) -> Chosen {
  let expires = agency_server::now_ms() + 3_600_000;
  let names: Vec<&str> = sources.iter().map(|(name, _)| *name).collect();

  consumer.register(ground);
  consumer.answers("order", json!({ "sources": names }));

  for (name, amount) in sources {
    consumer.answers(
      &format!("reserve:{name}"),
      json!({ "accepted": true, "reservation": format!("r-{name}"), "amountUsd": amount, "expiresAt": expires }),
    );
  }

  match choose(ground, &asked(Some(PERSON), "deepseek")).await.unwrap() {
    Choice::Go(chosen) => chosen,
    Choice::Closed(closed) => panic!("the run was due to go out: {closed:?}"),
  }
}

/// The row of a reservation a run's source holds.
pub fn row_of(chosen: &Chosen) -> i64 {
  match chosen.source {
    Source::Platform { reservation, .. } => reservation,
    ref other => panic!("a reserved platform source was due: {other:?}"),
  }
}

/// A reservation's state and what it covered, as the store keeps them.
pub async fn state_of(db: &sea_orm::DatabaseConnection, row: i64) -> (String, i64) {
  let kept = store::one(
    db,
    r#"SELECT "state", "covered_micros" FROM "supply_reservations" WHERE "id" = $1"#,
    vec![row.into()],
  )
  .await
  .unwrap()
  .unwrap();

  (kept.try_get("", "state").unwrap(), kept.try_get("", "covered_micros").unwrap())
}

/// A settlement the supply outbox owes.
#[derive(Debug)]
pub struct Owed {
  pub key: String,
  pub ask: Value,
  pub tries: i64,
  pub due_at: i64,
  pub failure: Option<String>,
}

/// Every settlement the supply outbox owes, by key.
pub async fn owed(db: &sea_orm::DatabaseConnection) -> Vec<Owed> {
  store::all(
    db,
    r#"SELECT "key", "ask", "tries", "due_at", "failure" FROM "supply_settlements" ORDER BY "key""#,
    vec![],
  )
  .await
  .unwrap()
  .iter()
  .map(|row| Owed {
    key: row.try_get("", "key").unwrap(),
    ask: serde_json::from_str(&row.try_get::<String>("", "ask").unwrap()).unwrap(),
    tries: row.try_get("", "tries").unwrap(),
    due_at: row.try_get("", "due_at").unwrap(),
    failure: row.try_get("", "failure").unwrap(),
  })
  .collect()
}

/// A server a test stands on the loopback, taken down as the test ends.
pub struct Serving {
  pub port: u16,
  stop: Option<oneshot::Sender<()>>,
  served: Option<JoinHandle<()>>,
}

impl Serving {
  async fn stand(app: axum::Router) -> Self {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop, stopped) = oneshot::channel::<()>();
    let served = tokio::spawn(async move {
      let _ = axum::serve(listener, app)
        .with_graceful_shutdown(async {
          let _ = stopped.await;
        })
        .await;
    });

    Self {
      port,
      stop: Some(stop),
      served: Some(served),
    }
  }

  /// Taken down: told to stop, and cut where its connections outlast a bounded wait.
  pub async fn down(mut self) {
    if let Some(stop) = self.stop.take() {
      let _ = stop.send(());
    }

    if let Some(served) = self.served.take() {
      let cut = served.abort_handle();

      if tokio::time::timeout(Duration::from_secs(5), served).await.is_err() {
        cut.abort();
      }
    }
  }
}

impl Drop for Serving {
  fn drop(&mut self) {
    if let Some(served) = self.served.take() {
      served.abort();
    }
  }
}

/// The agency front over a ground, on a port of its own.
pub async fn front(ground: Arc<Ground>) -> Serving {
  Serving::stand(router(Front::new(TOKEN.to_owned(), ground).unwrap())).await
}

/// A call of the deployment's own to the front: its status and what it answered.
pub async fn call(port: u16, method: reqwest::Method, path: &str, body: Option<Value>) -> (u16, Value) {
  let mut asked = reqwest::Client::new()
    .request(method, format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(TOKEN);

  if let Some(body) = body {
    asked = asked.json(&body);
  }

  let answer = asked.send().await.unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

/// One ask the consumer was posted: the whole body as it came, the ask itself, and the bearer it came under.
#[derive(Clone, Debug)]
pub struct Heard {
  pub body: Value,
  pub ask: Value,
  pub bearer: Option<String>,
}

type Script = HashMap<String, (StatusCode, Value)>;

/// A supply consumer on the loopback: it keeps every ask posted to it and answers each as the test scripted
/// `<op>:<source>`, else `<op>`, else 500.
pub struct Consumer {
  pub url: String,
  heard: Arc<Mutex<Vec<Heard>>>,
  script: Arc<Mutex<Script>>,
  serving: Serving,
}

impl Consumer {
  pub async fn stand() -> Self {
    let heard: Arc<Mutex<Vec<Heard>>> = Arc::default();
    let script: Arc<Mutex<Script>> = Arc::default();
    let (kept, scripted) = (heard.clone(), script.clone());
    let app = axum::Router::new().fallback(move |headers: HeaderMap, Json(body): Json<Value>| async move {
      let ask = body["ask"].clone();
      let bearer = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_owned);
      let op = ask["op"].as_str().unwrap_or_default().to_owned();
      let source = ask["source"].as_str().unwrap_or_default().to_owned();

      kept.lock().unwrap().push(Heard { body, ask, bearer });

      let script = scripted.lock().unwrap();
      let (status, answer) = script
        .get(&format!("{op}:{source}"))
        .or_else(|| script.get(&op))
        .cloned()
        .unwrap_or((StatusCode::INTERNAL_SERVER_ERROR, json!({})));

      (status, Json(answer))
    });
    let serving = Serving::stand(app).await;

    Self {
      url: format!("http://127.0.0.1:{}/supply", serving.port),
      heard,
      script,
      serving,
    }
  }

  /// What an ask is answered from now: `answer` under 200.
  pub fn answers(&self, asked: &str, answer: Value) {
    self.replies(asked, StatusCode::OK, json!({ "answer": answer }));
  }

  /// What an ask is answered from now, its status and its whole body as given.
  pub fn replies(&self, asked: &str, status: StatusCode, body: Value) {
    self.script.lock().unwrap().insert(asked.to_owned(), (status, body));
  }

  /// Every ask posted so far, in order.
  pub fn heard(&self) -> Vec<Heard> {
    self.heard.lock().unwrap().clone()
  }

  /// The ops of the asks posted so far, in order.
  pub fn ops(&self) -> Vec<String> {
    self
      .heard()
      .iter()
      .map(|heard| heard.ask["op"].as_str().unwrap_or_default().to_owned())
      .collect()
  }

  /// The idempotency keys of the asks that move money posted so far, in order.
  pub fn keys(&self) -> Vec<String> {
    self
      .heard()
      .iter()
      .filter_map(|heard| heard.ask["key"].as_str().map(str::to_owned))
      .collect()
  }

  /// Registered at the supply point of a ground, under no token.
  pub fn register(&self, ground: &Ground) {
    register_remote::<SupplyHook>(
      &ground.hooks,
      Remote {
        url: self.url.clone(),
        token: None,
      },
    );
  }

  pub async fn down(self) {
    self.serving.down().await;
  }
}
