//! A turn re-attached to by its caller's key (docs/api.md, 「sessions」; docs/sessions.md, 「A restart hands the
//! session over」): two sends under one key begin one turn and both are answered by it, a refusal keeps
//! nothing, a keyed end is kept for a repeat and dropped past its span; a keyed turn picked up at the start
//! answers the progress read under its own id, stands under its key and hands its end to a repeat of its
//! send; and the quiet counts a turn the server carries for a supervisor that stands that no program has
//! taken, and nothing else.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::herdr_backend::TurnEnded;
use agency_server::lines::place::Place;
use agency_server::now_ms;
use agency_server::places::Ground;
use agency_server::queue::settings;
use agency_server::turn_keys::{Claim, KeptEnd, Settled, TurnKeys, kept, prune};
use agency_server::turn_pickup::{Carrying, InFlight, at_start, keep, mark_node_wait, unresumable};
use agent_session::{Fault, FaultKind, SessionId, TurnEnd};
use harness_providers::Machine;
use serde_json::json;

use crate::support::rows;
use crate::support::scratch::Scratch;

fn ground(scratch: &Scratch, home: &std::path::Path) -> Arc<Ground> {
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), home.to_string_lossy().into_owned()),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
    ],
    home.join("data"),
  );

  Arc::new(Ground::new(&service, &machine, None))
}

// A line of a pane program's record, as the terminal client appends it.
fn append(place: &Place, line: serde_json::Value) {
  use std::io::Write as _;

  std::fs::create_dir_all(&place.path).unwrap();

  let mut file = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(place.path.join("loop.jsonl"))
    .unwrap();

  writeln!(file, "{line}").unwrap();
}

fn in_flight(session: &str, run: &str, turn: i64, key: Option<&str>, job: Option<i64>) -> InFlight {
  InFlight {
    session: session.to_owned(),
    turn,
    run: run.to_owned(),
    key: key.map(str::to_owned),
    agent: session.to_owned(),
    chat_id: Some(5),
    ledger: None,
    job,
    started_at: now_ms() - 1_000,
  }
}

fn answered_end(turn: i64, text: &str) -> TurnEnd {
  TurnEnd {
    turn,
    ..TurnEnd::answered("run-1", text.to_owned())
  }
}

#[tokio::test]
async fn two_sends_under_one_key_begin_one_turn_and_a_refusal_keeps_nothing() {
  let scratch = Scratch::new().await;
  let keys = TurnKeys::default();

  let Claim::New(first) = keys.claim(scratch.db(), "chat-agent@-5", "round-1").await else {
    panic!("the first send under a key begins the turn");
  };
  let Claim::Standing(second) = keys.claim(scratch.db(), "chat-agent@-5", "round-1").await else {
    panic!("a repeat while the turn stands attaches to it");
  };
  let Claim::New(_) = keys.claim(scratch.db(), "chat-agent@-6", "round-1").await else {
    panic!("a key is the session's own");
  };

  keys.settle(
    "chat-agent@-5",
    "round-1",
    Settled::Refused(Fault {
      kind: FaultKind::Busy,
      message: "chat-agent@-5 is in a turn".into(),
    }),
  );

  for waiting in [first, second] {
    assert!(
      matches!(waiting.settled().await, Some(Settled::Refused(fault)) if fault.kind == FaultKind::Busy)
    );
  }

  assert!(!keys.holds("chat-agent@-5", "round-1"));
  assert!(
    kept(scratch.db(), "chat-agent@-5", "round-1").await.is_none(),
    "a refusal keeps nothing"
  );
  assert!(
    matches!(keys.claim(scratch.db(), "chat-agent@-5", "round-1").await, Claim::New(_)),
    "a send refused before its turn began is asked again as a new one"
  );
}

#[test]
fn an_end_names_the_program_session_its_turn_answered_on() {
  let end = TurnEnd {
    reply: Some(Box::new(agent_session::Reply {
      mode: "claude-code".into(),
      model: None,
      effort: None,
      text: "Done.".into(),
      session_id: Some("the-program-session".into()),
      account: None,
      context_ratio: None,
      context_tokens: None,
      session_usage: None,
      spend: Vec::new(),
      live_tasks: Vec::new(),
      workflows: Vec::new(),
      upstream: None,
      request_usages: None,
    })),
    ..answered_end(7, "Done.")
  };
  let kept = KeptEnd::of("note-assistant@run:3", "ask-1", &end);

  assert_eq!(kept.harness_session.as_deref(), Some("the-program-session"));
  assert_eq!(
    kept.answer("the prompt", json!({}))["harnessSession"],
    "the-program-session",
    "a later start names it to continue the session"
  );
}

#[tokio::test]
async fn a_keyed_end_is_kept_answered_to_a_repeat_and_dropped_past_its_span() {
  let scratch = Scratch::new().await;
  let keys = TurnKeys::default();
  let Claim::New(waiting) = keys.claim(scratch.db(), "chat-agent@-5", "round-1").await else {
    panic!("the first send begins the turn");
  };

  keys.ended(scratch.db(), "chat-agent@-5", "round-1", &answered_end(41, "Done.")).await;

  let Some(Settled::Ended(end)) = waiting.settled().await else {
    panic!("the waiting send is answered")
  };
  let Claim::Kept(again) = keys.claim(scratch.db(), "chat-agent@-5", "round-1").await else {
    panic!("a repeat after the end is answered with it");
  };

  assert_eq!(end, again);
  assert_eq!(again.turn, 41);

  let answer = again.answer("the prompt", json!({ "round": "r" }));

  assert_eq!(answer["id"], 41);
  assert_eq!(answer["state"], "ended");
  assert_eq!(answer["outcome"], "answered");
  assert_eq!(answer["answer"], "Done.");
  assert_eq!(answer["text"], json!(["Done."]));
  assert_eq!(answer["context"], json!({ "round": "r" }), "the repeat's own context is echoed");
  assert_eq!(answer["harnessSession"], json!(null), "an end that read no program session names none");

  // An end past the policy's span reads as none, and the reconciliation's pass drops it.
  let old = KeptEnd {
    at: now_ms() - 10 * 86_400_000,
    key: "round-0".into(),
    ..again.clone()
  };

  settings::set(scratch.db(), "turn_end:chat-agent@-5:round-0", &serde_json::to_string(&old).unwrap())
    .await
    .unwrap();

  assert!(kept(scratch.db(), "chat-agent@-5", "round-0").await.is_none());
  assert_eq!(prune(scratch.db()).await, 1);
  assert!(kept(scratch.db(), "chat-agent@-5", "round-1").await.is_some(), "a fresh end stays");
}

#[tokio::test]
async fn a_keyed_turn_picked_up_at_start_answers_under_its_id_and_hands_its_end_to_its_key() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let place = Place::of(&ground.roots.data, "agent-b");
  let session = SessionId("agent-b".into());
  let mut ended = ground.events.subscribe::<TurnEnded>();
  let mut program = std::process::Command::new("sleep").arg("30").spawn().unwrap();
  let identity = agent_host::identity::ProcessIdentity::of(program.id() as i32).unwrap();

  std::fs::create_dir_all(&place.path).unwrap();
  std::fs::write(
    place.path.join("running.json"),
    json!({ "identity": "one", "pid": identity.pid, "birth": identity.birth, "loop": identity,
            "starts": 1, "startedAt": now_ms(), "recent": [] })
    .to_string(),
  )
  .unwrap();
  keep(scratch.db(), &in_flight("agent-b", "chat:5:3", 43, Some("round-7"), None)).await;
  keep(scratch.db(), &in_flight("agent-c", "chat:5:4", 44, Some("round-8"), None)).await;
  append(
    &place,
    json!({ "kind": "start", "id": "w", "name": "chat:5:3", "title": "a turn", "at": 1 }),
  );

  assert_eq!(at_start(&ground).await.unwrap(), 2);

  // The turn no record names ends failed, and a repeat of its send is answered with that end.
  let lost = ended.next().await.unwrap();

  assert_eq!(
    (lost.session.as_str(), lost.end.turn, lost.key.as_deref()),
    ("agent-c", 44, Some("round-8"))
  );
  assert!(matches!(
    ground.turn_keys.claim(scratch.db(), "agent-c", "round-8").await,
    Claim::Kept(end) if end.turn == 44 && end.answer.is_none()
  ));

  // The turn still working answers the progress read under the id it went in with, and stands under its key.
  let reading = ground
    .outputs
    .read(&session, Some(43))
    .expect("the progress read answers the picked-up turn");

  assert!(reading.running);
  assert!(ground.turn_keys.holds("agent-b", "round-7"));

  let Claim::Standing(waiting) = ground.turn_keys.claim(scratch.db(), "agent-b", "round-7").await else {
    panic!("a repeat of the send attaches to the picked-up turn");
  };

  append(
    &place,
    json!({ "kind": "answer", "id": "w", "text": "Worth the wait.", "model": "[\"deepseek\",\"deepseek-flash\"]",
            "usage": { "inputTokens": 10, "outputTokens": 5, "cacheReadTokens": 0 } }),
  );
  append(&place, json!({ "kind": "end", "id": "w" }));

  let settled = tokio::time::timeout(Duration::from_secs(10), waiting.settled()).await.unwrap();
  let Some(Settled::Ended(end)) = settled else {
    panic!("the attached send is answered with the end")
  };

  assert_eq!((end.turn, end.answer.as_deref()), (43, Some("Worth the wait.")));

  let told = ended.next().await.unwrap();

  assert_eq!(
    (told.session.as_str(), told.end.turn, told.key.as_deref()),
    ("agent-b", 43, Some("round-7"))
  );

  let reading = ground.outputs.read(&session, Some(43)).unwrap();

  assert!(!reading.running, "the progress read answers it ended");
  assert_eq!(reading.text, "Worth the wait.");

  let _ = program.kill();
  let _ = program.wait();
}

#[tokio::test]
async fn the_quiet_counts_a_carried_turn_of_a_standing_supervisor_no_program_has_taken_and_nothing_else() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());

  // Carried here, no record names it: a restart would cut it.
  keep(scratch.db(), &in_flight("agent-a", "chat:5:1", 1, Some("k-1"), None)).await;
  // Carried here, its program took it: the next start picks it up.
  keep(scratch.db(), &in_flight("agent-b", "chat:5:2", 2, None, None)).await;
  append(
    &Place::of(&ground.roots.data, "agent-b"),
    json!({ "kind": "start", "id": "w", "name": "chat:5:2", "title": "a turn", "at": 1 }),
  );
  // A job's: the lease pass takes its item again.
  keep(scratch.db(), &in_flight("agent-c", "job-3", 3, None, Some(3))).await;
  // Not carried here: the reap or the next start ends it.
  keep(scratch.db(), &in_flight("agent-d", "chat:5:4", 4, None, None)).await;
  // Carried here, no record names it, but its supervisor was removed: it ends with the supervisor.
  keep(scratch.db(), &in_flight("agent-e", "chat:5:5", 5, None, None)).await;
  rows::supervisor(scratch.db(), "agent-a", &[]).await;
  // Carried here, no record names it, waiting for its person's node: it has started nothing, and the next start
  // ends it with why.
  keep(scratch.db(), &in_flight("agent-f", "chat:5:6", 6, None, None)).await;
  mark_node_wait(scratch.db(), "chat:5:6", Some("pc-7")).await;
  rows::supervisor(scratch.db(), "agent-f", &[]).await;

  let _carried = ["chat:5:1", "chat:5:2", "job-3", "chat:5:5", "chat:5:6"].map(Carrying::of);
  let cut: Vec<String> = unresumable(&ground).await.unwrap().into_iter().map(|turn| turn.run).collect();

  assert_eq!(cut, ["chat:5:1"]);
}
