//! A slot request's changes published as `slot-request.changed` (docs/api.md, 「Events」): a request asked as
//! waiting, let in, bound to its run, given back, released with its owner, promoted into a reservation, and
//! given back on evidence its run ended; its data an object whose fields name the request and its lease.

use std::collections::HashSet;

use agency_server::order_run::dispatch::{self, Asked, COWORK, Seats, SlotRequestChanged, Wanted};
use serde_json::{Value, json};

use crate::support::grounds::{deepseek_balance, seeded};
use crate::support::scratch::Scratch;
use crate::support::standing;

fn asked<'a>(key: &str, owner: &str, title: Option<&'a str>, asked_by: Option<&'a str>) -> Asked<'a> {
  Asked {
    request_key: format!("test:{key}"),
    owner: owner.to_owned(),
    wanted: Wanted {
      role: "worker",
      path: Some("/checkouts/slot"),
      fixed: None,
      agent: None,
    },
    title,
    asked_by,
    ..Asked::default()
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_slot_ask_publishes_its_waiting_request_as_an_object_on_the_stream() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let mut typed = ground.events.subscribe::<SlotRequestChanged>();
  let mut wire = ground.events.subscribe_wire(Some(vec!["slot-request.changed".into()]));

  let held = dispatch::enqueue(
    &ground,
    &asked("cowork-one", "owner:cowork-one", Some("read the log"), Some("chat-agent")),
    COWORK,
    async |_, _| Ok(()),
  )
  .await
  .unwrap();

  let event = typed.next().await.expect("the ask was published");

  assert_eq!(event.token, held.token);
  assert_eq!(event.source, COWORK);
  assert_eq!(event.state, "waiting");
  assert_eq!(event.role, "worker");
  assert_eq!(event.owner, "owner:cowork-one");
  assert_eq!(event.asked_by.as_deref(), Some("chat-agent"));
  assert_eq!(event.title.as_deref(), Some("read the log"));
  assert_eq!(event.provider, None, "a waiting request names no road yet");
  assert_eq!(event.model, None);
  assert!(event.enqueued_at > 0);

  let named = wire.next().await.expect("the kind crosses the stream");

  assert_eq!(named.name, "slot-request.changed");
  let body = named.body.as_object().expect("the console relay reads an object");

  assert_eq!(body["token"], held.token);
  assert_eq!(body["state"], "waiting");
  assert_eq!(body["askedBy"], "chat-agent");
  assert_eq!(body["title"], "read the log");
  assert_eq!(body["enqueuedAt"], event.enqueued_at);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_admission_publishes_the_request_let_in_on_its_road() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;

  let mut events = ground.events.subscribe::<SlotRequestChanged>();
  let held =
    dispatch::enqueue(&ground, &asked("let-in", "owner:let-in", None, None), "job", async |_, _| Ok(()))
      .await
      .unwrap();
  let _waiting = events.next().await.expect("the ask was published");

  let let_in = dispatch::acquire(&ground, &held).await.unwrap().expect("within the cap");
  let event = events.next().await.expect("the admission was published");

  assert_eq!(event.token, let_in.token);
  assert_eq!(event.state, "active");
  assert_eq!(event.provider.as_deref(), Some("deepseek"));
  assert_eq!(event.model.as_deref(), Some("deepseek-flash"));
  assert!(event.leased_at.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_binding_publishes_the_child_and_the_run_it_names() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;

  let mut events = ground.events.subscribe::<SlotRequestChanged>();
  let held =
    dispatch::enqueue(&ground, &asked("bound", "owner:bound", None, None), "job", async |_, _| Ok(()))
      .await
      .unwrap();
  let _waiting = events.next().await.unwrap();
  let let_in = dispatch::acquire(&ground, &held).await.unwrap().expect("within the cap");
  let _active = events.next().await.unwrap();

  dispatch::bind(&ground.db, &ground.events, &let_in, "run-seven", Some("research-7"))
    .await
    .unwrap();

  let event = events.next().await.expect("the binding was published");

  assert_eq!(event.state, "active");
  assert_eq!(event.child_id.as_deref(), Some("run-seven"));
  assert_eq!(event.agent_name.as_deref(), Some("research-7"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_finish_publishes_the_request_given_back_with_its_outcome() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;

  let mut events = ground.events.subscribe::<SlotRequestChanged>();
  let held = dispatch::enqueue(
    &ground,
    &asked("finished", "owner:finished", None, None),
    "job",
    async |_, _| Ok(()),
  )
  .await
  .unwrap();
  let _waiting = events.next().await.unwrap();
  let let_in = dispatch::acquire(&ground, &held).await.unwrap().expect("within the cap");
  let _active = events.next().await.unwrap();

  assert!(
    dispatch::finish(&ground.db, &ground.events, &let_in.token, "done", false)
      .await
      .unwrap()
  );

  let event = events.next().await.expect("the finish was published");

  assert_eq!(event.state, "released");
  assert_eq!(event.outcome.as_deref(), Some("done"));
  assert!(event.ended_at.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_owner_release_publishes_each_request_it_gives_back() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let mut events = ground.events.subscribe::<SlotRequestChanged>();

  let first =
    dispatch::enqueue(&ground, &asked("batch-one", "owner:batch", None, None), "job", async |_, _| Ok(()))
      .await
      .unwrap();
  let second =
    dispatch::enqueue(&ground, &asked("batch-two", "owner:batch", None, None), "job", async |_, _| Ok(()))
      .await
      .unwrap();
  let _first = events.next().await.unwrap();
  let _second = events.next().await.unwrap();

  let done = dispatch::release_owner(&ground.db, &ground.events, "owner:batch").await.unwrap();

  assert_eq!(done.cancelled, 2);

  let released: HashSet<String> = [events.next().await.unwrap(), events.next().await.unwrap()]
    .into_iter()
    .map(|event| {
      assert_eq!(event.state, "cancelled");

      event.token
    })
    .collect();

  assert_eq!(released, HashSet::from([first.token, second.token]));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reservation_publishes_the_slot_it_raises_into_reserved() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let mut events = ground.events.subscribe::<SlotRequestChanged>();

  let reservation = dispatch::reserve(
    &ground.db,
    &ground.events,
    &Seats {
      reservation_key: "native:7:worker".into(),
      owner: "native:7".into(),
      source: "native",
      role: "worker",
      count: 1,
    },
  )
  .await
  .unwrap();
  let event = events.next().await.expect("the raised slot was published");

  assert_eq!(event.state, "reserved");
  assert_eq!(event.source, "native");
  assert_eq!(event.owner, "native:7");
  assert!(event.leased_at.is_some(), "{reservation:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_vanished_run_publishes_the_request_given_back_on_its_evidence() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;

  let mut events = ground.events.subscribe::<SlotRequestChanged>();
  let held = dispatch::enqueue(
    &ground,
    &asked("vanished", "owner:vanished", None, None),
    "job",
    async |_, _| Ok(()),
  )
  .await
  .unwrap();
  let _waiting = events.next().await.unwrap();
  let let_in = dispatch::acquire(&ground, &held).await.unwrap().expect("within the cap");
  let _active = events.next().await.unwrap();
  dispatch::bind(&ground.db, &ground.events, &let_in, "run-gone", None).await.unwrap();
  let _bound = events.next().await.unwrap();

  let released = dispatch::release_ended(
    &ground.db,
    &ground.events,
    &[(let_in.token.clone(), "run-gone".to_owned(), true)],
    &HashSet::new(),
  )
  .await
  .unwrap();

  assert_eq!(released, 1);

  let event = events.next().await.expect("the evidence was published");

  assert_eq!(event.state, "released");
  assert_eq!(event.outcome.as_deref(), Some("its launched child no longer stands"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_slot_ask_reaches_the_boundary_stream() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let client = reqwest::Client::new();
  let asked: Value = client
    .post(format!("http://127.0.0.1:{port}/slot-requests"))
    .bearer_auth(token)
    .json(&json!({ "role": "helper", "requestKey": "events:one", "owner": "events" }))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  let seat = asked["token"].as_str().expect("the ask was answered").to_owned();
  let page: Value = client
    .get(format!("http://127.0.0.1:{port}/events"))
    .bearer_auth(token)
    .header("accept", "application/json")
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  let event = page["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|event| event["kind"] == "slot-request.changed" && event["data"]["token"] == seat.as_str())
    .unwrap_or_else(|| panic!("the change left through the stream: {page}"));

  assert!(event["data"].is_object(), "{event}");
  assert_eq!(event["data"]["state"], "waiting");
  assert_eq!(event["data"]["source"], "cowork");
  assert_eq!(event["data"]["owner"], "events");
}
