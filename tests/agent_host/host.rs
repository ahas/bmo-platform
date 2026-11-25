//! The agent host as a launcher and a collector meet it: the binary started on a scratch data root,
//! envelopes written and pointed at as the launcher does, and their outcomes collected
//! (docs/agent-control.md, 「The host」). A policy names a scratch store of the test's own.

use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use agent_host::identity::ProcessIdentity;
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::support::scratch::{Scratch, address_of};

const SH: &str = "/bin/sh";

fn binary() -> PathBuf {
  crate::support::binary::path("agent-host")
}

fn now_ms() -> u64 {
  SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

// Whether `holds` comes true within `ms`.
fn within(ms: u64, mut holds: impl FnMut() -> bool) -> bool {
  let until = Instant::now() + Duration::from_millis(ms);

  while Instant::now() < until {
    if holds() {
      return true;
    }

    thread::sleep(Duration::from_millis(10));
  }

  holds()
}

fn alive(pid: i32) -> bool {
  ProcessIdentity::of(pid).is_some_and(|one| one.alive())
}

struct Running(Child);

impl Running {
  fn pid(&self) -> i32 {
    self.0.id() as i32
  }

  fn kill(&mut self, signal: libc::c_int) {
    // SAFETY: a signal to the host this test started.
    unsafe {
      libc::kill(self.pid(), signal);
    }
  }

  fn ended(&mut self, ms: u64) -> Option<ExitStatus> {
    let mut status = None;

    within(ms, || {
      status = self.0.try_wait().ok().flatten();
      status.is_some()
    });

    status
  }
}

impl Drop for Running {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

/// What a collector reads off a finished envelope.
#[derive(Debug)]
struct Collected {
  exit_code: i64,
  cause: String,
  stdout: Vec<u8>,
  stderr: String,
}

struct Root {
  data: TempDir,
  // Markers the children write, outside the data root.
  marks: TempDir,
}

impl Root {
  fn new() -> Self {
    let root = Self {
      data: tempfile::tempdir().unwrap(),
      marks: tempfile::tempdir().unwrap(),
    };

    fs::DirBuilder::new().recursive(true).mode(0o700).create(root.executions()).unwrap();
    root
  }

  fn executions(&self) -> PathBuf {
    self.data.path().join("executions")
  }

  fn home(&self) -> PathBuf {
    self.executions().join("agent-host")
  }

  fn mark(&self, name: &str) -> PathBuf {
    self.marks.path().join(name)
  }

  fn start(&self, more: &[&str]) -> Running {
    let child = Command::new(binary())
      .args(["serve", "--poll-ms", "10", "--code-root"])
      .arg(self.data.path())
      .arg("--data-root")
      .arg(self.data.path())
      .args(more)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .unwrap();
    let running = Running(child);
    let pid = running.pid();

    // The socket is bound after the claim and is the last thing a starting host writes, so a host
    // that answers on it writes nothing more under the root unasked. A socket file left by a host
    // killed before refuses a connection, so the answer is this host's.
    assert!(
      within(3_000, || {
        self.claim().is_some_and(|claim| claim["pid"] == pid) && UnixStream::connect(self.socket()).is_ok()
      }),
      "the host never claimed its root and answered on its socket"
    );
    running
  }

  fn socket(&self) -> PathBuf {
    self.executions().join("agent-host.sock")
  }

  fn claim(&self) -> Option<Value> {
    serde_json::from_slice(&fs::read(self.home().join("claim")).ok()?).ok()
  }

  /// An envelope as the launcher writes it, owned by this test unless `more` says otherwise.
  fn envelope(&self, name: &str, script: &str, more: Value) -> PathBuf {
    let dir = self.executions().join(name);
    let mut envelope = json!({
      "command": { "args": [SH, "-c", script], "cwd": self.executions(), "env": { "PATH": std::env::var("PATH").unwrap() }, "stdin": null },
      "title": "test",
      "owner": ProcessIdentity::own(),
    });

    for (key, value) in more.as_object().unwrap() {
      if value.is_null() {
        envelope.as_object_mut().unwrap().remove(key);
      } else {
        envelope[key] = value.clone();
      }
    }

    fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
    fs::write(dir.join("command.json"), envelope.to_string()).unwrap();
    dir
  }

  fn grant(&self, dir: &Path) {
    fs::write(dir.join("grant"), r#"{"nonce":"n"}"#).unwrap();
  }

  /// A pointer left in the inbox as a launcher leaves it.
  fn point(&self, agent: &str, target: &str) {
    let inbox = self.home().join("inbox");
    let name = format!("p-{}", now_ms() as u128 * 1000 + u128::from(rand_suffix()));

    fs::create_dir_all(&inbox).unwrap();
    fs::write(
      inbox.join(format!("{name}.new")),
      json!({ "agent": agent, "envelope": target }).to_string(),
    )
    .unwrap();
    fs::rename(inbox.join(format!("{name}.new")), inbox.join(name)).unwrap();
  }

  fn place(&self, agent: &str, dir: &Path) {
    self.point(agent, dir.to_str().unwrap());
  }

  /// The outcome waited for, read with the output, and the envelope removed as the collector does.
  fn collect(&self, dir: &Path, ms: u64) -> Collected {
    assert!(within(ms, || dir.join("outcome.json").exists()), "no outcome in {}", dir.display());

    let outcome: Value = serde_json::from_slice(&fs::read(dir.join("outcome.json")).unwrap()).unwrap();
    let collected = Collected {
      exit_code: outcome["exitCode"].as_i64().unwrap(),
      cause: outcome["cause"].as_str().unwrap().to_owned(),
      stdout: fs::read(dir.join("stdout")).unwrap(),
      stderr: fs::read_to_string(dir.join("stderr")).unwrap(),
    };

    fs::remove_dir_all(dir).unwrap();
    collected
  }

  fn started(&self, dir: &Path, ms: u64) -> Value {
    assert!(within(ms, || dir.join("started.json").exists()), "never started");
    serde_json::from_slice(&fs::read(dir.join("started.json")).unwrap()).unwrap()
  }

  fn view(&self, agent: &str) -> Vec<String> {
    fs::read_to_string(self.home().join("views").join(agent))
      .unwrap_or_default()
      .lines()
      .map(str::to_owned)
      .collect()
  }

  /// The records of what the host took, leaving out one staged beside its name.
  fn taken(&self) -> usize {
    fs::read_dir(self.home().join("taken")).map_or(0, |entries| {
      entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|extension| extension == "json"))
        .count()
    })
  }
}

/// A store of the test's own holding these settings, which a policy names by its address. The store's
/// connection lives on the runtime it was opened on, which goes after it.
struct Store {
  scratch: Scratch,
  runtime: tokio::runtime::Runtime,
}

impl Store {
  fn new(settings: &[(&str, &str)]) -> Self {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let scratch = runtime.block_on(Scratch::new());
    let store = Self { scratch, runtime };

    for (key, value) in settings {
      store.set(key, value);
    }

    store
  }

  /// A setting written, or written over.
  fn set(&self, key: &str, value: &str) {
    self.runtime.block_on(self.scratch.run(
      "INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
      vec![key.into(), value.into()],
    ));
  }

  fn address(&self) -> String {
    self.scratch.address()
  }
}

fn rand_suffix() -> u32 {
  use std::sync::atomic::{AtomicU32, Ordering};
  static NEXT: AtomicU32 = AtomicU32::new(0);

  NEXT.fetch_add(1, Ordering::SeqCst)
}

#[test]
fn an_execution_runs_in_the_host_and_its_raw_streams_come_back_exact() {
  let root = Root::new();
  let _host = root.start(&[]);
  let dir = root.envelope("exact", "cat; printf diagnostic >&2", json!({ "command": {
    "args": [SH, "-c", "cat; printf diagnostic >&2"], "cwd": root.executions(), "env": {}, "stdin": "secret-value\nnaïve café " } }));

  root.place("chat", &dir);
  root.grant(&dir);

  let started = root.started(&dir, 3_000);
  let answer = root.collect(&dir, 5_000);

  assert_eq!((answer.exit_code, answer.cause.as_str()), (0, "exited"));
  assert_eq!(answer.stdout, "secret-value\nnaïve café ".as_bytes());
  assert_eq!(answer.stderr, "diagnostic");
  assert!(started["pid"].as_i64().unwrap() > 1);
  assert!(started["supervisor"]["pid"].as_i64().unwrap() > 1);

  // The pane shows it begin, and the agent at rest once its collector has it.
  assert!(within(2_000, || root.view("chat").len() == 2), "{:?}", root.view("chat"));

  let view = root.view("chat");

  assert!(view[0].starts_with("── ") && view[0].ends_with(" test (exact) started"), "{view:?}");
  assert!(view[1].starts_with("resting · since "), "{view:?}");
  assert_eq!(root.taken(), 0);
}

#[test]
fn large_simultaneous_streams_come_back_whole() {
  let root = Root::new();
  let _host = root.start(&[]);
  let dir = root.envelope(
    "large",
    "(head -c 200000 /dev/zero | tr '\\0' o) & (head -c 200000 /dev/zero | tr '\\0' e >&2) & wait",
    json!({}),
  );

  root.place("chat", &dir);
  root.grant(&dir);

  let answer = root.collect(&dir, 10_000);

  assert_eq!(answer.stdout, vec![b'o'; 200_000]);
  assert_eq!(answer.stderr, "e".repeat(200_000));
}

#[test]
fn many_executions_run_side_by_side_in_one_host() {
  let root = Root::new();
  let host = root.start(&[]);
  let dirs: Vec<PathBuf> = (1..=4)
    .map(|n| root.envelope(&format!("side-{n}"), &format!("sleep 0.4; printf {n}"), json!({})))
    .collect();
  let began = Instant::now();

  for dir in &dirs {
    root.place("gate", dir);
    root.grant(dir);
  }

  let answers: Vec<Vec<u8>> = dirs.iter().map(|dir| root.collect(dir, 5_000).stdout).collect();

  assert_eq!(answers, [b"1", b"2", b"3", b"4"]);
  assert!(
    began.elapsed() < Duration::from_millis(1_400),
    "ran one after another: {:?}",
    began.elapsed()
  );
  assert_eq!(root.claim().unwrap()["pid"], host.pid());
}

#[test]
fn a_second_host_ends_at_once_and_a_name_already_running_is_not_started_again() {
  let root = Root::new();
  let host = root.start(&[]);
  // Its error stream is a file, so a second host that went on serving would fail the test, not hold it.
  let said = root.mark("second.err");
  let mut second = Running(
    Command::new(binary())
      .args(["serve", "--data-root"])
      .arg(root.data.path())
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(fs::File::create(&said).unwrap())
      .spawn()
      .unwrap(),
  );

  assert!(second.ended(5_000).is_some_and(|status| status.success()));
  assert!(fs::read_to_string(&said).unwrap().contains("is served by another host"));
  assert_eq!(root.claim().unwrap()["pid"], host.pid());

  let counted = root.mark("counted");
  let dir = root.envelope("twice", &format!("echo x >> {}; sleep 0.3", counted.display()), json!({}));

  root.place("gate", &dir);
  root.place("gate", &dir);
  root.grant(&dir);
  root.collect(&dir, 5_000);

  assert_eq!(fs::read_to_string(&counted).unwrap(), "x\n");
}

// A stop asks once and waits: the leader is given however long its own ending takes, and what it leaves of
// its group once it has ended is killed.
#[test]
fn a_stop_waits_for_the_leader_and_kills_what_it_leaves() {
  let root = Root::new();
  let _host = root.start(&[]);
  let dir = root.envelope(
    "slow-to-stop",
    "sh -c 'trap \"\" TERM; while :; do sleep 0.1; done' & echo $!; trap 'sleep 1; exit 3' TERM; while :; do sleep 0.1; done",
    json!({}),
  );

  root.place("gate", &dir);
  root.grant(&dir);
  root.started(&dir, 3_000);
  assert!(within(3_000, || fs::read(dir.join("stdout")).is_ok_and(|out| out.ends_with(b"\n"))));

  let asked = Instant::now();

  fs::write(dir.join("cancel"), "").unwrap();

  let answer = root.collect(&dir, 10_000);
  let descendant: i32 = String::from_utf8(answer.stdout).unwrap().trim().parse().unwrap();

  assert_eq!((answer.cause.as_str(), answer.exit_code), ("cancelled", 3));
  assert!(asked.elapsed() >= Duration::from_millis(900), "the leader's own ending was cut short");
  assert!(within(1_000, || !alive(descendant)), "the descendant outlived its execution");
}

#[test]
fn a_collector_gone_before_the_grant_or_a_cancel_starts_no_child_and_a_cancel_stops_one_running() {
  let root = Root::new();
  let _host = root.start(&[]);
  let marker = root.mark("started");
  let script = format!("touch {}", marker.display());
  let mut collector = Command::new("sleep").arg("1000").spawn().unwrap();
  let expired = root.envelope(
    "collector-gone",
    &script,
    json!({ "owner": ProcessIdentity::of(collector.id() as i32).unwrap() }),
  );

  collector.kill().unwrap();
  collector.wait().unwrap();
  let cancelled = root.envelope("cancelled-early", &script, json!({}));
  let pending = root.envelope("cancelled-pending", &script, json!({}));

  root.place("gate", &expired);
  fs::write(cancelled.join("cancel"), "").unwrap();
  root.grant(&cancelled);
  root.place("gate", &cancelled);
  fs::write(pending.join("cancel"), "").unwrap();
  root.place("gate", &pending);

  let expired = root.collect(&expired, 5_000);
  let cancelled = root.collect(&cancelled, 5_000);
  let pending = root.collect(&pending, 5_000);

  assert_eq!(
    (expired.cause.as_str(), expired.stderr.as_str()),
    ("launch-error", "launch cancelled or its collector gone before the grant")
  );
  assert_eq!(
    (cancelled.cause.as_str(), cancelled.stderr.as_str()),
    ("cancelled", "cancelled before launch")
  );
  assert_eq!(pending.cause, "launch-error");
  assert!(!marker.exists());

  let running = root.envelope("cancelled-running", "while :; do sleep 0.1; done", json!({}));

  root.place("gate", &running);
  root.grant(&running);
  root.started(&running, 3_000);
  fs::write(running.join("cancel"), "").unwrap();
  assert_eq!(root.collect(&running, 5_000).cause, "cancelled");
}

#[test]
fn a_switched_off_provider_a_cancelled_name_or_a_missing_program_starts_nothing() {
  let root = Root::new();
  let _host = root.start(&[]);
  let marker = root.mark("forbidden");
  let script = format!("touch {}", marker.display());
  let off = Store::new(&[("provider_switch:codex", "off")]);
  let stopped = Store::new(&[("execution_cancel:execution-stopped", "1")]);
  let nowhere = address_of(&format!("agency_rs_missing_{}", std::process::id()));
  let cases = [
    root.envelope("off", &script, json!({ "policy": { "address": off.address(), "provider": "codex" } })),
    root.envelope("no-store", &script, json!({ "policy": { "address": nowhere, "provider": "codex" } })),
    root.envelope("stopped", &script, json!({ "policy": { "address": stopped.address(), "provider": "codex", "names": ["execution-stopped", "task-1"] } })),
    root.envelope("missing", "", json!({ "command": { "args": [root.mark("missing-program")], "cwd": root.executions(), "env": {}, "stdin": null } })),
  ];

  for dir in &cases {
    root.place("gate", dir);
    root.grant(dir);
  }

  let [off, no_store, stopped, missing] = cases.map(|dir| root.collect(&dir, 5_000));

  assert_eq!(
    (off.cause.as_str(), off.stderr.as_str()),
    ("launch-error", "provider disabled at child launch")
  );
  assert_eq!(no_store.cause, "launch-error");
  assert_eq!(stopped.cause, "cancelled");
  assert!(stopped.stderr.contains("execution cancelled at child launch"));
  assert_eq!((missing.cause.as_str(), missing.exit_code), ("launch-error", 127));
  assert!(!marker.exists());
}

#[test]
fn the_providers_switch_is_read_again_at_the_childs_start() {
  let root = Root::new();
  let _host = root.start(&[]);
  let store = Store::new(&[]);
  let marker = root.mark("started");
  let switched = root.envelope(
    "switched",
    &format!("touch {}", marker.display()),
    json!({ "policy": { "address": store.address(), "provider": "codex" } }),
  );

  root.place("gate", &switched);
  // Switched off after the launcher read it on and before the grant.
  thread::sleep(Duration::from_millis(100));
  store.set("provider_switch:codex", "off");
  root.grant(&switched);
  assert_eq!(root.collect(&switched, 5_000).cause, "launch-error");
  assert!(!marker.exists());

  // A provider no switch names, and one switched back on, run.
  let on = Store::new(&[("provider_switch:claude-code", "on")]);
  let unnamed = root.envelope(
    "unnamed",
    "printf ran",
    json!({ "policy": { "address": on.address(), "provider": "gained" } }),
  );
  let kept = root.envelope(
    "kept",
    "printf ran",
    json!({ "policy": { "address": on.address(), "provider": "claude-code" } }),
  );

  for dir in [&unnamed, &kept] {
    root.place("gate", dir);
    root.grant(dir);
  }

  assert_eq!(root.collect(&unnamed, 5_000).stdout, b"ran");
  assert_eq!(root.collect(&kept, 5_000).stdout, b"ran");
}

#[test]
fn a_collector_that_dies_cancels_its_execution() {
  let root = Root::new();
  let _host = root.start(&[]);
  let mut owner = Command::new("sleep").arg("1000").spawn().unwrap();
  let dir = root.envelope(
    "orphaned",
    "while :; do sleep 0.1; done",
    json!({ "owner": ProcessIdentity::of(owner.id() as i32).unwrap() }),
  );

  root.place("gate", &dir);
  root.grant(&dir);

  let child = root.started(&dir, 3_000)["pid"].as_i64().unwrap() as i32;

  owner.kill().unwrap();
  owner.wait().unwrap();

  assert_eq!(root.collect(&dir, 5_000).cause, "cancelled");
  assert!(within(1_000, || !alive(child)));
}

#[test]
fn a_detached_run_is_started_left_and_shows_its_end() {
  let root = Root::new();
  let _host = root.start(&[]);
  let marker = root.mark("ran");
  let dir = root.envelope(
    "detached",
    &format!("sleep 0.1; printf yes > {}", marker.display()),
    json!({ "owner": null, "title": "detached" }),
  );

  root.grant(&dir);
  root.place("search", &dir);

  assert!(within(5_000, || dir.join("outcome.json").exists()));
  assert_eq!(
    fs::read_to_string(dir.join("outcome.json")).unwrap(),
    r#"{"exitCode":0,"cause":"exited"}"#
  );
  assert_eq!(fs::read_to_string(&marker).unwrap(), "yes");

  // Nobody collects it: the host writes its end, and lets it go.
  assert!(within(2_000, || root.view("search").len() == 3), "{:?}", root.view("search"));

  let view = root.view("search");

  assert!(view[0].ends_with(" detached (detached) started"));
  assert!(view[1].ends_with(" detached ended"));
  assert!(view[2].starts_with("resting"));
  assert_eq!(root.taken(), 0);

  let missing = root.envelope(
    "detached-missing",
    "",
    json!({ "owner": null, "command": { "args": [root.mark("missing-program")], "cwd": root.executions(), "env": {}, "stdin": null } }),
  );

  root.grant(&missing);
  root.place("search", &missing);
  assert!(within(5_000, || missing.join("outcome.json").exists()));
  assert!(!missing.join("started.json").exists());
  assert!(fs::read_to_string(missing.join("outcome.json")).unwrap().contains("launch-error"));
}

// The supervisor killed outright, and its guardian ending the child's group.
#[test]
fn a_supervisor_killed_outright_takes_its_childs_group_with_it() {
  let root = Root::new();
  let _host = root.start(&[]);
  let dir = root.envelope(
    "guarded",
    "sh -c 'while :; do sleep 0.1; done' & while :; do sleep 0.1; done",
    json!({}),
  );

  root.place("gate", &dir);
  root.grant(&dir);

  let started = root.started(&dir, 3_000);
  let child = started["pid"].as_i64().unwrap() as i32;
  let supervisor = started["supervisor"]["pid"].as_i64().unwrap() as i32;

  // SAFETY: a signal to the supervisor this test's host started.
  unsafe {
    libc::kill(supervisor, libc::SIGKILL);
  }

  assert!(within(2_000, || !alive(child)), "the child outlived its supervisor");
  assert!(!dir.join("outcome.json").exists());
  // The host lets go of what nobody is left to finish.
  assert!(within(2_000, || root.taken() == 0));
}

#[test]
fn only_an_agent_tagged_pointer_to_a_plain_envelope_among_the_envelopes_is_taken() {
  let root = Root::new();
  let _host = root.start(&[]);
  let marker = root.mark("outside-ran");
  // A complete, granted envelope that stands outside the executions directory.
  let outside = root.marks.path().join("outside");

  fs::create_dir(&outside).unwrap();
  fs::write(
    outside.join("command.json"),
    json!({ "command": { "args": [SH, "-c", format!("touch {}", marker.display())], "cwd": "/", "env": {}, "stdin": null } }).to_string(),
  )
  .unwrap();
  root.grant(&outside);

  let inbox = root.home().join("inbox");

  fs::write(inbox.join("not-json"), outside.to_str().unwrap()).unwrap();
  fs::write(
    inbox.join("no-agent"),
    json!({ "envelope": root.executions().join("fine-one") }).to_string(),
  )
  .unwrap();
  root.point("gate", outside.to_str().unwrap());
  root.point("gate", "/etc");
  root.point("gate", &format!("{}/../outside", root.executions().display()));
  root.point("gate", root.home().to_str().unwrap());

  let fine = root.envelope("fine-one", "printf fine", json!({}));

  root.place("gate", &fine);
  root.grant(&fine);

  assert_eq!(root.collect(&fine, 5_000).stdout, b"fine");
  assert!(within(1_000, || fs::read_dir(&inbox).unwrap().count() == 0));
  thread::sleep(Duration::from_millis(200));
  assert!(!marker.exists());
  assert!(!outside.join("outcome.json").exists());
}

// New: a deployment restarts the host and kills nothing (docs/agent-control.md, 「A deployment kills
// no execution」). Also replaces "an execution left with a host that retired is taken by the next
// one".
#[test]
fn a_running_execution_survives_a_restart_of_the_host_and_the_new_host_adopts_it() {
  let root = Root::new();
  let mut first = root.start(&[]);
  let streaming =
    root.envelope("streaming", "for i in 1 2 3 4 5 6 7 8 9 10; do echo $i; sleep 0.1; done", json!({}));

  root.place("gate", &streaming);
  root.grant(&streaming);

  let child = root.started(&streaming, 3_000)["pid"].as_i64().unwrap() as i32;

  first.kill(libc::SIGKILL);
  assert!(first.ended(2_000).is_some());
  assert!(alive(child), "the child went with the host");
  assert_eq!(root.taken(), 1);

  // Left in the inbox while no host stood.
  let waiting = root.envelope("waiting", "printf taken", json!({}));

  root.place("gate", &waiting);
  root.grant(&waiting);

  let _second = root.start(&[]);
  let answer = root.collect(&streaming, 5_000);

  assert_eq!((answer.exit_code, answer.cause.as_str()), (0, "exited"));
  assert_eq!(String::from_utf8(answer.stdout).unwrap(), "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n");
  assert_eq!(root.collect(&waiting, 5_000).stdout, b"taken");
  assert!(within(2_000, || root.taken() == 0));
  assert!(root.view("gate").last().unwrap().starts_with("resting"));
}

// New: an envelope taken but never handed to a supervisor, by a host gone in between, is handed to one
// by the next host, and a record that host left staged is removed.
#[test]
fn an_envelope_taken_but_never_started_is_started_by_the_next_host() {
  let root = Root::new();
  let dir = root.envelope("half-taken", "printf late", json!({}));
  let taken = root.home().join("taken");

  root.grant(&dir);
  fs::create_dir_all(&taken).unwrap();
  fs::write(
    taken.join("half-taken.json"),
    json!({ "agent": "gate", "envelope": dir, "title": "test", "since": now_ms(), "owner": ProcessIdentity::own(), "supervisor": null }).to_string(),
  )
  .unwrap();
  fs::write(taken.join("half-taken.json.new"), "{").unwrap();

  let _host = root.start(&[]);

  assert!(!taken.join("half-taken.json.new").exists());
  assert_eq!(root.collect(&dir, 5_000).stdout, b"late");
}

// New: a supervisor started by a host gone before it wrote the supervisor down still holds the
// envelope, so the next host watches it to its end rather than running the envelope again.
#[test]
fn an_envelope_a_supervisor_already_holds_is_watched_and_not_run_again() {
  let root = Root::new();
  let dir = root.envelope("held", "printf twice", json!({ "owner": null }));
  let taken = root.home().join("taken");

  root.grant(&dir);
  fs::write(dir.join("supervisor"), json!(ProcessIdentity::own()).to_string()).unwrap();
  fs::create_dir_all(&taken).unwrap();
  fs::write(
    taken.join("held.json"),
    json!({ "agent": "gate", "envelope": dir, "title": "test", "since": now_ms(), "owner": null, "supervisor": null }).to_string(),
  )
  .unwrap();

  let _host = root.start(&[]);

  fs::write(dir.join("outcome.json"), r#"{"exitCode":1,"cause":"cancelled"}"#).unwrap();
  assert!(within(5_000, || root.view("gate").iter().any(|line| line.ends_with("test cancelled"))));
  assert!(!dir.join("stdout").exists());
}

// New: a second supervisor started for an envelope another already holds ends at once, touching
// nothing.
#[test]
fn a_second_supervisor_for_a_held_envelope_ends_touching_nothing() {
  let root = Root::new();
  let dir = root.envelope("held", "printf twice", json!({}));

  root.grant(&dir);
  fs::write(dir.join("supervisor"), json!(ProcessIdentity::own()).to_string()).unwrap();

  let mut second = Running(
    Command::new(binary())
      .arg("supervise")
      .arg(&dir)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .unwrap(),
  );

  assert!(second.ended(5_000).is_some_and(|status| status.success()));

  let mut left: Vec<String> = fs::read_dir(&dir)
    .unwrap()
    .map(|entry| entry.unwrap().file_name().into_string().unwrap())
    .collect();

  left.sort();
  assert_eq!(left, ["command.json", "grant", "supervisor"]);
}

#[test]
fn a_host_whose_data_root_is_taken_away_ends() {
  let root = Root::new();
  let mut host = root.start(&[]);

  fs::remove_dir_all(root.executions()).unwrap();

  assert!(host.ended(5_000).is_some_and(|status| status.success()));
}

// The claim says who holds the root, in the fields the launchers read, and goes when the host is
// asked to end.
#[test]
fn the_claim_names_the_host_and_goes_when_it_ends() {
  let root = Root::new();
  let mut host = root.start(&[]);
  let claim = root.claim().unwrap();

  assert_eq!(claim["pid"], host.pid());
  assert_eq!(claim["birth"], ProcessIdentity::of(host.pid()).unwrap().birth);
  assert!(claim["paneId"].is_null());
  assert!(claim["startedAt"].as_u64().unwrap() > 0);
  assert_eq!(claim["codeRoot"], root.data.path().to_str().unwrap());

  host.kill(libc::SIGTERM);
  assert!(host.ended(2_000).is_some_and(|status| status.success()));
  assert!(root.claim().is_none());
}
