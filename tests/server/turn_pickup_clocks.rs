//! A turn picked up after agency's restart held to the door's clocks (docs/sessions.md, 「A restart hands
//! the session over」, docs/agent-control.md, 「A run's tool calls are held to a clock」): a blocked call is
//! cut from the resumed record and its lines kept as a live turn's are, a role session's and a supervisor's
//! turn are held to the call clock as a job's is, and a job's turn past its bound is marked stopped, cut,
//! and its item's lease given back with its assignment let go.

use std::collections::HashMap;
use std::io::Write as _;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::control::work::job_turn_ended;
use agency_server::door::tool_clock::{SilentClock, ToolClock, Watch as ToolWatch};
use agency_server::herdr_backend::{apart_id, conversation_id};
use agency_server::job_end::keeps_lease;
use agency_server::now_ms;
use agency_server::places::Ground;
use agency_server::queue::stops::job_stop_reason;
use agency_server::run_log::{RunLog, Target};
use agency_server::turn_pickup::{InFlight, holds_call_clock, over_bound, watch_tools};
use agent_session::TurnEnd;
use harness_providers::Machine;
use serde_json::{Value, json};

use crate::integration::common::Scratch;

struct Program {
  child: Child,
  pid: i32,
}

impl Program {
  fn spawn(script: &str) -> Self {
    let child = Command::new("bash")
      .arg("-c")
      .arg(script)
      .process_group(0)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .unwrap();
    let pid = child.id() as i32;

    Self { child, pid }
  }
}

impl Drop for Program {
  fn drop(&mut self) {
    // SAFETY: a signal sent to the process group this test started.
    unsafe {
      libc::kill(-self.pid, libc::SIGKILL);
    }

    let _ = self.child.wait();
  }
}

fn clock() -> ToolClock {
  ToolClock {
    min_ms: 300,
    stall_ms: 400,
    look_ms: 20,
    in_process_ms: 400,
    in_process_grace_ms: 2_000,
    in_process_cut: "The tool call {tool} was cut after {seconds} seconds; go on without it.".to_owned(),
    in_process_grace_cut: "cut at its bound after a tool call did not take its cut".to_owned(),
    log_after_ms: 100,
    still_every_ms: 100,
    stop_stall_ms: 100,
    stop_bound_ms: 5_000,
    silent: SilentClock {
      window_ms: i64::MAX / 4,
      idle_cpu_percent: 5,
      ask_chars: 400,
      cut: "The tool call {tool} was cut on the supervisor's call: {why}.".to_owned(),
    },
  }
}

fn stamp(ms: i64) -> String {
  chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms).unwrap().to_rfc3339()
}

fn used(id: &str, command: &str, at: i64) -> Value {
  json!({
    "type": "assistant", "timestamp": stamp(at),
    "message": { "content": [
      { "type": "tool_use", "id": id, "name": "Bash", "input": { "command": command } }
    ] }
  })
}

fn answered(id: &str, at: i64) -> Value {
  json!({
    "type": "user", "timestamp": stamp(at),
    "message": { "content": [
      { "type": "tool_result", "tool_use_id": id, "is_error": true, "content": "killed" }
    ] }
  })
}

fn pid_of(line: &str) -> i32 {
  line
    .split("pid=")
    .nth(1)
    .and_then(|rest| rest.split_whitespace().next())
    .and_then(|pid| pid.parse().ok())
    .unwrap_or(0)
}

async fn gone(pid: i32) -> bool {
  for _ in 0..40 {
    if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
      return true;
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
  }

  false
}

// A child of the program blocked reading a pipe nothing writes to.
const BLOCKED: &str = "f=$(mktemp -u); mkfifo \"$f\"; exec 3<>\"$f\"; cat <&3";

fn ground(scratch: &Scratch, home: &Path) -> Ground {
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

  Ground::new(&service, &machine, None)
}

#[tokio::test]
async fn a_resumed_turn_s_blocked_call_is_cut_and_its_lines_are_kept() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let program = Program::spawn(BLOCKED);
  let began = now_ms();
  let mut records = vec![used("t1", "cat", began)];
  let turn = InFlight {
    session: "session-41".into(),
    turn: 1,
    run: "job-41".into(),
    key: None,
    agent: "worker".into(),
    chat_id: Some(5),
    ledger: None,
    job: Some(41),
    started_at: began,
  };
  let mut watch = ToolWatch::new(clock());
  let mut log = RunLog::new(Target {
    agent: "worker".into(),
    run: "job-41".into(),
    chat_id: Some(5),
    job_id: Some(41),
  });
  let mut said = vec![];
  let mut cut = None;

  for step in 0..40 {
    let now = began + step * 100;

    said.extend(
      watch_tools(&ground, &turn, &records, Some(program.pid), &mut watch, None, &mut log, now).await,
    );

    if said.iter().any(|line| line.contains("cut after")) {
      cut = Some(now);

      break;
    }

    tokio::time::sleep(Duration::from_millis(100)).await;
  }

  let cut = cut.unwrap_or_else(|| panic!("the resumed turn's blocked call was cut: {said:?}"));

  assert!(cut - began <= 4_000, "the cut is bounded: {}ms", cut - began);

  let start = said.iter().find(|line| line.contains("started pid=")).expect("a start line");
  let still = said.iter().find(|line| line.contains("still running")).expect("a still-running line");
  let cut_line = said.iter().find(|line| line.contains("cut after")).unwrap();

  assert!(
    start.contains("tool Bash") && start.contains("cat"),
    "the start names the call: {start}"
  );
  assert!(pid_of(start) > 1, "the start carries its pid: {start}");
  assert!(pid_of(still) > 1, "the still-running line carries its pid: {still}");
  assert!(gone(pid_of(cut_line)).await, "the call's process is gone");

  // The program's tool answers the model with the failed result, and its end is read from the record.
  records.push(answered("t1", began + 2_000));
  said.extend(
    watch_tools(&ground, &turn, &records, Some(program.pid), &mut watch, None, &mut log, began + 2_000)
      .await,
  );

  let end = said.iter().find(|line| line.contains("ended pid=")).expect("an end line");

  assert!(end.contains("after 2.0s: failed"), "the end names its duration and outcome: {end}");

  let kept = store::all(
    scratch.db(),
    r#"SELECT "text" FROM "agent_log" WHERE "run" = 'job-41' AND "kind" = 'note'"#,
    vec![],
  )
  .await
  .unwrap();
  let lines: Vec<String> = kept.iter().filter_map(|row| row.try_get::<String>("", "text").ok()).collect();

  assert!(
    lines.iter().any(|line| line.contains("tool Bash started pid=")),
    "the resumed run's log keeps the start: {lines:?}"
  );
  assert!(
    lines.iter().any(|line| line.contains("cut after")),
    "the resumed run's log keeps the cut: {lines:?}"
  );
  assert!(
    lines.iter().any(|line| line.contains("ended pid=")),
    "the resumed run's log keeps the end: {lines:?}"
  );
}

// A turn picked up as it was carried: a session's id, its run and the job it holds.
fn carried(session: String, run: &str, job: Option<i64>) -> InFlight {
  InFlight {
    session,
    turn: 1,
    run: run.into(),
    key: None,
    agent: "doer".into(),
    chat_id: None,
    ledger: None,
    job,
    started_at: 0,
  }
}

#[test]
fn a_resumed_role_session_s_and_supervisor_s_turn_are_held_to_the_call_clock_as_a_job_s_is() {
  let role = carried(apart_id("doer", 7).0, "role-doer-1", None);
  let job = carried(apart_id("worker", 8).0, "job-41", Some(41));
  let round = carried("bmo@5".into(), "round-5", None);
  let across_rooms = carried("bmo".into(), "round-6", None);
  let conversation = carried(conversation_id("gatekeeper", 9).0, "talk-9", None);

  assert!(holds_call_clock(&role), "a role's run is held to the call clock with no job");
  assert!(holds_call_clock(&job), "a job's turn is held to it as before");
  assert!(holds_call_clock(&round), "a supervisor's round in a room's session is held to it");
  assert!(holds_call_clock(&across_rooms), "and in its one session across rooms");
  assert!(!holds_call_clock(&conversation), "a conversation a caller opened is not");
}

#[tokio::test]
async fn a_resumed_job_past_its_bound_ends_stopped_and_its_item_is_released() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let now = now_ms();
  let began = now - 241 * 60_000;

  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "run_id", "run_name", "room_id", "state", "started_at", "prompt")
         VALUES (41, 7, 9, 'harness', 10, 'worker', 100, 'job-41', 5, 'running', $1, 'work')"#,
      vec![began.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "runs" ("id", "agent", "work_unit", "depth", "started_at")
         VALUES (100, 'worker', 'job-41', 0, $1)"#,
      vec![began.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at",
           "changed_at")
         VALUES (10, 7, 'harness', 'worker', 'running', $1, $1)"#,
      vec![began.into()],
    )
    .await;

  let turn = InFlight {
    session: "session-41".into(),
    turn: 1,
    run: "job-41".into(),
    key: None,
    agent: "worker".into(),
    chat_id: Some(5),
    ledger: None,
    job: Some(41),
    started_at: began,
  };

  let why = over_bound(&ground, &turn, now).await.expect("the turn is past its bound");

  assert!(why.contains("240-minute"), "the reason names the bound: {why}");
  assert_eq!(job_stop_reason(scratch.db(), 41).await.unwrap().as_deref(), Some(why.as_str()));

  job_turn_ended(&ground, 41, TurnEnd::failed("job-41", why.clone(), None)).await;

  let job = store::one(
    scratch.db(),
    r#"SELECT "state", "outcome", "report", "lease_kept" FROM "jobs" WHERE "id" = 41"#,
    vec![],
  )
  .await
  .unwrap()
  .unwrap();

  assert_eq!(job.try_get::<String>("", "state").unwrap(), "stopped");
  assert_eq!(job.try_get::<String>("", "outcome").unwrap(), "stopped");
  assert_eq!(job.try_get::<String>("", "report").unwrap(), why);
  assert!(!job.try_get::<bool>("", "lease_kept").unwrap(), "a stopped job gives its lease back");
  assert!(!keeps_lease(&json!({ "id": 41, "state": "stopped", "leaseId": 9 })));

  let assignment = store::one(scratch.db(), r#"SELECT "state" FROM "assignments" WHERE "id" = 10"#, vec![])
    .await
    .unwrap()
    .unwrap();

  assert_eq!(assignment.try_get::<String>("", "state").unwrap(), "released");
}

fn answer(at: i64, words: &str) -> Value {
  json!({
    "type": "assistant", "timestamp": stamp(at),
    "message": { "id": "m-1", "role": "assistant", "content": [{ "type": "text", "text": words }] }
  })
}

fn turn_end(at: i64) -> Value {
  json!({ "type": "system", "subtype": "turn_duration", "timestamp": stamp(at) })
}

fn wrote(path: &Path, value: &Value) {
  std::fs::create_dir_all(path.parent().unwrap()).unwrap();
  std::fs::write(path, format!("{value}\n")).unwrap();
}

fn hooked(path: &Path, at: i64, kind: &str, payload: Value) {
  let line = json!({ "at": at, "kind": kind, "payload": payload });

  std::fs::create_dir_all(path.parent().unwrap()).unwrap();

  let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
  writeln!(file, "{line}").unwrap();
}

async fn all_logged(scratch: &Scratch) -> Vec<String> {
  store::all(
    scratch.db(),
    r#"SELECT "text" FROM "agent_log" WHERE "run" = 'job-41' ORDER BY "id""#,
    vec![],
  )
  .await
  .unwrap_or_default()
  .iter()
  .filter_map(|row| row.try_get::<String>("", "text").ok())
  .collect()
}

async fn logged(scratch: &Scratch, needle: &str) -> bool {
  all_logged(scratch).await.iter().any(|text| text.contains(needle))
}

// The whole pickup path: a kept turn found going, attached to, its blocked call cut from the record its
// program writes, the failed result and answer read, and its end handed back.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_resumed_through_the_pickup_is_cut_and_handed_back() {
  use agency_server::lines::place::Place;
  use agency_server::turn_pickup::waited;
  use agent_host::identity::ProcessIdentity;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = std::sync::Arc::new(ground(&scratch, home.path()));
  let data = home.path().join("data");
  let place = data.join("agent-places").join("worker");
  let transcript = home.path().join("session-41.jsonl");
  let program = Program::spawn(BLOCKED);
  let identity = ProcessIdentity::of(program.pid).unwrap();
  let began = now_ms();

  std::fs::create_dir_all(&place).unwrap();
  wrote(
    &place.join("running.json"),
    &json!({
      "identity": "cut", "pid": identity.pid, "birth": identity.birth,
      "loop": identity, "starts": 1, "startedAt": 1, "paneId": "pane-1", "recent": [],
    }),
  );
  wrote(
    &place.join("run.json"),
    &json!({ "name": "job-41", "title": "the work", "person": false, "startedAt": began, "endedAt": null }),
  );
  hooked(
    &place.join("hooks.jsonl"),
    began + 1,
    "submit",
    json!({ "session_id": "session-41", "transcript_path": transcript }),
  );
  {
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&transcript).unwrap();
    writeln!(file, "{}", used("t1", "cat", began + 2)).unwrap();
  }

  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "run_id", "run_name", "room_id", "state", "started_at", "prompt")
         VALUES (41, 7, 9, 'harness', 10, 'worker', 100, 'job-41', 5, 'running', $1, 'work')"#,
      vec![began.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "runs" ("id", "agent", "work_unit", "depth", "started_at")
         VALUES (100, 'worker', 'job-41', 0, $1)"#,
      vec![began.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at",
           "changed_at")
         VALUES (10, 7, 'harness', 'worker', 'running', $1, $1)"#,
      vec![began.into()],
    )
    .await;

  let turn = InFlight {
    session: "session-41".into(),
    turn: 1,
    run: "job-41".into(),
    key: None,
    agent: "worker".into(),
    chat_id: None,
    ledger: None,
    job: Some(41),
    started_at: began,
  };

  let held = Place::of(&data, "worker");

  assert!(held.program_alive().is_some(), "the place's program is alive: {:?}", held.running());

  let waiting = {
    let ground = ground.clone();
    let turn = turn.clone();

    tokio::spawn(async move {
      let place = Place::of(&data, "worker");

      waited(&ground, &place, None, &turn, clock()).await;
    })
  };

  let mut cut = false;

  for _ in 0..200 {
    if logged(&scratch, "cut after").await {
      cut = true;

      break;
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
  }

  assert!(cut, "the resumed turn's blocked call is cut; log: {:?}", all_logged(&scratch).await);

  // The program's tool answers the model with the failed result, the model answers, and its turn ends.
  {
    let mut file = std::fs::OpenOptions::new().append(true).open(&transcript).unwrap();
    let at = now_ms();

    for line in [
      answered("t1", at),
      answer(at + 1, "the call was cut and the work went on"),
      turn_end(at + 2),
    ] {
      writeln!(file, "{line}").unwrap();
    }
  }
  wrote(
    &place.join("run.json"),
    &json!({ "name": "job-41", "title": "the work", "person": false, "startedAt": began, "endedAt": now_ms() }),
  );
  hooked(&place.join("hooks.jsonl"), now_ms(), "stop", json!({}));

  tokio::time::timeout(Duration::from_secs(20), waiting)
    .await
    .expect("the picked-up turn's end is handed back")
    .unwrap();

  assert!(logged(&scratch, "cut after").await, "the run log keeps the cut");
  assert!(
    logged(&scratch, "the call was cut and the work went on").await || logged(&scratch, "answered").await,
    "the run log keeps the answer's end"
  );

  let job = store::one(scratch.db(), r#"SELECT "state" FROM "jobs" WHERE "id" = 41"#, vec![])
    .await
    .unwrap()
    .unwrap();

  assert_eq!(job.try_get::<String>("", "state").unwrap(), "reviewing");
}
