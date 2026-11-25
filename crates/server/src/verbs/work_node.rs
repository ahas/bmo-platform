//! The workplaces on the work nodes (docs/work-nodes.md, 「The workplace on a node」): every workplace
//! with its state and its link; `setup` stands one — the tools, installed into the node's account with
//! `--install`, the deploy tree's commit copied and built there, the herdr session, the checkouts placed
//! there cloned, set up and trusted — and writes down what is still missing; `check` reads the same
//! without writing anything on the node; `sync` copies and builds the code again after a deployment;
//! `runs` says whether heavy runs go to it; `remove` lets it go; `teardown` takes an account's isolation node
//! away whole.

use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use clap::Subcommand;
use futures_util::future::BoxFuture;
use policy::providers::claude_code::CLAUDE_CODE;
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};
use store::{all, one, run};

use super::work_node_hub::{HubDoor, HubHands};
use super::work_node_isolated::{IsolatedDoor, IsolatedHands};
use super::work_node_setup::{
  COPY_LEFT_OUT, FarEnd, SetupCheckout, SetupHands, SetupLogin, SetupOutcome, SetupPlace, SetupShell,
  ShellRun, claude_release_of, set_up_workplace, shell_word,
};
use super::{Answer, VerbError, deploy_link, home, service};
use crate::control::claude_accounts::accounts;
use crate::peers::node::{NodeKind, NodeService};
use crate::watches::links::{
  Roots, SETUP_CLAUDE, deployed_commit, link_heard, link_refused, link_unit, node_path,
};

/// The root a workplace takes where its setup names none, under the node's home.
pub const DEFAULT_WORKPLACE_ROOT: &str = "~/.local/share/bmo-node";

/// The herdr session the deployment's agents stand in on a node, apart from any session a person keeps
/// there.
pub const HERDR_SESSION: &str = "bmo";

/// The service a node's role names to carry a worker's jobs (docs/work-nodes.md, 「Which machines take
/// work」).
const JOBS_SERVICE: &str = "jobs";

#[derive(Subcommand, Clone, Debug)]
pub enum WorkNodeVerb {
  /// Stands the workplace up, installing missing tools into the node's account with --install.
  Setup {
    node: String,
    #[arg(long)]
    root: Option<String>,
    #[arg(long)]
    install: bool,
  },
  /// Reads the workplace as the setup would, writing nothing on the node.
  Check { node: String },
  /// Copies and builds the code again after a deployment.
  Sync { node: String },
  /// Whether heavy runs go to the workplace.
  Runs {
    node: String,
    #[arg(value_parser = ["on", "off"])]
    switched: String,
  },
  /// Lets the workplace go; what stands on the node is left as it is.
  Remove { node: String },
  /// Takes an account's isolation node away whole: its units, its user and home, its firewall table and its
  /// workplace; refuses any other node.
  Teardown { node: String },
}

/// One workplace as its row holds it.
#[derive(Clone, Debug)]
pub struct Workplace {
  pub node: String,
  pub root: String,
  pub home: Option<String>,
  pub runs: bool,
  pub state: String,
  pub detail: Option<String>,
  pub checked_at: Option<i64>,
}

fn workplace_of(row: &sea_orm::QueryResult) -> Result<Workplace, VerbError> {
  Ok(Workplace {
    node: row.try_get("", "node")?,
    root: row.try_get("", "root")?,
    home: row.try_get("", "home")?,
    runs: row.try_get::<i64>("", "runs")? != 0,
    state: row.try_get("", "state")?,
    detail: row.try_get("", "detail")?,
    checked_at: row.try_get("", "checked_at")?,
  })
}

/// Every workplace, by node.
pub async fn workplaces(db: &DatabaseConnection) -> Result<Vec<Workplace>, VerbError> {
  all(db, r#"SELECT * FROM "node_workplaces" ORDER BY "node""#, vec![])
    .await?
    .iter()
    .map(workplace_of)
    .collect()
}

/// One node's workplace.
pub async fn workplace(db: &DatabaseConnection, node: &str) -> Result<Option<Workplace>, VerbError> {
  one(db, r#"SELECT * FROM "node_workplaces" WHERE "node" = $1"#, vec![node.into()])
    .await?
    .as_ref()
    .map(workplace_of)
    .transpose()
}

fn now_seconds() -> i64 {
  chrono::Utc::now().timestamp()
}

/// A workplace written where none stands, or its root moved; a root moved clears the last check.
pub async fn place_workplace(
  db: &DatabaseConnection,
  node: &str,
  root: &str,
) -> Result<Workplace, VerbError> {
  let at = now_seconds();

  match workplace(db, node).await? {
    None => {
      run(
        db,
        r#"INSERT INTO "node_workplaces" ("node", "root", "runs", "state", "created_at", "updated_at") VALUES ($1, $2, 0, 'unknown', $3, $3)"#,
        vec![node.into(), root.into(), at.into()],
      )
      .await?;
    }
    Some(before) if before.root != root => {
      run(
        db,
        r#"UPDATE "node_workplaces" SET "root" = $1, "state" = 'unknown', "detail" = NULL, "checked_at" = NULL, "updated_at" = $2 WHERE "node" = $3"#,
        vec![root.into(), at.into(), node.into()],
      )
      .await?;
    }
    Some(_) => {}
  }

  workplace(db, node)
    .await?
    .ok_or_else(|| VerbError::Refused(format!("{node} has no workplace")))
}

/// Why a node takes no work, or none where it may: a node the node service names an SSH login to, whose
/// state it says is not reserved for a service other than the jobs.
pub fn work_refusal(name: &str, logs_in: bool, role: Option<&str>) -> Option<String> {
  if !logs_in {
    return Some(format!("{name} has no SSH route to carry a job"));
  }

  role
    .filter(|role| !role.split('+').any(|service| service == JOBS_SERVICE))
    .map(|role| format!("{name} is reserved for {role} and carries no {JOBS_SERVICE}"))
}

/// A moment in epoch seconds as the listings write it, in UTC.
pub fn clock(seconds: Option<i64>) -> String {
  seconds
    .and_then(|at| chrono::DateTime::from_timestamp(at, 0))
    .map_or_else(|| "never".to_owned(), |at| at.format("%Y-%m-%d %H:%M:%S UTC").to_string())
}

// Where this machine keeps what it knows of a node.
fn node_dir(data_root: &Path, node: &str) -> PathBuf {
  data_root.join("nodes").join(node)
}

// Where a node's setup leaves its record: its herdr session and socket, the commit its copy holds, and on a PC
// the machine id its Linux shell reads.
fn setup_record(data_root: &Path, node: &str) -> PathBuf {
  node_dir(data_root, node).join("setup.json")
}

fn read_json(path: &Path) -> Option<Value> {
  serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

// How long ago a moment was, in whole minutes.
fn ago(seconds: Option<i64>) -> String {
  seconds.map_or_else(
    || "never".to_owned(),
    |at| format!("{}m ago", ((now_seconds() - at) as f64 / 60.0).round().max(0.0) as i64),
  )
}

// How a node's link stands, as its far end last wrote it.
fn link_words(data_root: &Path, node: &str) -> String {
  let Some(state) = read_json(&node_dir(data_root, node).join("link-state.json")) else {
    return "no link".to_owned();
  };
  if link_heard(data_root, node) {
    let since = state.get("since").and_then(Value::as_i64).unwrap_or(0);

    return format!("link up since {}", clock(Some(since.div_euclid(1000))));
  }

  match state.get("dropped").and_then(Value::as_str) {
    Some(dropped) => format!("link down: {dropped}"),
    None => "link down".to_owned(),
  }
}

/// Every workplace, its state and its link, as the bare verb prints them.
pub async fn listing(db: &DatabaseConnection, data_root: &Path) -> Result<Vec<String>, VerbError> {
  let held = workplaces(db).await?;

  if held.is_empty() {
    return Ok(vec!["no workplace on any node".to_owned()]);
  }

  let mut lines = Vec::new();

  for one in held {
    lines.push(format!(
      "{:<12} {:<11} runs {} root {} · checked {} · {}",
      one.node,
      one.state,
      if one.runs { "on " } else { "off" },
      one.root,
      ago(one.checked_at),
      link_words(data_root, &one.node)
    ));

    for line in one.detail.as_deref().unwrap_or_default().split('\n').filter(|line| !line.is_empty()) {
      lines.push(format!("  - {line}"));
    }
  }

  Ok(lines)
}

/// The node's shell over the one connection the first command makes and the rest share: a node may refuse
/// a run of logins made in quick succession, as a firewall limiting SSH does.
pub struct SharedLogin {
  pub login: Vec<String>,
}

impl SharedLogin {
  /// The batch login the node service names, held open for a run of commands, kept a minute past the last
  /// of them, the arguments ending at the destination.
  pub fn new(mut login: Vec<String>) -> Self {
    let destination = login.pop().unwrap_or_default();
    let control = shared_login_path();

    login.extend([
      "-o".to_owned(),
      "ControlMaster=auto".to_owned(),
      "-o".to_owned(),
      format!("ControlPath={}", control.display()),
      "-o".to_owned(),
      "ControlPersist=60".to_owned(),
      destination,
    ]);

    Self { login }
  }

  fn destination(&self) -> &str {
    self.login.last().map_or("", String::as_str)
  }

  fn options(&self) -> &[String] {
    &self.login[..self.login.len().saturating_sub(1)]
  }

  /// The port the login names, 22 where it names none.
  fn port(&self) -> u16 {
    self
      .login
      .windows(2)
      .find(|pair| pair[0] == "-p")
      .and_then(|pair| pair[1].parse().ok())
      .unwrap_or(22)
  }
}

// Where the connection a run of commands shares is held: under the runtime directory, since a socket's
// path holds at most 107 bytes and a data root may be deep. OpenSSH names it by the login it holds.
fn shared_login_path() -> PathBuf {
  let runtime = std::env::var_os("XDG_RUNTIME_DIR").filter(|dir| !dir.is_empty()).map_or_else(
    // SAFETY: getuid reads the process's own user id and cannot fail.
    || PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })),
    PathBuf::from,
  );

  runtime.join("agency").join("ssh-%C")
}

pub(super) async fn captured(mut command: tokio::process::Command) -> ShellRun {
  command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());

  match command.output().await {
    Ok(done) => ShellRun {
      code: done.status.code().unwrap_or(255),
      stdout: String::from_utf8_lossy(&done.stdout).into_owned(),
      stderr: String::from_utf8_lossy(&done.stderr).into_owned(),
    },
    Err(why) => ShellRun {
      code: 255,
      stdout: String::new(),
      stderr: why.to_string(),
    },
  }
}

impl SetupShell for SharedLogin {
  fn run<'a>(&'a self, command: &'a str) -> BoxFuture<'a, ShellRun> {
    Box::pin(async move {
      // The shared connection holds a command's output for as long as the command runs there, so its end
      // there, or the connection breaking, ends the wait here; no clock cuts it.
      let mut ssh = tokio::process::Command::new("ssh");

      ssh.args(&self.login).arg(format!("bash -c {}", shell_word(command)));
      captured(ssh).await
    })
  }
}

// What the setup may do on this machine's side: copy over the shared login, save the node in herdr.
struct Hands<'a> {
  install: bool,
  copy_code: bool,
  login: &'a SharedLogin,
  label: String,
}

pub(super) fn said_tail(text: &str) -> String {
  let lines: Vec<&str> = text.trim().split('\n').collect();

  lines[lines.len().saturating_sub(3)..].join(" ")
}

impl SetupHands for Hands<'_> {
  fn install(&self) -> bool {
    self.install
  }

  fn copy_code(&self) -> bool {
    self.copy_code
  }

  fn copy<'a>(&'a self, from: &'a str, to: &'a str) -> BoxFuture<'a, (bool, String)> {
    Box::pin(async move {
      let to = format!("{}:{to}", self.login.destination());
      let ssh = std::iter::once("ssh".to_owned())
        .chain(self.login.options().iter().cloned())
        .map(|word| shell_word(&word))
        .collect::<Vec<_>>()
        .join(" ");
      let mut rsync = tokio::process::Command::new("rsync");

      rsync.args(["-a", "--delete"]);

      for pattern in COPY_LEFT_OUT {
        rsync.args(["--exclude", pattern]);
      }

      rsync.args(["-e", &ssh, from, &to]);

      let ran = captured(rsync).await;

      (ran.code == 0, said_tail(&ran.stderr))
    })
  }

  fn add_machine(&self) -> BoxFuture<'_, (bool, String)> {
    Box::pin(async move {
      let destination = self.login.destination();
      let port = self.login.port();
      let machine = if port == 22 { destination.to_owned() } else { format!("ssh://{destination}:{port}") };
      // Saved once: herdr keeps every machine added as a profile of its own, the same target or not.
      let mut listed = tokio::process::Command::new("herdr");

      listed.args(["machine", "list", "--json"]);

      let saved: Vec<Value> = serde_json::from_str(&captured(listed).await.stdout).unwrap_or_default();

      if saved.iter().any(|profile| {
        profile.get("target").and_then(Value::as_str) == Some(machine.as_str())
          && profile.get("session").and_then(Value::as_str) == Some(HERDR_SESSION)
      }) {
        return (true, "already saved".to_owned());
      }

      let mut add = tokio::process::Command::new("herdr");

      add.args(["machine", "add", "--label", &self.label, "--remote-session", HERDR_SESSION, &machine]);

      let ran = captured(add).await;

      (ran.code == 0, said_tail(&format!("{}{}", ran.stdout, ran.stderr)))
    })
  }

  fn say(&self, line: &str) {
    say(line);
  }
}

/// A line of a setup's progress, said as it happens.
pub fn say(line: &str) {
  println!("{line}");
  let _ = std::io::stdout().flush();
}

/// How the setup reaches a node: over its SSH login, through the hub its agent attaches to, or, for an account's
/// isolation node, in the sandbox its user runs in.
enum Door {
  Ssh(SharedLogin),
  Hub(HubDoor),
  Isolated(IsolatedDoor),
}

impl Door {
  fn shell(&self) -> &dyn SetupShell {
    match self {
      Self::Ssh(login) => login,
      Self::Hub(door) => door,
      Self::Isolated(door) => door,
    }
  }
}

// What a program on this machine says its version is: its first line's last word, or the word after
// a name.
fn local_version(program: &str, args: &[&str], after: Option<&str>) -> Option<String> {
  let done = std::process::Command::new(program).args(args).stderr(Stdio::null()).output().ok()?;
  let first = String::from_utf8_lossy(&done.stdout).lines().next()?.trim().to_owned();

  match after {
    Some(name) => first.strip_prefix(name)?.split_whitespace().next().map(str::to_owned),
    None => (!first.is_empty()).then_some(first),
  }
}

// Where a program stands on this machine's search path, its links read through.
fn local_program(program: &str) -> Option<String> {
  let path = std::env::var_os("PATH")?;

  std::env::split_paths(&path)
    .map(|dir| dir.join(program))
    .find(|candidate| candidate.is_file())
    .and_then(|found| std::fs::canonicalize(found).ok())
    .map(|found| found.to_string_lossy().into_owned())
}

// The remote a checkout's base clones from: its origin.
fn remote_of(base_cwd: &str) -> Option<String> {
  let done = std::process::Command::new("git")
    .args(["-C", base_cwd, "remote", "get-url", "origin"])
    .stderr(Stdio::null())
    .output()
    .ok()?;

  (done.status.success())
    .then(|| String::from_utf8_lossy(&done.stdout).trim().to_owned())
    .filter(|url| !url.is_empty())
}

// The node's place in this machine's paths: each of this machine's roots read as the node's, the longest
// first.
fn to_node_path(path: &str, data_root: &Path, node_home: &str, node_root: &str) -> String {
  let installed = home().join(".local/share/bmo");
  let agents = std::env::var_os("BMO_AGENTS_DIR").map_or_else(|| installed.join("agents"), PathBuf::from);
  let mut map = vec![
    (data_root.to_string_lossy().into_owned(), format!("{node_root}/data")),
    (deploy_link().to_string_lossy().into_owned(), format!("{node_root}/deploy")),
    (agents.to_string_lossy().into_owned(), format!("{node_root}/agents")),
    (home().to_string_lossy().into_owned(), node_home.to_owned()),
  ];

  map.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));

  for (from, to) in map {
    if path == from {
      return to;
    }

    if let Some(rest) = path.strip_prefix(&format!("{from}/")) {
      return format!("{to}/{rest}");
    }
  }

  path.to_owned()
}

// The logins a run on the node may go out on, each account as this machine keeps it; an endpoint account
// is not offered on a node, since its key file stays on this machine (docs/work-nodes.md). An isolation node is
// offered the logins of its own account's user alone (docs/account-isolation.md, 「Standing it up」).
async fn logins(
  db: &DatabaseConnection,
  data_root: &Path,
  only_user: Option<i64>,
) -> Result<Vec<SetupLogin>, VerbError> {
  let held = accounts(db).await.map_err(|refused| VerbError::Refused(refused.error))?;
  let config_dir = std::env::var_os("CLAUDE_CONFIG_DIR")
    .filter(|dir| !dir.is_empty())
    .map_or_else(|| home().join(".claude"), PathBuf::from);
  let deploy = deploy_link();

  Ok(
    held
      .iter()
      .filter(|account| account.credential.runs_on_nodes())
      .filter(|account| only_user.is_none() || account.user_id == only_user)
      .map(|account| SetupLogin {
        account: account.name.clone(),
        own_directory: account.credential.own_home().is_some(),
        path: account
          .credential
          .own_home()
          .unwrap_or(config_dir.as_path())
          .to_string_lossy()
          .into_owned(),
        node_token: account.node_token(data_root).is_some(),
        token_command: CLAUDE_CODE
          .node_login
          .command
          .replace("{deploy}", &deploy.to_string_lossy())
          .replace("{name}", &account.name.replace('\'', r"'\''")),
      })
      .collect(),
  )
}

// The door to a node the node service keeps, over its SSH login or through its hub, and the label herdr shows it
// by; refused for a node the service does not keep or that takes no work.
async fn service_door(nodes: &dyn NodeService, node: &str) -> Result<(Door, String), VerbError> {
  let unreachable = |fault: crate::peers::PeerFault| VerbError::Refused(fault.to_string());
  let unlisted = || VerbError::Refused(format!("{node} is no node the node service keeps"));
  let no_route = || VerbError::Refused(format!("{node} has no route the node service names"));
  let row = nodes
    .node_rows()
    .await
    .map_err(unreachable)?
    .into_iter()
    .find(|row| row.node == node)
    .ok_or_else(unlisted)?;
  let door = match row.kind {
    NodeKind::Ssh => Door::Ssh(SharedLogin::new(
      nodes.ssh_login(node).await.map_err(unreachable)?.ok_or_else(no_route)?,
    )),
    NodeKind::Jake => {
      let reach = nodes.hub_reach(node).await.map_err(unreachable)?.ok_or_else(no_route)?;

      Door::Hub(
        HubDoor::open(&reach, &home())
          .await
          .map_err(|why| VerbError::Refused(format!("{node}: {why}")))?,
      )
    }
  };

  if let Some(refused) = work_refusal(&row.node, true, None) {
    return Err(VerbError::Refused(format!("{node} takes no work: {refused}")));
  }

  Ok((door, row.label.clone().unwrap_or_else(|| row.node.clone())))
}

// The setup, a check or a sync of one node's workplace; whether nothing is missing.
async fn stand(
  db: &DatabaseConnection,
  nodes: &dyn NodeService,
  data_root: &Path,
  verb: &WorkNodeVerb,
) -> Result<Answer, VerbError> {
  let (node, root, install, copy_code) = match verb {
    WorkNodeVerb::Setup {
      node,
      root,
      install,
    } => (node, root.clone(), *install, true),
    WorkNodeVerb::Sync { node } => (node, None, false, true),
    WorkNodeVerb::Check { node } => (node, None, false, false),
    _ => return Err(VerbError::Refused("no such step".into())),
  };
  let isolation = crate::account_isolation::Isolation::of_node(node);
  // The ports of this machine's loopback the node's programs reach: an isolation node's network rule leaves them
  // open, and a PC's agent and hub must offer them.
  let home_ports =
    Roots::of_machine(&harness_providers::Machine::of_process(data_root), deploy_link()).node_ports();
  let open_ports: Vec<u16> = home_ports.iter().map(|(port, _)| *port).collect();
  let (door, label) = match &isolation {
    // An account's isolation node is no node the node service keeps: agency names it and stands its user.
    Some(isolation) => {
      let door = IsolatedDoor::open(isolation, copy_code, &open_ports)
        .await
        .map_err(|why| VerbError::Refused(format!("{node}: {why}")))?;

      (Door::Isolated(door), node.clone())
    }
    None => service_door(nodes, node).await?,
  };

  let held = workplace(db, node).await?;

  if held.is_none() && !matches!(verb, WorkNodeVerb::Setup { .. }) {
    return Err(VerbError::Refused(format!("{node} has no workplace; set it up first")));
  }

  let root = root
    .or_else(|| held.map(|held| held.root))
    .unwrap_or_else(|| DEFAULT_WORKPLACE_ROOT.to_owned());
  let workplace = place_workplace(db, node, &root).await?;
  // One PC's Linux shell holds one far end, so a workplace on a PC whose far end another ready workplace holds is
  // neither set up nor made ready, and nothing is laid on the PC (docs/person-nodes.md, 「The workplace on a PC」).
  let machine = match &door {
    Door::Hub(door) => Some(door.machine().await),
    _ => None,
  };

  if let Some(Ok(machine)) = &machine
    && let Some(taken) = far_end_taken(db, data_root, node, machine).await?
  {
    record_state(db, node, "missing", Some(taken.clone()), None).await?;

    return Ok(Answer {
      lines: vec![format!("{node}: not ready"), format!("  - {taken}")],
      code: 1,
    });
  }

  let standing = crate::queue::workers::workers(db, std::path::Path::new(""))
    .await
    .map_err(|why| VerbError::Refused(why.detail))?;
  let mut checkouts = Vec::new();

  for worker in standing.iter().filter(|worker| worker.node.as_deref() == Some(node.as_str())) {
    let base_cwd = worker
      .support_of
      .as_ref()
      .and_then(|base| standing.iter().find(|one| &one.name == base))
      .map(|base| base.cwd.clone());

    checkouts.push(SetupCheckout {
      worker: worker.name.clone(),
      cwd: worker.cwd.clone(),
      remote: base_cwd.as_deref().and_then(remote_of),
    });
  }

  std::fs::DirBuilder::new()
    .recursive(true)
    .mode(0o700)
    .create(node_dir(data_root, node))
    .map_err(|why| VerbError::Refused(why.to_string()))?;

  let to_node =
    |path: &str, node_home: &str, node_root: &str| to_node_path(path, data_root, node_home, node_root);
  let place = SetupPlace {
    node: node.clone(),
    root: workplace.root.clone(),
    deploy: deploy_link().to_string_lossy().into_owned(),
    checkouts: checkouts.clone(),
    logins: logins(
      db,
      data_root,
      isolation.as_ref().map(|isolation| isolation.account.parse().unwrap_or_default()),
    )
    .await?,
    to_node: &to_node,
    herdr: local_program("herdr"),
    rust: local_version("rustc", &["--version"], Some("rustc ")),
    bun: local_version("bun", &["--version"], None),
    claude: local_version("claude", &["--version"], None).and_then(|said| claude_release_of(&said)),
    session: HERDR_SESSION.to_owned(),
    far_end: matches!(door, Door::Hub(_)).then(person_far_end),
  };
  let hands: Box<dyn SetupHands + '_> = match &door {
    Door::Ssh(login) => Box::new(Hands {
      install,
      copy_code,
      login,
      label,
    }),
    Door::Isolated(door) => Box::new(IsolatedHands {
      install,
      copy_code,
      door,
    }),
    Door::Hub(door) => Box::new(HubHands {
      install,
      copy_code,
      door,
      staging: node_dir(data_root, node),
    }),
  };
  let mut outcome: SetupOutcome = set_up_workplace(door.shell(), &place, hands.as_ref()).await;

  // What an isolation node's user holds beyond its own, missing with the rest, so nothing launches there.
  if let Door::Isolated(door) = &door {
    outcome.missing.extend(door.wanting().await);
  }

  // What a PC's hub agent does not offer the workplace and its link, and each port its far end could not listen on
  // as the link last said, missing with the rest; WSL's networking on a Windows PC said beside them.
  let mut notes = Vec::new();

  if let (Door::Hub(door), Some(far_end)) = (&door, &place.far_end) {
    if let Some(Err(why)) = &machine {
      outcome.missing.push(format!(
        "far end: the PC's machine id could not be read, so whether another workplace holds its far end is not known: {why}"
      ));
    }

    outcome.missing.extend(door.wanting(far_end));
    outcome.missing.extend(refused_listens(data_root, node, &home_ports));

    let far_ports: Vec<(u16, &str)> = [(far_end.mirror_port, "the mirror"), (far_end.herdr_port, "herdr")]
      .into_iter()
      .chain(home_ports.iter().copied())
      .collect();

    notes = door.wsl_networking(&far_ports).await;
    notes.push(match &outcome.confinement {
      Some(why) => format!("personal tools: {why}"),
      None => "personal tools: confined by bubblewrap".to_owned(),
    });
  }

  // A path written from the node's home reads as the node's own from now on, so every lock, record and
  // transcript of the checkout names the one path.
  if let Some(node_home) = &outcome.home {
    for checkout in &checkouts {
      let absolute = node_path(&checkout.cwd, Some(node_home));

      if absolute != checkout.cwd {
        run(
          db,
          r#"UPDATE "repositories" SET "path" = $1 WHERE "name" = $2"#,
          vec![absolute.clone().into(), checkout.worker.clone().into()],
        )
        .await?;
        run(
          db,
          r#"UPDATE "repository_node_checkouts" SET "path" = $1 WHERE "repository" || '@' || "node" = $2"#,
          vec![absolute.into(), checkout.worker.clone().into()],
        )
        .await?;
      }
    }
  }

  if let Some(socket) = &outcome.herdr_socket {
    let path = setup_record(data_root, node);
    // The commit the code on the node now stands at, where this run laid its copy there, so agency knows when a
    // deployment has moved past it; a copy that failed leaves the commit the node held.
    let commit = if copy_code && outcome.copied {
      deployed_commit(&deploy_link()).map(Value::String)
    } else {
      read_json(&path).and_then(|setup| setup.get("commit").cloned())
    };
    let mut record = json!({ "herdrSession": HERDR_SESSION, "herdrSocket": socket, "commit": commit.unwrap_or(Value::Null) });

    // The release the workplace's own Claude Code reports, where it stands, by which the link runs it in place of the
    // word `claude` (docs/work-nodes.md, 「The link」).
    if let Some(claude) = &outcome.claude {
      record[SETUP_CLAUDE] = Value::String(claude.clone());
    }

    // On a PC, whether its personal tools run confined, and why not where they cannot.
    if place.far_end.is_some() {
      record["confinement"] = json!(outcome.confinement.as_deref().unwrap_or("ready"));
    }

    // The machine a PC's Linux shell reads, by which another workplace tells whether this one holds the PC's far end.
    if let Some(Ok(machine)) = &machine {
      record[SETUP_MACHINE] = Value::String(machine.clone());
    }

    let body = format!("{}\n", serde_json::to_string_pretty(&record).unwrap_or_default());
    let next = PathBuf::from(format!("{}.{}", path.display(), std::process::id()));
    let written = std::fs::OpenOptions::new()
      .write(true)
      .create(true)
      .truncate(true)
      .mode(0o600)
      .open(&next)
      .and_then(|mut file| file.write_all(body.as_bytes()))
      .and_then(|()| std::fs::rename(&next, &path));

    written.map_err(|why| VerbError::Refused(format!("{}: {why}", path.display())))?;
  }

  let state = match (&outcome.home, outcome.missing.is_empty()) {
    (None, _) => "unreachable",
    (Some(_), true) => "ready",
    (Some(_), false) => "missing",
  };
  let detail = (!outcome.missing.is_empty()).then(|| outcome.missing.join("\n"));

  record_state(db, node, state, detail, outcome.home.clone()).await?;

  // A copy or a build moved what the link's far end runs, so the link is stood again on it.
  if copy_code {
    let _ = tokio::process::Command::new("systemctl")
      .args(["--user", "stop", &format!("{}.service", link_unit(node))])
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .status()
      .await;
  }

  Ok(if outcome.missing.is_empty() {
    let mut lines = vec![format!("{node}: ready ({})", outcome.root.unwrap_or_default())];

    lines.extend(notes.iter().map(|line| format!("  {line}")));
    Answer::from(lines)
  } else {
    let mut lines = vec![format!("{node}: not ready")];

    lines.extend(outcome.missing.iter().map(|line| format!("  - {line}")));
    lines.extend(notes.iter().map(|line| format!("  {line}")));
    Answer { lines, code: 1 }
  })
}

/// Where a node's setup record keeps the machine id its PC's Linux shell reads.
const SETUP_MACHINE: &str = "machine";

// A workplace's state and what it still misses, as its last setup, check or sync found them.
async fn record_state(
  db: &DatabaseConnection,
  node: &str,
  state: &str,
  detail: Option<String>,
  home: Option<String>,
) -> Result<(), VerbError> {
  let at = now_seconds();

  run(
    db,
    r#"UPDATE "node_workplaces" SET "state" = $1, "detail" = $2, "home" = COALESCE($3, "home"), "checked_at" = $4, "updated_at" = $4 WHERE "node" = $5"#,
    vec![state.into(), detail.into(), home.into(), at.into(), node.into()],
  )
  .await?;

  Ok(())
}

/// What says another node's ready workplace already holds the far end of the PC whose Linux shell reads the
/// machine id given, as that workplace's setup record keeps it; or none where no other does
/// (docs/person-nodes.md, 「The workplace on a PC」).
pub async fn far_end_taken(
  db: &DatabaseConnection,
  data_root: &Path,
  node: &str,
  machine: &str,
) -> Result<Option<String>, VerbError> {
  let holds = |other: &str| {
    read_json(&setup_record(data_root, other))
      .is_some_and(|setup| setup.get(SETUP_MACHINE).and_then(Value::as_str) == Some(machine))
  };
  let holder = workplaces(db)
    .await?
    .into_iter()
    .find(|other| other.node != node && other.state == "ready" && holds(&other.node));

  Ok(holder.map(|holder| {
    format!(
      "far end: {}'s ready workplace holds this PC's far end, the same Linux shell answering both; one far end serves one node, so one of the two workplaces is removed first",
      holder.node
    )
  }))
}

/// Each port the far end on a person's PC could not listen on, as its link last said, by what it is for among the
/// ports given (docs/person-nodes.md, 「The workplace on a PC」).
pub fn refused_listens(data_root: &Path, node: &str, ports: &[(u16, &str)]) -> Vec<String> {
  link_refused(data_root, node)
    .into_iter()
    .map(|(port, why)| {
      let what = ports.iter().find(|(one, _)| *one == port).map_or("a program here", |(_, what)| *what);

      format!("far end: it could not listen on port {port} for {what}: {why}")
    })
    .collect()
}

// The ports the far end stands on at a person's PC, as agency's policy holds them.
fn person_far_end() -> FarEnd {
  let ports = &policy::person_nodes::PERSON_NODES;

  FarEnd {
    mirror_port: ports.mirror_port,
    herdr_port: ports.herdr_port,
  }
}

/// One run of `bmo-agency work-node`.
pub async fn work_node(data_root: &Path, verb: Option<WorkNodeVerb>) -> Result<Answer, VerbError> {
  let service = service(data_root).await?;
  let nodes = super::node_service(data_root, &service);

  work_node_on(&service.db, nodes.as_ref(), data_root, verb).await
}

/// One run of `bmo-agency work-node` on a store already open.
pub async fn work_node_on(
  db: &DatabaseConnection,
  nodes: &dyn NodeService,
  data_root: &Path,
  verb: Option<WorkNodeVerb>,
) -> Result<Answer, VerbError> {
  match verb {
    None => listing(db, data_root).await.map(Answer::from),
    Some(WorkNodeVerb::Runs { node, switched }) => {
      let changed = run(
        db,
        r#"UPDATE "node_workplaces" SET "runs" = $1, "updated_at" = $2 WHERE "node" = $3"#,
        vec![i64::from(switched == "on").into(), now_seconds().into(), node.clone().into()],
      )
      .await?;

      if changed == 0 {
        return Err(VerbError::Refused(format!("{node} has no workplace; set it up first")));
      }

      Ok(Answer::from(vec![format!("{node}: heavy runs {switched}")]))
    }
    Some(WorkNodeVerb::Remove { node }) => {
      if run(db, r#"DELETE FROM "node_workplaces" WHERE "node" = $1"#, vec![node.clone().into()]).await?
        == 0
      {
        return Err(VerbError::Refused(format!("{node} has no workplace")));
      }

      Ok(Answer::from(vec![format!(
        "{node}: its workplace is no longer one; what stands on the node is left as it is"
      )]))
    }
    Some(WorkNodeVerb::Teardown { node }) => {
      let reach = super::work_node_teardown::Reach::of(&harness_providers::Machine::of_process(data_root));
      let done = super::work_node_teardown::tear_down(db, data_root, &node, reach).await?;

      Ok(Answer::from(done.lines))
    }
    Some(verb) => stand(db, nodes, data_root, &verb).await,
  }
}
