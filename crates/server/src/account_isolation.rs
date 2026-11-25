//! An account's isolation node on this machine (docs/account-isolation.md): whose runs keep the whole machine, the
//! node, system user and home every other account is given here, and the sandbox its programs run in.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use policy::account_isolation::ACCOUNT_ISOLATION;
use policy::fill_marks;
use sea_orm::ConnectionTrait;

use crate::Refusal;

/// The name of the bridge socket and of the herdr socket in the link's folder.
const BRIDGE_SOCKET: &str = "bridge.sock";
const HERDR_SOCKET: &str = "herdr.sock";

/// The account the store names as administering the platform, the owner's own login account, or none where it
/// names none.
pub async fn administrator<C: ConnectionTrait>(db: &C) -> Result<Option<String>, Refusal> {
  let held = crate::queue::settings::get(db, store::settings::ADMINISTRATOR).await?;

  Ok(
    held
      .as_deref()
      .and_then(crate::control::claude_accounts::leading_integer)
      .map(|id| id.to_string()),
  )
}

/// Whether an account's runs are the owner's own, which keep the whole machine: the administrator's account.
pub async fn owners_own<C: ConnectionTrait>(db: &C, account: &str) -> Result<bool, Refusal> {
  Ok(administrator(db).await?.as_deref() == Some(account))
}

/// One account's isolation node: the account, the node's name, the system user it runs as and that user's home.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Isolation {
  pub account: String,
  pub node: String,
  pub user: String,
  pub home: PathBuf,
}

impl Isolation {
  /// The isolation node of a login account, by its id; none for a name that is no login id, which nothing of this
  /// machine is made for.
  pub fn of_account(account: &str) -> Option<Self> {
    if account.is_empty() || !account.bytes().all(|byte| byte.is_ascii_digit()) {
      return None;
    }

    let policy = &*ACCOUNT_ISOLATION;
    let user = format!("{}{account}", policy.user_prefix);

    Some(Self {
      account: account.to_owned(),
      node: node_name(account),
      home: PathBuf::from(&policy.home_base).join(&user),
      user,
    })
  }

  /// The isolation node a node's name stands for, or none for any other node.
  pub fn of_node(node: &str) -> Option<Self> {
    Self::of_account(node.strip_prefix(&ACCOUNT_ISOLATION.node_prefix)?)
  }

  /// The folder the link's two sockets stand in.
  pub fn link_folder(&self) -> PathBuf {
    self.home.join(&ACCOUNT_ISOLATION.link_folder)
  }

  /// The folder every program of the node sees as its temporary folders.
  pub fn scratch(&self) -> PathBuf {
    self.home.join(&ACCOUNT_ISOLATION.scratch_folder)
  }

  /// The home bridge's socket the near end holds and the node's programs reach.
  pub fn bridge_socket(&self) -> PathBuf {
    self.link_folder().join(BRIDGE_SOCKET)
  }

  /// The socket the far end joins to the node's herdr session, which the owner's user reaches.
  pub fn herdr_socket(&self) -> PathBuf {
    self.link_folder().join(HERDR_SOCKET)
  }

  /// The words every program of the node is started under, ahead of its own command line, with `path` the search
  /// path its programs are handed and `ports` the ports of this machine's loopback the node's network rule leaves
  /// it: the tool host's, and the model gateway's where it stands there.
  pub fn sandbox(&self, path: &str, ports: &[u16]) -> Vec<String> {
    let home = self.home.display().to_string();
    let scratch = self.scratch().display().to_string();
    let firewall = self.firewall(ports);
    let marks = [
      ("user", self.user.as_str()),
      ("home", home.as_str()),
      ("scratch", scratch.as_str()),
      ("path", path),
      ("nft", ACCOUNT_ISOLATION.user_programs.nft.as_str()),
      ("firewall", firewall.as_str()),
    ];

    ACCOUNT_ISOLATION.sandbox.iter().map(|word| fill_marks(word, &marks)).collect()
  }

  /// The firewall's script of the node's network rule, which replaces the user's own table whole and leaves the
  /// user, of this machine and the private networks, the ports given on the loopback and the stub resolver alone.
  pub fn firewall(&self, ports: &[u16]) -> String {
    let ports = ports.iter().map(u16::to_string).collect::<Vec<_>>().join(", ");

    fill_marks(&ACCOUNT_ISOLATION.firewall, &[("user", self.user.as_str()), ("ports", ports.as_str())])
  }
}

/// What this machine lacks to stand an isolation node, a line each: a program that makes the node's user missing
/// from the path the policy names it by, or a system manager that is not systemd, which the sandbox needs. None
/// where it lacks nothing.
pub fn machine_wants() -> Vec<String> {
  let policy = &*ACCOUNT_ISOLATION;
  let mut wants: Vec<String> = policy
    .user_programs
    .each()
    .into_iter()
    .filter(|(_, path)| !runnable(Path::new(path)))
    .map(|(name, path)| format!("{name} is not installed at {path}"))
    .collect();

  if !Path::new(&policy.system_manager_mark).is_dir() {
    wants.push(format!(
      "systemd is not this machine's system manager, as {} does not stand",
      policy.system_manager_mark
    ));
  }

  wants
}

// Whether a path is a file some user may run.
fn runnable(path: &Path) -> bool {
  std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// The isolation node's name for an account, whatever the account is.
pub fn node_name(account: &str) -> String {
  format!("{}{account}", ACCOUNT_ISOLATION.node_prefix)
}
