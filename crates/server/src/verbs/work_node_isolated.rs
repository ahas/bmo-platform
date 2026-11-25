//! The setup's door to an account's isolation node (docs/account-isolation.md, 「Standing it up」): the account's
//! system user made where it is not, each command of the setup run in the sandbox as that user, and a copy streamed
//! by the owner's user and written by the account's, so nothing running as root writes inside the account's home.

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::process::Stdio;

use futures_util::future::BoxFuture;
use policy::account_isolation::ACCOUNT_ISOLATION;
use tokio::io::AsyncWriteExt;

use super::work_node::{captured, said_tail, say};
use super::work_node_setup::{COPY_LEFT_OUT, SetupHands, SetupShell, ShellRun, shell_word};
use crate::account_isolation::{Isolation, machine_wants};

/// The system's own directories every command of the setup starts on; the setup lays the workplace's ahead of them.
const SYSTEM_PATH: &str = "/usr/local/bin:/usr/bin:/bin";

/// The mode the link's folder holds: the owner's user's, shared with the account's group alone.
const LINK_FOLDER_MODE: u32 = 0o770;

/// The account's isolation node as the setup reaches it: its user, and the words its commands run under.
pub struct IsolatedDoor {
  isolation: Isolation,
  sandbox: Vec<String>,
}

impl IsolatedDoor {
  /// The door to an isolation node, its user made with its home, its scratch folder and the link's folder where
  /// the user is not and `make` lets the door write, or why it could not be; `ports` are the ports of this machine's
  /// loopback the sandbox's network rule leaves the node, the same its link's sandbox leaves it.
  pub async fn open(isolation: &Isolation, make: bool, ports: &[u16]) -> Result<Self, String> {
    if !user_exists(&isolation.user).await {
      if !make {
        return Err(format!("the user {} is not made; run the setup", isolation.user));
      }

      make_user(isolation).await?;
    }

    Ok(Self {
      sandbox: isolation.sandbox(SYSTEM_PATH, ports),
      isolation: isolation.clone(),
    })
  }

  /// What the account's user holds beyond its own, or what its home lacks, each a line the node is missing: a group
  /// but its own, or a link folder that is not the owner's own folder shared with the account's group alone.
  pub async fn wanting(&self) -> Vec<String> {
    let mut wanting = Vec::new();
    let user = &self.isolation.user;
    let groups = command_output("id", &["-Gn", user]).await.unwrap_or_default();
    let extra: Vec<&str> = groups.split_whitespace().filter(|group| group != user).collect();

    if !extra.is_empty() {
      wanting.push(format!(
        "isolation: {user} is a member of {}, and an isolated account's user holds its own group alone",
        extra.join(", ")
      ));
    }

    let folder = self.isolation.link_folder();
    // SAFETY: getuid reads the process's own user id and cannot fail.
    let owner = unsafe { libc::getuid() };
    let shared = std::fs::symlink_metadata(&folder).is_ok_and(|meta| {
      meta.file_type().is_dir()
        && meta.uid() == owner
        && meta.permissions().mode() & 0o7777 == LINK_FOLDER_MODE
    });

    if !shared {
      wanting.push(format!(
        "isolation: {} is not a folder of this machine's user with mode {LINK_FOLDER_MODE:o}, which the link's sockets need",
        folder.display()
      ));
    }

    wanting
  }

  fn command(&self, line: &[&str]) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(&self.sandbox[0]);

    command.args(&self.sandbox[1..]).args(line);
    command
  }
}

impl SetupShell for IsolatedDoor {
  fn run<'a>(&'a self, command: &'a str) -> BoxFuture<'a, ShellRun> {
    Box::pin(async move { captured(self.command(&["/bin/bash", "-c", command])).await })
  }
}

/// What the setup may do on this machine's side for an isolation node: copy as the account's user; nothing is saved
/// in herdr, since the node is reached through its link alone.
pub struct IsolatedHands<'a> {
  pub install: bool,
  pub copy_code: bool,
  pub door: &'a IsolatedDoor,
}

impl SetupHands for IsolatedHands<'_> {
  fn install(&self) -> bool {
    self.install
  }

  fn copy_code(&self) -> bool {
    self.copy_code
  }

  fn copy<'a>(&'a self, from: &'a str, to: &'a str) -> BoxFuture<'a, (bool, String)> {
    Box::pin(async move {
      match from.strip_suffix('/') {
        Some(tree) => self.copy_tree(tree, to.trim_end_matches('/')).await,
        None => self.copy_file(from, to).await,
      }
    })
  }

  fn add_machine(&self) -> BoxFuture<'_, (bool, String)> {
    Box::pin(async { (true, "already reached through its link alone".to_owned()) })
  }

  fn say(&self, line: &str) {
    say(line);
  }
}

impl IsolatedHands<'_> {
  // A tree read by this machine's user and written by the account's: packed here, unpacked into a folder of its
  // own beside the target, and laid over the target with what is left out of the copy kept there.
  async fn copy_tree(&self, tree: &str, to: &str) -> (bool, String) {
    let mut pack = tokio::process::Command::new("tar");

    pack.args(["-C", tree, "-cf", "-"]);

    for pattern in COPY_LEFT_OUT {
      pack.arg(format!("--exclude={}", tar_pattern(pattern)));
    }

    pack.arg(".");

    let excludes = COPY_LEFT_OUT
      .iter()
      .map(|pattern| format!("--exclude {}", shell_word(pattern)))
      .collect::<Vec<_>>()
      .join(" ");
    let stage = format!("{to}.incoming");
    let unpack = format!(
      "set -e; rm -rf {stage}; mkdir -p {stage} {to}; tar -C {stage} -xf -; rsync -a --delete {excludes} {stage}/ {to}/; rm -rf {stage}",
      stage = shell_word(&stage),
      to = shell_word(to),
    );

    self.piped(pack, &unpack).await
  }

  // A file read by this machine's user and written by the account's, beside the target and moved over it, keeping
  // whether it is a program.
  async fn copy_file(&self, from: &str, to: &str) -> (bool, String) {
    let mode = std::fs::metadata(from).map_or(0o644, |meta| meta.permissions().mode() & 0o777);
    let beside = format!("{to}.incoming");
    let parent = Path::new(to).parent().map(|dir| dir.display().to_string()).unwrap_or_default();
    let mut read = tokio::process::Command::new("cat");

    read.arg(from);

    let write = format!(
      "set -e; mkdir -p {parent}; cat >{beside}; chmod {mode:o} {beside}; mv -f {beside} {to}",
      parent = shell_word(&parent),
      beside = shell_word(&beside),
      to = shell_word(to),
    );

    self.piped(read, &write).await
  }

  // What `reading` prints, handed to `writing` run in the sandbox as the account's user.
  async fn piped(&self, reading: tokio::process::Command, writing: &str) -> (bool, String) {
    carried(reading, self.door.command(&["/bin/bash", "-c", writing])).await
  }
}

/// What `reading` prints, handed to `writing` on its standard input: whether both ended well, else what either said.
/// A writer that ends before the reader has printed everything ends the carry, and the reader, its output no longer
/// read, ends on the broken pipe rather than waiting on a pipe nobody reads.
pub async fn carried(
  mut reading: tokio::process::Command,
  mut writing: tokio::process::Command,
) -> (bool, String) {
  writing.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
  reading.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());

  let (mut reader, mut writer) = match (reading.spawn(), writing.spawn()) {
    (Ok(reader), Ok(writer)) => (reader, writer),
    (Err(why), _) | (_, Err(why)) => return (false, why.to_string()),
  };
  let (Some(mut out), Some(mut input)) = (reader.stdout.take(), writer.stdin.take()) else {
    return (false, "the copy's pipe could not be opened".to_owned());
  };
  let carry = tokio::io::copy(&mut out, &mut input).await;
  let _ = input.shutdown().await;

  drop((out, input));

  let (read, written) = (reader.wait_with_output().await, writer.wait_with_output().await);

  match (carry, read, written) {
    (Ok(_), Ok(read), Ok(written)) if read.status.success() && written.status.success() => {
      (true, String::new())
    }
    (_, Ok(read), Ok(written)) => (
      false,
      said_tail(&format!(
        "{}{}",
        String::from_utf8_lossy(&read.stderr),
        String::from_utf8_lossy(&written.stderr)
      )),
    ),
    (_, Err(why), _) | (_, _, Err(why)) => (false, why.to_string()),
  }
}

// A pattern the copy leaves out, as tar reads one against the tree's members: one written from the tree's root is
// anchored to it, a folder's trailing slash dropped.
fn tar_pattern(pattern: &str) -> String {
  let bare = pattern.trim_end_matches('/');

  match bare.strip_prefix('/') {
    Some(rooted) => format!("./{rooted}"),
    None => bare.to_owned(),
  }
}

async fn user_exists(user: &str) -> bool {
  tokio::process::Command::new("id")
    .args(["-u", user])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .await
    .is_ok_and(|status| status.success())
}

async fn command_output(program: &str, args: &[&str]) -> Option<String> {
  let done = tokio::process::Command::new(program)
    .args(args)
    .stdin(Stdio::null())
    .output()
    .await
    .ok()?;

  done.status.success().then(|| String::from_utf8_lossy(&done.stdout).trim().to_owned())
}

// The account's system user made, as root, with its home fresh: a member of its own group alone, its home left to
// it with passage through it for this machine's user, its scratch folder, and the link's folder this machine's user
// owns and shares with the account's group. Every step runs on a home just made, so none of them follows a link the
// account's user laid. A machine that lacks a program of it, or the sandbox's system manager, is refused before
// any step runs.
async fn make_user(isolation: &Isolation) -> Result<(), String> {
  let wants = machine_wants();

  if !wants.is_empty() {
    return Err(format!("the user {} could not be made: {}", isolation.user, wants.join("; ")));
  }

  // SAFETY: getuid reads the process's own user id and cannot fail.
  let owner = unsafe { libc::getuid() };
  let programs = &ACCOUNT_ISOLATION.user_programs;
  let (useradd, chmod, setfacl, install) = (
    shell_word(&programs.useradd),
    shell_word(&programs.chmod),
    shell_word(&programs.setfacl),
    shell_word(&programs.install),
  );
  let user = shell_word(&isolation.user);
  let home = shell_word(&isolation.home.display().to_string());
  let steps = [
    format!("{useradd} --system --create-home --home-dir {home} --shell /bin/bash --user-group {user}"),
    format!("{chmod} 0700 {home}"),
    format!("{setfacl} -m u:{owner}:x {home}"),
    format!(
      "{install} -d -m 0700 -o {user} -g {user} {}",
      shell_word(&isolation.scratch().display().to_string())
    ),
    format!(
      "{install} -d -m {LINK_FOLDER_MODE:o} -o {owner} -g {user} {}",
      shell_word(&isolation.link_folder().display().to_string())
    ),
  ];
  let mut command = tokio::process::Command::new("sudo");

  command.args(["-n", "/bin/sh", "-c", &format!("set -e; {}", steps.join("; "))]);

  let ran = captured(command).await;

  if ran.code == 0 {
    say(&format!("{}: made the user {}", isolation.node, isolation.user));
    Ok(())
  } else {
    Err(format!("the user {} could not be made: {}", isolation.user, said_tail(&ran.stderr)))
  }
}
