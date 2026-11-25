//! The verbs a person, a job or a session runs from a shell through the `bmo-agency` binary: each one's
//! work as a function over agency's store, answering the lines it prints, so a test holds it without the
//! binary. A verb stands on a service of its own process — the store at the data root's address, the
//! configuration the environment names — and tells the running server what it changed over the server's
//! control port.

pub mod agent_status;
pub mod agents;
pub mod borrow;
pub mod claude_batch;
pub mod claude_token;
pub mod dev_servers;
pub mod hand_tokens;
pub mod job_guard;
pub mod live_claude;
pub mod models;
pub mod node_pane;
pub mod overtime;
pub mod person_node;
pub mod process_memory;
pub mod quiet;
pub mod redact_records;
pub mod reload;
pub mod repository;
pub mod run_list;
pub mod run_say;
pub mod run_stop;
pub mod supervisor_follow;
pub mod supervisor_reset;
pub mod turns;
pub mod work_node;
pub mod work_node_hub;
pub mod work_node_isolated;
pub mod work_node_setup;
pub mod work_node_teardown;

use std::fmt;
use std::path::{Path, PathBuf};

use sea_orm::DatabaseConnection;

use crate::Service;
use crate::config::Config;
use crate::places::Ground;

/// What a verb answers: the lines it prints, and the code it ends with where that is not success — a
/// verb that runs a command ends with that command's code.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Answer {
  pub lines: Vec<String>,
  pub code: u8,
}

impl From<Vec<String>> for Answer {
  fn from(lines: Vec<String>) -> Self {
    Self { lines, code: 0 }
  }
}

/// Why a verb did not do what it was asked, in the line it prints before it ends with a failure.
#[derive(Debug)]
pub enum VerbError {
  /// The caller asked for something the verb refuses, said in the verb's own sentence.
  Refused(String),
  /// The verb ended with a code of its own, said in its own sentence.
  Ended { code: u8, said: String },
  /// The verb did part of what it was asked: the lines it prints of that, and the refusal of the rest.
  Partly { lines: Vec<String>, refusal: String },
  /// The store could not be read or written.
  Store(crate::Refusal),
}

impl fmt::Display for VerbError {
  fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Refused(said) => out.write_str(said),
      Self::Ended { said, .. } => out.write_str(said),
      Self::Partly { refusal, .. } => out.write_str(refusal),
      Self::Store(refusal) => out.write_str(&refusal.detail),
    }
  }
}

impl std::error::Error for VerbError {}

impl From<crate::Refusal> for VerbError {
  fn from(refusal: crate::Refusal) -> Self {
    Self::Store(refusal)
  }
}

impl From<sea_orm::DbErr> for VerbError {
  fn from(error: sea_orm::DbErr) -> Self {
    Self::Store(error.into())
  }
}

impl From<store::Fault> for VerbError {
  fn from(fault: store::Fault) -> Self {
    Self::Store(fault.into())
  }
}

impl From<crate::control::Refused> for VerbError {
  fn from(refused: crate::control::Refused) -> Self {
    Self::Refused(refused.error)
  }
}

/// A number written with `places` digits after the point, rounded on the number's exact value, a half
/// going up and away from nought, where the formatter's own rounding takes a half to the even digit.
pub fn fixed(value: f64, places: usize) -> String {
  // A double's exact decimal expansion never runs past this many digits after the point.
  let exact = format!("{:.1100}", value.abs());
  let (whole, fraction) = exact.split_once('.').unwrap_or((&exact, ""));
  let mut digits: Vec<u8> = format!("{whole}{}", &fraction[..places]).into_bytes();

  if fraction.as_bytes()[places] >= b'5' {
    let mut at = digits.len();

    loop {
      if at == 0 {
        digits.insert(0, b'1');
        break;
      }

      at -= 1;

      if digits[at] == b'9' {
        digits[at] = b'0';
      } else {
        digits[at] += 1;
        break;
      }
    }
  }

  let digits = String::from_utf8(digits).unwrap_or_default();
  let (whole, fraction) = digits.split_at(digits.len() - places);
  let sign = if value < 0.0 { "-" } else { "" };

  if places == 0 { format!("{sign}{whole}") } else { format!("{sign}{whole}.{fraction}") }
}

/// The home the verbs' own places stand under.
pub fn home() -> PathBuf {
  std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

/// The data root a verb stands on: the one `BMO_DATA_DIR` names, or this machine's.
pub fn data_root() -> PathBuf {
  match std::env::var_os("BMO_DATA_DIR") {
    Some(named) if !named.is_empty() => PathBuf::from(named),
    _ => home().join(".local/share/bmo/data"),
  }
}

/// The deploy tree a verb reads the code out of: the one `BMO_DEPLOY_DIR` names — a job's own release
/// among them — or else the configured code tree, or else this machine's deploy tree's link.
pub fn deploy_root() -> PathBuf {
  match std::env::var_os(crate::places::DEPLOY_VARIABLE) {
    Some(named) if !named.is_empty() => PathBuf::from(named),
    _ => Config::read(|name| std::env::var(name).ok())
      .ok()
      .and_then(|config| config.trees.code.clone())
      .unwrap_or_else(deploy_link),
  }
}

/// The deploy tree's link, the stable path every unit names: the configured one, else agency's own.
pub fn deploy_link() -> PathBuf {
  Config::read(|name| std::env::var(name).ok())
    .ok()
    .map(|config| config.trees)
    .unwrap_or_default()
    .deploy_link(&home())
}

/// The tree a verb stands in: the one its binary was placed in, where it holds agency's prompts, and the
/// directory it runs in otherwise.
pub fn code_tree() -> PathBuf {
  std::env::current_exe()
    .ok()
    .and_then(|binary| binary.parent()?.parent().map(Path::to_path_buf))
    .filter(|tree| tree.join("prompts").is_dir())
    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

/// One of the variables of the run a process works for, where an agent's place hands it the run's own
/// (docs/agent-ownership.md); none where it is unset or empty.
pub fn turn_variable(name: &str) -> Option<String> {
  crate::door::place_run::turn_environment(&std::env::vars().collect())
    .remove(name)
    .map(|value| value.trim().to_owned())
    .filter(|value| !value.is_empty())
}

/// Agency's store at the data root's address. A data root that names no store is refused.
pub async fn store(data_root: &Path) -> Result<DatabaseConnection, VerbError> {
  let Some(place) = crate::archive_place(data_root) else {
    return Err(VerbError::Refused(format!(
      "no store is named for {}: set BMO_ARCHIVE_URL, or name it in env/agency.env or env/archive-url",
      data_root.display()
    )));
  };

  store_at(&place).await
}

/// Agency's store at an address a verb was handed.
pub async fn store_at(place: &str) -> Result<DatabaseConnection, VerbError> {
  store::connect(place, store::pool(1))
    .await
    .map_err(|why| VerbError::Refused(format!("agency's store could not be reached: {why}")))
}

// The streams of every event the services a verb's process stood published, read once the verb is done.
static PUBLISHED: std::sync::Mutex<Vec<crate::events::WireSubscription>> =
  std::sync::Mutex::new(Vec::new());

/// The service a verb's process stands on: the store at the data root's address and the configuration the
/// environment names. What its bus publishes is relayed to the server once the verb is done
/// ([`relay_events`]).
pub async fn service(data_root: &Path) -> Result<Service, VerbError> {
  let db = store(data_root).await?;
  let config = Config::read(|name| std::env::var(name).ok()).map_err(VerbError::Refused)?;
  let service = Service::new(db, config);

  PUBLISHED
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .push(service.events.subscribe_wire(None));

  Ok(service)
}

/// The node service a node verb asks (docs/work-nodes.md, 「The workplace on a node」): the one this process's
/// environment names, else the one the data root's `env/agency.env` names, the file the links' units run on, so a
/// verb run from a shell reaches the node service those units reach. None is asked where neither names one.
pub fn node_service(
  data_root: &Path,
  service: &Service,
) -> std::sync::Arc<dyn crate::peers::node::NodeService> {
  let address = &crate::peers::node::ADDRESS;

  if std::env::var(address.url).is_ok_and(|url| !url.trim().is_empty()) {
    return service.peers.node.clone();
  }

  match crate::peers::PeerAddress::read(address, |name| deployment_setting(data_root, name)) {
    Ok(named) => crate::peers::node::client(named),
    Err(why) => {
      eprintln!("agency: the node service's address in env/agency.env did not read: {why}");
      service.peers.node.clone()
    }
  }
}

/// Every event this verb's process published told to the running server, which publishes each on its own
/// streams (docs/api.md, 「Events」): what a verb publishes lands on its own process's bus, which no consumer
/// reads. The verb is never failed by it; an event that did not land is said once on the error stream.
pub async fn relay_events() {
  let streams = std::mem::take(&mut *PUBLISHED.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
  let mut published = vec![];

  for mut stream in streams {
    while let Some(event) = stream.ready() {
      published.push(event);
    }
  }

  if published.is_empty() {
    return;
  }

  let Some(token) = control_token() else {
    eprintln!(
      "agency was not told of {} events this verb published — {CONTROL_TOKEN} is not set in this process",
      published.len()
    );
    return;
  };

  for (at, event) in published.iter().enumerate() {
    let body = serde_json::json!({ "kind": event.name, "data": event.body });
    let missed = match control_call(&token, reqwest::Method::POST, "/events", Some(&body)).await {
      Ok((status, _)) if (200..300).contains(&status) => None,
      Ok((status, _)) => Some(format!("the server answered {status}")),
      Err(why) => Some(format!("no server answered on port {} ({why})", control_port())),
    };

    if let Some(missed) = missed {
      eprintln!(
        "agency was not told of {} of the events this verb published, {} first — {missed}",
        published.len() - at,
        event.name
      );
      return;
    }
  }
}

/// The ground a verb's process stands on, on the data root it was given.
pub async fn ground(data_root: &Path) -> Result<Ground, VerbError> {
  Ok(Ground::of_data_root(&service(data_root).await?, data_root))
}

/// The variable naming the port agency's server listens on, and the one holding its token.
pub const CONTROL_PORT: &str = "AGENCY_CONTROL_PORT";
pub const CONTROL_TOKEN: &str = "AGENCY_CONTROL_TOKEN";

/// The port agency's server listens on where the environment names none.
pub const DEFAULT_CONTROL_PORT: u16 = 3220;

/// The port agency's server listens on: the one the environment names, else the one the deployment's
/// `env/agency.env` under the data root names — a job's own program is handed neither, and finds the server
/// there (docs/api.md, 「The control port」) — or else its own.
pub fn control_port() -> u16 {
  control_port_in(&data_root())
}

/// The port agency's server listens on, the deployment's settings read under `data_root`.
pub fn control_port_in(data_root: &Path) -> u16 {
  std::env::var(CONTROL_PORT)
    .ok()
    .filter(|port| !port.trim().is_empty())
    .or_else(|| deployment_setting(data_root, CONTROL_PORT))
    .and_then(|port| port.trim().parse::<u16>().ok())
    .unwrap_or(DEFAULT_CONTROL_PORT)
}

/// A value the deployment's `env/agency.env` under `data_root` names, none where the file or the value is
/// missing.
pub fn deployment_setting(data_root: &Path, name: &str) -> Option<String> {
  dotenvy::from_path_iter(data_root.join("env").join("agency.env"))
    .ok()?
    .filter_map(Result::ok)
    .find(|(key, _)| key == name)
    .map(|(_, value)| value.trim().to_owned())
    .filter(|value| !value.is_empty())
}

/// The server's token, where this process holds one: by value, or in the file `AGENCY_CONTROL_TOKEN_FILE`
/// names, as a program agency starts is handed it.
pub fn control_token() -> Option<String> {
  crate::places::token_or_file(
    std::env::var(CONTROL_TOKEN).ok(),
    std::env::var(crate::places::file_variable(CONTROL_TOKEN)).ok(),
  )
}

/// One call to agency's server under its token: the status and the JSON it answered with, none where the
/// answer did not read as JSON; the transport's own fault where the connection was refused or broken. The
/// call waits for the server's answer however long it takes.
pub async fn control_call(
  token: &str,
  method: reqwest::Method,
  path: &str,
  body: Option<&serde_json::Value>,
) -> Result<(u16, Option<serde_json::Value>), reqwest::Error> {
  let mut call = reqwest::Client::new()
    .request(method, format!("http://127.0.0.1:{}{path}", control_port()))
    .bearer_auth(token);

  if let Some(body) = body {
    call = call.json(body);
  }

  let answer = call.send().await?;
  let status = answer.status().as_u16();

  Ok((status, answer.json().await.ok()))
}

/// One word to the running server, as a verb outside it tells it what it changed: a POST of `body` to
/// `path`. The caller is never failed by it: what it wrote stands whether the server hears or not, but a
/// word that did not land says so once on the error stream.
pub async fn tell_server(path: &str, body: serde_json::Value) {
  let missed = match control_token() {
    None => Some(format!("{CONTROL_TOKEN} is not set in this process")),
    Some(token) => match control_call(&token, reqwest::Method::POST, path, Some(&body)).await {
      Ok((status, _)) if (200..300).contains(&status) => None,
      Ok((status, _)) => Some(format!("the server answered {status}")),
      Err(why) => Some(format!("no server answered on port {} ({why})", control_port())),
    },
  };

  if let Some(missed) = missed {
    eprintln!("agency was not told of {path} — {missed}; it catches up on its next pass");
  }
}

/// The two ways a run is ended from outside, heard while it works.
pub struct Stop {
  terminate: tokio::signal::unix::Signal,
  interrupt: tokio::signal::unix::Signal,
}

impl Stop {
  pub fn listen() -> Result<Self, VerbError> {
    use tokio::signal::unix::{SignalKind, signal};

    let heard =
      |kind| signal(kind).map_err(|why| VerbError::Refused(format!("the stop could not be heard: {why}")));

    Ok(Self {
      terminate: heard(SignalKind::terminate())?,
      interrupt: heard(SignalKind::interrupt())?,
    })
  }

  /// The name of the signal that came.
  pub async fn next(&mut self) -> &'static str {
    tokio::select! {
      _ = self.terminate.recv() => "SIGTERM",
      _ = self.interrupt.recv() => "SIGINT",
    }
  }
}
