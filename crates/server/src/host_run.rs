//! One run of a provider's program carried in its agent's host (docs/agent-control.md, 「The host」), as
//! the TypeScript side's `spawnAgentProcess` carries one: its execution row begun, the launch checks made
//! — a standing cancellation over its names, its provider switched off — the envelope written with the
//! policy its supervisor reads again at the start, the pointer put in the inbox of the data root's agent
//! host (started where a copy of the data root has none), the grant given, and the output and the outcome
//! collected, the row moving with each. What the program writes is read by the provider that runs it: the
//! session a line names, and what a person watching the agent's view is shown of it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use agent_host::disk::{private_dir, publish, publish_bytes, read_json};
use agent_host::envelope::{Cause, Outcome, Started};
use agent_host::identity::ProcessIdentity;
use agent_host::layout::Layout;
use agent_host::view::{Views, ended_line};
use policy::deployment::DEPLOYMENT;
use policy::door_words::DOOR_WORDS;
use serde::Serialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::control::harness::provider_enabled;
use crate::door::execution::{Row, cancelled, execution_root};
use crate::lines::token;
use crate::places::Ground;

// The waits before a launch whose host did not come up tries again; a test names a shorter one, which
// stands for each.
const PLACE_RETRIES: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(5)];

fn place_retries() -> [Duration; 2] {
  match std::env::var("BMO_PLACE_RETRY_MS").ok().and_then(|wait| wait.parse().ok()) {
    Some(wait) => [Duration::from_millis(wait); 2],
    None => PLACE_RETRIES,
  }
}

// How often the envelope is looked at, how often whoever answers for it is asked after, and how often a
// launch looks again for a host to stand.
const POLL: Duration = Duration::from_millis(25);
const LOOK: Duration = Duration::from_millis(200);
const HOST_LOOK: Duration = Duration::from_millis(100);

/// The program a run starts: its words, the directory it works in, its environment and what it reads
/// on its standard input.
#[derive(Clone, Debug, serde::Deserialize, Serialize)]
pub struct Command {
  pub args: Vec<String>,
  pub cwd: String,
  pub env: BTreeMap<String, String>,
  pub stdin: Option<String>,
}

/// One run as its caller names it: whose host takes it, the line and the provider it runs, the program,
/// and who its row says it is.
pub struct Asked<'a> {
  pub agent: &'a str,
  pub line: &'a str,
  pub provider: &'a str,
  pub command: Command,
  pub root: Option<String>,
  pub parent: Option<String>,
  /// The role the run holds, where it is a role's.
  pub role: Option<String>,
  pub model: Option<String>,
  pub effort: Option<String>,
  pub session: Option<String>,
}

/// What a run came to: how its program ended, what it wrote, and its row as it was left.
#[derive(Debug)]
pub struct Ran {
  pub exit_code: i32,
  pub cause: Cause,
  pub stdout: String,
  pub stderr: String,
  pub row: Row,
}

/// Why a run gave no outcome: stopped by somebody, the transport under it breaking, or refused before
/// it was placed.
#[derive(Debug)]
pub enum Failure {
  Stopped(String),
  Transport(String),
  Refused(String),
}

impl Failure {
  /// The failure as the TypeScript side's error says it.
  pub fn said(&self) -> String {
    match self {
      Self::Stopped(why) => format!("ExecutionStopped: {why}"),
      Self::Transport(why) => format!("PaneTransportError: {why}"),
      Self::Refused(why) => format!("Error: {why}"),
    }
  }
}

// The data root's agent host holding the claim now, where a live one does.
fn host_claim(data: &Path) -> Option<ProcessIdentity> {
  let held: ProcessIdentity = read_json(&Layout::new(data).claim())?;

  held.alive().then_some(held)
}

// The data root's agent host, waited for until it takes the claim: started first where a copy of the data
// root has none. The bot's own host is its unit's, which systemd stands up again however long that takes;
// a copy's is the transient unit started here, and its ending without a claim is what ends the wait. A
// launch whose run is cancelled meanwhile stops waiting.
async fn ensure_host(ground: &Ground, names: &[&str]) -> Result<ProcessIdentity, String> {
  let data = &ground.roots.data;

  if let Some(held) = host_claim(data) {
    return Ok(held);
  }

  let copy_unit = match ground.copy() {
    Some(_) => Some(start_copy_host(ground).await.map_err(|why| {
      format!("host-unavailable: agent-host for {} could not be started: {why}", data.display())
    })?),
    None => None,
  };

  loop {
    tokio::time::sleep(HOST_LOOK).await;

    if let Some(held) = host_claim(data) {
      return Ok(held);
    }

    if cancelled(&ground.db, names).await.unwrap_or(false) {
      return Err("cancelled: the run was cancelled while it waited for its host".to_owned());
    }

    if let Some(unit) = &copy_unit
      && !unit_standing(ground, unit).await
      && host_claim(data).is_none()
    {
      return Err(format!(
        "host-unavailable: the copy's agent-host unit {unit} ended without taking the claim at {}",
        Layout::new(data).claim().display()
      ));
    }
  }
}

// Whether a user unit is still standing: anything but systemd saying it is inactive or failed.
async fn unit_standing(ground: &Ground, unit: &str) -> bool {
  let Some(program) = ground.machine().program("systemctl") else { return true };
  let mut command = std::process::Command::new(program);

  command
    .args(["--user", "is-active", &format!("{unit}.service")])
    .stdin(std::process::Stdio::null())
    .stderr(std::process::Stdio::null());
  harness_machine::model_use::with_user_bus(&mut command);

  match tokio::process::Command::from(command).output().await {
    Ok(done) => !matches!(String::from_utf8_lossy(&done.stdout).trim(), "inactive" | "failed"),
    // A systemctl that cannot be asked has not said the unit is gone.
    Err(_) => true,
  }
}

// The pointer to the envelope put in the host's inbox, written beside its name and moved into place.
fn point(data: &Path, agent: &str, name: &str, directory: &Path) -> std::io::Result<()> {
  let inbox = Layout::new(data).inbox();
  let body = json!({ "agent": agent, "envelope": directory }).to_string();

  private_dir(&inbox)?;
  publish_bytes(&inbox.join(name), body.as_bytes())
}

// A copy's host, started as a transient user unit with the properties of the bot's own; answers the unit's
// name.
async fn start_copy_host(ground: &Ground) -> Result<String, String> {
  let root = ground.roots.data.to_string_lossy().into_owned();
  let unit = format!("bmo-agent-host-copy-{}", &format!("{:x}", Sha256::digest(root.as_bytes()))[..12]);
  let binary = [ground.roots.deploy.clone(), ground.code_root()]
    .into_iter()
    .map(|tree| tree.join("bin").join("agent-host"))
    .find(|binary| binary.exists())
    .unwrap_or_else(|| ground.roots.deploy.join("bin").join("agent-host"));
  let mut command = tokio::process::Command::new("systemd-run");

  command
    .args(["--user", "--quiet", "--collect", &format!("--unit={unit}"), "-p", "KillMode=process"])
    .args(["-p", "OOMPolicy=continue"])
    .arg(&binary)
    .args(["serve", "--data-root", &root, "--idle"])
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null());

  if ground.vars().get("XDG_RUNTIME_DIR").is_none_or(String::is_empty) {
    // SAFETY: `getuid` reads the process's own user id and cannot fail.
    command.env("XDG_RUNTIME_DIR", format!("/run/user/{}", unsafe { libc::getuid() }));
  }

  if ground.vars().get("DBUS_SESSION_BUS_ADDRESS").map(String::as_str) == Some("disabled:") {
    command.env_remove("DBUS_SESSION_BUS_ADDRESS");
  }

  let done = command.output().await.map_err(|why| why.to_string())?;

  if done.status.success() {
    return Ok(unit);
  }

  let said = String::from_utf8_lossy(&done.stderr).trim().to_owned();

  Err(if said.is_empty() { format!("systemd-run exited {}", done.status) } else { said })
}

// The envelope as the supervisor reads it, in the launcher's field names.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<'a> {
  command: &'a Command,
  title: &'a str,
  policy: Policy<'a>,
  owner: ProcessIdentity,
}

#[derive(Serialize)]
struct Policy<'a> {
  address: String,
  provider: &'a str,
  names: Vec<String>,
  providers: Vec<&'static str>,
}

fn now_ms() -> u64 {
  agent_host::disk::now_ms()
}

// The row as the TypeScript side begins one for a run in a host.
fn first_body(asked: &Asked<'_>) -> Map<String, Value> {
  let mut body = Map::new();
  let root = asked.root.clone().unwrap_or_else(|| execution_root(""));

  body.insert("id".into(), json!(format!("bmo-execution-{}", uuid::Uuid::new_v4())));
  body.insert("agent".into(), json!(asked.agent));
  body.insert("root".into(), json!(root));
  body.insert("parent".into(), json!(asked.parent));
  body.insert("role".into(), json!(asked.role));
  body.insert("model".into(), json!(asked.model));
  body.insert("effort".into(), json!(asked.effort));
  body.insert("sessionId".into(), json!(asked.session));
  body.insert("provider".into(), json!(asked.provider));
  body.insert("line".into(), json!(asked.line));
  body.insert("paneId".into(), Value::Null);
  body.insert("state".into(), json!("launching"));
  body.insert("startedAt".into(), json!(crate::now_ms()));
  body.insert("endedAt".into(), Value::Null);
  body.insert("exitCode".into(), Value::Null);
  body
}

// Why the run must not start now: a standing cancellation over one of its names, or its provider off.
async fn launch_refusal(ground: &Ground, row: &Row, asked: &Asked<'_>) -> Option<Failure> {
  let names = [row.id(), asked.parent.as_deref().unwrap_or(""), row.root()];

  if cancelled(&ground.db, &names).await.unwrap_or(false) {
    return Some(Failure::Stopped("execution cancelled".to_owned()));
  }

  if !provider_enabled(&ground.db, asked.provider).await.unwrap_or(false) {
    return Some(Failure::Refused(format!("{}: {}", asked.provider, DOOR_WORDS.provider_off)));
  }

  None
}

// The row closed as a launch that never ran: cancelled where a stop refused it, a launch error otherwise.
async fn close_unlaunched(ground: &Ground, row: &mut Row, failure: &Failure) {
  row.set(
    "state",
    json!(if matches!(failure, Failure::Stopped(_)) { "cancelled" } else { "launch-error" }),
  );
  row.set("endedAt", json!(crate::now_ms()));

  if let Err(why) = row.save(&ground.db).await {
    eprintln!("{}: the row could not be closed: {}", row.id(), why.detail);
  }
}

/// What of a file lies past `offset`, up to its last ended line, or the whole of it where `whole` asks.
pub fn unread(path: &Path, offset: usize, whole: bool) -> (String, usize) {
  let Ok(bytes) = std::fs::read(path) else { return (String::new(), offset) };
  let Some(fresh) = bytes.get(offset..) else { return (String::new(), offset) };
  let end = if whole {
    fresh.len()
  } else {
    fresh.iter().rposition(|byte| *byte == b'\n').map_or(0, |at| at + 1)
  };

  (String::from_utf8_lossy(&fresh[..end]).into_owned(), offset + end)
}

/// Whoever reads what a run's program prints as it prints it, a batch of whole lines at a time, and may ask
/// the run stopped from what it read or from its own clock: a run stopped so ends as its caller's stop.
pub trait Heard {
  fn heard(&mut self, lines: &[String]) -> impl Future<Output = ()>;

  fn stopping(&mut self) -> bool {
    false
  }
}

/// A caller that reads the output once the run has ended.
pub struct Deaf;

impl Heard for Deaf {
  async fn heard(&mut self, _lines: &[String]) {}
}

// What the run's program said that has not been read yet: the session it names saved to the row, what a
// person is shown written into the agent's view, and the lines handed to whoever reads them as they come.
struct Follow<'r, H: Heard> {
  provider: Option<&'static dyn harness_providers::Provider>,
  offset: usize,
  views: (Views, &'r str),
  heard: &'r mut H,
}

impl<H: Heard> Follow<'_, H> {
  async fn look(&mut self, ground: &Ground, directory: &Path, row: &mut Row, whole: bool) {
    let (text, next) = unread(&directory.join("stdout"), self.offset, whole);

    self.offset = next;

    let lines: Vec<String> = text.lines().map(str::to_owned).collect();

    if !lines.is_empty() {
      self.heard.heard(&lines).await;
    }

    let mut shown = vec![];

    for line in text.lines() {
      if let Ok(event) = serde_json::from_str::<Value>(line)
        && let Some(session) = self.provider.and_then(|provider| provider.session_of_event(&event))
        && row.text("sessionId") != Some(session.as_str())
      {
        row.set("sessionId", json!(session));
        let _ = row.save(&ground.db).await;
      }

      if let Some(said) = self.provider.and_then(|provider| provider.progress_of(line)) {
        shown.push(format!("   {}", flat_line(&said)));
      }
    }

    if !shown.is_empty() {
      let (views, agent) = &self.views;
      let _ = views.write(agent, &shown);
    }
  }
}

/// How wide a line of the view is kept, and the line as the view holds it: one line, cut with a mark.
pub const PROGRESS_WIDTH: usize = 160;

pub fn flat_line(text: &str) -> String {
  let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");

  if flat.chars().count() <= PROGRESS_WIDTH {
    return flat;
  }

  format!("{}…", flat.chars().take(PROGRESS_WIDTH - 1).collect::<String>().trim_end())
}

// How long a run's tool session may stand if nothing takes its token back.
const TOKEN_BACKSTOP: Duration = Duration::from_secs(24 * 3_600);

/// One run carried in its agent's host, to its outcome.
pub async fn run(ground: &Ground, asked: Asked<'_>) -> Result<Ran, Failure> {
  run_heard(ground, asked, &mut Deaf).await
}

/// One run carried in its agent's host, to its outcome, what its program prints handed to `heard` as it
/// prints it.
pub async fn run_heard<H: Heard>(
  ground: &Ground,
  mut asked: Asked<'_>,
  heard: &mut H,
) -> Result<Ran, Failure> {
  let mut row = Row::open(&ground.db, first_body(&asked))
    .await
    .map_err(|why| Failure::Transport(format!("execution archive unavailable: {}", why.detail)))?;

  if let Some(failure) = launch_refusal(ground, &row, &asked).await {
    close_unlaunched(ground, &mut row, &failure).await;
    return Err(failure);
  }

  // Issued before anything is placed: a tool host that does not answer refuses the run here.
  let deadline = now_ms() + TOKEN_BACKSTOP.as_millis() as u64;
  let token = match token::issue_for_run(
    ground,
    asked.line,
    asked.agent,
    &asked.command.cwd,
    &asked.command.env,
    asked.model.as_deref().map(|model| (asked.provider, model)),
    deadline,
  )
  .await
  {
    Ok(token) => token,
    Err(why) => {
      let failure = Failure::Transport(why.detail);

      close_unlaunched(ground, &mut row, &failure).await;
      return Err(failure);
    }
  };

  if let Some(token) = &token {
    asked.command.env.insert(token::TOKEN_VARIABLE.to_owned(), token.clone());
  }

  let ran = placed_run(ground, &asked, &mut row, heard).await;

  if let Some(token) = &token {
    token::revoke(ground, token).await;
  }

  ran.map(|(exit_code, cause, stdout, stderr)| Ran {
    exit_code,
    cause,
    stdout,
    stderr,
    row,
  })
}

/// One run left in its agent's host, as its caller names it: whose host takes it, the agent it is carried
/// for where that is another, the name its row is rooted on and its unit goes by, what a person watching
/// herdr reads it as, and the program.
pub struct Left<'a> {
  pub agent: &'a str,
  /// The agent the run works for where the host is another's, such as a supervisor's person-book run in the
  /// writer's host, which its row names in the host's place.
  pub for_agent: Option<&'a str>,
  pub name: &'a str,
  pub title: &'a str,
  pub command: Command,
}

// The envelope of a run nobody collects: no owner and no policy, so the host neither answers to a collector
// for it nor reads a launch policy again at its start.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LeftEnvelope<'a> {
  command: &'a Command,
  title: &'a str,
}

/// One run started and left in its agent's host — a runner's coordinator — its
/// execution row rooted on the run's own name, which is what makes the name read as alive while it runs and
/// what a stop of that name reaches. The run closes its own row; what this answers is whether it started,
/// or the code of the reason it did not, with the row closed as a launch that never ran.
pub async fn leave(ground: &Ground, left: Left<'_>) -> Result<(), String> {
  let asked = Asked {
    agent: left.agent,
    line: "coordinator",
    provider: &DEPLOYMENT.agent_key,
    command: left.command,
    root: Some(left.name.to_owned()),
    parent: None,
    role: None,
    model: None,
    effort: None,
    session: None,
  };
  let mut body = first_body(&asked);

  if let Some(served) = left.for_agent {
    body.insert("agent".into(), json!(served));
  }

  let mut row = Row::open(&ground.db, body).await.map_err(|why| why.detail)?;
  let ended = match left_in_host(ground, &asked, left.title, &row).await {
    Ok(()) => {
      row.set("paneId", Value::Null);
      row.set("state", json!("running"));
      Ok(())
    }
    Err(code) => {
      row.set("state", json!("launch-error"));
      row.set("endedAt", json!(crate::now_ms()));
      row.set("exitCode", json!(1));
      row.set("reconciliationEvidence", json!(code));
      Err(code)
    }
  };

  // A run that started closes its own row, and one with nothing to do may have closed it already: running
  // is written only over a row still open (docs/agent-control.md, 「Identity and launch handshake」).
  let saved = if ended.is_ok() {
    row.save_while_open(&ground.db).await.map(drop)
  } else {
    row.save(&ground.db).await
  };

  if let Err(why) = saved {
    eprintln!("{}: the row could not be saved: {}", row.id(), why.detail);
  }

  ended
}

// The run placed in its host and its start waited for, or the code of why it did not start.
async fn left_in_host(ground: &Ground, asked: &Asked<'_>, title: &str, row: &Row) -> Result<(), String> {
  let data = &ground.roots.data;
  let directory = data.join("executions").join(row.id());

  private_dir(&data.join("executions")).map_err(|why| why.to_string())?;
  std::fs::create_dir(&directory).map_err(|why| why.to_string())?;

  let names = [row.id(), row.root()];
  let mut placed = Err(String::new());

  for wait in std::iter::once(None).chain(place_retries().into_iter().map(Some)) {
    if let Some(wait) = wait {
      eprintln!(
        "{}: {} could not be placed ({}); trying again in {}ms",
        asked.agent,
        row.id(),
        placed.as_ref().err().map_or("", String::as_str),
        wait.as_millis()
      );
      tokio::time::sleep(wait).await;
    }

    // The envelope is whole and granted before the pointer is put down: the host may take the pointer the
    // moment it is written.
    let envelope = LeftEnvelope {
      command: &asked.command,
      title,
    };

    publish(&directory.join("command.json"), &envelope).map_err(|why| why.to_string())?;
    publish(&directory.join("grant"), &json!({ "nonce": uuid::Uuid::new_v4().to_string() }))
      .map_err(|why| why.to_string())?;
    placed = match ensure_host(ground, &names).await {
      Ok(held) => point(data, asked.agent, row.id(), &directory)
        .map(|()| held)
        .map_err(|why| format!("host-unavailable: {why}")),
      Err(why) => Err(why),
    };

    if placed.as_ref().is_ok() || placed.as_ref().is_err_and(|why| !why.starts_with("host-unavailable")) {
      break;
    }
  }

  let mut holder = match placed {
    Ok(holder) => holder,
    Err(why) => {
      let _ = std::fs::remove_dir_all(&directory);

      return Err(why.split(':').next().unwrap_or("launch-error").to_owned());
    }
  };
  // The host's start is waited for until it says the run started or how it ended; a host gone meanwhile
  // is waited for again, and only one that can no longer stand ends the wait.
  while !directory.join("started.json").exists() && !directory.join("outcome.json").exists() {
    let cancel = || publish_bytes(&directory.join("cancel"), b"");

    if !holder.alive() {
      holder = match ensure_host(ground, &names).await {
        Ok(held) => held,
        Err(why) => {
          let _ = cancel();
          return Err(why.split(':').next().unwrap_or("host-unavailable").to_owned());
        }
      };
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
  }

  if !directory.join("started.json").exists() {
    let outcome: Option<Outcome> = read_json(&directory.join("outcome.json"));

    return Err(outcome.map_or("launch-error", |outcome| outcome.cause.as_str()).to_owned());
  }

  Ok(())
}

// The run placed in its host and followed to its outcome.
async fn placed_run<H: Heard>(
  ground: &Ground,
  asked: &Asked<'_>,
  row: &mut Row,
  heard: &mut H,
) -> Result<(i32, Cause, String, String), Failure> {
  let data = ground.roots.data.clone();
  let db = &ground.db;
  let title = format!(
    "{} · {}{} · attempt {}",
    asked.line,
    asked.provider,
    asked.model.as_deref().map(|model| format!(" · {model}")).unwrap_or_default(),
    row.body.get("attempt").and_then(Value::as_i64).unwrap_or(1)
  );
  let name = row.id().to_owned();
  let directory = data.join("executions").join(&name);

  if private_dir(&data.join("executions")).is_err() || std::fs::create_dir(&directory).is_err() {
    let failure = Failure::Transport("invalid execution identity".to_owned());

    close_unlaunched(ground, row, &failure).await;
    return Err(failure);
  }

  let mut follow = Follow {
    provider: harness_providers::provider(asked.provider),
    offset: 0,
    views: (Views::new(&Layout::new(&data)), asked.agent),
    heard,
  };
  let mut launch = Launch {
    title,
    directory: directory.clone(),
    placed: false,
  };
  let carried = carry(ground, asked, &mut launch, row, &mut follow).await;
  let placed = launch.placed;
  let collected = carried.is_ok();

  // A run not collected is cancelled, and its supervisor is given until it has published the
  // cancellation or is gone itself.
  if !collected {
    let _ = publish_bytes(&directory.join("cancel"), b"");

    while placed && !directory.join("outcome.json").exists() && supervisor_answers(&directory) {
      tokio::time::sleep(POLL).await;
    }
  }

  // A directory left behind by a placed run that was not collected is the reconciler's to read.
  if collected || !placed {
    let _ = std::fs::remove_dir_all(&directory);
  }

  match carried {
    Ok((outcome, stdout, stderr, stopped_by_caller)) => {
      if outcome.cause == Cause::Cancelled && !stopped_by_caller {
        return Err(Failure::Stopped(format!("execution {}", outcome.cause.as_str())));
      }

      if outcome.cause == Cause::LaunchError {
        return Err(Failure::Transport(format!("execution {}", outcome.cause.as_str())));
      }

      // A run that did not exit on its own ends on a code that says it failed.
      let code = if outcome.cause == Cause::Exited || outcome.exit_code != 0 {
        outcome.exit_code
      } else {
        1
      };

      Ok((code, outcome.cause, stdout, stderr))
    }
    Err(failure) => {
      let stopped = matches!(failure, Failure::Stopped(_));
      let state = row.text("state").unwrap_or_default().to_owned();

      row.set(
        "state",
        json!(if stopped {
          "cancelled"
        } else if state == "launching" {
          "launch-error"
        } else {
          "unknown"
        }),
      );

      if stopped || state == "launching" {
        row.set("endedAt", json!(crate::now_ms()));
      }

      let _ = row.save(db).await;

      Err(failure)
    }
  }
}

// Whether the supervisor that acknowledged the run's start is still alive; one that never acknowledged has
// nothing of the run's to publish.
fn supervisor_answers(directory: &Path) -> bool {
  let started: Option<Started> = read_json(&directory.join("started.json"));

  started.is_some_and(|started| started.supervisor.alive())
}

// What a run is shown as: its title, its envelope's directory, and whether its pointer has been put in the
// host's inbox.
struct Launch {
  title: String,
  directory: PathBuf,
  placed: bool,
}

// The run placed, granted and followed to its outcome: the outcome, what the program wrote, and whether
// its caller stopped it.
async fn carry<H: Heard>(
  ground: &Ground,
  asked: &Asked<'_>,
  launch: &mut Launch,
  row: &mut Row,
  follow: &mut Follow<'_, H>,
) -> Result<(Outcome, String, String, bool), Failure> {
  let data = &ground.roots.data;
  let title = launch.title.as_str();
  let directory = launch.directory.as_path();
  let transport = |why: String| Failure::Transport(why);
  let envelope = || Envelope {
    command: &asked.command,
    title,
    policy: Policy {
      address: crate::archive_place(data).unwrap_or_default(),
      provider: asked.provider,
      names: [Some(row.id().to_owned()), Some(row.root().to_owned()), asked.parent.clone()]
        .into_iter()
        .flatten()
        .filter(|name| !name.is_empty())
        .collect(),
      providers: harness_providers::providers().into_iter().map(|one| one.name()).collect(),
    },
    owner: ProcessIdentity::own(),
  };
  let names = [row.id().to_owned(), row.root().to_owned()];
  let names: Vec<&str> = names.iter().map(String::as_str).collect();
  let mut placed = Err(String::new());

  for wait in std::iter::once(None).chain(place_retries().into_iter().map(Some)) {
    if let Some(wait) = wait {
      eprintln!(
        "{}: {} could not be placed ({}); trying again in {}ms",
        asked.agent,
        row.id(),
        placed.as_ref().err().map_or("", String::as_str),
        wait.as_millis()
      );
      tokio::time::sleep(wait).await;
    }

    publish(&directory.join("command.json"), &envelope()).map_err(|why| transport(why.to_string()))?;
    placed = match ensure_host(ground, &names).await {
      Ok(held) => point(data, asked.agent, row.id(), directory)
        .map(|()| held)
        .map_err(|why| format!("host-unavailable: {why}")),
      Err(why) => Err(why),
    };

    if placed.as_ref().is_ok() || placed.as_ref().is_err_and(|why| !why.starts_with("host-unavailable")) {
      break;
    }
  }

  let mut holder = placed.map_err(|why| {
    if why.starts_with("cancelled") {
      Failure::Stopped(why)
    } else {
      transport(format!("herdr launch failed: {why}"))
    }
  })?;

  launch.placed = true;

  // The checks again, at the moment the run would be granted.
  if let Some(failure) = launch_refusal(ground, row, asked).await {
    return Err(failure);
  }

  publish(&directory.join("grant"), &json!({ "nonce": uuid::Uuid::new_v4().to_string() }))
    .map_err(|why| transport(why.to_string()))?;

  let mut acknowledged = false;
  let mut stopped_by_caller = false;
  let mut last_look = Instant::now();

  while !directory.join("outcome.json").exists() {
    if !acknowledged && directory.join("started.json").exists() {
      acknowledged = true;
      row.set("state", json!("running"));
      let _ = row.save(&ground.db).await;
    }

    if !stopped_by_caller && follow.heard.stopping() {
      stopped_by_caller = true;
      let _ = publish_bytes(&directory.join("cancel"), b"");
    }

    // Stopped before any supervisor holds the run, nothing is waited for: the cancel file stands for the
    // host that takes the pointer, whose supervisor then starts nothing.
    if stopped_by_caller && !acknowledged && !directory.join("supervisor").exists() {
      return Err(Failure::Stopped("execution cancelled before launch".to_owned()));
    }

    follow.look(ground, directory, row, false).await;

    if last_look.elapsed() >= LOOK {
      last_look = Instant::now();

      let started: Option<Started> = read_json(&directory.join("started.json"));
      let answering = started.as_ref().map_or_else(|| holder.alive(), |started| started.supervisor.alive());

      if !answering && !directory.join("outcome.json").exists() {
        // A host gone before taking the pointer leaves it for the next host; one gone after starting the
        // child took the child with it, and that is never an answer.
        if acknowledged || started.is_some() {
          return Err(transport("herdr execution missing".to_owned()));
        }

        holder = ensure_host(ground, &names)
          .await
          .map_err(|why| transport(format!("herdr execution unavailable: {why}")))?;
      }
    }

    tokio::time::sleep(POLL).await;
  }

  follow.look(ground, directory, row, true).await;

  let outcome: Outcome = read_json(&directory.join("outcome.json"))
    .ok_or_else(|| transport("the outcome could not be read".to_owned()))?;
  let stdout = std::fs::read_to_string(directory.join("stdout")).unwrap_or_default();
  let stderr = std::fs::read_to_string(directory.join("stderr")).unwrap_or_default();

  row.set("state", json!(outcome.cause.as_str()));
  row.set("exitCode", json!(outcome.exit_code));
  row.set("endedAt", json!(crate::now_ms()));
  let _ = row.save(&ground.db).await;

  let (views, agent) = &follow.views;
  let _ = views.write(agent, &[ended_line(title, now_ms(), &outcome)]);

  Ok((outcome, stdout, stderr, stopped_by_caller))
}
