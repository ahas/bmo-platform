//! The deployment's names (docs/policy.md, 「The deployment's names」): what its agent and its owner are
//! called in the records and in a room. Each is the deployment's configuration: the environment variable
//! the part names beside it, or the part's neutral default where that is unset or empty.

use serde::Deserialize;

/// One name: its neutral default and the variable a deployment names its own in.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Name {
  pub default: String,
  pub variable: String,
}

impl Name {
  /// The name read through `variable`: its value where it holds one that is not blank, else the default.
  pub fn read(&self, variable: impl Fn(&str) -> Option<String>) -> String {
    variable(&self.variable)
      .map(|value| value.trim().to_owned())
      .filter(|value| !value.is_empty())
      .unwrap_or_else(|| self.default.clone())
  }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeploymentNames {
  /// The name the deployment's agent stands under in the records.
  pub agent_key: Name,
  /// What the deployment's agent is called by in a room.
  pub agent_name: Name,
  /// What the deployment calls its owner, where no road ever showed the owner by a name.
  pub owner_name: Name,
  /// The name the deployment's agent's own messages are stored under.
  pub sender_name: Name,
}

/// The names as read, each one settled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deployment {
  pub agent_key: String,
  pub agent_name: String,
  pub owner_name: String,
  pub sender_name: String,
}

impl DeploymentNames {
  /// Every name read through `variable`.
  pub fn read(&self, variable: impl Fn(&str) -> Option<String>) -> Deployment {
    Deployment {
      agent_key: self.agent_key.read(&variable),
      agent_name: self.agent_name.read(&variable),
      owner_name: self.owner_name.read(&variable),
      sender_name: self.sender_name.read(&variable),
    }
  }
}

crate::part!(pub DEPLOYMENT_NAMES: DeploymentNames = "deployment.toml");

/// The names this process runs under, read from its environment once, on first use.
pub static DEPLOYMENT: std::sync::LazyLock<Deployment> =
  std::sync::LazyLock::new(|| DEPLOYMENT_NAMES.read(|name| std::env::var(name).ok()));
