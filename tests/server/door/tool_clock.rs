//! A run's tool calls held to a clock (docs/agent-control.md, 「A run's tool calls are held to a clock」): a
//! call blocked on a pipe nothing writes to is cut after it stops burning processor time, a call that keeps
//! burning it is left alone, a blocked call waiting on the job's own question runs on, and a stop cuts a
//! blocked call so its words can be read. The lines the watcher writes name the call, its pid, its duration
//! and its outcome. A call gone silent and idle is handed over to be asked about, but not while it polls
//! the log of the job's own work that still lives.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agency_server::door::execution::Identity;
use agency_server::door::looks::GoneConfirm;
use agency_server::door::stall::{Progress, StallChange, StallClock};
use agency_server::door::stand::Interrupt;
use agency_server::door::tool_clock::{
  BackgroundCall, BackgroundClock, CallVerdict, ClockNote, NoQuestion, Owns, QuestionWait, SilentClock,
  ToolClock, WaitedWork, Watch as ToolWatch, end_processes, task_processes,
};
use agency_server::door::turn::{Ending, Seat, Timings, Turn, Watch};
use agency_server::job_run::claude::turn::work_clocks;
use agency_server::places::Ground;
use agent_session::JobMark;
use harness_providers::Machine;
use harness_providers::claude_code::ClaudeCode;
use serde_json::{Value, json};

use crate::integration::common;
use crate::integration::common::Scratch;
use crate::integration::common::herdr::HerdrModel;

// A program of the test's own, in a process group of its own so every process it starts is taken down with
// it.
struct Program {
  child: Child,
  pid: i32,
}

impl Program {
  fn spawn(script: &str) -> Self {
    Self::spawn_args(&["bash", "-c", script])
  }

  fn spawn_args(args: &[&str]) -> Self {
    let child = Command::new(args[0])
      .args(&args[1..])
      .process_group(0)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .unwrap();
    let pid = child.id() as i32;

    Self { child, pid }
  }

  // A program making a shell call as the turn's program makes one: the call a shell of its own under it,
  // its standard output and standard error on a file of its own.
  fn spawn_writing(script: &str, output: &Path) -> Self {
    let file = std::fs::File::create(output).unwrap();
    let child = Command::new("bash")
      .args(["-c", "bash -c \"$0\" & wait", script])
      .process_group(0)
      .stdin(Stdio::null())
      .stdout(file.try_clone().unwrap())
      .stderr(file)
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

// A session id of this test's own. The door finds a turn's program by the session its command line carries
// on the whole machine, so a fixed id would find the same test's program in another harness running beside
// this one, and read that program's end as this turn's.
fn own_session(name: &str) -> String {
  format!("{name}-{}-{}", std::process::id(), agency_server::now_ms())
}

fn clock(min_ms: i64, stall_ms: i64, stop_stall_ms: i64) -> ToolClock {
  ToolClock {
    min_ms,
    stall_ms,
    look_ms: 20,
    in_process_ms: 400,
    in_process_grace_ms: 2_000,
    in_process_cut: "The tool call {tool} was cut after {seconds} seconds; go on without it.".to_owned(),
    in_process_grace_cut: "cut at its bound after a tool call did not take its cut".to_owned(),
    log_after_ms: 100,
    still_every_ms: 100,
    stop_stall_ms,
    stop_bound_ms: 5_000,
    silent: unasked(),
  }
}

// A silent rule whose window no test's call reaches, for the tests of the first rule.
fn unasked() -> SilentClock {
  SilentClock {
    window_ms: i64::MAX / 4,
    idle_cpu_percent: 5,
    ask_chars: 400,
    cut: "The tool call {tool} was cut on the supervisor's call after {seconds} seconds: {why}.".to_owned(),
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

fn answered(id: &str, at: i64, is_error: bool) -> Value {
  json!({
    "type": "user", "timestamp": stamp(at),
    "message": { "content": [
      { "type": "tool_result", "tool_use_id": id, "is_error": is_error, "content": "killed" }
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

// A child of the program blocked reading a pipe nothing writes to: the script opens a fifo read-write and
// hands it to `cat`, which then waits on a read that never comes.
const BLOCKED: &str = "f=$(mktemp -u); mkfifo \"$f\"; exec 3<>\"$f\"; cat <&3";

#[tokio::test]
async fn a_call_blocked_on_a_pipe_is_cut_and_its_lines_carry_the_pid_and_duration() {
  let program = Program::spawn(BLOCKED);
  let mut watch = ToolWatch::new(clock(300, 400, 100));
  let began = agency_server::now_ms();
  let mut records = vec![used("t1", "cat", began)];
  let mut said = vec![];
  let mut cut = None;

  for step in 0..40 {
    let now = began + step * 100;

    said.extend(sentences(watch.look("job-41", &records, 0, program.pid, now, &mut NoQuestion).await));

    if said.iter().any(|line| line.contains("cut after")) {
      cut = Some(now);

      break;
    }

    tokio::time::sleep(Duration::from_millis(100)).await;
  }

  let cut = cut.unwrap_or_else(|| panic!("the blocked call was cut within its clock: {said:?}"));

  assert!(cut - began <= 4_000, "the cut is bounded: {}ms", cut - began);

  let start = said.iter().find(|line| line.contains("started pid=")).expect("a start line");
  let still = said.iter().find(|line| line.contains("still running")).expect("a still-running line");
  let cut_line = said.iter().find(|line| line.contains("cut after")).unwrap();

  assert!(
    start.contains("tool Bash") && start.contains("cat"),
    "the start names the call: {start}"
  );
  assert!(pid_of(start) > 1, "the start carries its pid: {start}");
  assert!(still.contains("pid="), "the still-running line carries its pid: {still}");
  assert!(cut_line.contains("pid="), "the cut line carries its pid: {cut_line}");
  assert!(gone(pid_of(cut_line)).await, "the call's process is gone");

  // The harness reports the killed call to the model, and its end is written with its duration and outcome.
  records.push(answered("t1", began + 2_000, true));
  said.extend(sentences(
    watch.look("job-41", &records, 0, program.pid, began + 2_000, &mut NoQuestion).await,
  ));

  let end = said.iter().find(|line| line.contains("ended pid=")).expect("an end line");

  assert!(end.contains("after 2.0s: failed"), "the end names its duration and outcome: {end}");
  assert!(pid_of(end) > 1, "the end carries its pid: {end}");
}

#[tokio::test]
async fn a_call_that_keeps_burning_processor_is_not_cut() {
  let program = Program::spawn("bash -c 'while :; do :; done' & wait");
  let mut watch = ToolWatch::new(clock(200, 200, 100));
  let began = agency_server::now_ms();
  let records = vec![used("t1", "loop", began)];
  let mut said = vec![];

  for step in 0..15 {
    said.extend(sentences(
      watch
        .look("job-41", &records, 0, program.pid, began + step * 100, &mut NoQuestion)
        .await,
    ));
    tokio::time::sleep(Duration::from_millis(100)).await;
  }

  assert!(
    !said.iter().any(|line| line.contains("cut after")),
    "a working call is left to finish: {said:?}"
  );
  assert!(said.iter().any(|line| line.contains("still running")), "it is still reported: {said:?}");
}

// A check waiting on a step run on another machine: its processes here burn next to nothing and begin
// none, but it writes a waiting line every so often. The shell's own `read` with a timeout on a fifo nothing
// writes to stands in for the wait, so no pass begins a process.
const SAYS_IT_WAITS: &str = "f=$(mktemp -u); mkfifo \"$f\"; exec 3<>\"$f\"; rm \"$f\"; \
  while :; do echo still waiting for the placed step; read -t 0.15 -u 3; done";

#[tokio::test]
async fn a_call_whose_output_keeps_growing_is_not_cut_for_its_stall() {
  let scratch = tempfile::tempdir().unwrap();
  let program = Program::spawn_writing(SAYS_IT_WAITS, &scratch.path().join("call.output"));
  let mut watch = ToolWatch::new(clock(200, 400, 100));
  let began = agency_server::now_ms();
  let records = vec![used("t1", SAYS_IT_WAITS, began)];
  let mut said = vec![];

  for step in 0..15 {
    said.extend(sentences(
      watch
        .look("job-41", &records, 0, program.pid, began + step * 100, &mut NoQuestion)
        .await,
    ));
    tokio::time::sleep(Duration::from_millis(100)).await;
  }

  assert!(
    !said.iter().any(|line| line.contains("cut after")),
    "a call whose output grows is left to finish: {said:?}"
  );
  assert!(said.iter().any(|line| line.contains("still running")), "it is still reported: {said:?}");
}

// A call waiting inside the program with no process of its own: the program has no children, so its
// call owns no tree. Past the call's own bound it is cut, and the words that end it are handed out.
#[tokio::test]
async fn a_call_with_no_process_of_its_own_is_cut_at_its_in_process_bound() {
  let program = Program::spawn("exec sleep 100000");
  let mut watch = ToolWatch::new(clock(300, 400, 100));
  let began = agency_server::now_ms();
  let mut records = vec![json!({
    "type": "assistant", "timestamp": stamp(began),
    "message": { "content": [
      { "type": "tool_use", "id": "t1", "name": "WebFetch", "input": { "url": "https://example.invalid/" } }
    ] }
  })];
  let mut said = vec![];
  let mut cut = None;

  for step in 0..40 {
    let now = began + step * 100;

    said.extend(sentences(watch.look("job-41", &records, 0, program.pid, now, &mut NoQuestion).await));

    if let Some(found) = watch.take_in_process_cut() {
      cut = Some((now, found));

      break;
    }

    tokio::time::sleep(Duration::from_millis(100)).await;
  }

  let (at, cut) = cut.unwrap_or_else(|| panic!("the in-process call was cut: {said:?}"));

  assert!(at - began <= 1_500, "the cut is bounded: {}ms", at - began);
  assert!(
    said
      .iter()
      .any(|line| line.contains("cut after") && line.contains("no process of its own")),
    "the cut line says the call had no process: {said:?}"
  );
  assert!(cut.words.contains("WebFetch"), "the words name the call: {}", cut.words);
  assert!(watch.in_process_cut_still(), "the call stands until its result is written");

  // The program takes the words and answers the call, so the clock lets it go.
  records.push(answered("t1", at + 100, true));
  said.extend(sentences(
    watch.look("job-41", &records, 0, program.pid, at + 100, &mut NoQuestion).await,
  ));

  assert!(!watch.in_process_cut_still(), "the answered call is let go");
  assert!(
    said.iter().any(|line| line.contains("ended pid=-") && line.contains("failed")),
    "the end is written with the call's outcome: {said:?}"
  );
}

#[tokio::test]
async fn a_stop_cuts_a_call_blocked_on_a_pipe_within_its_bound() {
  let program = Program::spawn(BLOCKED);
  let mut watch = ToolWatch::new(clock(60_000, 60_000, 100));
  let began = agency_server::now_ms();
  let records = vec![used("t1", "cat", began)];

  // A look first, so the call's process is known.
  watch.look("job-41", &records, 0, program.pid, began, &mut NoQuestion).await;
  tokio::time::sleep(Duration::from_millis(200)).await;

  let said = sentences(watch.cut_for_stop("job-41", &records, 0, program.pid, began + 300).await);
  let cut = said.iter().find(|line| line.contains("cut for the stop")).unwrap_or_else(|| {
    panic!("the blocked call was cut for the stop: {said:?}");
  });

  assert!(pid_of(cut) > 1, "the cut carries its pid: {cut}");
  assert!(gone(pid_of(cut)).await, "the call's process is gone");
}

// The job's own question still waiting, counting how often it is read.
struct QuestionWaits {
  read: usize,
}

impl QuestionWait for QuestionWaits {
  async fn waits_on_question(&mut self, _call: &str) -> bool {
    self.read += 1;

    true
  }
}

// A job's `bmo job ask` waits on its answer burning nothing, as a blocked read does, so its stall falls due
// however the silent rule stands: while the job's question waits the call runs on through stall after stall,
// the question read only when one falls due, and a stop still cuts it.
#[tokio::test]
async fn a_blocked_call_waiting_on_the_job_s_question_runs_on_until_a_stop() {
  let program = Program::spawn(BLOCKED);
  let mut watch = ToolWatch::new(clock(300, 400, 100));
  let began = agency_server::now_ms();
  let records = vec![used("t1", "bmo job ask --talk 7 --ack", began)];
  let mut question = QuestionWaits { read: 0 };
  let mut said = vec![];

  for step in 0..20 {
    let now = began + step * 100;

    said.extend(sentences(watch.look("job-41", &records, 0, program.pid, now, &mut question).await));
    tokio::time::sleep(Duration::from_millis(100)).await;
  }

  let waiting: Vec<&String> =
    said.iter().filter(|line| line.contains("waits on the job's own question")).collect();

  assert!(
    !said.iter().any(|line| line.contains("cut after")),
    "the waiting call is not cut: {said:?}"
  );
  assert!(waiting.len() >= 2, "each stall falls due and is let go: {said:?}");
  assert_eq!(question.read, waiting.len(), "the question is read only as a stall falls due: {said:?}");
  assert!(waiting.iter().all(|line| pid_of(line) > 1), "the line carries its pid: {waiting:?}");

  let stopped = sentences(watch.cut_for_stop("job-41", &records, 0, program.pid, began + 2_100).await);
  let cut = stopped.iter().find(|line| line.contains("cut for the stop")).unwrap_or_else(|| {
    panic!("a stop cut the waiting call: {stopped:?}");
  });

  assert!(gone(pid_of(cut)).await, "the call's process is gone");
}

// A silent rule a test's call reaches within a second: its idle share is generous, since a loaded machine
// spends more on starting a process than the policy's share allows.
fn asking(min_ms: i64, window_ms: i64) -> ToolClock {
  ToolClock {
    silent: SilentClock {
      window_ms,
      idle_cpu_percent: 30,
      ..unasked()
    },
    ..clock(min_ms, 60_000, 100)
  }
}

// A wait that polls as a job's does: one line said, then a loop that sleeps and looks for a file that never
// comes, beginning a process on every pass and writing nothing.
const POLLING: &str =
  "echo waiting for the other job to close; until [ -e /nonexistent/closed ]; do sleep 0.1; done";

// The clock looked at every 100 ms for up to `steps` looks, until a call is handed over as silent.
async fn looked_until_silent(
  watch: &mut ToolWatch,
  records: &[Value],
  program: i32,
  began: i64,
  steps: i64,
  said: &mut Vec<String>,
) -> Option<(i64, agency_server::door::tool_clock::SilentCall)> {
  for step in 0..steps {
    let now = began + step * 100;

    said.extend(sentences(watch.look("job-41", records, 0, program, now, &mut NoQuestion).await));

    if let Some(silent) = watch.take_silent() {
      return Some((now, silent));
    }

    tokio::time::sleep(Duration::from_millis(100)).await;
  }

  None
}

#[tokio::test]
async fn a_call_that_polls_with_no_new_output_is_asked_about_and_runs_on() {
  let scratch = tempfile::tempdir().unwrap();
  let program = Program::spawn_writing(POLLING, &scratch.path().join("call.output"));
  let mut watch = ToolWatch::new(asking(200, 400));
  let began = agency_server::now_ms();
  let records = vec![used("t1", POLLING, began)];
  let mut said = vec![];
  let (at, silent) = looked_until_silent(&mut watch, &records, program.pid, began, 30, &mut said)
    .await
    .unwrap_or_else(|| panic!("the polling call was handed over as silent: {said:?}"));

  assert!(at - began <= 2_000, "it is asked about within its window: {}ms", at - began);
  assert_eq!(silent.id, "t1");
  assert!(
    silent.tool.starts_with("Bash echo waiting"),
    "the ask carries the command: {}",
    silent.tool
  );
  assert!(
    silent.tool.contains("sleep 0.1"),
    "the whole command, not the line's cut: {}",
    silent.tool
  );
  assert_eq!(silent.started_at, began, "how long it has run is read from its start");
  assert!(silent.output_changed_at < at, "its output last changed before the window");
  assert_eq!(silent.last_line.as_deref(), Some("waiting for the other job to close"));
  assert!(silent.cpu_percent < 30.0, "its share is under the idle share: {}", silent.cpu_percent);
  assert_eq!(silent.window_ms, 400);
  assert!(
    !said.iter().any(|line| line.contains("cut after")),
    "nothing cuts a call that is asked about: {said:?}"
  );

  // An ask stands with no verdict: the call runs on, and the verdict is looked for on the next read.
  let answered = watch.answer("job-41", "t1", CallVerdict::Waiting, at).await;

  assert!(
    sentences(answered.lines.clone()).iter().any(|line| line.contains("asked about")),
    "the ask is written: {answered:?}"
  );
  assert!(answered.words.is_none());
  tokio::time::sleep(Duration::from_millis(100)).await;
  said.extend(sentences(
    watch.look("job-41", &records, 0, program.pid, at + 100, &mut NoQuestion).await,
  ));

  let polled = watch.take_silent().expect("an ask that stands is looked at again");

  assert_eq!(polled.id, "t1");
  assert!(std::path::Path::new(&format!("/proc/{}", program.pid)).exists(), "the call runs on");
  assert!(
    watch.answer("job-41", "t1", CallVerdict::Waiting, at + 100).await.lines.is_empty(),
    "a verdict looked for again writes no second ask"
  );
}

#[tokio::test]
async fn a_call_whose_output_keeps_growing_is_not_asked_about() {
  let scratch = tempfile::tempdir().unwrap();
  let script = "while :; do echo one more pass; sleep 0.1; done";
  let program = Program::spawn_writing(script, &scratch.path().join("call.output"));
  let mut watch = ToolWatch::new(asking(200, 400));
  let began = agency_server::now_ms();
  let records = vec![used("t1", script, began)];
  let mut said = vec![];

  assert!(
    looked_until_silent(&mut watch, &records, program.pid, began, 15, &mut said)
      .await
      .is_none(),
    "a call writing on every pass is left alone: {said:?}"
  );
  assert!(!said.iter().any(|line| line.contains("cut after")), "and is not cut: {said:?}");
}

#[tokio::test]
async fn a_kept_wait_goes_on_and_is_asked_about_again_once_its_keep_runs_out() {
  let scratch = tempfile::tempdir().unwrap();
  let program = Program::spawn_writing(POLLING, &scratch.path().join("call.output"));
  let mut watch = ToolWatch::new(asking(200, 400));
  let began = agency_server::now_ms();
  let records = vec![used("t1", POLLING, began)];
  let mut said = vec![];
  let (at, _) = looked_until_silent(&mut watch, &records, program.pid, began, 30, &mut said)
    .await
    .expect("the polling call was handed over");

  // The supervisor keeps it, with a time to look again.
  let until = at + 600;
  let kept = watch.answer("job-41", "t1", CallVerdict::Kept(until), at).await;

  assert!(
    sentences(kept.lines.clone())
      .iter()
      .any(|line| line.contains("kept on the supervisor's call")),
    "{kept:?}"
  );

  let mut again = None;

  for step in 1..20 {
    let now = at + step * 100;

    tokio::time::sleep(Duration::from_millis(100)).await;
    said.extend(sentences(watch.look("job-41", &records, 0, program.pid, now, &mut NoQuestion).await));

    if let Some(silent) = watch.take_silent() {
      again = Some((now, silent));

      break;
    }

    assert!(
      std::path::Path::new(&format!("/proc/{}", program.pid)).exists(),
      "the kept call runs on"
    );
  }

  let (again_at, again) = again.expect("still silent and idle, it is asked about again");

  assert!(again_at >= until, "not before its keep runs out: {}ms early", until - again_at);
  assert_eq!(again.last_line.as_deref(), Some("waiting for the other job to close"));

  // A cut stops its processes and gives the words for the turn with the supervisor's reason.
  let cut = watch
    .answer("job-41", "t1", CallVerdict::Cut("it waits on another job's close".into()), again_at)
    .await;
  let said = sentences(cut.lines);
  let line = said
    .iter()
    .find(|line| line.contains("cut on the supervisor's call"))
    .expect("the cut line");
  let words = cut.words.expect("the words for the turn");

  assert!(words.contains("it waits on another job's close") && words.contains("Bash"), "{words}");
  assert!(gone(pid_of(line)).await, "the cut call's process is gone: {line}");
}

// The processes of the program's own tree whose command line is `line`. The tree is the program's process group:
// another run of the suite on the machine starts processes of the same command lines, which are not this test's.
fn running(program: &Program, line: &str) -> Vec<i32> {
  let Ok(all) = procfs::process::all_processes() else { return vec![] };

  all
    .flatten()
    .filter(|process| process.stat().is_ok_and(|stat| stat.pgrp == program.pid))
    .filter(|process| process.cmdline().is_ok_and(|args| args.join(" ") == line))
    .map(|process| process.pid)
    .collect()
}

// Waits, a bounded while, for the program's process whose command line is `line`.
async fn started(program: &Program, line: &str) -> i32 {
  for _ in 0..100 {
    if let Some(pid) = running(program, line).first() {
      return *pid;
    }

    tokio::time::sleep(Duration::from_millis(20)).await;
  }

  panic!("no process came up as {line}");
}

// The clock looked at once at `now` on a moment of the test's own, every process read as it stands.
async fn looked_at(
  watch: &mut ToolWatch,
  records: &[Value],
  program: i32,
  now: i64,
  said: &mut Vec<String>,
) -> Option<agency_server::door::tool_clock::SilentCall> {
  said.extend(sentences(watch.look("job-41", records, 0, program, now, &mut NoQuestion).await));
  watch.take_silent()
}

#[tokio::test]
async fn a_call_polling_the_log_of_the_job_s_own_work_is_asked_about_only_once_that_work_has_ended() {
  let scratch = tempfile::tempdir().unwrap();
  let log = scratch.path().join("check.log").to_string_lossy().into_owned();
  // The job's own work, put in the background before the call: a process of the program's tree, not the
  // call's, holding the log open as a check's output; and the call polling that log, writing nothing.
  let poll = format!("while :; do sleep 0.05; done; cat {log}");
  // The call's record is written before its process starts, as a program writes it: a moment taken once the
  // processes stood would leave the call's own shell older than the call by however long a loaded machine took
  // to show it, past the slack a call's processes are read within.
  let began = agency_server::now_ms();
  let program = Program::spawn_args(&[
    "bash",
    "-c",
    "( exec -a held-log sleep 1000 ) > \"$0\" & bash -c \"$1\" & wait",
    &log,
    &poll,
  ]);
  let writer = started(&program, "held-log 1000").await;
  started(&program, &format!("bash -c {poll}")).await;
  let mut watch = ToolWatch::new(ToolClock {
    in_process_ms: 60_000,
    ..asking(1_000, 2_000)
  });
  let records = vec![used("t1", &poll, began)];
  let mut said = vec![];
  let mut now = began;

  // While the work lives, the call is never asked about, though it writes nothing for many windows.
  while now < began + 10_000 {
    let handed = looked_at(&mut watch, &records, program.pid, now, &mut said).await;

    assert!(handed.is_none(), "a wait on the job's own living work is not asked about: {said:?}");
    now += 250;
  }

  let first_line = said.first().cloned().unwrap_or_default();

  assert!(
    pid_of(&first_line) > 0 && pid_of(&first_line) != writer,
    "the call was watched with processes of its own, the work not among them: {said:?}"
  );

  // Once the work ends, the log stands still: the call is asked about a window after the work was last seen.
  let last_seen = now - 250;

  // SAFETY: a signal sent to the process this test started.
  unsafe {
    libc::kill(writer, libc::SIGKILL);
  }
  assert!(gone(writer).await, "the work ended");

  let mut handed = None;

  while now < last_seen + 6_000 {
    if let Some(silent) = looked_at(&mut watch, &records, program.pid, now, &mut said).await {
      handed = Some((now, silent));

      break;
    }

    now += 250;
  }

  let (handed_at, silent) = handed.unwrap_or_else(|| panic!("the stalled wait was asked about: {said:?}"));

  assert!(
    (last_seen + 2_000..=last_seen + 2_500).contains(&handed_at),
    "asked a window after the work was last seen moving, at {}ms",
    handed_at - last_seen
  );

  // The ask counts its silence from the work, as the clock did, and names the file that work wrote.
  assert_eq!(silent.quiet_since, last_seen);
  assert!(silent.output_changed_at < last_seen, "not from the call's own output");
  assert_eq!(
    silent.work,
    Some(WaitedWork {
      path: log.clone(),
      moved_at: last_seen
    })
  );

  let asked = sentences(watch.answer("job-41", "t1", CallVerdict::Waiting, handed_at).await.lines);
  let silent_for = (handed_at - last_seen) / 1_000;

  assert!(
    asked.iter().any(|line| line.ends_with(&format!("silent and idle for {silent_for}s"))),
    "the ask's line says the silence it was opened by: {asked:?}"
  );
}

// A stall span the test's own moments reach in two seconds.
fn stalling() -> Progress {
  Progress::new(StallClock {
    after_ms: 2_000,
    idle_cpu_percent: 30,
    recent: 16,
    lines: 5,
    chars: 300,
  })
}

// The stall changes the watch has handed over since this was last asked.
fn stall_changes(watch: &mut ToolWatch) -> Vec<StallChange> {
  std::iter::from_fn(|| watch.take_stall()).collect()
}

#[tokio::test]
async fn a_wait_on_the_job_s_own_work_that_stands_still_is_told_once_as_a_stall_and_not_while_that_work_writes()
 {
  let scratch = tempfile::tempdir().unwrap();
  let log = scratch.path().join("run-clip.txt").to_string_lossy().into_owned();
  let waiting = "every card it may use is taken; asking again every minute";

  std::fs::write(&log, format!("{waiting}\n")).unwrap();

  // The job's own work in the background, alive and holding its log open, writing nothing more; and the call
  // polling that log, which the call clock holds as a wait on moving work for as long as the writer lives.
  let poll = format!("until rg -q '^exit ' {log}; do sleep 30; done");
  // The call's record is written before its process starts, as a program writes it.
  let began = agency_server::now_ms();
  let program = Program::spawn_args(&[
    "bash",
    "-c",
    "( exec -a held-clip sleep 1000 ) >> \"$0\" & bash -c \"$1\" & wait",
    &log,
    &poll,
  ]);

  started(&program, "held-clip 1000").await;
  started(&program, &format!("bash -c {poll}")).await;

  let mut watch = ToolWatch::stalling(
    ToolClock {
      in_process_ms: 60_000,
      ..asking(1_000, i64::MAX / 4)
    },
    stalling(),
  );
  let records = vec![used("t1", &poll, began)];
  let mut said = vec![];
  let mut changes = vec![];
  let mut now = began;

  while now < began + 10_000 {
    assert!(
      looked_at(&mut watch, &records, program.pid, now, &mut said).await.is_none(),
      "the call clock asks nothing of a wait on living work: {said:?}"
    );
    changes.extend(stall_changes(&mut watch));
    now += 250;
  }

  assert_eq!(changes.len(), 1, "told once however long it stands: {changes:?}");

  let StallChange::Stalled(stall) = &changes[0] else { panic!("a stall: {changes:?}") };

  assert_eq!(stall.since, began, "counted from the record's last line");
  assert!(
    (began + 2_000..began + 2_250).contains(&stall.at),
    "told at the first read past the span, {}ms after",
    stall.at - began
  );
  assert_eq!(stall.waits.len(), 1, "{stall:?}");
  assert!(stall.waits[0].tool.contains(&log), "naming the running command: {stall:?}");
  assert_eq!(stall.waits[0].lines, [waiting], "and the last lines of the log it waits on");

  // The work writes new lines: the turn moves, said once, and is not told again while the work goes on.
  let mut changes = vec![];

  for step in 0..40u8 {
    let mut file = std::fs::OpenOptions::new().append(true).open(&log).unwrap();

    let clip = format!("{}{}", char::from(b'a' + step / 26), char::from(b'a' + step % 26));

    writeln!(file, "clip {clip} written").unwrap();
    looked_at(&mut watch, &records, program.pid, now, &mut said).await;
    changes.extend(stall_changes(&mut watch));
    now += 250;
  }

  assert_eq!(changes, [StallChange::Resumed], "{said:?}");
}

// Waits, a bounded while, for a child of `parent` whose command line is `line`.
async fn child_named(parent: i32, line: &str) -> i32 {
  for _ in 0..100 {
    let children =
      std::fs::read_to_string(format!("/proc/{parent}/task/{parent}/children")).unwrap_or_default();

    if let Some(pid) = children.split_whitespace().filter_map(|pid| pid.parse::<i32>().ok()).find(|pid| {
      std::fs::read(format!("/proc/{pid}/cmdline")).is_ok_and(|read| read == format!("{line}\0").as_bytes())
    }) {
      return pid;
    }

    tokio::time::sleep(Duration::from_millis(20)).await;
  }

  panic!("no child of {parent} came up as {line}");
}

#[tokio::test]
async fn a_call_waiting_on_the_job_s_question_is_neither_asked_about_nor_stopped_until_a_window_after_it() {
  // A wait that burns nothing and begins no process, as the job's ask does while its question waits.
  let program = Program::spawn(BLOCKED);
  let mut watch = ToolWatch::new(ToolClock {
    stall_ms: 1_000,
    in_process_ms: 60_000,
    ..asking(200, 400)
  });
  let began = agency_server::now_ms();
  let records = vec![used("t1", "cat", began)];
  let mut said = vec![];
  let mut handed = 0;
  let mut now = began;

  child_named(program.pid, "cat").await;

  // While the question waits, the policy answers so on every hand-over: no ask, and no stall cut, though
  // the stall passes several times over.
  while now < began + 4_000 {
    if looked_at(&mut watch, &records, program.pid, now, &mut said).await.is_some() {
      handed += 1;

      let answered = watch.answer("job-41", "t1", CallVerdict::Awaiting(now), now).await;

      assert!(answered.lines.is_empty() && answered.words.is_none(), "nothing is asked: {answered:?}");
    }

    now += 100;
  }

  assert!(handed >= 30, "it is looked at again on every read while it waits: {handed}");
  assert!(
    !said.iter().any(|line| line.contains("cut after")),
    "a wait on the question is not cut: {said:?}"
  );

  // Settled, the question holds the call a window from when it was last seen waiting; then the call is
  // asked about, and the stall from that moment stops it.
  let last_seen = now - 100;
  let mut asked_at = None;
  let mut cut_at = None;

  while now < last_seen + 3_000 && cut_at.is_none() {
    if looked_at(&mut watch, &records, program.pid, now, &mut said).await.is_some() {
      let verdict = if now - last_seen < 400 {
        CallVerdict::Awaiting(last_seen)
      } else {
        asked_at.get_or_insert(now);
        CallVerdict::Waiting
      };

      watch.answer("job-41", "t1", verdict, now).await;
    }

    if said.iter().any(|line| line.contains("cut after")) {
      cut_at = Some(now);
    }

    now += 100;
  }

  let asked_at =
    asked_at.unwrap_or_else(|| panic!("the call was asked about once the question was settled: {said:?}"));
  let cut_at = cut_at.unwrap_or_else(|| panic!("the stalled call was stopped: {said:?}"));

  assert!(
    (last_seen + 400..=last_seen + 500).contains(&asked_at),
    "asked a window after, at {}ms",
    asked_at - last_seen
  );
  assert!(
    (last_seen + 1_000..=last_seen + 1_100).contains(&cut_at),
    "stopped a stall after, at {}ms",
    cut_at - last_seen
  );
}

// The sentences a watcher's notes read as.
fn sentences(notes: Vec<ClockNote>) -> Vec<String> {
  notes.iter().map(ClockNote::sentence).collect()
}

fn append(path: &Path, lines: &[Value]) {
  std::fs::create_dir_all(path.parent().unwrap()).unwrap();

  let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();

  for line in lines {
    writeln!(file, "{line}").unwrap();
  }
}

// The lines a program writes: the words it was handed, a call it made, the answer to the call, and the end
// of its turn.
fn handed(words: &str) -> Value {
  json!({ "type": "user", "message": { "role": "user", "content": words } })
}

fn answer(words: &str) -> Value {
  json!({ "type": "assistant", "message": {
    "id": "m-1", "model": "claude-sonnet-5", "role": "assistant",
    "content": [{ "type": "text", "text": words }],
    "usage": { "input_tokens": 10, "output_tokens": 2 },
  }})
}

fn turn_end() -> Value {
  json!({ "type": "system", "subtype": "turn_duration" })
}

// The lines a test's watcher keeps, and the call clock and the turn's bound it hands the door.
#[derive(Clone)]
struct Notes(Arc<Mutex<Vec<String>>>, ToolClock, Option<Duration>);

impl Watch for Notes {
  async fn note(&mut self, text: &str) {
    self.0.lock().unwrap().push(text.to_owned());
  }

  fn tool_clock(&self) -> Option<ToolClock> {
    Some(self.1.clone())
  }

  fn turn_clock(&self) -> Option<Duration> {
    self.2
  }
}

// The clocks the door is handed for a turn of the job named, or of a role session's turn where none is, as
// their caller's watch hands them, the call clock's timings shortened so the test waits no real minutes.
fn watched_as(notes: &Arc<Mutex<Vec<String>>>, job: Option<JobMark>) -> Notes {
  let clocks = work_clocks(job);
  let call = ToolClock {
    min_ms: 400,
    stall_ms: 500,
    look_ms: 20,
    in_process_ms: 400,
    in_process_grace_ms: 2_000,
    log_after_ms: 100,
    still_every_ms: 100,
    stop_stall_ms: 100,
    stop_bound_ms: 5_000,
    silent: unasked(),
    ..clocks.call
  };

  Notes(notes.clone(), call, clocks.bound)
}

fn wrote(notes: &Arc<Mutex<Vec<String>>>, text: &str) -> bool {
  notes.lock().unwrap().iter().any(|line| line.contains(text))
}

// A job's turn whose program blocks on a pipe: the door cuts the call by its clock, the model is handed the
// failed result, and the turn ends with its answer rather than waiting for ever. The job's watch still hands
// the door its turn's hard bound beside the call clock.
#[tokio::test(flavor = "multi_thread")]
async fn the_door_cuts_a_job_turn_s_blocked_call_and_the_turn_ends() {
  let notes = Arc::new(Mutex::new(Vec::new()));
  let watch = watched_as(
    &notes,
    Some(JobMark {
      job: 41,
      item: Some(7),
    }),
  );

  assert!(watch.2.is_some(), "a job's turn keeps its hard bound");
  cut_and_answered("job-41", "worker", watch, &notes).await;
}

// A role's run whose program blocks on a pipe, as the call that held a role's slot for over an hour did: it is
// held to the same call clock with no job and no bound, the call is cut, and the run goes on to its answer.
#[tokio::test(flavor = "multi_thread")]
async fn the_door_cuts_a_role_run_s_blocked_call_and_the_run_goes_on() {
  let notes = Arc::new(Mutex::new(Vec::new()));
  let watch = watched_as(&notes, None);

  assert!(watch.2.is_none(), "a role's run has no hard bound");
  cut_and_answered("role-helper-1", "helper", watch, &notes).await;
}

// One turn put into a pane whose program makes a call that blocks on a pipe, watched with `watch`: the door
// cuts the call, the program then answers, and the turn ends answered with the clock's lines kept.
async fn cut_and_answered(name: &str, role: &str, watch: Notes, notes: &Arc<Mutex<Vec<String>>>) {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let config = home.path().join("config");
  let cwd = home.path().join("work");
  std::fs::create_dir_all(&cwd).unwrap();
  std::fs::create_dir_all(config.join("herdr")).unwrap();

  let herdr_model = HerdrModel::stand(config.join("herdr").join("herdr.sock"));
  let at = |path: &Path| path.to_string_lossy().into_owned();
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), at(home.path())),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
      ("XDG_CONFIG_HOME".to_owned(), at(&config)),
      ("BMO_DEPLOY_DIR".to_owned(), env!("CARGO_MANIFEST_DIR").to_owned()),
    ],
    home.path().join("data"),
  );
  let service = common::service(&scratch, &[("AGENCY_SOURCE_DIR", at(&home.path().join("source")))]);
  let ground = Ground::new(&service, &machine, Some(herdr::Herdr::there(herdr_model.socket.clone())));
  let session = &own_session("session-cut-1");
  let record = ClaudeCode::records_under(&config, &cwd, session).swap_remove(0);
  append(&record, &[]);

  let command = vec!["bash".to_owned(), "-c".to_owned(), "the work".to_owned()];
  // The blocked call begins only once the turn has made it, as a program's own call does: the script
  // waits for the trigger the test writes with the tool_use.
  let trigger = home.path().join("go");
  let blocked = format!(
    "while [ ! -e {} ]; do sleep 0.05; done; ( {} ) & while :; do sleep 0.05; done",
    trigger.display(),
    BLOCKED
  );
  let program = Program::spawn_args(&["bash", "-c", &blocked, "--session-id", session]);

  let turn = Turn {
    name: name.into(),
    title: "the work".into(),
    seat: Seat::Own(herdr::Herdr::there(herdr_model.socket.clone())),
    cwd: at(&cwd),
    command: command.clone(),
    env: BTreeMap::from([
      ("CLAUDE_CONFIG_DIR".to_owned(), at(&config)),
      ("HOME".to_owned(), at(home.path())),
    ]),
    prompt: "the work".into(),
    session: session.into(),
    identity: Identity {
      agent: None,
      root: None,
      parent: None,
      role: Some(role.into()),
      model: Some("claude-sonnet-5".into()),
      effort: None,
    },
    line: "worker".into(),
    provider: "claude-code".into(),
    lead: String::new(),
    trusts_folder: true,
    clock: None,
    interrupt: Interrupt::Keys(vec!["escape".into()]),
    timings: Timings {
      poll: Duration::from_millis(30),
      redraw: Duration::from_millis(500),
      submit: Duration::from_secs(5),
      gone: GoneConfirm::default(),
    },
  };

  // The program the pane plays: the door's pane made working, the prompt seen, a call made, and once the
  // call is cut the result, answer and end written.
  let herdr_for_writer = herdr_model.clone();
  let record_for_writer = record.clone();
  let command_for_writer = command.clone();
  let trigger_for_writer = trigger.clone();
  let writing = tokio::spawn(async move {
    let mut pane = None;

    for _ in 0..400 {
      pane = herdr_for_writer
        .panes()
        .into_iter()
        .find(|pane| pane.command.as_ref() == Some(&command_for_writer))
        .map(|pane| pane.id);

      if pane.is_some() {
        break;
      }

      tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let pane = pane.expect("the door's pane");

    // The pane comes up at rest, so the door may put its prompt in; then it works.
    herdr_for_writer.agent_status(&pane, "idle");

    for _ in 0..400 {
      if !herdr_for_writer.typed().is_empty() {
        break;
      }

      tokio::time::sleep(Duration::from_millis(10)).await;
    }

    herdr_for_writer.agent_status(&pane, "working");
    append(&record_for_writer, &[handed("the work")]);
    tokio::time::sleep(Duration::from_millis(300)).await;
    append(
      &record_for_writer,
      &[json!({ "type": "assistant", "message": { "content": [
        { "type": "tool_use", "id": "t1", "name": "Bash", "input": { "command": "cat" } }
      ] } })],
    );
    std::fs::write(&trigger_for_writer, "").unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    append(
      &record_for_writer,
      &[
        json!({ "type": "user", "message": { "content": [
          { "type": "tool_result", "tool_use_id": "t1", "is_error": true, "content": "killed" }
        ] } }),
        answer("the call was cut and the work went on"),
        turn_end(),
      ],
    );
    herdr_for_writer.agent_status(&pane, "idle");
  });

  let checking = record.clone();
  let ended = tokio::time::timeout(Duration::from_secs(20), async {
    let mut watch = watch;

    agency_server::door::turn::put(&ground, &turn, &mut watch).await
  })
  .await
  .unwrap_or_else(|_| {
    panic!(
      "the turn ends within its bound; notes: {:?}; writer finished: {}; record:\n{}",
      notes.lock().unwrap(),
      writing.is_finished(),
      std::fs::read_to_string(&checking).unwrap_or_default()
    )
  })
  .expect("the turn is put in and ended");

  writing.await.unwrap();
  drop(program);

  assert!(
    matches!(ended, Ending::Answered { .. }),
    "the turn answered after the call was cut: {ended:?}"
  );
  assert!(wrote(notes, "tool Bash started pid="), "the start line: {:?}", notes.lock().unwrap());
  assert!(wrote(notes, "still running"), "the still-running line: {:?}", notes.lock().unwrap());
  assert!(wrote(notes, "cut after"), "the cut line: {:?}", notes.lock().unwrap());
}

// A job's turn whose program waits on a call with no process of its own: the door cuts the call at its
// own bound, puts the words into the working turn, and the model is handed the call's result and answers.
#[tokio::test(flavor = "multi_thread")]
async fn the_door_cuts_a_job_turn_s_in_process_call_and_the_model_gets_a_result() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let config = home.path().join("config");
  let cwd = home.path().join("work");
  std::fs::create_dir_all(&cwd).unwrap();
  std::fs::create_dir_all(config.join("herdr")).unwrap();

  let herdr_model = HerdrModel::stand(config.join("herdr").join("herdr.sock"));
  let at = |path: &Path| path.to_string_lossy().into_owned();
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), at(home.path())),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
      ("XDG_CONFIG_HOME".to_owned(), at(&config)),
      ("BMO_DEPLOY_DIR".to_owned(), env!("CARGO_MANIFEST_DIR").to_owned()),
    ],
    home.path().join("data"),
  );
  let service = common::service(&scratch, &[("AGENCY_SOURCE_DIR", at(&home.path().join("source")))]);
  let ground = Ground::new(&service, &machine, Some(herdr::Herdr::there(herdr_model.socket.clone())));
  let session = &own_session("session-cut-2");
  let record = ClaudeCode::records_under(&config, &cwd, session).swap_remove(0);
  append(&record, &[]);

  let command = vec!["bash".to_owned(), "-c".to_owned(), "the work".to_owned()];
  // The program has no child of its own, so the call it makes owns no process.
  let program = Program::spawn_args(&["bash", "-c", "while :; do :; done", "--session-id", session]);

  let turn = Turn {
    name: "job-42".into(),
    title: "the work".into(),
    seat: Seat::Own(herdr::Herdr::there(herdr_model.socket.clone())),
    cwd: at(&cwd),
    command: command.clone(),
    env: BTreeMap::from([
      ("CLAUDE_CONFIG_DIR".to_owned(), at(&config)),
      ("HOME".to_owned(), at(home.path())),
    ]),
    prompt: "the work".into(),
    session: session.into(),
    identity: Identity {
      agent: None,
      root: None,
      parent: None,
      role: Some("worker".into()),
      model: Some("claude-sonnet-5".into()),
      effort: None,
    },
    line: "worker".into(),
    provider: "claude-code".into(),
    lead: String::new(),
    trusts_folder: true,
    clock: None,
    interrupt: Interrupt::Keys(vec!["escape".into()]),
    timings: Timings {
      poll: Duration::from_millis(30),
      redraw: Duration::from_millis(500),
      submit: Duration::from_secs(5),
      gone: GoneConfirm::default(),
    },
  };
  let notes = Arc::new(Mutex::new(Vec::new()));
  let watch = Notes(
    notes.clone(),
    ToolClock {
      min_ms: 400,
      stall_ms: 500,
      look_ms: 20,
      in_process_ms: 400,
      in_process_grace_ms: 5_000,
      in_process_cut: "The tool call {tool} was cut after {seconds} seconds; go on without it.".to_owned(),
      in_process_grace_cut: "cut at its bound after a tool call did not take its cut".to_owned(),
      log_after_ms: 100,
      still_every_ms: 100,
      stop_stall_ms: 100,
      stop_bound_ms: 5_000,
      silent: unasked(),
    },
    None,
  );

  let herdr_for_writer = herdr_model.clone();
  let record_for_writer = record.clone();
  let command_for_writer = command.clone();
  let writing = tokio::spawn(async move {
    let mut pane = None;

    for _ in 0..400 {
      pane = herdr_for_writer
        .panes()
        .into_iter()
        .find(|pane| pane.command.as_ref() == Some(&command_for_writer))
        .map(|pane| pane.id);

      if pane.is_some() {
        break;
      }

      tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let pane = pane.expect("the door's pane");

    herdr_for_writer.agent_status(&pane, "idle");

    for _ in 0..400 {
      if !herdr_for_writer.prompts().is_empty() {
        break;
      }

      tokio::time::sleep(Duration::from_millis(10)).await;
    }

    herdr_for_writer.agent_status(&pane, "working");
    append(&record_for_writer, &[handed("the work")]);
    tokio::time::sleep(Duration::from_millis(300)).await;
    append(
      &record_for_writer,
      &[json!({ "type": "assistant", "message": { "content": [
        { "type": "tool_use", "id": "t1", "name": "WebFetch", "input": { "url": "https://example.invalid/" } }
      ] } })],
    );

    // The call has no process to signal, so the door cuts it by putting words into the turn.
    let mut cut = false;

    for _ in 0..600 {
      if herdr_for_writer.prompts().iter().any(|(_, text)| text.contains("go on without it")) {
        cut = true;

        break;
      }

      tokio::time::sleep(Duration::from_millis(20)).await;
    }

    assert!(cut, "the door put the cut's words into the working turn");

    // The program ends the waiting call on them and hands the model its result.
    append(
      &record_for_writer,
      &[
        json!({ "type": "user", "message": { "content": [
          { "type": "tool_result", "tool_use_id": "t1", "is_error": true, "content": "cut" }
        ] } }),
        answer("the call was cut and the work went on"),
        turn_end(),
      ],
    );
    herdr_for_writer.agent_status(&pane, "idle");
  });

  let checking = record.clone();
  let ended = tokio::time::timeout(Duration::from_secs(20), async {
    let mut watch = watch;

    agency_server::door::turn::put(&ground, &turn, &mut watch).await
  })
  .await
  .unwrap_or_else(|_| {
    panic!(
      "the turn ends within its bound; notes: {:?}; writer finished: {}; record:\n{}",
      notes.lock().unwrap(),
      writing.is_finished(),
      std::fs::read_to_string(&checking).unwrap_or_default()
    )
  })
  .expect("the turn is put in and ended");

  writing.await.unwrap();
  drop(program);

  assert!(
    matches!(ended, Ending::Answered { .. }),
    "the model answered after the in-process call was cut: {ended:?}"
  );
  assert!(
    wrote(&notes, "tool WebFetch started pid=-"),
    "the start line: {:?}",
    notes.lock().unwrap()
  );
  assert!(wrote(&notes, "cut after"), "the cut line: {:?}", notes.lock().unwrap());
  assert!(
    wrote(&notes, "no process of its own"),
    "the cut names the missing process: {:?}",
    notes.lock().unwrap()
  );
}

// A program of the test's own whose background shells each write to the output file of the task named, under a
// `tasks` folder as the program writes its tasks' output, beside a child of its own that is no task's.
// A job's turn watched for its stall as a job's runner watches it, its stall span shortened so the test waits
// no real minutes: the changes the door hands over are kept.
#[derive(Clone)]
struct Stalls {
  notes: Arc<Mutex<Vec<String>>>,
  changes: Arc<Mutex<Vec<StallChange>>>,
  clock: ToolClock,
}

impl Watch for Stalls {
  async fn note(&mut self, text: &str) {
    self.notes.lock().unwrap().push(text.to_owned());
  }

  fn tool_clock(&self) -> Option<ToolClock> {
    Some(self.clock.clone())
  }

  fn stall_clock(&self) -> StallClock {
    StallClock {
      after_ms: 600,
      idle_cpu_percent: 30,
      recent: 16,
      lines: 5,
      chars: 300,
    }
  }

  async fn stalled(&mut self, change: &StallChange) {
    self.changes.lock().unwrap().push(change.clone());
  }
}

// One job's turn put in through the door: where it `stands_still`, its program makes a call blocked on a pipe
// and answers only once the door has told the stall; else it keeps making calls answered at once, each unlike
// the last, for several stall spans. Answers the changes the door handed over.
async fn stall_turn(stands_still: bool) -> Vec<StallChange> {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let config = home.path().join("config");
  let cwd = home.path().join("work");
  std::fs::create_dir_all(&cwd).unwrap();
  std::fs::create_dir_all(config.join("herdr")).unwrap();

  let herdr_model = HerdrModel::stand(config.join("herdr").join("herdr.sock"));
  let at = |path: &Path| path.to_string_lossy().into_owned();
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), at(home.path())),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
      ("XDG_CONFIG_HOME".to_owned(), at(&config)),
      ("BMO_DEPLOY_DIR".to_owned(), env!("CARGO_MANIFEST_DIR").to_owned()),
    ],
    home.path().join("data"),
  );
  let service = common::service(&scratch, &[("AGENCY_SOURCE_DIR", at(&home.path().join("source")))]);
  let ground = Ground::new(&service, &machine, Some(herdr::Herdr::there(herdr_model.socket.clone())));
  let session = &own_session("session-stall");
  let record = ClaudeCode::records_under(&config, &cwd, session).swap_remove(0);
  append(&record, &[]);

  let command = vec!["bash".to_owned(), "-c".to_owned(), "the work".to_owned()];
  let trigger = home.path().join("go");
  let script = format!(
    "while [ ! -e {} ]; do sleep 0.05; done; ( {} ) & while :; do sleep 0.05; done",
    trigger.display(),
    BLOCKED
  );
  let program = Program::spawn_args(&["bash", "-c", &script, "--session-id", session]);
  let watch = Stalls {
    notes: Arc::new(Mutex::new(Vec::new())),
    changes: Arc::new(Mutex::new(Vec::new())),
    // No call is cut and none asked about, so the stall alone is what the turn can show.
    clock: ToolClock {
      min_ms: 200,
      stall_ms: 600_000,
      look_ms: 20,
      in_process_ms: 600_000,
      ..clock(200, 600_000, 100)
    },
  };
  let changes = watch.changes.clone();

  let turn = Turn {
    name: "job-41".into(),
    title: "the work".into(),
    seat: Seat::Own(herdr::Herdr::there(herdr_model.socket.clone())),
    cwd: at(&cwd),
    command: command.clone(),
    env: BTreeMap::from([
      ("CLAUDE_CONFIG_DIR".to_owned(), at(&config)),
      ("HOME".to_owned(), at(home.path())),
    ]),
    prompt: "the work".into(),
    session: session.into(),
    identity: Identity {
      agent: None,
      root: None,
      parent: None,
      role: Some("worker".into()),
      model: Some("claude-sonnet-5".into()),
      effort: None,
    },
    line: "worker".into(),
    provider: "claude-code".into(),
    lead: String::new(),
    trusts_folder: true,
    clock: None,
    interrupt: Interrupt::Keys(vec!["escape".into()]),
    timings: Timings {
      poll: Duration::from_millis(30),
      redraw: Duration::from_millis(500),
      submit: Duration::from_secs(5),
      gone: GoneConfirm::default(),
    },
  };

  let herdr_for_writer = herdr_model.clone();
  let record_for_writer = record.clone();
  let changes_for_writer = changes.clone();
  let writing = tokio::spawn(async move {
    let pane = 'found: {
      for _ in 0..400 {
        if let Some(pane) = herdr_for_writer
          .panes()
          .into_iter()
          .find(|pane| pane.command.as_ref() == Some(&command))
        {
          break 'found pane.id;
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
      }

      panic!("the door's pane");
    };

    herdr_for_writer.agent_status(&pane, "idle");

    for _ in 0..400 {
      if !herdr_for_writer.typed().is_empty() {
        break;
      }

      tokio::time::sleep(Duration::from_millis(10)).await;
    }

    herdr_for_writer.agent_status(&pane, "working");
    append(&record_for_writer, &[handed("the work")]);

    let call = |id: &str, command: &str| {
      json!({ "type": "assistant", "timestamp": stamp(agency_server::now_ms()), "message": { "content": [
        { "type": "tool_use", "id": id, "name": "Bash", "input": { "command": command } }
      ] } })
    };
    let result = |id: &str, text: &str| {
      json!({ "type": "user", "timestamp": stamp(agency_server::now_ms()), "message": { "content": [
        { "type": "tool_result", "tool_use_id": id, "content": text }
      ] } })
    };

    if stands_still {
      append(&record_for_writer, &[call("t1", "cat")]);
      std::fs::write(&trigger, "").unwrap();

      // The call stands until the door has told its stall, and a while after, so a second telling would come.
      for _ in 0..300 {
        if !changes_for_writer.lock().unwrap().is_empty() {
          break;
        }

        tokio::time::sleep(Duration::from_millis(20)).await;
      }

      tokio::time::sleep(Duration::from_millis(1_500)).await;
      append(&record_for_writer, &[result("t1", "killed")]);
    } else {
      let files = [
        "store", "server", "policy", "herdr", "tools", "gateway", "siren", "agent", "host", "link",
      ];

      for (step, file) in files.iter().cycle().take(20).enumerate() {
        let id = format!("w{step}");

        append(
          &record_for_writer,
          &[call(&id, &format!("cat {file}.rs")), result(&id, &format!("the {file} source"))],
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
      }
    }

    tokio::time::sleep(Duration::from_millis(200)).await;
    append(&record_for_writer, &[answer("the work is done"), turn_end()]);
    herdr_for_writer.agent_status(&pane, "idle");
  });

  let notes = watch.notes.clone();
  let ended = tokio::time::timeout(Duration::from_secs(30), async {
    let mut watch = watch;

    agency_server::door::turn::put(&ground, &turn, &mut watch).await
  })
  .await
  .unwrap_or_else(|_| panic!("the turn ends within its bound; notes: {:?}", notes.lock().unwrap()))
  .expect("the turn is put in and ended");

  writing.await.unwrap();
  drop(program);

  assert!(matches!(ended, Ending::Answered { .. }), "the turn answered: {ended:?}");

  changes.lock().unwrap().clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_door_tells_a_job_turn_standing_still_once_and_its_move_after_it() {
  let changes = stall_turn(true).await;

  assert_eq!(changes.len(), 2, "one stall and one move, however long it stood: {changes:?}");

  let StallChange::Stalled(stall) = &changes[0] else {
    panic!("the stall first: {changes:?}")
  };

  assert_eq!(stall.waits.len(), 1, "{stall:?}");
  assert_eq!(stall.waits[0].tool, "Bash cat", "naming the call it stands on");
  assert!(stall.at - stall.since >= 600, "not before its span: {stall:?}");
  assert_eq!(changes[1], StallChange::Resumed, "the call's result moves it");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_door_tells_nothing_of_a_job_turn_that_keeps_working() {
  let changes = stall_turn(false).await;

  assert!(
    changes.is_empty(),
    "a turn making new calls for five stall spans is not told: {changes:?}"
  );
}

fn with_tasks(dir: &Path, tasks: &[(&str, &str)]) -> Program {
  std::fs::create_dir_all(dir.join("tasks")).unwrap();

  let shells: String = tasks
    .iter()
    .map(|(task, script)| format!("( {script} ) >{}/tasks/{task}.output 2>&1 & ", dir.display()))
    .collect();

  Program::spawn(&format!("{shells} sleep 1000 & wait"))
}

fn background(task: &str, at: i64) -> BackgroundCall {
  BackgroundCall {
    id: task.to_owned(),
    command: format!("the work of {task}"),
    owns: Owns::Output(task.to_owned()),
    started_at: at,
  }
}

// The clock looked at the calls until it cuts one, or its bound passes: what it cut.
async fn cut_within(clock: &mut BackgroundClock, program: i32, calls: &[BackgroundCall]) -> Vec<String> {
  for _ in 0..100 {
    let cuts = clock.look(program, calls, agency_server::now_ms()).await;

    if !cuts.is_empty() {
      return cuts.into_iter().map(|cut| format!("{} {}", cut.id, cut.stalled)).collect();
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
  }

  vec![]
}

/// a background shell of the program that only waits and polls, writing nothing, is cut once it has stood idle
/// across the silent window, and one burning processor beside it is left alone
#[tokio::test]
async fn a_background_wait_that_only_polls_is_cut_and_a_working_one_is_not() {
  let dir = tempfile::tempdir().unwrap();
  let program = with_tasks(
    dir.path(),
    &[("bwait", "while :; do sleep 0.2; done"), ("bwork", "while :; do :; done")],
  );
  let now = agency_server::now_ms();
  let mut clock = BackgroundClock::new(ToolClock {
    min_ms: 0,
    stall_ms: i64::MAX / 4,
    silent: SilentClock {
      window_ms: 400,
      ..unasked()
    },
    ..clock(0, 0, 0)
  });
  let calls = [background("bwait", now), background("bwork", now)];

  assert_eq!(cut_within(&mut clock, program.pid, &calls).await, ["bwait false"]);
  assert!(
    task_processes(program.pid, &["bwait".to_owned()]).is_empty(),
    "the wait's processes are gone"
  );
  assert!(
    !task_processes(program.pid, &["bwork".to_owned()]).is_empty(),
    "the working shell runs on"
  );
}

/// a background shell whose processes burn no processor time is cut once it has stood past the stall
#[tokio::test]
async fn a_background_shell_that_burns_nothing_is_cut_at_its_stall() {
  let dir = tempfile::tempdir().unwrap();
  let program = with_tasks(dir.path(), &[("bstall", "sleep 1000")]);
  let mut clock = BackgroundClock::new(clock(0, 150, 0));

  assert_eq!(
    cut_within(&mut clock, program.pid, &[background("bstall", agency_server::now_ms())]).await,
    ["bstall true"]
  );
}

/// a background shell that burns nothing but whose output grows, as a check that says a placed step still runs,
/// is not cut at its stall, and is cut once its output stops growing for the stall
#[tokio::test]
async fn a_background_shell_whose_output_grows_is_not_cut_at_its_stall() {
  let dir = tempfile::tempdir().unwrap();
  let output = dir.path().join("tasks/bsays.output");
  let program = with_tasks(dir.path(), &[("bsays", "sleep 1000")]);
  let stall = 1_000;
  let mut clock = BackgroundClock::new(clock(0, stall, 0));
  let began = agency_server::now_ms();
  let calls = [background("bsays", began)];

  for _ in 0..200 {
    if !task_processes(program.pid, &["bsays".to_owned()]).is_empty() {
      break;
    }

    tokio::time::sleep(Duration::from_millis(10)).await;
  }

  assert!(
    clock.look(program.pid, &calls, began).await.is_empty(),
    "the first look takes its measure"
  );

  for pass in 1..=3 {
    let mut file = std::fs::OpenOptions::new().append(true).open(&output).unwrap();
    writeln!(file, "check: a placed step still running").unwrap();

    let cuts = clock.look(program.pid, &calls, began + pass * (stall + 1)).await;
    assert!(cuts.is_empty(), "a shell whose output grew is left to run: {cuts:?}");
  }

  let cuts = clock.look(program.pid, &calls, began + 4 * (stall + 1)).await;
  assert_eq!(
    cuts.iter().map(|cut| (cut.id.as_str(), cut.stalled)).collect::<Vec<_>>(),
    [("bsays", true)],
    "a shell whose output stopped is cut at its stall"
  );
}

/// a stop's ending reaches the processes of the tasks it names and every process they began, within its grace,
/// and leaves the program's other children standing
#[tokio::test]
async fn a_stop_ends_the_named_background_shells_and_nothing_else() {
  let dir = tempfile::tempdir().unwrap();
  let program = with_tasks(dir.path(), &[("bend", "sleep 1000 & sleep 1000; wait")]);
  let mut named = vec![];

  for _ in 0..100 {
    named = task_processes(program.pid, &["bend".to_owned()]);

    if named.len() >= 3 {
      break;
    }

    tokio::time::sleep(Duration::from_millis(20)).await;
  }

  assert!(named.len() >= 3, "the shell and the two sleeps it began: {named:?}");

  let left = end_processes(&named, Duration::from_millis(500)).await;

  assert!(left.is_empty(), "{left:?}");
  assert!(task_processes(program.pid, &["bend".to_owned()]).is_empty());
  let children =
    std::fs::read_to_string(format!("/proc/{0}/task/{0}/children", program.pid)).unwrap_or_default();

  assert!(
    children
      .split_whitespace()
      .any(|child| std::fs::read(format!("/proc/{child}/cmdline"))
        .is_ok_and(|line| line.starts_with(b"sleep"))),
    "the program's own child stands: {children}"
  );
}
