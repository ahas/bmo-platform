//! A supervisor's session across rooms folded in the program standing on it (docs/sessions.md, 「Session
//! compaction」; docs/agent-control.md, 「How a command of the program's own ends」): the fold word reaches that
//! program bare and on the command it stands on, the turn ends on what the record says though the program closes
//! no turn and fires no stop, the session keeps its id, and a fold that did not happen is a fault that leaves the
//! context the session's last turn left. The program is played here as Claude Code 2.1.318 was measured to write.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agency_server::door::endpoint::session_named;
use agency_server::herdr_backend::{HerdrSessions, agent_session, keep_agent_session};
use agency_server::lines::place::Place;
use agency_server::now_ms;
use agency_server::places::{Ground, Places};
use agent_host::identity::ProcessIdentity;
use agent_session::{FaultKind, RoomBlock, Scope, SessionId, Sessions, Start, Turn, TurnContext};
use harness_providers::Machine;
use harness_providers::claude_code::ClaudeCode;
use serde_json::{Value, json};

use crate::integration::common;
use crate::integration::common::Scratch;
use crate::integration::common::herdr::HerdrModel;
use crate::support::grounds::{deepseek_balance, seeded};
use crate::support::rows::supervisor;

const AGENT: &str = "chat-agent";
const TOOL_TOKEN: &str = "a tool token";

// What the program played here does with the fold word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Compact {
  Folds,
  TooShort,
  RequestFails,
  CutShort,
}

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

fn user(content: &str) -> Value {
  json!({ "type": "user", "message": { "role": "user", "content": content } })
}

fn caveat() -> Value {
  json!({ "type": "user", "isMeta": true, "message": { "content":
    "<local-command-caveat>The command below was run directly in Claude Code.</local-command-caveat>" } })
}

fn command_name() -> Value {
  user(
    "<command-name>/compact</command-name>\n<command-message>compact</command-message>\n<command-args></command-args>",
  )
}

// The program as its place and its record see it: stood on whatever the place holds, a process of its own for
// each standing, answering what is put into its pane.
struct Played {
  herdr: HerdrModel,
  place: Place,
  pane: String,
  compact: Arc<Mutex<Compact>>,
  stands: Arc<AtomicUsize>,
  held_turn: Arc<Mutex<bool>>,
}

impl Played {
  fn hooked(&self, kind: &str, payload: Value) {
    written(
      &self.place.path.join("hooks.jsonl"),
      &[json!({ "at": now_ms(), "kind": kind, "payload": payload })],
    );
  }

  fn stood(&self) {
    let Some(program) = self.place.program() else { return };
    let up = self.place.program_alive().is_some_and(|running| running.identity == program.identity);

    if up {
      return;
    }

    // The stand-in outlives its test, so it holds none of the test run's streams: a reader of the run's output
    // would otherwise wait on it long after the tests ended.
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
    written(&self.place.path.join("status.jsonl"), &[json!({ "at": at, "payload": {} })]);
    self.stands.fetch_add(1, Ordering::SeqCst);
  }

  async fn answers(&self, record: &std::path::Path, words: &str) {
    self.herdr.agent_status(&self.pane, "working");
    written(record, &[user(words)]);

    while *self.held_turn.lock().unwrap() {
      tokio::time::sleep(Duration::from_millis(20)).await;
    }

    tokio::time::sleep(Duration::from_millis(1500)).await;

    let session = record.file_stem().unwrap().to_string_lossy().into_owned();

    written(
      record,
      &[
        json!({ "type": "assistant", "message": {
          "id": format!("m-{}", now_ms()), "model": "deepseek-flash", "role": "assistant",
          "content": [{ "type": "text", "text": "an answer" }],
          "usage": { "input_tokens": 830_000, "output_tokens": 5 },
        }}),
        json!({ "type": "system", "subtype": "turn_duration" }),
      ],
    );
    self.hooked("stop", json!({ "session_id": session }));
    self.herdr.agent_status(&self.pane, "idle");
    tokio::time::sleep(Duration::from_millis(5)).await;
    written(
      &self.place.path.join("status.jsonl"),
      &[json!({ "at": now_ms(), "payload": { "session_id": session } })],
    );
  }

  async fn folds(&self, record: &std::path::Path) {
    let working = |seconds: u64| async move {
      self.herdr.agent_status(&self.pane, "working");
      written(record, &[user("/compact")]);
      tokio::time::sleep(Duration::from_secs(seconds)).await;
    };
    let compact = *self.compact.lock().unwrap();

    self.hooked("pre-compact", json!({}));

    match compact {
      Compact::Folds => {
        working(2).await;
        written(
          record,
          &[
            json!({ "type": "system", "subtype": "compact_boundary", "content": "Conversation compacted" }),
            json!({ "type": "user", "isCompactSummary": true, "message": { "content": "This session is being continued." } }),
            caveat(),
            command_name(),
            user(
              "<local-command-stdout>\u{1b}[2mCompacted (ctrl+o to see full summary)\u{1b}[22m</local-command-stdout>",
            ),
          ],
        );
      }
      // Read at rest throughout, as a program that had nothing to fold is.
      Compact::TooShort => written(
        record,
        &[
          user("/compact"),
          caveat(),
          command_name(),
          json!({ "type": "system", "subtype": "local_command", "isMeta": false,
                  "content": "<local-command-stdout>Not enough messages to compact.</local-command-stdout>" }),
        ],
      ),
      Compact::RequestFails => {
        working(2).await;
        written(
          record,
          &[
            caveat(),
            command_name(),
            json!({ "type": "system", "subtype": "local_command", "isMeta": false, "content":
              "<local-command-stderr>Error during compaction: API Error: Connection refused</local-command-stderr>" }),
          ],
        );
        self.hooked("failure", json!({ "error": "unknown" }));
      }
      Compact::CutShort => {
        working(2).await;
        written(record, &[caveat()]);
      }
    }

    self.herdr.agent_status(&self.pane, "idle");
  }

  // The program for as long as the test stands.
  async fn run(self) {
    let mut seen = 0;

    loop {
      self.stood();

      let prompts = self.herdr.prompts();

      for (_, words) in prompts.iter().skip(seen) {
        let record = record_of(&self.place).expect("a prompt goes into a program standing on a session");

        if words == "/compact" {
          self.folds(&record).await;
        } else {
          self.answers(&record, words).await;
        }
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
  scratch: Scratch,
  ground: Arc<Ground>,
  sessions: Arc<HerdrSessions>,
  herdr: HerdrModel,
  place: Place,
  compact: Arc<Mutex<Compact>>,
  stands: Arc<AtomicUsize>,
  held_turn: Arc<Mutex<bool>>,
  session: SessionId,
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
    let compact = Arc::new(Mutex::new(Compact::Folds));
    let stands = Arc::new(AtomicUsize::new(0));
    let held_turn = Arc::new(Mutex::new(false));

    herdr.agent_status(&pane, "idle");
    std::fs::create_dir_all(&place.path).unwrap();
    tokio::spawn(
      Played {
        herdr: herdr.clone(),
        place: Place::of(&ground.roots.data, AGENT),
        pane,
        compact: compact.clone(),
        stands: stands.clone(),
        held_turn: held_turn.clone(),
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
      scratch,
      ground,
      sessions,
      herdr,
      place,
      compact,
      stands,
      held_turn,
      session,
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

  async fn fold(&self) -> Result<(), agent_session::Fault> {
    tokio::time::timeout(Duration::from_secs(60), self.sessions.fold(&self.session))
      .await
      .expect("a fold's turn ends though the program closes no turn and fires no stop")
  }

  async fn context(&self) -> Option<agent_session::Context> {
    self.sessions.read_state(&self.session, None).await.unwrap().context
  }

  fn compacts(&self, compact: Compact) {
    *self.compact.lock().unwrap() = compact;
  }

  fn fold_words(&self) -> usize {
    self.herdr.prompts().iter().filter(|(_, words)| words == "/compact").count()
  }

  async fn held(&self) -> String {
    agent_session(self.scratch.db(), AGENT)
      .await
      .unwrap()
      .expect("the supervisor keeps its session")
  }

  // Whether the place's own run of the fold's turn has ended, as every turn's does.
  fn run_ended(&self) -> bool {
    agency_server::door::place_run::read_run(&self.place.path).is_some_and(|run| run.ended_at.is_some())
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_across_rooms_folds_in_its_program_keeping_its_id_and_a_fold_that_did_not_happen_leaves_its_context()
 {
  let stand = Stand::up().await;

  // A turn's end leaves the session with its context, on the provider's own road.
  let ended = tokio::time::timeout(Duration::from_secs(60), stand.sessions.send_turn(stand.turn()))
    .await
    .expect("the turn ends")
    .unwrap();

  assert_eq!(ended.outcome, agent_session::Outcome::Answered, "{ended:?}");

  let left = stand.context().await.expect("the turn's end left its context");
  let held = stand.held().await;
  let standing = stand.place.program().unwrap();
  let pid = stand.place.running().unwrap().pid;
  let stands = stand.stands.load(Ordering::SeqCst);
  let typed = stand.herdr.typed().len();

  assert_eq!(session_named(&standing.command.args), Some(held.as_str()));

  // While a turn goes, the fold is told so and nothing is typed.
  *stand.held_turn.lock().unwrap() = true;

  let sending = stand.sessions.clone();
  let turn = stand.turn();
  let going = tokio::spawn(async move { sending.send_turn(turn).await });

  while stand.sessions.turn_run(&stand.session).is_none() {
    tokio::time::sleep(Duration::from_millis(20)).await;
  }

  assert_eq!(stand.fold().await.unwrap_err().kind, FaultKind::Busy);
  assert_eq!(stand.fold_words(), 0);
  *stand.held_turn.lock().unwrap() = false;
  going.await.unwrap().unwrap();

  let left = stand.context().await.unwrap_or(left);
  let typed = stand.herdr.typed().len().max(typed);

  // The program has too little to fold: a failed compaction, not a silent success.
  stand.compacts(Compact::TooShort);

  let refused = stand.fold().await.unwrap_err();

  assert_eq!(refused.kind, FaultKind::Refused);
  assert!(refused.message.contains("Not enough messages to compact"), "{refused:?}");
  assert_eq!(stand.context().await, Some(left), "a fold that did not happen leaves the context");

  // The request the fold goes out on fails: the program's own words, and the context stands.
  stand.compacts(Compact::RequestFails);

  let refused = stand.fold().await.unwrap_err();

  assert_eq!(refused.kind, FaultKind::Refused);
  assert!(refused.message.contains("Error during compaction"), "{refused:?}");
  assert_eq!(stand.context().await, Some(left));

  // A program standing on another session than the one the supervisor keeps is told nothing.
  let words = stand.fold_words();

  keep_agent_session(stand.scratch.db(), AGENT, Some(("another", -5))).await.unwrap();
  assert_eq!(stand.fold().await.unwrap_err().kind, FaultKind::Refused);
  assert_eq!(stand.fold_words(), words, "nothing is typed");
  assert_eq!(stand.context().await, Some(left));
  keep_agent_session(stand.scratch.db(), AGENT, Some((held.as_str(), -5))).await.unwrap();

  // The fold word goes in: bare, into the program standing, whose turn ends on its record.
  let mut folded = stand.ground.events.subscribe::<agency_server::worker_session::told::SessionFolded>();

  stand.compacts(Compact::Folds);
  stand.fold().await.unwrap();

  assert_eq!(stand.fold_words(), words + 1);
  assert_eq!(
    stand.herdr.typed().len(),
    typed,
    "no lead stands before the word, which would make it words"
  );
  assert!(stand.run_ended(), "the place's run of the fold's turn ended");
  assert!(stand.sessions.turn_run(&stand.session).is_none());
  assert_eq!(stand.held().await, held, "the session keeps its id");
  assert_eq!(stand.place.program().unwrap().identity, standing.identity, "on the command it stood on");
  assert_eq!(stand.place.running().unwrap().pid, pid, "the program was not started again");
  assert_eq!(
    stand
      .place
      .program()
      .unwrap()
      .command
      .env
      .get(agency_server::lines::token::TOKEN_VARIABLE)
      .map(String::as_str),
    Some(TOOL_TOKEN),
    "the program keeps the token its place handed it"
  );
  assert_eq!(stand.stands.load(Ordering::SeqCst), stands);
  assert_eq!(stand.context().await, None, "what its last turn left is no longer what it carries");
  assert_eq!(folded.ready().map(|told| told.session), Some(held.clone()));

  // Cut short the word stands back in the box, so the program is stood again on the same session and the next
  // turn finds it empty.
  stand.compacts(Compact::CutShort);
  assert_eq!(stand.fold().await.unwrap_err().kind, FaultKind::Refused);
  assert_eq!(stand.held().await, held);

  let again = tokio::time::timeout(Duration::from_secs(60), stand.sessions.send_turn(stand.turn()))
    .await
    .expect("the next turn is not held up")
    .unwrap();

  assert_eq!(again.outcome, agent_session::Outcome::Answered, "{again:?}");
  assert!(stand.stands.load(Ordering::SeqCst) > stands, "the program stood again");
  assert_eq!(stand.held().await, held, "on the same session");
  assert!(stand.context().await.is_some(), "the turn after a fold fills its context again");
}

#[test]
fn only_a_program_that_folds_itself_is_refused_for_it() {
  let session = SessionId(AGENT.to_owned());
  let refused = agency_server::herdr_backend::folds_itself(&session, true).unwrap();

  assert_eq!(refused.kind, FaultKind::Refused);
  assert_eq!(
    agency_server::herdr_backend::folds_itself(&session, false).map(|fault| fault.kind),
    None
  );
}
