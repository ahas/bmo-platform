//! A worker's dev server (docs/worker-bench.md, 「Dev server」): asked from the shell with `bmo-agency
//! dev-server start|restart|stop <worker>`, read with `bmo-agency dev-server list [--sweep]`, and run by
//! `bmo-agency dev-server pane <worker>` in the worker's herdr workspace.
//!
//! The server belongs to the dev-server pane, and stands only while a job of the worker has asked for it,
//! so each verb is asked of that pane through the workbench. Start leaves the job's request, brings the
//! pane to the worker's row (started where it is missing) and waits until a server stands, or until the
//! request or the job is gone; restart leaves a restart request beside it and waits for a new one; stop
//! takes the request back and waits until the pane has taken the server down. No wait here ends on a clock.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use policy::fill_marks;
use policy::machine_room::MACHINE_ROOM;
use policy::verb_words::VERB_WORDS;
use serde_json::json;
use tokio::io::AsyncReadExt;

use super::{Answer, VerbError};
use crate::places::{AgentRow, Ground, dev, process_table};
use crate::queue::{holds, settings};

/// The process group the dev server was last started in, so a pane that died without tidying up does not
/// leave a server holding the port the next one needs.
pub const DEV_SERVER_PID: &str = "dev-server.pid";
/// A request to start the dev server again, left in the workbench for the pane to take on its next look.
pub const DEV_SERVER_RESTART: &str = "dev-server.restart";
/// The job's request that its worker's dev server stand.
pub const DEV_SERVER_WANTED: &str = "dev-server.wanted";
/// The server's output, as the pane writes it beside its screen.
pub const DEV_SERVER_LOG: &str = "dev-server.log";

// How often a start looks for the server to come up, and a stop for it to go down.
const POLL: Duration = Duration::from_millis(500);

/// How often the pane reads the worker's row again.
pub const DEV_LOOK_MS: i64 = 3_000;
/// A server that ends sooner than this after it started is one failing to come up.
pub const DEV_QUICK_EXIT_MS: i64 = 30_000;
/// Where the log is rotated; one older generation is kept beside it.
pub const DEV_LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;
/// How often the pane asks who is connected to the server while memory is short: one asking of `lsof`
/// costs about a third of a second of CPU, too much for every look.
pub const DEV_USE_LOOK_MS: i64 = 30_000;

// The machine's launch limits under which a server nobody is on goes down, each a settings row under this
// prefix, with the default policy/machine-room.toml holds where no row does (docs/agent-control.md, 「The
// limits」).
const MACHINE_LIMIT: &str = "machine_limit:";
const GIGABYTE: f64 = 1024.0 * 1024.0 * 1024.0;

/// The workbench's files of one worker's dev server.
struct Bench {
  dir: PathBuf,
}

impl Bench {
  fn of(ground: &Ground, worker: &str) -> Self {
    let dir = dev::workbench(ground, worker);
    let _ = std::fs::create_dir_all(&dir);

    Self { dir }
  }

  fn file(&self, name: &str) -> PathBuf {
    self.dir.join(name)
  }

  fn log(&self) -> PathBuf {
    self.file(DEV_SERVER_LOG)
  }

  // The group the pid file names, where it names one.
  fn group(&self) -> Option<i32> {
    std::fs::read_to_string(self.file(DEV_SERVER_PID))
      .ok()?
      .trim()
      .parse()
      .ok()
      .filter(|group| *group > 1)
  }

  // The server standing now: the group the pid file names, while it lives.
  fn standing(&self) -> Option<i32> {
    self.group().filter(|group| group_alive(*group))
  }

  fn asked(&self) -> bool {
    self.file(DEV_SERVER_WANTED).exists()
  }

  fn drop_request(&self) {
    let _ = std::fs::remove_file(self.file(DEV_SERVER_WANTED));
  }

  fn stamp(&self, name: &str) {
    let _ =
      std::fs::write(self.file(name), format!("{}\n", chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ")));
  }
}

fn group_alive(group: i32) -> bool {
  // SAFETY: a zero signal only asks whether the group exists and may be signalled.
  unsafe { libc::kill(-group, 0) == 0 }
}

fn signal_group(group: i32, signal: i32) {
  // SAFETY: the group named is one this machine's dev server stands in.
  unsafe {
    libc::kill(-group, signal);
  }
}

// A server asked to end with TERM, waited on until its group has gone however long its ending takes.
async fn end_group(group: i32) {
  signal_group(group, libc::SIGTERM);

  while group_alive(group) {
    tokio::time::sleep(Duration::from_millis(100)).await;
  }
}

// Whether a job is at work in the worker's checkout, in any stage or round of it.
async fn job_at_work(ground: &Ground, worker: &str) -> Result<bool, VerbError> {
  holds::job_at_work(&ground.db, worker)
    .await
    .map_err(|refusal| VerbError::Refused(refusal.detail))
}

/// What `bmo-agency dev-server` is asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum DevAction {
  Start,
  Restart,
  Stop,
}

/// One worker's dev server started, started again or stopped, waited on until it has, or until the request
/// or the job it stood for is gone.
pub async fn dev_server(data_root: &Path, action: DevAction, worker: &str) -> Result<Answer, VerbError> {
  let ground = super::ground(data_root).await?;
  let words = &VERB_WORDS.dev_servers.server;
  let refused = |template: &str, marks: &[(&str, &str)]| VerbError::Refused(fill_marks(template, marks));
  let row = AgentRow::named(&ground.db, worker)
    .await
    .map_err(|refusal| VerbError::Refused(refusal.detail))?
    .filter(|row| row.kind == "worker")
    .ok_or_else(|| refused(&words.no_worker, &[("worker", worker)]))?;
  let bench = Bench::of(&ground, worker);
  let log = bench.log().to_string_lossy().into_owned();

  // Stopping asks nothing of the job: the request is taken back, and the pane, on its next look, stops
  // the server and closes itself.
  if action == DevAction::Stop {
    bench.drop_request();

    let Some(before) = bench.standing() else {
      return Ok(Answer::from(vec![fill_marks(&words.not_up, &[("worker", worker)])]));
    };

    while group_alive(before) {
      tokio::time::sleep(POLL).await;
    }

    return Ok(Answer::from(vec![fill_marks(&words.down, &[("worker", worker)])]));
  }

  if dev::dev_command(&ground, &row).await.map_err(VerbError::Refused)?.is_none() {
    return Err(refused(&words.no_command, &[("worker", worker)]));
  }

  // The server stands only while a job of the worker runs, so with none the pane would close again at once.
  if !job_at_work(&ground, worker).await? {
    return Err(refused(&words.no_job, &[("worker", worker)]));
  }

  let before = bench.standing();

  bench.stamp(DEV_SERVER_WANTED);

  if action == DevAction::Start
    && let Some(before) = before
  {
    return Ok(Answer::from(vec![fill_marks(
      &words.already_up,
      &[("worker", worker), ("group", &before.to_string()), ("log", &log)],
    )]));
  }

  if action == DevAction::Restart {
    bench.stamp(DEV_SERVER_RESTART);
  }

  dev::dev_pane(&ground, worker).await;

  // The wait is for the new server's group, and ends without one only where the pane took the request
  // away or the job let its hold go: the pane closed, or the job ended.
  let now = loop {
    if let Some(now) = bench.standing().filter(|group| Some(*group) != before) {
      break Some(now);
    }

    if !bench.asked() || !job_at_work(&ground, worker).await? {
      break None;
    }

    tokio::time::sleep(POLL).await;
  };

  let Some(now) = now else {
    let mut said = fill_marks(&words.not_started, &[("log", &log)]);

    if let Ok(text) = std::fs::read_to_string(bench.log()) {
      let lines: Vec<&str> = text.split('\n').collect();

      said.push('\n');
      said.push_str(&lines[lines.len().saturating_sub(30)..].join("\n"));
    }

    return Err(VerbError::Refused(said));
  };
  let done = if action == DevAction::Start { &words.started } else { &words.restarted };

  Ok(Answer::from(vec![fill_marks(
    &words.up,
    &[("worker", worker), ("done", done), ("group", &now.to_string()), ("log", &log)],
  )]))
}

/// Every worker's dev server and every run's record with what the run still holds, with the memory each
/// holds, and with `sweep` every run whose program is gone taken down, as the bot's sweep does
/// (docs/run-groups.md).
pub async fn dev_servers(data_root: &Path, sweep: bool) -> Result<Answer, VerbError> {
  let ground = super::ground(data_root).await?;
  let words = &VERB_WORDS.dev_servers.listing;
  let fault = |refusal: crate::Refusal| VerbError::Refused(refusal.detail);
  let processes = process_table::processes().await;
  let mut lines = vec![words.head.clone()];
  let mut count = 0;
  let mut kilobytes = 0;

  for row in AgentRow::workers(&ground.db).await.map_err(fault)? {
    if dev::dev_command(&ground, &row).await.map_err(VerbError::Refused)?.is_none() {
      continue;
    }

    let Some(leader) = Bench::of(&ground, &row.name).group() else { continue };
    let members: Vec<&process_table::ProcessRow> =
      processes.iter().filter(|one| one.group == leader).collect();

    if members.is_empty() {
      continue;
    }

    let held: i64 = members.iter().map(|one| one.kilobytes).sum();
    let age = members
      .iter()
      .find(|one| one.pid == leader)
      .map_or_else(|| members.iter().map(|one| one.age_seconds).max().unwrap_or(0), |one| one.age_seconds);
    let job = if job_at_work(&ground, &row.name).await? { &words.job_running } else { &words.no_job };

    count += 1;
    kilobytes += held;
    lines.push(fill_marks(
      &words.worker,
      &[
        ("worker", &row.name),
        ("group", &leader.to_string()),
        ("minutes", &((age as f64 / 60.0).round() as i64).to_string()),
        ("megabytes", &((held as f64 / 1024.0).round() as i64).to_string()),
        ("job", job),
      ],
    ));
  }

  let held = crate::run_groups::standing(&ground.roots.data).await;

  lines.push(String::new());
  lines.push(words.left_head.clone());

  for run in &held {
    let run_kilobytes: u64 = run.holds.iter().map(|seen| seen.kilobytes).sum();

    count += 1;
    kilobytes += run_kilobytes as i64;
    lines.push(fill_marks(
      &words.run,
      &[
        ("run", &run.record.run),
        ("cwd", &run.record.cwd),
        ("program", if run.program_runs { &words.program_running } else { &words.program_ended }),
        ("count", &run.holds.len().to_string()),
        ("megabytes", &((run_kilobytes as f64 / 1024.0).round() as i64).to_string()),
      ],
    ));
  }

  if held.is_empty() {
    lines.push(words.none.clone());
  }

  lines.push(String::new());
  lines.push(fill_marks(
    &words.total,
    &[
      ("count", &count.to_string()),
      ("megabytes", &((kilobytes as f64 / 1024.0).round() as i64).to_string()),
    ],
  ));

  // A copy of the data root takes down nothing of the machine's.
  if sweep && ground.roots.own_data_root() {
    let ended = crate::run_groups::sweep(&ground.roots.data, None).await;

    lines.push(String::new());
    lines.push(if ended.is_empty() { words.nothing_to_end.clone() } else { ended.join("\n") });
  }

  Ok(Answer::from(lines))
}

/// What the pane does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevStep {
  Exit,
  Pressed,
  Stop,
  Start,
  Wait,
}

/// What the pane knows at a look.
#[derive(Clone, Debug, Default)]
pub struct DevState {
  /// What should be running — the command and the directory it runs in — or none.
  pub wanted: Option<String>,
  /// What is running, or none.
  pub running: Option<String>,
  /// When a stopped server may start again.
  pub start_at: i64,
  pub restart: bool,
  /// The machine is short of memory and no client outside the server's group is connected to it.
  pub pressed: bool,
}

/// What to do now. The command the row names is what should be running; a different one running is stopped
/// first and the named one started on a later look, and no command named at all ends the pane — once
/// whatever was running has been stopped. A restart a job asked for stops the matching server too, and
/// starts a stopped one without waiting out the delay after a failure. A server nobody is on while memory
/// is short is ended with the pane, unless a restart was just asked for. Neither its age nor how long it
/// went unasked ends it.
pub fn dev_step(state: &DevState, now: i64) -> DevStep {
  let Some(wanted) = &state.wanted else {
    return if state.running.is_none() { DevStep::Exit } else { DevStep::Stop };
  };

  if state.running.is_some() && !state.restart && state.pressed {
    return DevStep::Pressed;
  }

  match &state.running {
    Some(running) => {
      if running == wanted && !state.restart {
        DevStep::Wait
      } else {
        DevStep::Stop
      }
    }
    None if state.restart || now >= state.start_at => DevStep::Start,
    None => DevStep::Wait,
  }
}

/// How long to wait before starting again after an ending: three seconds after a server that ran a while,
/// doubling with each quick ending in a row up to a minute.
pub fn dev_restart_delay_ms(quick_endings: u32) -> i64 {
  (3_000_i64 << quick_endings.saturating_sub(1).min(20)).min(60_000)
}

/// The environment the dev server runs in: the pane's own with the bot's variables taken out, and the
/// workbench named, with a support worker's number beside it so a server that binds ports moves them.
pub fn dev_environment(
  env: impl IntoIterator<Item = (String, String)>,
  workbench: &Path,
  slot: Option<i64>,
) -> Vec<(String, String)> {
  let mut kept: Vec<(String, String)> =
    env.into_iter().filter(|(name, _)| !name.starts_with("BMO_")).collect();

  kept.push(("BMO_WORKBENCH_DIR".into(), workbench.to_string_lossy().into_owned()));

  if let Some(slot) = slot {
    kept.push(("BMO_WORKER_SLOT".into(), slot.to_string()));
  }

  kept
}

/// The log moved aside once it has grown past its bound, one older generation kept; whether it was.
pub fn rotate_dev_log(path: &Path, max_bytes: u64) -> bool {
  match std::fs::metadata(path) {
    Ok(held) if held.len() > max_bytes => std::fs::rename(path, format!("{}.1", path.display())).is_ok(),
    _ => false,
  }
}

/// Whether a client outside the server's process group holds a connection to a port the group listens on,
/// out of the socket names `lsof -F n` gives for the group's TCP sockets: `host:port` for a listening
/// socket, `local->remote` for a connected one. Field output is lsof's own form for programs, one field to
/// a line behind its letter, which no library reads. A connection whose far end is one of the group's own
/// sockets is not a client.
pub fn outside_client(fields: &str) -> bool {
  let names: Vec<&str> = fields.lines().filter_map(|line| line.strip_prefix('n')).collect();
  let port = |address: &str| address.rsplit(':').next().unwrap_or_default().to_owned();
  let listening: Vec<String> =
    names.iter().filter(|name| !name.contains("->")).map(|name| port(name)).collect();
  let connected: Vec<(&str, &str)> = names.iter().filter_map(|name| name.split_once("->")).collect();
  let own: Vec<&str> = connected.iter().map(|(local, _)| *local).collect();

  connected
    .iter()
    .any(|(local, remote)| listening.contains(&port(local)) && !own.contains(remote))
}

// The moment a line of the pane is stamped with, in Korean time.
fn stamp() -> String {
  let now = chrono::Utc::now();
  let shown = "%Y-%m-%d %H:%M:%S";

  chrono::FixedOffset::east_opt(9 * 3600).map_or_else(
    || now.format(shown).to_string(),
    |offset| now.with_timezone(&offset).format(shown).to_string(),
  )
}

fn now_ms() -> i64 {
  chrono::Utc::now().timestamp_millis()
}

// The pane's screen and the workbench's log, written together.
struct Output {
  log: PathBuf,
}

impl Output {
  fn write(&self, text: &str) {
    let mut out = std::io::stdout();

    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();

    // A log that cannot be written is no reason to take the server down with it.
    if let Ok(mut log) = std::fs::OpenOptions::new().create(true).append(true).open(&self.log) {
      let _ = log.write_all(text.as_bytes());
    }
  }

  fn say(&self, line: &str) {
    self.write(&format!("\n[{}] {line}\n", stamp()));
  }
}

// The server the pane holds: its process group, and what it was started as.
struct Running {
  group: i32,
  key: String,
}

// A server's ending as its watcher tells it: its group, and its code where it exited with one.
type Exit = (i32, Option<i32>);

// Everything the pane keeps across its looks.
struct Pane {
  ground: Ground,
  worker: String,
  bench: Bench,
  out: std::sync::Arc<Output>,
  exits: tokio::sync::mpsc::UnboundedSender<Exit>,
  running: Option<Running>,
  started_at: i64,
  start_at: i64,
  quick_endings: u32,
  // What lsof last said of a client on the server, and when it was asked.
  client_on: bool,
  clients_asked_at: i64,
}

fn pump(out: std::sync::Arc<Output>, mut from: impl tokio::io::AsyncRead + Unpin + Send + 'static) {
  tokio::spawn(async move {
    let mut buffer = vec![0; 16 * 1024];

    while let Ok(read) = from.read(&mut buffer).await {
      if read == 0 {
        return;
      }

      out.write(&String::from_utf8_lossy(&buffer[..read]));
    }
  });
}

impl Pane {
  // A restart asked for, taken away as it is read so it is done once.
  fn take_restart(&self) -> bool {
    std::fs::remove_file(self.bench.file(DEV_SERVER_RESTART)).is_ok()
  }

  // Whether a client outside the server's group is connected to it, asked of lsof every half minute at
  // most; until it is first asked, the server reads as one a client is on.
  fn client_on(&mut self, now: i64) -> bool {
    let Some(group) = self.running.as_ref().map(|running| running.group) else {
      return false;
    };

    if self.clients_asked_at != 0 && now - self.clients_asked_at < DEV_USE_LOOK_MS {
      return self.client_on;
    }

    self.clients_asked_at = now;

    let listed = std::process::Command::new("lsof")
      .args(["-w", "-nP", "-a", &format!("-g{group}"), "-iTCP", "-F", "n"])
      .stdin(Stdio::null())
      .stderr(Stdio::null())
      .output();

    self.client_on = listed.is_ok_and(|listed| outside_client(&String::from_utf8_lossy(&listed.stdout)));
    self.client_on
  }

  async fn end_leftover(&self) {
    // A fresh pane starts the server anyway, so a restart asked before it came up is already done.
    self.take_restart();

    if let Some(group) = self.bench.group()
      && group_alive(group)
    {
      self
        .out
        .say(&fill_marks(&VERB_WORDS.dev_servers.pane.leftover, &[("group", &group.to_string())]));
      end_group(group).await;
    }

    let _ = std::fs::remove_file(self.bench.file(DEV_SERVER_PID));
  }

  async fn slot(&self) -> Option<i64> {
    crate::queue::workers::worker_row(&self.ground.db, &self.worker, &self.ground.source)
      .await
      .ok()
      .flatten()
      .and_then(|checkout| checkout.support_slot)
  }

  async fn start(&mut self, command: &str, cwd: &str, key: String) {
    self
      .out
      .say(&fill_marks(&VERB_WORDS.dev_servers.pane.start, &[("command", command), ("cwd", cwd)]));

    let env = dev_environment(std::env::vars(), &self.bench.dir, self.slot().await);
    // setsid gives the server a session and a process group of its own, whose number is its pid, so
    // stopping it takes every process it started rather than the shell alone.
    let spawned = tokio::process::Command::new("setsid")
      .args(["zsh", "-c", command])
      .current_dir(cwd)
      .env_clear()
      .envs(env)
      .stdin(Stdio::null())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn();
    let mut child = match spawned {
      Ok(child) => child,
      Err(why) => {
        self.out.say(&why.to_string());
        return;
      }
    };
    let group = child.id().map_or(0, |pid| pid as i32);

    if let Some(stdout) = child.stdout.take() {
      pump(self.out.clone(), stdout);
    }

    if let Some(stderr) = child.stderr.take() {
      pump(self.out.clone(), stderr);
    }

    let exits = self.exits.clone();

    tokio::spawn(async move {
      let code = child.wait().await.ok().and_then(|status| status.code());
      let _ = exits.send((group, code));
    });
    self.started_at = now_ms();
    let _ = std::fs::write(self.bench.file(DEV_SERVER_PID), format!("{group}\n"));
    self.running = Some(Running { group, key });
  }

  async fn stop(&mut self, reason: &str) {
    let Some(running) = self.running.take() else { return };

    self.out.say(reason);
    end_group(running.group).await;
    let _ = std::fs::remove_file(self.bench.file(DEV_SERVER_PID));
    self.quick_endings = 0;
    self.start_at = 0;
  }

  // A server ended by itself: started again after a delay that grows with each quick ending. One the pane
  // stopped, or one before it, is past.
  fn ended(&mut self, (group, code): Exit) {
    if self.running.as_ref().is_none_or(|running| running.group != group) {
      return;
    }

    self.running = None;
    let _ = std::fs::remove_file(self.bench.file(DEV_SERVER_PID));
    self.quick_endings =
      if now_ms() - self.started_at < DEV_QUICK_EXIT_MS { self.quick_endings + 1 } else { 0 };

    let delay = dev_restart_delay_ms(self.quick_endings);
    let words = &VERB_WORDS.dev_servers.pane;
    let quick = if self.quick_endings > 1 {
      fill_marks(&words.quick, &[("count", &self.quick_endings.to_string())])
    } else {
      String::new()
    };

    self.start_at = now_ms() + delay;
    self.out.say(&fill_marks(
      &words.ended,
      &[
        ("code", &code.map_or_else(|| "null".to_owned(), |code| code.to_string())),
        ("seconds", &(delay / 1000).to_string()),
        ("quick", &quick),
      ],
    ));
  }

  // The server ended for good, with the job's request for it, and the pane with it.
  async fn close(&mut self, reason: &str) -> Answer {
    self.stop(reason).await;
    self.bench.drop_request();

    Answer::default()
  }

  // One look; an answer where the pane is done.
  async fn look(&mut self) -> Result<Option<Answer>, VerbError> {
    let words = &VERB_WORDS.dev_servers.pane;
    let fault = |refusal: crate::Refusal| VerbError::Refused(refusal.detail);
    let row = AgentRow::named(&self.ground.db, &self.worker).await.map_err(fault)?;
    let cwd = row.as_ref().map_or_else(
      || self.ground.source.to_string_lossy().into_owned(),
      |row| self.ground.checkout_of(row),
    );
    // Every worker's server runs only while a job is at work in its checkout and has asked for it, since a
    // dev console nobody looks at would hold gigabytes for nothing. A support worker runs its base's command.
    let job_over = !job_at_work(&self.ground, &self.worker).await?;
    let asked = self.bench.asked();
    let command = match &row {
      Some(row) if row.kind == "worker" && !job_over && asked && !row.resting => {
        dev::dev_command(&self.ground, row).await.map_err(VerbError::Refused)?
      }
      _ => None,
    };
    let wanted = command.as_ref().map(|command| json!([cwd, command]).to_string());
    let restart = self.take_restart();
    let now = now_ms();
    // A running server nobody is on goes down while the machine is short of memory by the limits a job's
    // launch is held to; who is on it is asked only then.
    let short = self.running.is_some() && self.memory_short().await?;
    let pressed = short && !self.client_on(now);
    let state = DevState {
      wanted: wanted.clone(),
      running: self.running.as_ref().map(|running| running.key.clone()),
      start_at: self.start_at,
      restart,
      pressed,
    };
    let why = if job_over {
      &words.job_over
    } else if !asked {
      &words.not_asked
    } else {
      &words.no_command
    };

    match dev_step(&state, now) {
      DevStep::Exit => {
        self.out.say(&fill_marks(&words.closing, &[("why", why)]));
        self.bench.drop_request();

        return Ok(Some(Answer::default()));
      }
      DevStep::Pressed => {
        let reason = words.pressed.clone();

        return Ok(Some(self.close(&reason).await));
      }
      DevStep::Stop => {
        let reason = if wanted.is_none() {
          fill_marks(&words.stopping, &[("why", why)])
        } else if restart {
          words.restarting.clone()
        } else {
          words.moved.clone()
        };

        self.stop(&reason).await;

        if restart && let (Some(command), Some(wanted)) = (&command, wanted) {
          self.start(command, &cwd, wanted).await;
        }
      }
      DevStep::Start => {
        if restart {
          self.out.say(&words.starting_now);
          self.quick_endings = 0;
        }

        if let (Some(command), Some(wanted)) = (&command, wanted) {
          self.start(command, &cwd, wanted).await;
        }
      }
      DevStep::Wait => {}
    }

    rotate_dev_log(&self.bench.log(), DEV_LOG_MAX_BYTES);

    Ok(None)
  }

  async fn memory_short(&self) -> Result<bool, VerbError> {
    let limit = |name: &'static str, default: f64| async move {
      settings::get(&self.ground.db, &format!("{MACHINE_LIMIT}{name}"))
        .await
        .map(|value| {
          value
            .and_then(|value| value.trim().parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(default)
        })
        .map_err(|refusal| VerbError::Refused(refusal.detail))
    };
    let available_gb =
      limit("launch_memory_available_gb", MACHINE_ROOM.launch_memory_available_gb.default).await?;
    let full_pct = limit("launch_memory_full_pct", MACHINE_ROOM.launch_memory_full_pct.default).await?;

    Ok(memory_reading().is_some_and(|(available, full)| {
      available < available_gb || full.is_some_and(|full| full >= full_pct)
    }))
  }
}

// The machine's available memory in gigabytes and the share of the last minute every task stood stalled on
// memory, in percent, where the kernel keeps it; none where the memory cannot be read, which holds nothing.
fn memory_reading() -> Option<(f64, Option<f64>)> {
  use procfs::Current;

  let available = procfs::Meminfo::current().ok()?.mem_available?;
  let full = procfs::MemoryPressure::current().ok().map(|pressure| f64::from(pressure.full.avg60));

  Some((available as f64 / GIGABYTE, full))
}

/// The dev-server pane of one worker, run until it closes: the worker's dev command in its checkout, in a
/// session and process group of its own, with its output on the screen and in the workbench's log, while
/// a job of the worker runs and has asked for it.
pub async fn dev_pane(data_root: &Path, worker: &str) -> Result<Answer, VerbError> {
  let ground = super::ground(data_root).await?;
  let bench = Bench::of(&ground, worker);
  let out = std::sync::Arc::new(Output { log: bench.log() });
  let words = &VERB_WORDS.dev_servers.pane;

  out.write(&format!(
    "{}\n",
    fill_marks(
      &words.head,
      &[
        ("worker", worker),
        ("workbench", &bench.dir.to_string_lossy()),
        ("log", &bench.log().to_string_lossy())
      ],
    )
  ));

  let (exits, mut ended) = tokio::sync::mpsc::unbounded_channel();
  let mut pane = Pane {
    ground,
    worker: worker.to_owned(),
    bench,
    out,
    exits,
    running: None,
    started_at: 0,
    start_at: 0,
    quick_endings: 0,
    client_on: false,
    clients_asked_at: 0,
  };
  let mut closed = Vec::new();

  for kind in [
    tokio::signal::unix::SignalKind::terminate(),
    tokio::signal::unix::SignalKind::hangup(),
    tokio::signal::unix::SignalKind::interrupt(),
  ] {
    if let Ok(signal) = tokio::signal::unix::signal(kind) {
      closed.push(signal);
    }
  }

  let (mut terminate, mut hangup, mut interrupt) = {
    let mut drained = closed.into_iter();

    (drained.next(), drained.next(), drained.next())
  };
  let mut looks = tokio::time::interval(Duration::from_millis(DEV_LOOK_MS as u64));

  pane.end_leftover().await;

  loop {
    let signalled = async {
      tokio::select! {
        Some(()) = async { terminate.as_mut()?.recv().await } => {}
        Some(()) = async { hangup.as_mut()?.recv().await } => {}
        Some(()) = async { interrupt.as_mut()?.recv().await } => {}
        else => std::future::pending::<()>().await,
      }
    };

    tokio::select! {
      _ = looks.tick() => {
        if let Some(answer) = pane.look().await? {
          return Ok(answer);
        }
      }
      Some(exit) = ended.recv() => pane.ended(exit),
      () = signalled => {
        let reason = words.pane_closed.clone();

        pane.stop(&reason).await;

        return Ok(Answer::default());
      }
    }
  }
}
