//! An account's isolation node on this machine (docs/account-isolation.md): its name, the system user it runs as,
//! that user's home and the folders in it, and the sandbox every program of it runs in.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountIsolation {
  pub node_prefix: String,
  pub user_prefix: String,
  pub home_base: String,
  pub link_folder: String,
  pub scratch_folder: String,
  /// How long after a setup that left something missing it is run again by the links pass, in seconds.
  pub setup_again_seconds: i64,
  /// The folder that stands only where systemd is the system manager, which the sandbox needs.
  pub system_manager_mark: String,
  /// The words ahead of every program's command line, with `{user}`, `{home}`, `{scratch}`, `{path}`, `{nft}` and
  /// `{firewall}` marks.
  pub sandbox: Vec<String>,
  /// The firewall's script of the user's network rule, which the sandbox stands before its program starts, with
  /// `{user}` and `{tool_port}` marks.
  pub firewall: String,
  /// The programs that make the node's user and stand its firewall, and those its teardown takes them away with,
  /// run as root by their paths.
  pub user_programs: UserPrograms,
}

/// The programs that make an isolation node's user, its home and its folders, the firewall's program its sandbox
/// stands the network rule with, and the programs its teardown stops the user's units, ends its processes and
/// takes the user away with, each by the path it stands at.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserPrograms {
  pub useradd: String,
  pub chmod: String,
  pub setfacl: String,
  pub install: String,
  pub nft: String,
  pub systemctl: String,
  pub pkill: String,
  pub userdel: String,
}

impl UserPrograms {
  /// Each program by its name and the path it is run by.
  pub fn each(&self) -> [(&'static str, &str); 8] {
    [
      ("useradd", &self.useradd),
      ("chmod", &self.chmod),
      ("setfacl", &self.setfacl),
      ("install", &self.install),
      ("nft", &self.nft),
      ("systemctl", &self.systemctl),
      ("pkill", &self.pkill),
      ("userdel", &self.userdel),
    ]
  }
}

crate::part!(pub ACCOUNT_ISOLATION: AccountIsolation = "account-isolation.toml");
