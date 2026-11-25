//! A turn whose program ended it while the reading of its price never came (docs/agent-control.md, 「What a
//! resident turn reads」, 「Telling that a turn's program is gone」): a program on a node answers its status line
//! through the home bridge, and a call made while the node's link is being made again is lost. The turn the record
//! closed ends answered with what the program wrote, within the bound on that reading, and a stop put to it while
//! it waits on that reading ends it stopped at once, so the session takes its next turn. The program is played here
//! as Claude Code writes a turn, with the status line's call after the turn never arriving.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use agency_server::door::endpoint::session_named;
use agency_server::herdr_backend::HerdrSessions;
use agency_server::lines::place::Place;
use agency_server::now_ms;
use agency_server::places::{Ground, Places};
use agent_host::identity::ProcessIdentity;
use agent_session::{Outcome, RoomBlock, Scope, SessionId, Sessions, Start, Turn, TurnContext};
use harness_providers::Machine;
use harness_providers::claude_code::ClaudeCode;
use policy::resident_turn::RESIDENT_TURN;
use serde_json::{Value, json};

use crate::integration::common;
use crate::integration::common::Scratch;
use crate::integration::common::herdr::HerdrModel;
use crate::support::grounds::{deepseek_balance, seeded};
use crate::support::rows::supervisor;

const AGENT: &str = "node-agent";
const TOOL_TOKEN: &str = "a tool token";

// The record of the session the program stands on.
fn record_of(place: &Place) -> Option<PathBuf> {
  let program = place.program()?;
  let session = session_named(&program.command.args)?.to_owned();
  let home = PathBuf::from(program.command.env.get("CLAUDE_CONFIG_DIR")?);

  Some(ClaudeCode::records_under(&home, &program.command.cwd, &session).swap_remove(0))
}

fn written(path: &std::path::Path, lines: &[Value]) {
  std::fs::create_dir_all(path.parent().unwrap()).unwrap();

  let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();

  for line in lines {
    writeln!(file, "{line}").unwrap();
  }
}

// The program as its place and its record see it: stood on whatever the place holds, a process of its own, and
// each prompt answered and its turn closed with its `Stop` hook, but no status-line call after it.
struct Played {
  herdr: HerdrModel,
  place: Place,
  pane: String,
}

impl Played {
  fn stood(&self) {
    let Some(program) = self.place.program() else { return };

    if self.place.program_alive().is_some_and(|running| running.identity == program.identity) {
      return;
    }

    // The stand-in outlives its test, so it holds none of the test run's streams.
    let mut process = std::process::Command::new("sleep")
      .arg("600")
      .stdin(std::process::Stdio::null())
      .stdout(std::process::Stdio::null())
      .stderr(std::process::Stdio::null())
      .spawn()
      .unwrap();
    let identity = ProcessIdentity::of(process.id() as i32).unwrap();
    let at = now_ms();

    std::thread::spawn(move || process.wait());
    std::fs::write(
      self.place.path.join("running.json"),
      json!({ "identity": program.identity, "pid": identity.pid, "birth": identity.birth, "loop": identity,
              "starts": 1, "startedAt": at, "paneId": self.pane, "recent": [] })
      .to_string(),
    )
    .unwrap();
    // The call as the program draws itself, which says it is up; none follows a turn.
    written(&self.place.path.join("status.jsonl"), &[json!({ "at": at, "payload": {} })]);
  }

  async fn answers(&self, record: &std::path::Path, words: &str) {
    self.herdr.agent_status(&self.pane, "working");
    written(record, &[json!({ "type": "user", "message": { "role": "user", "content": words } })]);
    tokio::time::sleep(Duration::from_millis(500)).await;

    let session = record.file_stem().unwrap().to_string_lossy().into_owned();

    written(
      record,
      &[
        json!({ "type": "assistant", "message": {
          "id": format!("m-{}", now_ms()), "model": "deepseek-flash", "role": "assistant",
          "content": [{ "type": "text", "text": "the answer written on the node" }],
          "usage": { "input_tokens": 1_000, "output_tokens": 5 },
        }}),
        json!({ "type": "system", "subtype": "turn_duration" }),
      ],
    );
    written(
      &self.place.path.join("hooks.jsonl"),
      &[json!({ "at": now_ms(), "kind": "stop", "payload": { "session_id": session } })],
    );
    self.herdr.agent_status(&self.pane, "idle");
  }

  async fn run(self) {
    let mut seen = 0;

    loop {
      self.stood();

      let prompts = self.herdr.prompts();

      for (_, words) in prompts.iter().skip(seen) {
        let record = record_of(&self.place).expect("a prompt goes into a program standing on a session");

        self.answers(&record, words).await;
      }

      seen = prompts.len();
      tokio::time::sleep(Duration::from_millis(20)).await;
    }
  }
}

// A tool host that issues a program its session token.
fn tool_host(socket: &std::path::Path) {
  let listener = tokio::net::UnixListener::bind(socket).unwrap();
  let app = axum::Router::new().route(
    "/tool-sessions",
    axum::routing::post(|| async {
      (
        axum::http::StatusCode::CREATED,
        axum::Json(json!({
          "token": TOOL_TOKEN, "holderName": AGENT, "namespaces": [], "capabilities": [], "grants": [], "shown": [],
          "variables": {}, "variablesFile": null, "labels": {}, "expiresAt": null, "createdAt": 1,
        })),
      )
    }),
  );

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
}

struct Stand {
  _scratch: Scratch,
  sessions: Arc<HerdrSessions>,
  place: Place,
  session: SessionId,
  _ground: Arc<Ground>,
  _home: tempfile::TempDir,
}

impl Stand {
  async fn up() -> Self {
    let scratch = Scratch::new().await;
    let home = tempfile::tempdir().unwrap();
    let line = seeded(scratch.db(), home.path()).await;

    deepseek_balance(&line, 5.0).await;
    supervisor(
      scratch.db(),
      AGENT,
      &[("provider", "deepseek".into()), ("model", "deepseek-flash".into())],
    )
    .await;

    let config = home.path().join("config");

    std::fs::create_dir_all(config.join("herdr")).unwrap();

    let herdr = HerdrModel::stand(config.join("herdr").join("herdr.sock"));
    let at = |path: &std::path::Path| path.to_string_lossy().into_owned();
    let tools = home.path().join("tools.sock");

    tool_host(&tools);

    let machine = Machine::with_env(
      [
        ("HOME".to_owned(), at(home.path())),
        ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
        ("XDG_CONFIG_HOME".to_owned(), at(&config)),
        ("BMO_DEPLOY_DIR".to_owned(), env!("CARGO_MANIFEST_DIR").to_owned()),
        ("BMO_MCP_HOST_SOCKET".to_owned(), at(&tools)),
        ("BMO_MCP_HOST_ISSUER_TOKEN".to_owned(), "issuer".to_owned()),
      ],
      home.path().join("data"),
    );
    let service = common::service(&scratch, &[("AGENCY_SOURCE_DIR", at(&home.path().join("source")))]);
    let ground = Ground::new(&service, &machine, Some(herdr::Herdr::there(herdr.socket.clone())));
    let place = Place::of(&ground.roots.data, AGENT);
    let (_, pane) = herdr.workspace("played", Value::Null, None);

    herdr.agent_status(&pane, "idle");
    std::fs::create_dir_all(&place.path).unwrap();
    tokio::spawn(
      Played {
        herdr: herdr.clone(),
        place: Place::of(&ground.roots.data, AGENT),
        pane,
      }
      .run(),
    );

    let places = Places::new(ground, Arc::default());
    let ground = places.ground.clone();
    let sessions = Arc::new(HerdrSessions::new(places));
    let session = sessions
      .start(Start {
        agent: AGENT.into(),
        scope: Scope::Agent,
      })
      .await
      .unwrap()
      .id;

    Self {
      _scratch: scratch,
      sessions,
      place,
      session,
      _ground: ground,
      _home: home,
    }
  }

  fn turn(&self) -> Turn {
    Turn {
      session: self.session.clone(),
      prompt: "hello".into(),
      context: TurnContext {
        room: Some(RoomBlock {
          chat_id: -5,
          text: "the room's block".into(),
          mark: "[a]".into(),
        }),
        ..TurnContext::default()
      },
    }
  }

  // Whether the program's `Stop` hook has been written since `since`.
  fn stopped_since(&self, since: i64) -> bool {
    agency_server::door::place_run::read_hooks(&self.place.path, since)
      .iter()
      .any(|hook| hook.kind == "stop")
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_its_program_ended_is_answered_though_the_status_line_s_call_after_it_never_comes() {
  let stand = Stand::up().await;
  let began = Instant::now();
  let ended = tokio::time::timeout(Duration::from_secs(90), stand.sessions.send_turn(stand.turn()))
    .await
    .expect("a turn its record closed is not held on a reading that never comes")
    .unwrap();

  assert_eq!(ended.outcome, Outcome::Answered, "{ended:?}");
  assert_eq!(ended.ending.as_deref(), Some("answered"));
  assert_eq!(ended.answer.as_deref(), Some("the answer written on the node"));
  assert!(
    began.elapsed() >= Duration::from_millis(RESIDENT_TURN.status_after_ms),
    "the reading is waited for up to its bound: {:?}",
    began.elapsed()
  );

  // The session takes its next turn.
  let next = tokio::time::timeout(Duration::from_secs(90), stand.sessions.send_turn(stand.turn()))
    .await
    .expect("the next turn goes in")
    .unwrap();

  assert_eq!(next.outcome, Outcome::Answered, "{next:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_interrupt_while_the_reading_after_a_turn_is_waited_on_ends_the_turn_stopped_at_once() {
  let stand = Stand::up().await;
  let since = now_ms();
  let sessions = stand.sessions.clone();
  let turn = stand.turn();
  let sent = tokio::spawn(async move { sessions.send_turn(turn).await });

  // The program has ended its turn; the door now waits on the status line's call that does not come.
  let deadline = Instant::now() + Duration::from_secs(60);

  while !stand.stopped_since(since) {
    assert!(Instant::now() < deadline, "the program ends its turn");
    tokio::time::sleep(Duration::from_millis(50)).await;
  }

  tokio::time::sleep(Duration::from_secs(2)).await;

  let going = stand.sessions.output(&stand.session, None).await.unwrap();

  assert!(going.running, "the turn still waits on the reading after its end");

  let asked = Instant::now();

  stand.sessions.interrupt(&stand.session, going.turn).await.unwrap();

  let ended = tokio::time::timeout(Duration::from_secs(5), sent)
    .await
    .expect("the interrupt ends the turn's wait whatever it waits on")
    .unwrap()
    .unwrap();

  assert_eq!(ended.ending.as_deref(), Some("stopped"), "{ended:?}");
  assert_eq!(ended.answer, None, "a turn ended stopped hands on no answer");
  assert!(
    asked.elapsed() < Duration::from_millis(RESIDENT_TURN.status_after_ms),
    "ended by the stop, not the bound: {:?}",
    asked.elapsed()
  );
  assert!(!stand.sessions.output(&stand.session, None).await.unwrap().running);
}
