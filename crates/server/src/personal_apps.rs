//! A person's personal apps put, sent back, removed and listed (docs/person-nodes.md, 「Personal apps」): the runner
//! the account's records name asked over its passage to place, describe, point at, list and take an app's versions
//! on the person's PC, and the tool host's registry of the account's personal tools written through `bmo-pocket`, so
//! the description the PC built stands at home under the PC's own version. Who may put one, and the ceiling of what
//! one may ask for, is the delegation chain's.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use axum::http::StatusCode;
use bmo_pocket_client::Runner;
use policy::organisation::ORGANISATION;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use crate::chain::{self, Subject};
use crate::control::Refused;
use crate::places::Ground;

/// The variable naming the `bmo-pocket` binary the registry is written through; `bmo-pocket` on the path where it
/// names none.
pub const POCKET_VARIABLE: &str = "BMO_POCKET_BIN";

/// How long a listing waits for the PC's versions before it answers without them.
const LISTING_WAIT: Duration = Duration::from_secs(15);

/// The first run word of the manifest the runner makes for an MCP server: its own binary, bound there inside the
/// confinement (docs/person-nodes.md, 「MCP servers」).
pub const MCP_RUN: &str = "/.bmo/node-link";

/// Where a put stages the description it hands the tool host, under the data root.
const STAGING: &str = "agency/personal-apps";

/// Why an app was not put, sent back, removed or listed.
#[derive(Debug, PartialEq, Eq)]
pub enum Fault {
  BadName(String),
  NoRunner(String),
  Unreached(String),
  Refused(String),
  Withheld,
  PermissionWithheld(String),
  Unchecked,
  NoOlder(String),
  NotFound(String),
  SecretMissing(String),
}

impl From<Fault> for Refused {
  fn from(fault: Fault) -> Self {
    match fault {
      Fault::BadName(name) => Refused::new(
        StatusCode::BAD_REQUEST,
        format!("{name:?} is no app's name: a lower-case letter, then lower-case letters, digits and `_`"),
      )
      .with_field("name"),
      Fault::NoRunner(account) => Refused::new(
        StatusCode::CONFLICT,
        format!(
          "the account {account} has no PC its personal apps run on, or shares one with other accounts"
        ),
      )
      .with_code("no-runner"),
      Fault::Unreached(why) => {
        Refused::new(StatusCode::BAD_GATEWAY, format!("the PC could not be reached: {why}"))
          .with_code("runner-unreached")
      }
      Fault::Refused(why) => Refused::new(StatusCode::UNPROCESSABLE_ENTITY, why).with_code("app-refused"),
      Fault::Withheld => Refused::new(
        StatusCode::FORBIDDEN,
        "The person's organisation does not let them make personal apps",
      )
      .with_code("personal-apps-withheld"),
      Fault::PermissionWithheld(permission) => Refused::new(
        StatusCode::FORBIDDEN,
        format!("The person's organisation does not let a personal app ask for {permission}"),
      )
      .with_code("permission-withheld"),
      Fault::Unchecked => Refused::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "The account service could not be asked whether the person may make personal apps",
      )
      .with_code("access-unchecked"),
      Fault::NoOlder(name) => Refused::new(
        StatusCode::CONFLICT,
        format!("the PC keeps no version of the app {name} older than the one in place"),
      )
      .with_code("no-older-version"),
      Fault::NotFound(what) => Refused::new(StatusCode::NOT_FOUND, what),
      Fault::SecretMissing(name) => Refused::new(
        StatusCode::CONFLICT,
        format!("the person's secret space holds no secret {name}, which the MCP server asks for"),
      )
      .with_code("secret-missing"),
    }
  }
}

/// What a put, a going back or a removal came to: the version now in place, none after a removal, the entries the
/// account's sessions reach, and what the tool host and the build said.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
  pub name: String,
  pub version: Option<String>,
  pub entries: Vec<Value>,
  pub said: Vec<String>,
}

impl Outcome {
  pub fn answer(&self) -> Value {
    json!({ "name": self.name, "version": self.version, "entries": self.entries, "said": self.said })
  }
}

/// An app's name: a lower-case letter, then lower-case letters, digits and `_`, which is its tool's and its entry's.
pub fn app_name(name: &str) -> bool {
  name.chars().next().is_some_and(|first| first.is_ascii_lowercase())
    && name.chars().all(|one| one.is_ascii_lowercase() || one.is_ascii_digit() || one == '_')
}

fn named(name: &str) -> Result<(), Fault> {
  if app_name(name) { Ok(()) } else { Err(Fault::BadName(name.to_owned())) }
}

// One put, going back or removal of an app at a time, so the PC's pointer and the host's never cross.
static HELD: LazyLock<std::sync::Mutex<BTreeMap<String, Arc<tokio::sync::Mutex<()>>>>> =
  LazyLock::new(Default::default);

pub fn hold(account: &str, name: &str) -> Arc<tokio::sync::Mutex<()>> {
  let mut held = HELD.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

  held.entry(format!("{account}/{name}")).or_default().clone()
}

/// What the runner answered: its last line, and each line a build said before it.
pub struct Answered {
  pub last: Value,
  pub said: Vec<String>,
}

/// Whether the chain lets the person make personal apps of their own.
pub async fn may_make(ground: &Ground, account: &str) -> Result<(), Fault> {
  may_put(ground, account, None, true).await
}

// One asking of the runner over its passage: the line sent and kept open, every `said` line read, and the first
// other line its answer. The passage closing unanswered, or a runner that cannot be reached, is unreached.
pub async fn ask(runner: &Runner, line: &Value) -> Result<Answered, Fault> {
  let mut asking = line.to_string();

  asking.push('\n');

  match runner {
    Runner::Socket(path) => {
      let stream = tokio::net::UnixStream::connect(path)
        .await
        .map_err(|why| Fault::Unreached(format!("{}: {why}", path.display())))?;
      let (reading, mut writing) = stream.into_split();

      read_answer(&mut writing, BufReader::new(reading), &asking).await
    }
    Runner::Command(words) => {
      let Some((program, rest)) = words.split_first() else {
        return Err(Fault::Unreached("the runner's command names nothing".to_owned()));
      };
      let mut child = tokio::process::Command::new(program)
        .args(rest)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|why| Fault::Unreached(format!("{program}: {why}")))?;
      let (Some(mut input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
        return Err(Fault::Unreached("the runner's passage could not be opened".to_owned()));
      };
      let answered = read_answer(&mut input, BufReader::new(output), &asking).await;

      drop(input);
      let _ = child.wait().await;
      answered
    }
  }
}

async fn read_answer(
  writing: &mut (impl AsyncWrite + Unpin),
  reading: impl tokio::io::AsyncBufRead + Unpin,
  asking: &str,
) -> Result<Answered, Fault> {
  writing
    .write_all(asking.as_bytes())
    .await
    .map_err(|why| Fault::Unreached(format!("the passage closed: {why}")))?;
  let _ = writing.flush().await;

  let mut lines = reading.lines();
  let mut said = Vec::new();

  while let Ok(Some(line)) = lines.next_line().await {
    let Ok(read) = serde_json::from_str::<Value>(&line) else { continue };

    match read["t"].as_str() {
      Some("said") => said.push(read["line"].as_str().unwrap_or_default().to_owned()),
      Some("refused") => return Err(Fault::Refused(read["why"].as_str().unwrap_or("refused").to_owned())),
      Some(_) => return Ok(Answered { last: read, said }),
      None => {}
    }
  }

  Err(Fault::Unreached("the passage ended unanswered".to_owned()))
}

// The answer of the kind asked for, else the runner's line said as a refusal.
pub fn expect(answered: &Answered, kind: &str) -> Result<(), Fault> {
  if answered.last["t"] == kind {
    Ok(())
  } else {
    Err(Fault::Refused(format!("the PC answered {} where {kind} was asked", answered.last)))
  }
}

/// A short-lived tool session a console reads and calls the account's personal screens under (docs/person-nodes.md,
/// 「Personal screens」): its token and when it lapses.
pub async fn screen_session(ground: &Ground, account: &str) -> Result<Value, Refused> {
  match crate::lines::token::issue_screen_session(ground, account).await {
    Ok(Some((token, expires_at))) => Ok(json!({ "token": token, "expiresAt": expires_at })),
    Ok(None) => Err(Fault::NoRunner(account.to_owned()).into()),
    Err(refusal) => Err(
      Refused::new(
        StatusCode::SERVICE_UNAVAILABLE,
        format!("the tool host did not issue it: {}", refusal.detail),
      )
      .with_code("screen-session-unissued"),
    ),
  }
}

/// The runner the account's apps are kept and run by, or the refusal saying it has none.
pub async fn runner(ground: &Ground, account: &str) -> Result<Runner, Fault> {
  crate::lines::token::runner_of(ground, account)
    .await
    .map_err(|refusal| Fault::Unreached(refusal.detail))?
    .ok_or_else(|| Fault::NoRunner(account.to_owned()))
}

// Whether the chain lets the person put a personal app of their own (`make`), and whether it lets one ask for each
// permission its manifest names; the administrator's account is held to none of it. A received app is held to the
// permissions alone.
async fn may_put(ground: &Ground, account: &str, manifest: Option<&str>, make: bool) -> Result<(), Fault> {
  if crate::account_isolation::owners_own(&ground.db, account)
    .await
    .map_err(|_| Fault::Unchecked)?
  {
    return Ok(());
  }

  let person = Subject::Person(account.parse().map_err(|_| Fault::Unchecked)?);
  let service = crate::apps::service(ground);
  let settings = &ORGANISATION.personal_apps;
  let flag = |key: &'static str, fallback: bool| {
    let service = service.clone();

    async move {
      chain::resolve(&service, key, person)
        .await
        .map(|answer| chain::flag(answer.as_ref(), fallback))
        .map_err(|_| Fault::Unchecked)
    }
  };

  if make && !flag(settings.make.key.as_str(), true).await? {
    return Err(Fault::Withheld);
  }

  let Some(manifest) = manifest else { return Ok(()) };
  let read: toml::Table = manifest
    .parse()
    .map_err(|why| Fault::Refused(format!("its tool.toml does not read: {why}")))?;
  let asks = |permission: &str| {
    read
      .get("permissions")
      .and_then(|permissions| permissions.get(permission))
      .and_then(toml::Value::as_array)
      .is_some_and(|named| !named.is_empty())
  };

  may_ask(ground, account, asks("network"), asks("secrets")).await
}

// Whether the chain lets a personal app of the person's ask for the network and for secrets, where it asks for them;
// the administrator's account is held to neither.
async fn may_ask(ground: &Ground, account: &str, network: bool, secrets: bool) -> Result<(), Fault> {
  if !network && !secrets
    || crate::account_isolation::owners_own(&ground.db, account)
      .await
      .map_err(|_| Fault::Unchecked)?
  {
    return Ok(());
  }

  let person = Subject::Person(account.parse().map_err(|_| Fault::Unchecked)?);
  let service = crate::apps::service(ground);
  let settings = &ORGANISATION.personal_apps;

  for (asked, said, key) in [
    (network, "the network", settings.network.key.as_str()),
    (secrets, "secrets", settings.secrets.key.as_str()),
  ] {
    if !asked {
      continue;
    }

    let allowed = chain::resolve(&service, key, person)
      .await
      .map(|answer| chain::flag(answer.as_ref(), false))
      .map_err(|_| Fault::Unchecked)?;

    if !allowed {
      return Err(Fault::PermissionWithheld(said.to_owned()));
    }
  }

  Ok(())
}

// A secret's name: a plain file name of letters, digits, `.`, `_` and `-`, opening on no dot.
fn secret_name(name: &str) -> bool {
  !name.is_empty()
    && !name.starts_with('.')
    && name.chars().all(|one| one.is_ascii_alphanumeric() || matches!(one, '.' | '_' | '-'))
}

/// The secrets a place of an MCP server carries (docs/person-nodes.md, 「MCP servers」): each read from the
/// account's secret space as the tool host's relay reads it, its bytes in base64 by its name; one the space holds no
/// file of is refused, naming it and never what it holds.
pub async fn read_secrets(
  ground: &Ground,
  account: &str,
  names: &[String],
) -> Result<BTreeMap<String, String>, Fault> {
  use base64::Engine;

  let space = crate::lines::token::secret_space_of(ground, account)
    .await
    .map_err(|refusal| Fault::Refused(refusal.detail))?;
  let mut read = BTreeMap::new();

  for name in names {
    if !secret_name(name) {
      return Err(Fault::Refused(format!("{name:?} is no secret's name")));
    }

    let bytes = std::fs::read(space.join(name)).map_err(|_| Fault::SecretMissing(name.clone()))?;

    read.insert(name.clone(), base64::engine::general_purpose::STANDARD.encode(bytes));
  }

  Ok(read)
}

// What a place carries for an MCP server: the addresses, and the secrets read, once the chain allows them.
async fn carried(
  ground: &Ground,
  account: &str,
  network: &[String],
  secrets: &[String],
) -> Result<Value, Fault> {
  may_ask(ground, account, !network.is_empty(), !secrets.is_empty()).await?;

  Ok(json!({ "network": network, "secrets": read_secrets(ground, account, secrets).await? }))
}

fn strings(value: &Value) -> Vec<String> {
  value
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .map(str::to_owned)
    .collect()
}

// The app's folder placed on the PC: a place carrying what `asks` names where it names anything, asked once more
// carrying what the runner wants where it answers `wants` (docs/person-nodes.md, 「MCP servers」).
async fn place(
  ground: &Ground,
  runner: &Runner,
  account: &str,
  name: &str,
  asks: Option<(&[String], &[String])>,
) -> Result<Answered, Fault> {
  let mut carrying = match asks {
    Some((network, secrets)) if !network.is_empty() || !secrets.is_empty() => {
      Some(carried(ground, account, network, secrets).await?)
    }
    _ => None,
  };

  for _ in 0..2 {
    let mut asked = json!({ "tool": name });

    if let Some(carrying) = &carrying {
      asked["network"] = carrying["network"].clone();
      asked["secrets"] = carrying["secrets"].clone();
    }

    let placed = ask(runner, &json!({ "owner": account, "place": asked })).await?;

    if placed.last["t"] == "wants" && carrying.is_none() {
      let (network, secrets) = (strings(&placed.last["network"]), strings(&placed.last["secrets"]));

      carrying = Some(carried(ground, account, &network, &secrets).await?);
      continue;
    }

    expect(&placed, "placed")?;
    return Ok(placed);
  }

  Err(Fault::Refused(format!("the PC wants more than the place of {name} carries")))
}

/// Whether a manifest the runner answered is an MCP server's, the one it makes: its run words open on the runner's
/// own binary's `mcp` (docs/person-nodes.md, 「MCP servers」).
pub fn is_mcp(manifest: &str) -> bool {
  manifest.parse::<toml::Table>().is_ok_and(|read| {
    let run: Vec<&str> = read
      .get("tool")
      .and_then(|tool| tool.get("run"))
      .and_then(toml::Value::as_array)
      .map(|words| words.iter().filter_map(toml::Value::as_str).collect())
      .unwrap_or_default();

    run.first() == Some(&MCP_RUN) && run.get(1) == Some(&"mcp")
  })
}

// `bmo-pocket` run on the data root with the words given: what it said, or its refusal.
pub async fn pocket(ground: &Ground, words: &[&str]) -> Result<String, Fault> {
  let program = ground.vars().get(POCKET_VARIABLE).cloned().unwrap_or_else(|| "bmo-pocket".to_owned());
  let ran = tokio::process::Command::new(&program)
    .arg("--data-root")
    .arg(&ground.roots.data)
    .args(words)
    .stdin(Stdio::null())
    .output()
    .await
    .map_err(|why| {
      Fault::Refused(format!("the tool host's registry could not be written: {program}: {why}"))
    })?;

  if ran.status.success() {
    return Ok(String::from_utf8_lossy(&ran.stdout).into_owned());
  }

  let why = String::from_utf8_lossy(&ran.stderr);

  Err(Fault::Refused(why.trim().trim_start_matches("bmo-pocket: ").to_owned()))
}

/// The account's personal tools as the tool host holds them.
pub async fn listed_tools(ground: &Ground, account: &str) -> Result<Vec<Value>, Fault> {
  let said = pocket(ground, &["list", "--owner", account, "--json"]).await?;

  serde_json::from_str(&said)
    .map_err(|why| Fault::Refused(format!("the tool host's listing does not read: {why}")))
}

// The entries of the account's app as the tool host holds it.
async fn entries(ground: &Ground, account: &str, name: &str) -> Vec<Value> {
  listed_tools(ground, account)
    .await
    .unwrap_or_default()
    .into_iter()
    .find(|tool| tool["name"] == name)
    .and_then(|tool| tool["entries"].as_array().cloned())
    .unwrap_or_default()
}

// A description written into a folder of its own under the data root, for the tool host to put; taken away after.
struct Staged(PathBuf);

impl Drop for Staged {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.0);
  }
}

fn stage(data: &Path, name: &str, files: &Value) -> Result<Staged, Fault> {
  let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
  let dir = data.join(STAGING).join(format!("{name}-{}-{}", now.as_nanos(), std::process::id()));
  let staged = Staged(dir.clone());
  let fail = |why: std::io::Error| Fault::Refused(format!("the description could not be staged: {why}"));

  for (path, text) in files.as_object().into_iter().flatten() {
    let inside = !path.is_empty()
      && Path::new(path).components().all(|part| matches!(part, std::path::Component::Normal(_)));

    if !inside {
      return Err(Fault::Refused(format!("the PC answered a file {path:?} outside the app's folder")));
    }

    let to = dir.join(path);

    if let Some(parent) = to.parent() {
      std::fs::create_dir_all(parent).map_err(fail)?;
    }

    std::fs::write(&to, text.as_str().unwrap_or_default()).map_err(fail)?;
  }

  Ok(staged)
}

// A description the PC answered put at home under its version.
async fn put_home(
  ground: &Ground,
  account: &str,
  (name, app): (&str, Option<&str>),
  version: &str,
  files: &Value,
) -> Result<String, Fault> {
  let staged = stage(&ground.roots.data, name, files)?;
  let folder = staged.0.to_string_lossy().into_owned();
  let mut words = vec!["put", "--owner", account, "--version", version];

  if let Some(app) = app {
    words.extend(["--app", app]);
  }

  words.push(&folder);
  pocket(ground, &words).await
}

fn lines(said: &str) -> impl Iterator<Item = String> + '_ {
  said.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_owned)
}

/// The app's folder on the account's PC built into a new version, its description put at home under that version,
/// and the PC pointed at it. A refusal at home leaves the PC's pointer where it stood.
pub async fn put(ground: &Ground, account: &str, name: &str) -> Result<Outcome, Fault> {
  named(name)?;
  may_put(ground, account, None, true).await?;

  let runner = runner(ground, account).await?;
  let held = hold(account, name);
  let _held = held.lock().await;

  put_held(ground, &runner, account, (name, None), None).await
}

/// A received copy of a shared app put as a personal app standing for the shared app's key (docs/person-nodes.md,
/// 「Shared apps」): its source already in its folder, built, put at home with the key and pointed at; held to the
/// permissions the chain allows a personal app, not to whether the person may make one. The caller holds the app.
/// `asks` names the network and the secrets the shared version asks for, which the place of an MCP server carries.
pub async fn put_received(
  ground: &Ground,
  runner: &Runner,
  account: &str,
  name: &str,
  key: Option<&str>,
  asks: Option<(&[String], &[String])>,
) -> Result<Outcome, Fault> {
  named(name)?;
  put_held(ground, runner, account, (name, key), asks).await
}

// A put under the app's hold: the PC places the folder, the chain judges its manifest, the description is put at
// home under the PC's version, naming the shared app where it is a copy of one and the person's own key where it is
// an MCP server of theirs, and the PC is pointed at it.
async fn put_held(
  ground: &Ground,
  runner: &Runner,
  account: &str,
  (name, app): (&str, Option<&str>),
  asks: Option<(&[String], &[String])>,
) -> Result<Outcome, Fault> {
  let runner = runner.clone();
  let placed = place(ground, &runner, account, name, asks).await?;
  let version = placed.last["version"].as_str().unwrap_or_default().to_owned();
  let files = &placed.last["files"];
  let manifest = files["tool.toml"].as_str();

  may_put(ground, account, manifest, false).await?;

  let own =
    (app.is_none() && manifest.is_some_and(is_mcp)).then(|| crate::attachments::own_key(account, name));
  let key = app.or(own.as_deref());

  keep_server(ground, account, name, key, manifest).await?;

  let home = put_home(ground, account, (name, key), &version, files).await?;
  let pointed =
    ask(&runner, &json!({ "owner": account, "point": { "tool": name, "version": version } })).await?;

  expect(&pointed, "pointed")?;

  Ok(Outcome {
    name: name.to_owned(),
    version: Some(version),
    entries: entries(ground, account, name).await,
    said: placed.said.into_iter().chain(lines(&home)).collect(),
  })
}

// Agency's record of the account's MCP servers kept as one is put at home (docs/person-nodes.md, 「MCP servers」):
// an MCP server's row written before the put, which is refused where it cannot be, so its key is withheld from the
// moment it can stand; an app's taking any row of its name away, a row left where that fails withholding a key no
// server stands under.
async fn keep_server(
  ground: &Ground,
  account: &str,
  name: &str,
  key: Option<&str>,
  manifest: Option<&str>,
) -> Result<(), Fault> {
  match key.filter(|_| manifest.is_some_and(is_mcp)) {
    Some(key) => crate::attachments::record_server(&ground.db, account, name, key).await.map_err(|why| {
      Fault::Refused(format!("the record of the account's MCP servers was not written: {}", why.detail))
    }),
    None => {
      if let Err(why) = crate::attachments::forget_server(&ground.db, account, name).await {
        eprintln!("the record of {account}'s {name} was not taken away: {}", why.detail);
      }

      Ok(())
    }
  }
}

/// The app sent back to a version its PC keeps: the one named, else the newest older than the one in place; its
/// description put at home under it, and the PC pointed at it.
pub async fn back(
  ground: &Ground,
  account: &str,
  name: &str,
  version: Option<&str>,
) -> Result<Outcome, Fault> {
  named(name)?;

  let runner = runner(ground, account).await?;
  let held = hold(account, name);
  let _held = held.lock().await;
  let kept = ask(&runner, &json!({ "owner": account, "versions": {} })).await?;

  expect(&kept, "versions")?;

  let Some(tool) = kept.last["tools"].get(name) else {
    return Err(Fault::NotFound(format!("the account {account} keeps no app {name}")));
  };
  let older: Vec<String> = tool["kept"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .map(str::to_owned)
    .collect();
  let target = match version {
    Some(version) if older.iter().any(|one| one == version) || tool["current"] == version => {
      version.to_owned()
    }
    Some(version) => {
      return Err(Fault::NotFound(format!("the PC keeps no version {version} of the app {name}")));
    }
    None => older.first().cloned().ok_or_else(|| Fault::NoOlder(name.to_owned()))?,
  };
  let described =
    ask(&runner, &json!({ "owner": account, "describe": { "tool": name, "version": target } })).await?;

  expect(&described, "described")?;

  let listed = listed_tools(ground, account)
    .await
    .unwrap_or_default()
    .into_iter()
    .find(|tool| tool["name"] == name)
    .and_then(|tool| tool["app"].as_str().map(str::to_owned));
  // A server's key stands in agency's record where the tool host's listing does not read.
  let app = match listed {
    Some(app) => Some(app),
    None => crate::attachments::recorded_servers(&ground.db, account)
      .await
      .map_err(|why| {
        Fault::Refused(format!("the record of the account's MCP servers was not read: {}", why.detail))
      })?
      .remove(name),
  };
  let files = &described.last["files"];

  keep_server(ground, account, name, app.as_deref(), files["tool.toml"].as_str()).await?;

  let home = put_home(ground, account, (name, app.as_deref()), &target, files).await?;
  let pointed =
    ask(&runner, &json!({ "owner": account, "point": { "tool": name, "version": target } })).await?;

  expect(&pointed, "pointed")?;

  Ok(Outcome {
    name: name.to_owned(),
    version: Some(target),
    entries: entries(ground, account, name).await,
    said: lines(&home).collect(),
  })
}

/// The app taken out at home first, so no session reaches it from its next call, then on the PC; a PC that cannot
/// be reached keeps its versions, which no call reaches any more.
pub async fn remove(ground: &Ground, account: &str, name: &str) -> Result<Outcome, Fault> {
  named(name)?;

  let held = hold(account, name);
  let _held = held.lock().await;
  let home = pocket(ground, &["take", "--owner", account, name]).await;
  let mut said: Vec<String> = home.as_deref().map(|said| lines(said).collect()).unwrap_or_default();

  if home.is_ok() {
    if let Err(why) = crate::attachments::forget_item(&ground.db, account, "mcp", name).await {
      eprintln!("the attachments of {account}'s {name} were not taken away: {}", why.detail);
    }

    // A row left standing withholds a key no tool stands under, until the tool host's listing takes it away.
    if let Err(why) = crate::attachments::forget_server(&ground.db, account, name).await {
      eprintln!("the record of {account}'s {name} was not taken away: {}", why.detail);
    }
  }

  let pc = match runner(ground, account).await {
    Ok(runner) => ask(&runner, &json!({ "owner": account, "take": { "tool": name } })).await.map(|_| ()),
    Err(fault) => Err(fault),
  };

  match (home, pc) {
    (Err(_), Err(Fault::Refused(_))) => {
      return Err(Fault::NotFound(format!("the account {account} holds no app {name}")));
    }
    (Err(fault), Err(_)) => return Err(fault),
    (_, Ok(())) => said.push(format!("took {name} off the PC")),
    (Ok(_), Err(fault)) => {
      said.push(format!("the PC keeps its versions of {name}: {}", Refused::from(fault).error));
    }
  }

  Ok(Outcome {
    name: name.to_owned(),
    version: None,
    entries: Vec::new(),
    said,
  })
}

/// The account's apps as the tool host holds them, each with the versions its PC keeps older than the one in place,
/// null where the PC was not reached; and whether it was.
pub async fn list(ground: &Ground, account: &str) -> Result<Value, Fault> {
  let tools = listed_tools(ground, account).await?;
  let servers: std::collections::BTreeSet<String> = crate::attachments::mcp_items(ground, account, false)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|item| item.name)
    .collect();
  let kept = match runner(ground, account).await {
    Ok(runner) => {
      tokio::time::timeout(LISTING_WAIT, ask(&runner, &json!({ "owner": account, "versions": {} })))
        .await
        .ok()
        .and_then(Result::ok)
        .filter(|answered| answered.last["t"] == "versions")
        .map(|answered| answered.last["tools"].clone())
    }
    Err(_) => None,
  };
  let items: Vec<Value> = tools
    .into_iter()
    .map(|tool| {
      let older = kept
        .as_ref()
        .map(|kept| kept[tool["name"].as_str().unwrap_or_default()]["kept"].clone());

      let name = tool["name"].as_str().unwrap_or_default();

      json!({
        "name": tool["name"],
        "kind": if servers.contains(name) { "mcp" } else { "app" },
        "version": tool["version"],
        "putAt": tool["putAt"],
        "entries": tool["entries"],
        "permissions": tool["permissions"],
        "kept": match older {
          Some(Value::Array(older)) => Value::Array(older),
          Some(_) => json!([]),
          None => Value::Null,
        },
      })
    })
    .collect();

  Ok(json!({ "items": items, "reached": kept.is_some() }))
}
