//! The event bus: an event reaches every subscriber of its kind from the moment it subscribed, and nobody
//! where there is none; a stream across the boundary reads the kinds it names, each by its name and its
//! JSON; and a durable bus keeps its events in the store under numbers that go on rising across a restart,
//! those published on the heels of a drained server's stop among them, so a reader resumes after the last
//! number it read, and is told where what it asks is no longer kept (docs/api.md, 「events」, 「health」).

use std::borrow::Cow;
use std::sync::atomic::AtomicI64;

use agency_server::events::{Bus, Event, MISSED, Missed, Named, drop_oldest};
use futures_util::StreamExt;
use serde_json::{Value, json};

use crate::support::scratch::Scratch;
use crate::support::standing;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
struct Said(String);

impl Event for Said {
  const NAME: &'static str = "said";
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
struct Ended(u32);

impl Event for Ended {
  const NAME: &'static str = "ended";
}

#[test]
fn an_event_with_no_subscriber_reaches_nobody() {
  let bus = Bus::default();

  bus.publish(Said("unheard".into()));

  assert_eq!(bus.subscribers::<Said>(), 0);

  let mut late = bus.subscribe::<Said>();

  assert_eq!(late.ready(), None, "a subscription reads from the moment it was made");
}

#[tokio::test]
async fn every_subscriber_of_a_kind_takes_each_event_and_no_other_kind() {
  let bus = Bus::default();
  let mut first = bus.subscribe::<Said>();
  let mut second = bus.clone().subscribe::<Said>();
  let mut ended = bus.subscribe::<Ended>();

  bus.publish(Said("hello".into()));
  bus.publish(Said("again".into()));

  assert_eq!(first.next().await, Some(Said("hello".into())));
  assert_eq!(first.next().await, Some(Said("again".into())));
  assert_eq!(second.ready(), Some(Said("hello".into())));
  assert_eq!(second.ready(), Some(Said("again".into())));
  assert_eq!(ended.ready(), None);

  bus.publish(Ended(7));

  assert_eq!(ended.ready(), Some(Ended(7)));
  assert_eq!(first.ready(), None);
}

#[test]
fn a_dropped_subscription_is_let_go() {
  let bus = Bus::default();
  let kept = bus.subscribe::<Said>();
  let dropped = bus.subscribe::<Said>();

  assert_eq!(bus.subscribers::<Said>(), 2);

  drop(dropped);

  assert_eq!(bus.subscribers::<Said>(), 1);

  bus.publish(Said("still heard".into()));
  drop(kept);
  bus.publish(Said("unheard".into()));

  assert_eq!(bus.subscribers::<Said>(), 0);
}

#[tokio::test]
async fn a_stream_reads_the_kinds_it_names_by_name_and_json() {
  let bus = Bus::default();
  let mut said_only = bus.subscribe_wire(Some(vec!["said".into()]));
  let mut every = bus.subscribe_wire(None);

  bus.publish(Ended(7));
  bus.publish(Said("hello".into()));

  let said = said_only.next().await.unwrap();
  assert_eq!(said.name, "said");
  assert_eq!(said.body, json!("hello"));
  let ended = every.next().await.unwrap();
  assert_eq!(ended.name, "ended");
  assert_eq!(ended.body, json!(7));
  assert!(said.id > ended.id);
  let relayed = every.next().await.unwrap();
  assert_eq!(relayed.name, "said");
  assert_eq!(relayed.body, json!("hello"));
}

#[tokio::test]
async fn an_event_known_by_its_name_reaches_the_streams_reading_its_kind_and_no_typed_subscriber() {
  let bus = Bus::default();
  let mut typed = bus.subscribe::<Said>();
  let mut said_only = bus.subscribe_wire(Some(vec!["said".into()]));
  let mut ended_only = bus.subscribe_wire(Some(vec!["ended".into()]));

  bus.publish_named(Named {
    id: 0,
    at: 0,
    name: "said".to_owned().into(),
    body: json!("relayed"),
  });

  let relayed = said_only.ready().unwrap();
  assert_eq!(relayed.name, "said");
  assert_eq!(relayed.body, json!("relayed"));
  assert_eq!(said_only.ready(), None);
  assert_eq!(ended_only.ready(), None);
  assert_eq!(typed.ready(), None);
}

#[tokio::test]
async fn a_road_a_turn_walked_crosses_the_stream_by_its_kind() {
  use agency_server::herdr_backend::RoadWalked;

  let bus = Bus::default();
  let mut roads = bus.subscribe_wire(Some(vec!["road-walked".into()]));
  let road =
    RoadWalked::of(-5, "run-1", "claude-code", "model-a", true, std::time::Instant::now(), &[], None);

  bus.publish(road);

  let walked = roads.ready().expect("a road walked is published across the boundary");
  assert_eq!(walked.name, "road-walked");
  assert_eq!(
    (&walked.body["chatId"], &walked.body["run"], &walked.body["outcome"]),
    (&json!(-5), &json!("run-1"), &json!("answered"))
  );
}

fn named(kind: &str, data: Value) -> Named {
  Named {
    id: 0,
    at: 0,
    name: Cow::Owned(kind.to_owned()),
    body: data,
  }
}

fn ids(events: &[Named]) -> Vec<i64> {
  events.iter().map(|event| event.id).collect()
}

#[tokio::test]
async fn a_durable_bus_s_numbers_rise_across_a_restart_and_a_reader_resumes_from_the_store() {
  let scratch = Scratch::new().await;
  let before = Bus::default();

  before.durable(scratch.db(), 100).await.unwrap();

  for at in 0..3 {
    before.publish_named(named("job.changed", json!({ "n": at })));
  }

  before.publish_named(named("run.changed", json!({})));
  before.flushed().await;

  let first = before.newest() - 3;

  // The next process starts with nothing in memory: what it answers is the store's.
  let after = Bus::default();

  after.durable(scratch.db(), 100).await.unwrap();

  let kinds = ["job.changed".to_owned()];
  let resumed = after.replay(Some(first), Some(&kinds), 10).await;

  assert_eq!(ids(&resumed.events), vec![first + 1, first + 2]);
  assert_eq!(resumed.events[1].body, json!({ "n": 2 }));
  assert_eq!((resumed.more, resumed.missed), (false, None));

  let paged = after.replay(Some(first), None, 2).await;

  assert_eq!((ids(&paged.events), paged.more), (vec![first + 1, first + 2], true));

  // Its own numbers go on above every number the process before it gave.
  after.publish_named(named("job.changed", json!({ "n": 3 })));

  assert!(after.newest() > before.newest());
  assert_eq!(ids(&after.replay(Some(before.newest()), None, 10).await.events), vec![after.newest()]);
}

#[tokio::test]
async fn a_reader_asking_after_what_is_no_longer_kept_or_a_number_never_given_is_told_it_missed() {
  let scratch = Scratch::new().await;
  let before = Bus::default();

  before.durable(scratch.db(), 100).await.unwrap();

  for at in 0..5 {
    before.publish_named(named("job.changed", json!({ "n": at })));
  }

  before.flushed().await;

  let first = before.newest() - 4;

  drop_oldest(scratch.db(), 2, &AtomicI64::new(0)).await.unwrap();

  let after = Bus::default();

  after.durable(scratch.db(), 100).await.unwrap();

  let late = after.replay(Some(first), None, 10).await;

  assert_eq!(ids(&late.events), vec![first + 3, first + 4]);
  assert_eq!(
    late.missed,
    Some(Missed {
      after: first,
      kept_from: Some(first + 3),
    })
  );
  assert_eq!(late.missed.unwrap().data(), json!({ "after": first, "keptFrom": first + 3 }));

  // A number at the edge of what is kept misses nothing.
  assert_eq!(after.replay(Some(first + 2), None, 10).await.missed, None);

  // A number the service never gave is a numbering the reader cannot resume from.
  let unknown = after.replay(Some(after.newest() + 50), None, 10).await;

  assert!(unknown.events.is_empty());
  assert_eq!(unknown.missed.map(|missed| missed.after), Some(after.newest() + 50));
}

async fn published(port: u16, token: &str, kind: &str) {
  let answer = reqwest::Client::new()
    .post(format!("http://127.0.0.1:{port}/events"))
    .bearer_auth(token)
    .json(&json!({ "kind": kind, "data": { "said": kind } }))
    .send()
    .await
    .unwrap();

  assert_eq!(answer.status().as_u16(), 201);
}

async fn page(port: u16, token: &str, after: i64) -> Value {
  reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}/events?after={after}"))
    .bearer_auth(token)
    .header("accept", "application/json")
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap()
}

// A stream resumed after a number.
async fn stream_after(port: u16, token: &str, after: i64) -> reqwest::Response {
  reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}/events"))
    .bearer_auth(token)
    .header("last-event-id", after.to_string())
    .send()
    .await
    .unwrap()
}

// What a stream sends next, read until `until` stands in it.
async fn read_until(answer: &mut reqwest::Response, until: &str) -> String {
  let mut text = String::new();

  while !text.contains(until) {
    let Some(chunk) = answer.chunk().await.unwrap() else { break };

    text.push_str(&String::from_utf8_lossy(&chunk));
  }

  text
}

#[tokio::test]
async fn a_stream_resumed_across_the_server_s_restart_sends_what_it_kept_and_tells_a_number_never_given() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  let first = standing::stand_server(home.path(), &data, &scratch.address(), token);

  published(first.port, token, "probe.one").await;
  published(first.port, token, "probe.two").await;

  let kept = page(first.port, token, 0).await;
  let probes: Vec<i64> = kept["items"]
    .as_array()
    .unwrap()
    .iter()
    .filter(|item| item["kind"].as_str().is_some_and(|kind| kind.starts_with("probe.")))
    .map(|item| item["id"].as_i64().unwrap())
    .collect();

  assert_eq!(probes.len(), 2, "{kept}");

  // The events reach the store as they are published; the server is stopped once they stand there.
  for _ in 0..200 {
    let stored = scratch
      .count(r#"SELECT count(*) FROM "stream_events" WHERE "kind" LIKE 'probe.%'"#, vec![])
      .await;

    if stored == 2 {
      break;
    }

    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
  }

  drop(first);

  let second = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let resumed = page(second.port, token, probes[0]).await;
  let after_first: Vec<&str> = resumed["items"]
    .as_array()
    .unwrap()
    .iter()
    .filter_map(|item| item["kind"].as_str())
    .filter(|kind| kind.starts_with("probe."))
    .collect();

  assert_eq!(after_first, vec!["probe.two"], "{resumed}");
  assert_eq!(resumed["missed"], Value::Null, "{resumed}");

  let text = read_until(&mut stream_after(second.port, token, probes[0]).await, "probe.two").await;

  assert!(text.contains(&format!("id: {}", probes[1])), "{text}");
  assert!(!text.contains(MISSED), "{text}");

  // A number this numbering never gave: the notice comes first, and what is published after it is sent.
  let mut stream = stream_after(second.port, token, i64::MAX - 1).await;
  let text = read_until(&mut stream, MISSED).await;

  assert!(text.contains(&format!("event: {MISSED}")), "{text}");
  assert!(text.contains(&format!("\"after\":{}", i64::MAX - 1)), "{text}");

  published(second.port, token, "probe.three").await;

  let text = read_until(&mut stream, "probe.three").await;

  assert!(text.contains("event: probe.three"), "{text}");
}

// How many publishes of the burst are in hand at once.
const IN_FLIGHT: usize = 32;

#[tokio::test]
async fn every_event_published_before_the_server_s_stop_is_read_by_the_reader_resuming_on_the_server_after()
{
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";
  let published_before = 1000;

  std::fs::create_dir_all(&data).unwrap();

  let mut first = standing::stand_server(home.path(), &data, &scratch.address(), token);
  // One client for every call, speaking plain HTTP: it loads no root certificates, so it stands where the
  // machine offers none, and its connections stay few however many events go out.
  let client = reqwest::Client::builder().tls_certs_only(Vec::new()).build().unwrap();

  // Each publish is answered once the event has its number, before the store has it; they come a few at a
  // time, faster than the store takes them, and the stop on the heels of the last.
  let port = first.port;
  let publish = |_| {
    let client = client.clone();

    async move {
      client
        .post(format!("http://127.0.0.1:{port}/events"))
        .bearer_auth(token)
        .json(&json!({ "kind": "probe.burst", "data": { "said": "probe.burst" } }))
        .send()
        .await
        .unwrap()
        .status()
    }
  };
  let answered: Vec<reqwest::StatusCode> = futures_util::stream::iter(0..published_before)
    .map(publish)
    .buffer_unordered(IN_FLIGHT)
    .collect()
    .await;

  assert!(answered.iter().all(|status| status.as_u16() == 201), "{answered:?}");

  let ended = tokio::task::spawn_blocking(move || {
    let ended = first.stop(std::time::Duration::from_secs(30));

    (first, ended)
  })
  .await
  .unwrap();

  assert!(
    ended.1.is_some_and(|ended| ended.success()),
    "the server drains and ends: {:?}",
    ended.1
  );
  drop(ended.0);

  let second = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let read: Value = client
    .get(format!("http://127.0.0.1:{}/events?after=0&kinds=probe.burst&limit=1000", second.port))
    .bearer_auth(token)
    .header("accept", "application/json")
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();

  assert_eq!(read["items"].as_array().unwrap().len(), published_before, "{read}");
}
