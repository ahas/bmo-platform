//! One execution's private directory, as the launcher writes it and the supervisor answers it
//! (docs/agent-control.md, 「The host」): what to run, the grant, the cancel file, the start
//! acknowledgement, the raw output and the outcome. The field names are the TypeScript launcher's.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::identity::ProcessIdentity;

/// The program an execution runs.
#[derive(Clone, Debug, Deserialize)]
pub struct Command {
  pub args: Vec<String>,
  pub cwd: PathBuf,
  #[serde(default)]
  pub env: BTreeMap<String, Option<String>>,
  #[serde(default)]
  pub stdin: Option<String>,
}

/// The store the child's start is checked against, the names a standing cancellation refuses the
/// child under, and the provider whose switch it reads.
#[derive(Clone, Debug, Deserialize)]
pub struct Policy {
  /// The store's PostgreSQL address.
  pub address: String,
  pub provider: String,
  #[serde(default)]
  pub names: Vec<String>,
  /// The providers the launcher's build knows, which the stored order is read against where given.
  #[serde(default)]
  pub providers: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
  pub command: Command,
  #[serde(default)]
  pub title: Option<String>,
  /// The collector that owns the execution; an execution with none is detached.
  #[serde(default)]
  pub owner: Option<ProcessIdentity>,
  #[serde(default)]
  pub policy: Option<Policy>,
}

/// How an execution ended, as the collector reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Cause {
  Exited,
  Cancelled,
  LaunchError,
}

impl Cause {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Exited => "exited",
      Self::Cancelled => "cancelled",
      Self::LaunchError => "launch-error",
    }
  }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
  pub exit_code: i32,
  pub cause: Cause,
}

/// The start acknowledgement: the child, and the supervisor answering for it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Started {
  pub pid: i32,
  pub birth: String,
  pub supervisor: ProcessIdentity,
}

/// The files of one envelope directory.
pub struct EnvelopeDir(pub PathBuf);

impl EnvelopeDir {
  pub fn path(&self, name: &str) -> PathBuf {
    self.0.join(name)
  }

  pub fn has(&self, name: &str) -> bool {
    self.path(name).exists()
  }

  pub fn command(&self) -> PathBuf {
    self.path("command.json")
  }

  pub fn outcome(&self) -> PathBuf {
    self.path("outcome.json")
  }

  pub fn started(&self) -> PathBuf {
    self.path("started.json")
  }

  /// The supervisor holding the envelope, created exclusively by the one that runs it.
  pub fn supervisor(&self) -> PathBuf {
    self.path("supervisor")
  }

  pub fn dir(&self) -> &Path {
    &self.0
  }
}
