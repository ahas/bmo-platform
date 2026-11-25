//! Where the agent host keeps what it writes for itself, under one data root (docs/agent-control.md,
//! 「The host」): its directory beside the envelopes in the executions directory.

use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::disk::plain_name;

/// The host's own directory's name among the envelopes, which no envelope may take.
pub const OWN_DIRECTORY: &str = "agent-host";

/// The directory name `hosts` among the envelopes, which no envelope may take either.
pub const BUN_HOSTS: &str = "hosts";

/// A name as one path component: itself where it is a plain word of at most 64, a hash otherwise.
pub fn name_component(name: &str) -> String {
  if plain_name(name) && name.len() <= 64 {
    return name.to_owned();
  }

  let digest: String = Sha256::digest(name.as_bytes())
    .iter()
    .take(8)
    .map(|byte| format!("{byte:02x}"))
    .collect();

  format!("h-{digest}")
}

pub struct Layout {
  pub executions: PathBuf,
}

impl Layout {
  pub fn new(data_root: &Path) -> Self {
    Self {
      executions: data_root.join("executions"),
    }
  }

  pub fn home(&self) -> PathBuf {
    self.executions.join(OWN_DIRECTORY)
  }

  pub fn claim(&self) -> PathBuf {
    self.home().join("claim")
  }

  pub fn inbox(&self) -> PathBuf {
    self.home().join("inbox")
  }

  pub fn taken(&self) -> PathBuf {
    self.home().join("taken")
  }

  pub fn views(&self) -> PathBuf {
    self.home().join("views")
  }

  /// The head records, one directory per agent.
  pub fn heads(&self) -> PathBuf {
    self.home().join("heads")
  }

  /// One agent's view file.
  pub fn view(&self, agent: &str) -> PathBuf {
    self.views().join(name_component(agent))
  }

  /// One agent's directory of head records.
  pub fn agent_heads(&self, agent: &str) -> PathBuf {
    self.heads().join(name_component(agent))
  }

  /// What the host keeps under one agent's name, which goes once the agent itself is gone.
  pub fn agent_files(&self, agent: &str) -> [PathBuf; 2] {
    [self.view(agent), self.agent_heads(agent)]
  }

  /// The socket heads and launchers ask the host on.
  pub fn socket(&self) -> PathBuf {
    self.executions.join("agent-host.sock")
  }

  /// The envelope a pointer names, where it is one the host may supervise: directly in the
  /// executions directory, under a plain name that is not one of the directories kept there.
  pub fn envelope(&self, named: &str) -> Option<(PathBuf, String)> {
    let path = normalised(Path::new(named))?;
    let name = path.file_name()?.to_str()?.to_owned();

    (path.parent() == Some(self.executions.as_path())
      && plain_name(&name)
      && name != OWN_DIRECTORY
      && name != BUN_HOSTS)
      .then_some((path, name))
  }
}

// An absolute path with `.` and `..` worked out by their words alone, as a pointer is read.
fn normalised(path: &Path) -> Option<PathBuf> {
  if !path.is_absolute() {
    return None;
  }

  let mut out = PathBuf::new();

  for part in path.components() {
    match part {
      Component::ParentDir => {
        out.pop();
      }
      Component::CurDir => {}
      other => out.push(other),
    }
  }

  Some(out)
}
