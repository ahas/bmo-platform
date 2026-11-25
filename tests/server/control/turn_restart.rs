//! A turn across agency's own restart, proved on the server's own process (docs/sessions.md, 「A restart
//! hands the session over」; docs/api.md, 「sessions」, 「health」, 「events」): a keyed turn the process before
//! left in flight is picked up at the start, answers the progress read under its own id and takes a repeat of
//! its send under its key; the server stopped by `SIGTERM` while that send waits answers it `503` `stopping`
//! with the key and ends; the server started again picks the turn up once more, the send asked again under the
//! key attaches to it and is answered with its end once its program writes it, the end published once under
//! its id and its key; and a repeat after the end is answered with it at once. A server killed outright under
//! the send and the stream leaves its caller a cut, which is no outcome: the send asked again of the server
//! after attaches to the same turn, and the stream resumed from the last number read sends its end.

use std::path::Path;
use std::time::Duration;

use agency_server::lines::place::Place;
use agency_server::now_ms;
use agency_server::turn_pickup::{InFlight, keep};
use futures_util::StreamExt;
use serde_json::{Value, json};
use sse_stream::SseStream;

use crate::support::scratch::Scratch;
use crate::support::standing::{Standing, stand_server};

const TOKEN: &str = "restart-token";
const SESSION: &str = "agent-b@conversation:9";

// The turn's program, standing for as long as the test does and taken down with it however the test ends.
struct Program(std::process::Child);

impl Program {
  fn stand() -> Self {
    Self(std::process::Command::new("sleep").arg("300").spawn().unwrap())
  }
}

impl Drop for Program {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

// A line of the pane program's record, as the terminal client appends it.
fn append(place: &Place, line: Value) {
  use std::io::Write as _;

  std::fs::create_dir_all(&place.path).unwrap();

  let mut file = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(place.path.join("loop.jsonl"))
    .unwrap();

  writeln!(file, "{line}").unwrap();
}

// The turn a process before the server sent under `key` and its program took: kept in flight, its program
// standing and its record naming it as it begun.
async fn left_in_flight(
  scratch: &Scratch,
  place: &Place,
  program: &Program,
  turn: i64,
  run: &str,
  key: &str,
) {
  let identity = agent_host::identity::ProcessIdentity::of(program.0.id() as i32).unwrap();

  std::fs::create_dir_all(&place.path).unwrap();
  std::fs::write(
    place.path.join("running.json"),
    json!({ "identity": "one", "pid": identity.pid, "birth": identity.birth, "loop": identity,
            "starts": 1, "startedAt": now_ms(), "recent": [] })
    .to_string(),
  )
  .unwrap();
  keep(
    scratch.db(),
    &InFlight {
      session: SESSION.into(),
      turn,
      run: run.into(),
      key: Some(key.into()),
      agent: "agent-b".into(),
      chat_id: Some(5),
      ledger: None,
      job: None,
      started_at: now_ms() - 1_000,
    },
  )
  .await;
  append(place, json!({ "kind": "start", "id": "w", "name": run, "title": "a turn", "at": 1 }));
}

// The program writes the turn's answer and its end.
fn answer(place: &Place) {
  append(
    place,
    json!({ "kind": "answer", "id": "w", "text": "The disk is 81% full.",
            "model": "[\"deepseek\",\"deepseek-flash\"]",
            "usage": { "inputTokens": 10, "outputTokens": 5, "cacheReadTokens": 0 } }),
  );
  append(place, json!({ "kind": "end", "id": "w" }));
}

// The send under `key`, answered whenever the server answers it, or the cut its connection met.
async fn try_send(port: u16, key: &str) -> Result<(u16, Value), reqwest::Error> {
  let answer = reqwest::Client::new()
    .post(format!("http://127.0.0.1:{port}/sessions/{SESSION}/turns"))
    .bearer_auth(TOKEN)
    .json(&json!({ "prompt": "What is the disk?", "key": key }))
    .send()
    .await?;
  let status = answer.status().as_u16();

  Ok((status, answer.json().await.unwrap_or(Value::Null)))
}

async fn send(port: u16, key: &str) -> (u16, Value) {
  try_send(port, key).await.unwrap()
}

async fn read(port: u16, path: &str) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(TOKEN)
    .header("Accept", "application/json")
    .send()
    .await
    .unwrap();
  let status = answer.status().as_u16();

  (status, answer.json().await.unwrap_or(Value::Null))
}

// The event stream of `turn.ended`, resumed after `after`.
async fn stream(port: u16, after: i64) -> reqwest::Response {
  let answer = reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}/events?kinds=turn.ended"))
    .bearer_auth(TOKEN)
    .header("last-event-id", after.to_string())
    .send()
    .await
    .unwrap();

  assert_eq!(answer.status(), 200);
  answer
}

fn server(home: &Path, scratch: &Scratch) -> Standing {
  stand_server(home, &home.join("data"), &scratch.address(), TOKEN)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_keyed_turn_outlives_two_restarts_of_the_server_and_its_send_asked_again_gets_its_end() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let place = Place::of(&home.path().join("data"), "agent-b");
  let program = Program::stand();

  left_in_flight(&scratch, &place, &program, 43, "chat:5:3", "round-7").await;

  // The first server picks it up: the progress read answers it running under its id, and nothing is left
  // that a restart would cut.
  let mut first = server(home.path(), &scratch);
  let (status, reading) = read(first.port, &format!("/sessions/{SESSION}/turns/43")).await;

  assert_eq!(status, 200, "{reading}");
  assert_eq!((reading["id"].as_i64(), reading["state"].as_str()), (Some(43), Some("running")));
  assert_eq!(read(first.port, "/quiet").await.1, json!({ "quiet": true, "work": [] }));

  // A send under the turn's key attaches to it and waits; the server's stop answers it so its caller asks
  // again, and the server ends.
  let port = first.port;
  let waiting = tokio::spawn(async move { send(port, "round-7").await });

  tokio::time::sleep(Duration::from_millis(500)).await;
  assert!(!waiting.is_finished(), "the send waits on the turn standing under its key");

  let ended = tokio::task::spawn_blocking(move || {
    let ended = first.stop(Duration::from_secs(30));

    (first, ended)
  });
  let (status, said) = tokio::time::timeout(Duration::from_secs(30), waiting).await.unwrap().unwrap();

  assert_eq!(status, 503, "{said}");
  assert_eq!((said["code"].as_str(), said["key"].as_str()), (Some("stopping"), Some("round-7")));

  let (first, ended) = ended.await.unwrap();

  assert!(ended.is_some_and(|ended| ended.success()), "the server drains and ends: {ended:?}");
  drop(first);

  // The program went on with the turn while no server stood: it is still working on it.
  let second = server(home.path(), &scratch);
  let port = second.port;
  let (status, reading) = read(port, &format!("/sessions/{SESSION}/turns/43")).await;

  assert_eq!((status, reading["state"].as_str()), (200, Some("running")), "{reading}");

  let waiting = tokio::spawn(async move { send(port, "round-7").await });

  tokio::time::sleep(Duration::from_millis(500)).await;
  assert!(!waiting.is_finished(), "the send asked again attaches to the picked-up turn");

  answer(&place);

  let (status, answered) = tokio::time::timeout(Duration::from_secs(30), waiting).await.unwrap().unwrap();

  assert_eq!(status, 201, "{answered}");
  assert_eq!(answered["id"], 43);
  assert_eq!(answered["answer"], "The disk is 81% full.");
  assert_eq!(answered["prompt"], "What is the disk?");

  // A repeat after the end is answered with it at once, and nothing new goes into the session.
  let (status, again) = send(port, "round-7").await;

  assert_eq!(
    (status, again["id"].as_i64(), again["answer"].as_str()),
    (201, Some(43), Some("The disk is 81% full."))
  );

  // The end was published once, under the turn's own id and its caller's key.
  let (_, page) = read(port, "/events?kinds=turn.ended&limit=100").await;
  let ends: Vec<&Value> = page["items"]
    .as_array()
    .unwrap()
    .iter()
    .filter(|event| event["data"]["session"] == SESSION)
    .collect();

  assert_eq!(ends.len(), 1, "{page}");
  assert_eq!(
    (ends[0]["data"]["turn"].as_i64(), ends[0]["data"]["key"].as_str()),
    (Some(43), Some("round-7"))
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_kill_under_a_keyed_send_and_a_stream_leaves_cuts_both_resume_on_the_next_server() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let place = Place::of(&home.path().join("data"), "agent-b");
  let program = Program::stand();

  left_in_flight(&scratch, &place, &program, 44, "chat:5:4", "round-8").await;

  // The reader's last number before the cut, and its stream open on the first server.
  let first = server(home.path(), &scratch);
  let (_, page) = read(first.port, "/events?limit=1000").await;
  let cursor = page["items"].as_array().unwrap().iter().filter_map(|event| event["id"].as_i64()).max();
  let cursor = cursor.unwrap_or(0);
  let cut_stream = stream(first.port, cursor).await;
  let port = first.port;
  let waiting = tokio::spawn(async move { try_send(port, "round-8").await });

  tokio::time::sleep(Duration::from_millis(500)).await;
  assert!(!waiting.is_finished(), "the send waits on the turn standing under its key");

  // Killed where it stands: no drain, no answer, no `stopping` frame.
  drop(first);

  let cut = tokio::time::timeout(Duration::from_secs(30), waiting).await.unwrap().unwrap();

  assert!(cut.is_err(), "the kill answered the send: {cut:?}");

  let mut frames = std::pin::pin!(SseStream::from_bytes_stream(cut_stream.bytes_stream()));

  while let Some(frame) = tokio::time::timeout(Duration::from_secs(30), frames.next()).await.unwrap() {
    let Ok(frame) = frame else { break };

    assert_ne!(frame.event.as_deref(), Some("stopping"), "a killed server sent its drain's frame");
  }

  // The program went on with the turn; the same send asked again of the server after attaches to it.
  let second = server(home.path(), &scratch);
  let port = second.port;
  let waiting = tokio::spawn(async move { send(port, "round-8").await });

  tokio::time::sleep(Duration::from_millis(500)).await;
  assert!(!waiting.is_finished(), "the send asked again attaches to the picked-up turn");

  answer(&place);

  let (status, answered) = tokio::time::timeout(Duration::from_secs(30), waiting).await.unwrap().unwrap();

  assert_eq!((status, answered["id"].as_i64()), (201, Some(44)), "{answered}");
  assert_eq!(answered["answer"], "The disk is 81% full.");

  // The stream resumed from the last number read before the cut sends the turn's end, under its id and key.
  let mut frames = std::pin::pin!(SseStream::from_bytes_stream(stream(port, cursor).await.bytes_stream()));
  let ended = loop {
    let frame = tokio::time::timeout(Duration::from_secs(30), frames.next())
      .await
      .unwrap()
      .expect("the resumed stream ended before the turn's end")
      .unwrap();
    let data: Value = serde_json::from_str(frame.data.as_deref().unwrap_or("null")).unwrap();

    if frame.event.as_deref() == Some("turn.ended") && data["data"]["session"] == SESSION {
      assert!(frame.id.unwrap().parse::<i64>().unwrap() > cursor);
      break data;
    }
  };

  assert_eq!(
    (ended["data"]["turn"].as_i64(), ended["data"]["key"].as_str()),
    (Some(44), Some("round-8"))
  );
}
