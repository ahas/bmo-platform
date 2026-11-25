//! A supervisor's background work after the turn that launched it (docs/agent-control.md, 「Background work on a
//! line that carries it apart」): a task its notice ended is told once to the room it was launched in, as an order
//! for its supervisor opening a turn there; a task whose program is gone, or that a stop ended, is told once as
//! lost; a task with no room, and one still running under its program, is told nowhere. A notice the program
//! took up without a submit ends its task from the session record, and work a stop of its session left out with
//! no notice is settled once the grace passed, told to no room. What still runs is answered as room state, and
//! each launch and each end is told once on the stream (docs/api.md, 「background-work」).

use agency_server::door::place_run::{BackgroundLine, Launch, background_file, hooks_file};
use agency_server::door::record::task_notice;
use agency_server::events::Bus;
use agency_server::lines::place::place_of;
use agency_server::watches::background_work::{Watch, kept_file, pass, running_work};
use agent_host::identity::ProcessIdentity;
use serde_json::{Value, json};

use crate::support::peers::stand;
use crate::support::scratch::Scratch;

const AGENT: &str = "bmo";

// A place whose program is this test's own process, started at `started_at`, and whose background record holds
// `lines`.
fn place_with(data: &std::path::Path, started_at: i64, lines: &[BackgroundLine]) -> std::path::PathBuf {
  let place = place_of(data, AGENT);
  let identity = ProcessIdentity::own();

  std::fs::create_dir_all(&place).unwrap();
  std::fs::write(
    place.join("running.json"),
    json!({ "identity": "one", "pid": identity.pid, "birth": identity.birth, "loop": identity,
            "starts": 1, "startedAt": started_at, "recent": [] })
    .to_string(),
  )
  .unwrap();
  std::fs::write(
    background_file(&place),
    lines
      .iter()
      .map(|line| format!("{}\n", serde_json::to_string(line).unwrap()))
      .collect::<String>(),
  )
  .unwrap();

  place
}

fn launched(task: &str, kind: &str, room: Option<&str>, at: i64) -> BackgroundLine {
  BackgroundLine::Launched(Launch {
    at,
    task: task.to_owned(),
    kind: kind.to_owned(),
    description: format!("the work of {task}"),
    room: room.map(str::to_owned),
    agent: room.map(|_| AGENT.to_owned()),
    run: room.map(|room| format!("chat:{room}:1")),
    ..Launch::default()
  })
}

fn noticed(task: &str, at: i64) -> BackgroundLine {
  let notice = task_notice(&format!(
    "<task-notification>\n<task-id>{task}</task-id>\n<output-file>/tmp/tasks/{task}.output</output-file>\n\
     <status>completed</status>\n<result>Six sets made.</result>\n</task-notification>"
  ))
  .unwrap();

  BackgroundLine::Notice { at, notice }
}

fn order(body: &Value) -> &str {
  body["said"]["order"].as_str().unwrap_or_default()
}

/// a task its notice ended is told once to the room it was launched in, as an order for the supervisor that
/// launched it, opening a turn there and carrying what it handed back
#[tokio::test]
async fn a_finished_notice_is_told_once_to_the_room_its_work_was_launched_in() {
  let scratch = Scratch::new().await;
  let (peers, vars) = stand().await;
  let data = tempfile::tempdir().unwrap();
  let place = place_with(
    data.path(),
    1_000,
    &[launched("a60", "subagent", Some("-500"), 2_000), noticed("a60", 3_000)],
  );
  let mut watch = Watch::default();

  pass(scratch.db(), &vars, &Bus::default(), data.path(), &mut watch).await;
  pass(scratch.db(), &vars, &Bus::default(), data.path(), &mut watch).await;

  let told = peers.lock().unwrap().instructed();

  assert_eq!(told.len(), 1, "told once: {told:?}");
  assert_eq!(told[0]["room"], -500);
  assert_eq!(told[0]["agentName"], AGENT);
  assert_eq!(told[0]["sort"], "background");
  assert_eq!(told[0]["kind"], "background-ended:a60");
  assert_eq!(told[0]["cause"], "reply");
  assert!(
    order(&told[0]).contains("has ended (completed): the subagent \"the work of a60\""),
    "{}",
    order(&told[0])
  );
  assert!(order(&told[0]).contains("Six sets made.") && order(&told[0]).contains("/tmp/tasks/a60.output"));
  assert!(std::fs::read_to_string(kept_file(&place)).unwrap().contains("a60"));
}

/// a task whose program was stood afresh after it was launched is told once as lost, one a stop ended is told as
/// stopped, and a task with no room, or one still running under the program that launched it, is told nowhere
#[tokio::test]
async fn work_lost_or_stopped_is_told_once_and_work_with_no_room_or_still_running_nowhere() {
  let scratch = Scratch::new().await;
  let (peers, vars) = stand().await;
  let data = tempfile::tempdir().unwrap();

  place_with(
    data.path(),
    5_000,
    &[
      launched("a61", "subagent", Some("-500"), 2_000),
      launched("b62", "shell", Some("-600"), 2_000),
      BackgroundLine::Stopped {
        at: 2_500,
        task: "b62".to_owned(),
      },
      launched("b63", "shell", None, 2_000),
      noticed("b63", 3_000),
      launched("b64", "shell", Some("-500"), 6_000),
    ],
  );

  let mut watch = Watch::default();

  pass(scratch.db(), &vars, &Bus::default(), data.path(), &mut watch).await;
  pass(scratch.db(), &vars, &Bus::default(), data.path(), &mut watch).await;

  let told = peers.lock().unwrap().instructed();
  let kinds: Vec<&str> = told.iter().map(|body| body["kind"].as_str().unwrap()).collect();

  assert_eq!(kinds, ["background-lost:a61", "background-lost:b62"], "{told:?}");
  assert!(
    order(&told[0]).contains("Your program was ended or stood afresh"),
    "{}",
    order(&told[0])
  );
  assert!(order(&told[1]).contains("A stop of your turn ended it."), "{}", order(&told[1]));
  assert_eq!(told[1]["room"], -600);
}

/// the work still running is answered with its room, oldest first, by room where one is named; what a notice, a
/// stop or the program's loss ended stands nowhere
#[test]
fn only_the_work_still_running_is_answered_and_by_its_room() {
  let data = tempfile::tempdir().unwrap();

  place_with(
    data.path(),
    1_000,
    &[
      launched("b70", "shell", Some("-600"), 3_000),
      launched("a71", "subagent", Some("-500"), 2_000),
      launched("a72", "subagent", Some("-500"), 2_500),
      noticed("a72", 2_800),
      launched("b73", "shell", Some("-500"), 500),
    ],
  );

  let every: Vec<(String, Option<i64>)> = running_work(data.path(), None)
    .into_iter()
    .map(|work| (work.task, work.room_id))
    .collect();
  let one = running_work(data.path(), Some(-500));

  assert_eq!(every, [("a71".to_owned(), Some(-500)), ("b70".to_owned(), Some(-600))]);
  assert_eq!(one.len(), 1);
  assert_eq!(one[0].description, "the work of a71");
  assert_eq!(
    (one[0].kind.as_str(), one[0].status.as_str(), one[0].launched_at),
    ("subagent", "running", 2_000)
  );
  assert_eq!(one[0].agent_name.as_deref(), Some(AGENT));
}

/// a launch is told on the stream once, on the look that first reads it running, and its end once, with the word
/// its notice ended on
#[tokio::test]
async fn a_launch_and_its_end_are_each_told_once_on_the_stream() {
  let scratch = Scratch::new().await;
  let (_peers, vars) = stand().await;
  let data = tempfile::tempdir().unwrap();
  let events = Bus::default();
  let mut heard = events.subscribe_wire(None);
  let place = place_with(data.path(), 1_000, &[launched("a80", "subagent", Some("-500"), 2_000)]);
  let mut watch = Watch::default();

  pass(scratch.db(), &vars, &events, data.path(), &mut watch).await;
  pass(scratch.db(), &vars, &events, data.path(), &mut watch).await;

  let mut record = std::fs::OpenOptions::new().append(true).open(background_file(&place)).unwrap();

  std::io::Write::write_all(
    &mut record,
    format!("{}\n", serde_json::to_string(&noticed("a80", 3_000)).unwrap()).as_bytes(),
  )
  .unwrap();
  pass(scratch.db(), &vars, &events, data.path(), &mut watch).await;
  pass(scratch.db(), &vars, &events, data.path(), &mut watch).await;

  let told: Vec<(String, Value)> = std::iter::from_fn(|| heard.ready())
    .map(|event| (event.name.to_string(), event.body))
    .collect();
  let kinds: Vec<&str> = told.iter().map(|(kind, _)| kind.as_str()).collect();

  assert_eq!(kinds, ["background-work.created", "background-work.ended"], "{told:?}");
  assert_eq!(told[0].1["status"], "running");
  assert_eq!(told[0].1["roomId"], -500);
  assert_eq!(told[1].1["status"], "completed");
  assert_eq!(told[1].1["task"], "a80");
}

// The stream's background-work events heard so far, each its kind and its status.
fn heard(events: &mut agency_server::events::WireSubscription) -> Vec<(String, String)> {
  std::iter::from_fn(|| events.ready())
    .map(|event| (event.name.to_string(), event.body["status"].as_str().unwrap_or_default().to_owned()))
    .collect()
}

/// a notice the program took off its queue into a running turn, which no submit handed over, ends its task from
/// the session record its launch names, and the room is told as for any ended notice
#[tokio::test]
async fn a_notice_only_the_session_record_holds_ends_its_task() {
  let scratch = Scratch::new().await;
  let (peers, vars) = stand().await;
  let data = tempfile::tempdir().unwrap();
  let session = data.path().join("session-a.jsonl");
  let queued = json!({ "type": "queue-operation", "operation": "enqueue", "timestamp": "2026-11-20T07:07:36.500Z",
    "content": "<task-notification>\n<task-id>b90</task-id>\n<output-file>/tmp/tasks/b90.output</output-file>\n\
      <status>completed</status>\n<summary>Background command done</summary>\n</task-notification>" });

  std::fs::write(
    &session,
    format!("{}\n{}\n", json!({ "type": "user", "message": { "content": "hello" } }), queued),
  )
  .unwrap();

  let launch = match launched("b90", "shell", Some("-500"), 2_000) {
    BackgroundLine::Launched(launch) => Launch {
      transcript: Some(session.display().to_string()),
      ..launch
    },
    _ => unreachable!(),
  };

  place_with(data.path(), 1_000, &[BackgroundLine::Launched(launch)]);

  let events = Bus::default();
  let mut stream = events.subscribe_wire(None);
  let mut watch = Watch::default();

  pass(scratch.db(), &vars, &events, data.path(), &mut watch).await;
  pass(scratch.db(), &vars, &events, data.path(), &mut watch).await;

  let told = peers.lock().unwrap().instructed();

  assert_eq!(told.len(), 1, "{told:?}");
  assert_eq!(told[0]["kind"], "background-ended:b90");
  assert!(order(&told[0]).contains("has ended (completed)"), "{}", order(&told[0]));
  assert_eq!(heard(&mut stream), [("background-work.ended".to_owned(), "completed".to_owned())]);
  assert!(running_work(data.path(), None).is_empty());
}

/// a running task a later stop of its own session leaves out, with no notice anywhere, is settled as gone once the
/// grace passed since that stop: no room is told, the stream hears it ended, and it leaves the list; a task
/// launched after that stop still runs
#[tokio::test]
async fn work_a_later_stop_left_out_is_settled_as_gone_and_told_to_no_room() {
  let scratch = Scratch::new().await;
  let (peers, vars) = stand().await;
  let data = tempfile::tempdir().unwrap();
  let now = chrono::Utc::now().timestamp_millis();
  let long_ago = now - 600_000;
  let in_session = |task: &str, at: i64| match launched(task, "shell", Some("-500"), at) {
    BackgroundLine::Launched(launch) => BackgroundLine::Launched(Launch {
      session: Some("session-a".to_owned()),
      ..launch
    }),
    _ => unreachable!(),
  };
  let place = place_with(
    data.path(),
    1_000,
    &[in_session("b91", long_ago - 1_000), in_session("b92", long_ago + 1_000)],
  );

  std::fs::write(
    hooks_file(&place),
    format!(
      "{}\n",
      json!({ "at": long_ago, "kind": "stop", "payload": { "session_id": "session-a", "background_tasks": [] } })
    ),
  )
  .unwrap();

  let events = Bus::default();
  let mut stream = events.subscribe_wire(None);
  let mut watch = Watch::default();

  pass(scratch.db(), &vars, &events, data.path(), &mut watch).await;
  pass(scratch.db(), &vars, &events, data.path(), &mut watch).await;

  assert!(peers.lock().unwrap().instructed().is_empty());

  let mut kinds = heard(&mut stream);

  kinds.sort();
  assert_eq!(
    kinds,
    [
      ("background-work.created".to_owned(), "running".to_owned()),
      ("background-work.ended".to_owned(), "gone".to_owned())
    ]
  );

  let left: Vec<String> = running_work(data.path(), None).into_iter().map(|work| work.task).collect();

  assert_eq!(left, ["b92"]);
}
