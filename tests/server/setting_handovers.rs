//! A supervisor's sessions replaced through their handoff whenever its settings move them (docs/supervisors.md,
//! 「Where a supervisor's turn runs」, 「A session in each room」, 「Clearing it」; docs/sessions.md, 「A restart
//! hands the session over」): a road moved on a room's supervisor hands each room's session over, a road moved
//! while a room carries a turn waits for the turn, a reset of a room's session is carried out by its handover, and
//! an ask for the one session across rooms paced by the last one waits rather than being skipped. A clean reset of a
//! room's session opens its generation on no note and marks no handoff lost, and every room handover is told to the
//! receiver so the room shows its line (docs/session-resets.md, 「What a reset is」).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::Request;
use axum::response::IntoResponse;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::support::rows::supervisor;
use crate::support::scratch::Scratch;
use agency_server::Service;
use agency_server::config::Config;
use agency_server::herdr_backend::{HerdrSessions, agent_session};
use agency_server::lines::place::{Place, Program};
use agency_server::places::{Ground, Places};
use agency_server::seeds::deployment;
use agency_server::supervisor_turn::handoff::Why;
use agency_server::supervisor_turn::replacement;
use agency_server::supervisor_turn::sessions;
use agent_host::place::ResidentCommand;
use harness_providers::Machine;

const AGENT: &str = "pal-chat";
const CHAT: i64 = -7;

// A deployment of the test's own, with a room's supervisor whose road is Claude Code's opus model and whose room
// session still stands on the sonnet model it was opened on, its program standing on that session.
struct Room {
  scratch: Scratch,
  _home: tempfile::TempDir,
  sessions: HerdrSessions,
  place: Place,
}

async fn room(vars: &[(&str, String)]) -> Room {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let mut env = vec![
    ("HOME".to_owned(), home.path().to_string_lossy().into_owned()),
    ("PATH".to_owned(), "/nowhere".to_owned()),
  ];

  env.extend(vars.iter().map(|(name, value)| ((*name).to_owned(), value.clone())));

  let machine = Machine::with_env(env, home.path().join("data"));

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();
  supervisor(
    scratch.db(),
    AGENT,
    &[("session_scope", "room".into()), ("turn_shape", "pieces".into())],
  )
  .await;
  scratch
    .run(
      r#"INSERT INTO "room_sessions" ("agent", "chat_id", "generation", "opened_at", "harness_session", "harness_road")
         VALUES ($1, $2, 1, 1, 's-9', 'claude-code::claude-sonnet-5')"#,
      vec![AGENT.into(), CHAT.into()],
    )
    .await;

  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let ground = Ground::new(&service, &machine, None);
  let place = Place::of(&ground.roots.data, AGENT);
  let resumed = |args: &[&str]| ResidentCommand {
    args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    cwd: home.path().to_path_buf(),
    env: [].into(),
    login_file: None,
  };
  let standing = Program {
    identity: "old-road".into(),
    command: resumed(&["claude", "--model", "claude-sonnet-5"]),
    restart: Some(resumed(&["claude", "--model", "claude-sonnet-5", "--resume", "s-9"])),
    opened: None,
  };

  std::fs::create_dir_all(&place.path).unwrap();
  std::fs::write(place.path.join("program.json"), serde_json::to_vec(&standing).unwrap()).unwrap();

  let sessions = HerdrSessions::new(Places::new(ground, Arc::default()));

  Room {
    scratch,
    _home: home,
    sessions,
    place,
  }
}

impl Room {
  // The lines the handoff runs of the room wrote, oldest first.
  async fn handoff_lines(&self) -> Vec<String> {
    store::all(
      self.scratch.db(),
      r#"SELECT "text" FROM "agent_log" WHERE "agent" = $1 AND "run" LIKE 'handoff:%' ORDER BY "id""#,
      vec![AGENT.into()],
    )
    .await
    .unwrap()
    .iter()
    .map(|row| row.try_get::<String>("", "text").unwrap())
    .collect()
  }

  async fn held(&self) -> sessions::RoomSession {
    sessions::session(self.scratch.db(), AGENT, CHAT)
      .await
      .unwrap()
      .expect("the room's session")
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_road_moved_on_a_rooms_supervisor_hands_each_rooms_session_over_before_its_next_generation() {
  let room = room(&[]).await;

  room.sessions.road_moved(AGENT).await;

  // The session is asked for its note in one more turn on the road it was opened on; with no program here to write
  // one, the next generation opens all the same, said to have lost its handoff, and the program goes with it.
  let lines = room.handoff_lines().await;

  assert!(lines.iter().any(|line| line.contains("handoff because its road changed")), "{lines:?}");

  let held = room.held().await;

  assert_eq!(held.generation, 2, "the next generation opened after the handover");
  assert!(held.harness.is_none(), "the next generation opens a fresh harness session");
  assert_eq!(
    replacement::room_owed(room.scratch.db(), AGENT, CHAT).await.unwrap(),
    None,
    "the debt is settled"
  );
  assert!(
    replacement::lost(room.scratch.db(), &sessions::room_session_id(AGENT, CHAT))
      .await
      .unwrap(),
    "a generation opened with no note says so on its state"
  );
  assert!(room.place.program().is_none(), "the program on the session that went is taken down");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_road_moved_while_the_room_carries_a_turn_signals_nothing_and_hands_over_once_the_turn_has_ended()
{
  let room = room(&[]).await;
  let db = room.scratch.db();
  let turn = agency_server::turn_pickup::InFlight {
    session: sessions::room_session_id(AGENT, CHAT),
    turn: 1,
    run: "chat:-7:1".into(),
    key: None,
    agent: AGENT.into(),
    chat_id: Some(CHAT),
    ledger: None,
    job: None,
    started_at: agency_server::now_ms(),
  };

  agency_server::turn_pickup::keep(db, &turn).await;

  // A pin or a model of the catalogue moved while the room's turn runs: the handover is owed, nothing is
  // signalled and the turn's session stands.
  room.sessions.road_moved(AGENT).await;

  let owed = replacement::room_owed(db, AGENT, CHAT).await.unwrap().expect("the handover is owed");

  assert_eq!((owed.session.as_str(), owed.why), ("s-9", Why::Road));
  assert_eq!((owed.provider.as_str(), owed.model.as_str()), ("claude-code", "claude-sonnet-5"));
  assert!(room.place.program().is_some(), "the program the turn stands on is not signalled");
  assert_eq!(room.held().await.generation, 1, "the turn's session stands");
  assert!(room.handoff_lines().await.is_empty(), "no handoff runs under the turn");

  // The turn ended: the pass its end runs makes the handover.
  agency_server::turn_pickup::forget(db, &turn.run, "the turn ended").await;
  room.sessions.settle_pass().await;

  assert!(
    room
      .handoff_lines()
      .await
      .iter()
      .any(|line| line.contains("handoff because its road changed"))
  );
  assert_eq!(room.held().await.generation, 2);
  assert!(room.place.program().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reset_asked_of_a_room_session_is_carried_out_by_its_handover_and_settled_with_it() {
  let room = room(&[]).await;
  let db = room.scratch.db();

  room
    .scratch
    .run(
      r#"INSERT INTO "session_resets" ("agent", "chat_id", "asked_by", "asked_at", "reason")
         VALUES ($1, $2, 'owner', 1, 'the generation settings changed')"#,
      vec![AGENT.into(), CHAT.into()],
    )
    .await;

  room.sessions.replace_reset_rooms().await;

  assert!(
    room
      .handoff_lines()
      .await
      .iter()
      .any(|line| line.contains("a reset of its session was asked"))
  );
  assert_eq!(room.held().await.generation, 2);
  assert!(
    !sessions::reset_waiting(db, AGENT, CHAT).await.unwrap(),
    "the handover settled the reset"
  );
  assert!(room.place.program().is_none());
}

// A receiver of the test's own keeping the body of every ask, answering each 204.
async fn bodies() -> (Arc<Mutex<Vec<Value>>>, u16) {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let heard: Arc<Mutex<Vec<Value>>> = Arc::default();
  let kept = heard.clone();
  let app = Router::new().fallback(move |request: Request| {
    let kept = kept.clone();

    async move {
      let path = request.uri().path().to_owned();
      let bytes = axum::body::to_bytes(request.into_body(), 1 << 20).await.unwrap();
      let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);

      kept.lock().unwrap().push(json!({ "path": path, "body": body }));
      axum::http::StatusCode::NO_CONTENT
    }
  });

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  (heard, port)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_clean_reset_of_a_room_session_opens_on_no_note_and_is_told_to_the_receiver() {
  let (heard, port) = bodies().await;
  let room = room(&[
    ("BMO_RECEIVER_URL", format!("http://127.0.0.1:{port}")),
    ("BMO_RECEIVER_TOKEN", "receiver-token".to_owned()),
  ])
  .await;
  let db = room.scratch.db();

  room
    .scratch
    .run(
      r#"INSERT INTO "session_resets" ("agent", "chat_id", "asked_by", "asked_at", "reason", "clean")
         VALUES ($1, $2, '7', 1, 'the owner asked', true)"#,
      vec![AGENT.into(), CHAT.into()],
    )
    .await;

  room.sessions.replace_reset_rooms().await;

  let held = room.held().await;

  assert_eq!(held.generation, 2);
  assert_eq!(held.handoff, None, "a clean reset carries no note");
  assert!(
    !replacement::lost(db, &sessions::room_session_id(AGENT, CHAT)).await.unwrap(),
    "a clean start is no handoff lost"
  );
  assert!(!sessions::reset_waiting(db, AGENT, CHAT).await.unwrap(), "the reset is settled");
  assert!(
    room
      .handoff_lines()
      .await
      .iter()
      .any(|line| line.contains("consolidates its brain and writes no note"))
  );
  assert_eq!(
    heard.lock().unwrap().clone(),
    vec![json!({
      "path": format!("/handoffs/{AGENT}"),
      "body": {
        "reason": "a clean reset of its session was asked",
        "room": CHAT,
        "cause": "reset",
        "clean": true,
        "by": 7,
        "ask": false
      }
    })],
    "the receiver is told once, asking no session"
  );
}

// What a receiver of the test's own was asked for: each agent whose handoff was asked, when.
type Asked = Arc<Mutex<Vec<(String, Instant)>>>;

async fn receiver() -> (Asked, u16) {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let asked: Asked = Arc::default();
  let kept = asked.clone();
  let app = Router::new().fallback(move |request: Request| {
    let kept = kept.clone();

    async move {
      let agent = request.uri().path().rsplit('/').next().unwrap_or_default().to_owned();

      kept.lock().unwrap().push((agent, Instant::now()));
      axum::Json(json!({ "path": "/handoffs/agent.md", "bytes": 12 })).into_response()
    }
  });

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  (asked, port)
}

#[tokio::test(flavor = "multi_thread")]
async fn an_ask_sooner_than_the_pace_after_the_last_waits_out_the_rest_and_is_made_never_skipped() {
  let (asked, port) = receiver().await;
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let agent = "chat-agent";
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), home.path().to_string_lossy().into_owned()),
      ("PATH".to_owned(), "/nowhere".to_owned()),
      ("BMO_RECEIVER_URL".to_owned(), format!("http://127.0.0.1:{port}")),
      ("BMO_RECEIVER_TOKEN".to_owned(), "receiver-token".to_owned()),
    ],
    home.path().join("data"),
  );

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();
  supervisor(
    scratch.db(),
    agent,
    &[("session_id", "s-1".into()), ("system_hash", "the prompt".into())],
  )
  .await;

  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let sessions = HerdrSessions::new(Places::new(Ground::new(&service, &machine, None), Arc::default()));
  let pace = Duration::from_millis(300);

  sessions.pace_handoffs(pace);

  // The session was asked for its handoff a moment ago, and its road moves again now.
  let began = Instant::now();

  agency_server::queue::settings::set(
    scratch.db(),
    &format!("handoff_asked:{agent}:0"),
    &agency_server::now_ms().to_string(),
  )
  .await
  .unwrap();
  sessions.replaced_road(agent).await;

  let asked = asked.lock().unwrap().clone();

  assert_eq!(asked.len(), 1, "the ask is made, not skipped: {asked:?}");
  assert!(
    asked[0].1.duration_since(began) >= pace - Duration::from_millis(50),
    "the ask waited out the rest of the pace"
  );
  assert!(
    agent_session(scratch.db(), agent).await.unwrap().is_none(),
    "the old session is let go after it"
  );
  assert_eq!(replacement::owed(scratch.db(), agent).await.unwrap(), None, "the debt is settled");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lost_handoff_is_told_until_a_turn_begun_after_it_answers_and_survives_the_turn_it_was_lost_under()
 {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let session = sessions::room_session_id(AGENT, CHAT);
  let under = agency_server::now_ms() - 1;

  // A turn that began before the handover at its head lost the note: the receiver composed it without the
  // transcript, so its answer leaves the word for the next turn.
  replacement::lose(db, &session).await.unwrap();
  replacement::answered_since(db, &session, under).await.unwrap();
  assert!(
    replacement::lost(db, &session).await.unwrap(),
    "the turn it was lost under keeps it told"
  );

  // The next turn, begun after, opened on the transcript: its answer lets the word go.
  let after = agency_server::now_ms() + 1;

  replacement::answered_since(db, &session, after).await.unwrap();
  assert!(!replacement::lost(db, &session).await.unwrap());
}
