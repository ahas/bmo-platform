//! An account's isolation node taken down (docs/account-isolation.md, 「Taking it down」): the setup's inverse. The
//! units this machine's user keeps for the node stopped; where the account's user stands, the system units its
//! programs run in stopped, its processes still left killed and the user taken away with its home; its firewall
//! table taken away; and the node's folder under the data root and its workplace row. A part already gone is
//! nothing to do, and any node that is not an isolation node is refused.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use harness_machine::model_use::with_user_bus;
use harness_providers::Machine;
use policy::account_isolation::ACCOUNT_ISOLATION;
use sea_orm::ConnectionTrait;
use store::{one, run};

use super::VerbError;
use super::work_node::{captured, said_tail};
use crate::account_isolation::{Isolation, machine_wants};
use crate::watches::links::{check_unit, link_unit, setup_unit, sync_unit};

/// How long the teardown waits for the account's user's processes to end once they are killed.
const PROCESSES_END_WITHIN: Duration = Duration::from_secs(10);

/// What nft says of a table that does not stand.
const NO_TABLE: &str = "No such file or directory";

/// What a teardown may reach of this machine: all of it, or, on a test's server, which stands no user, what agency
/// keeps of the node alone — its folder and its row (docs/account-isolation.md, 「Taking it down」).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
  Machine,
  StoreAlone,
}

impl Reach {
  /// The reach of a process on the machine given: a test's ground, which names a test's home, reaches none of the
  /// machine's users, units or firewall.
  pub fn of(machine: &Machine) -> Self {
    if crate::door::guard::names_test_home(machine) {
      Self::StoreAlone
    } else {
      Self::Machine
    }
  }
}

/// What one teardown took.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TornDown {
  /// Whether the account's user stood and was taken away with its home.
  pub user: bool,
  /// Whether the user's firewall table stood and was taken away.
  pub firewall: bool,
  /// Whether the node's workplace row stood and was taken away.
  pub workplace: bool,
  /// What it did, a line each.
  pub lines: Vec<String>,
}

/// The isolation node a name stands for, or the refusal of any other node.
pub fn isolation_of(node: &str) -> Result<Isolation, VerbError> {
  match Isolation::of_node(node) {
    Some(isolation) if account_user(&isolation.user) => Ok(isolation),
    _ => Err(VerbError::Refused(format!(
      "{node} is no account's isolation node, {}<id>; only an isolation node is taken down",
      ACCOUNT_ISOLATION.node_prefix
    ))),
  }
}

/// Whether a system user's name is an isolation node's own: the policy's prefix and an account's digits, so nothing
/// the teardown takes is ever the owner's user or another user of the system.
pub fn account_user(user: &str) -> bool {
  user
    .strip_prefix(&ACCOUNT_ISOLATION.user_prefix)
    .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Whether the account's user stands on this machine, as the reach given sees it.
pub async fn user_stands(reach: Reach, user: &str) -> bool {
  reach == Reach::Machine && uid_of(user).await.is_some()
}

/// Whether the node's workplace row stands.
pub async fn workplace_stands<C: ConnectionTrait>(db: &C, node: &str) -> Result<bool, crate::Refusal> {
  Ok(
    one(db, r#"SELECT 1 AS "one" FROM "node_workplaces" WHERE "node" = $1"#, vec![node.into()])
      .await?
      .is_some(),
  )
}

/// Where this machine keeps what it knows of a node, under the data root.
pub fn node_folder(data_root: &Path, node: &str) -> PathBuf {
  data_root.join("nodes").join(node)
}

/// The units this machine's user keeps for a node — its link, its setup, its check and its sync — stopped; a unit
/// that does not run is nothing to do.
pub async fn stop_node_units(node: &str) {
  for unit in [link_unit(node), setup_unit(node), check_unit(node), sync_unit(node)] {
    let mut command = std::process::Command::new("systemctl");

    command
      .args(["--user", "stop", &format!("{unit}.service")])
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null());
    with_user_bus(&mut command);

    let _ = tokio::process::Command::from(command).status().await;
  }
}

/// What this machine keeps of a node taken away: its folder under the data root and its workplace row; whether the
/// row stood.
pub async fn forget_node<C: ConnectionTrait>(
  db: &C,
  data_root: &Path,
  node: &str,
) -> Result<bool, VerbError> {
  let folder = node_folder(data_root, node);

  match std::fs::remove_dir_all(&folder) {
    Ok(()) => {}
    Err(why) if why.kind() == std::io::ErrorKind::NotFound => {}
    Err(why) => return Err(VerbError::Refused(format!("{}: {why}", folder.display()))),
  }

  Ok(run(db, r#"DELETE FROM "node_workplaces" WHERE "node" = $1"#, vec![node.into()]).await? > 0)
}

/// An isolation node taken down whole, each part that does not stand passed over.
pub async fn tear_down<C: ConnectionTrait>(
  db: &C,
  data_root: &Path,
  node: &str,
  reach: Reach,
) -> Result<TornDown, VerbError> {
  let isolation = isolation_of(node)?;
  let mut done = TornDown::default();

  if reach == Reach::Machine {
    stop_node_units(node).await;

    let wants = machine_wants();

    if let Some(uid) = uid_of(&isolation.user).await {
      if !wants.is_empty() {
        return Err(VerbError::Refused(format!(
          "{node}: the user {} could not be taken away: {}",
          isolation.user,
          wants.join("; ")
        )));
      }

      remove_user(&isolation, uid).await.map_err(VerbError::Refused)?;
      done.user = true;
      done
        .lines
        .push(format!("{node}: the user {} was taken away with its home", isolation.user));
    }

    // A table stands only where the sandbox stood it, which needs what the machine would want for it.
    if wants.is_empty() {
      done.firewall = drop_firewall(&isolation.user).await.map_err(VerbError::Refused)?;

      if done.firewall {
        done.lines.push(format!("{node}: its firewall table was taken away"));
      }
    }
  }

  done.workplace = forget_node(db, data_root, node).await?;

  if done.workplace {
    done.lines.push(format!("{node}: its workplace is no longer one"));
  }

  if done.lines.is_empty() {
    done.lines.push(format!("{node}: nothing of it stood"));
  }

  Ok(done)
}

// The user's numeric id, where it stands.
async fn uid_of(user: &str) -> Option<u32> {
  let done = tokio::process::Command::new("id")
    .args(["-u", user])
    .stdin(Stdio::null())
    .stderr(Stdio::null())
    .output()
    .await
    .ok()?;

  done
    .status
    .success()
    .then(|| String::from_utf8_lossy(&done.stdout).trim().parse().ok())?
}

// The ids of the processes running as a user, read off the proc files.
fn processes_of(uid: u32) -> Vec<u32> {
  let Ok(entries) = std::fs::read_dir("/proc") else {
    return vec![];
  };

  entries
    .filter_map(Result::ok)
    .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
    .filter(|pid| {
      std::fs::read_to_string(format!("/proc/{pid}/status")).is_ok_and(|status| {
        status
          .lines()
          .find_map(|line| line.strip_prefix("Uid:"))
          .and_then(|ids| ids.split_whitespace().next())
          .and_then(|real| real.parse::<u32>().ok())
          == Some(uid)
      })
    })
    .collect()
}

// The system units the processes given run in, each once: a sandbox is a transient unit of the system's.
fn units_of(pids: &[u32]) -> Vec<String> {
  let mut units: Vec<String> = pids
    .iter()
    .filter_map(|pid| std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok())
    .flat_map(|groups| {
      groups
        .lines()
        .filter_map(|line| {
          let path = line.rsplit(':').next()?;
          let unit = path.strip_prefix("/system.slice/")?.split('/').next()?;

          (unit.ends_with(".service") || unit.ends_with(".scope")).then(|| unit.to_owned())
        })
        .collect::<Vec<_>>()
    })
    .collect();

  units.sort();
  units.dedup();
  units
}

// One program of the policy's run as root, by its path.
async fn as_root(program: &str, args: &[&str]) -> super::work_node_setup::ShellRun {
  let mut command = tokio::process::Command::new("sudo");

  command.arg("-n").arg(program).args(args);
  captured(command).await
}

// The user taken away: the system units its programs run in stopped, its processes still left killed and waited
// on, and the user with its home removed. Done only once the user and its home no longer stand.
async fn remove_user(isolation: &Isolation, uid: u32) -> Result<(), String> {
  let programs = &ACCOUNT_ISOLATION.user_programs;
  let user = isolation.user.as_str();

  if !account_user(user) {
    return Err(format!("{user} is no isolation node's user"));
  }

  let units = units_of(&processes_of(uid));

  // A unit already collected, whose processes outlived its main one, is not loaded to stop: the kill below takes
  // what is left of it, so a stop that is refused is nothing to wait on.
  if !units.is_empty() {
    let mut args = vec!["stop", "--"];

    args.extend(units.iter().map(String::as_str));
    as_root(&programs.systemctl, &args).await;
  }

  // pkill ends with 1 where no process matched, which is nothing to do.
  let killed = as_root(&programs.pkill, &["-KILL", "-u", user]).await;

  if killed.code > 1 {
    return Err(format!("the processes of {user} could not be killed: {}", said_tail(&killed.stderr)));
  }

  let bound = tokio::time::Instant::now() + PROCESSES_END_WITHIN;

  while !processes_of(uid).is_empty() {
    if tokio::time::Instant::now() >= bound {
      return Err(format!("the processes of {user} did not end once killed"));
    }

    tokio::time::sleep(Duration::from_millis(100)).await;
  }

  let removed = as_root(&programs.userdel, &["--remove", user]).await;

  if uid_of(user).await.is_some() || isolation.home.symlink_metadata().is_ok() {
    return Err(format!(
      "the user {user} could not be taken away with its home: {}",
      said_tail(&removed.stderr)
    ));
  }

  Ok(())
}

// The user's firewall table taken away; whether it stood.
async fn drop_firewall(user: &str) -> Result<bool, String> {
  if !account_user(user) {
    return Err(format!("{user} is no isolation node's user"));
  }

  let dropped = as_root(&ACCOUNT_ISOLATION.user_programs.nft, &["delete", "table", "inet", user]).await;

  match dropped.code {
    0 => Ok(true),
    _ if dropped.stderr.contains(NO_TABLE) => Ok(false),
    _ => Err(format!(
      "the firewall table of {user} could not be taken away: {}",
      said_tail(&dropped.stderr)
    )),
  }
}
