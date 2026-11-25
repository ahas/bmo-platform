//! What the chat receiver writes for one node's link and the link reads (docs/work-nodes.md, 「The
//! link」): how the node is logged into, what is forwarded, what is mirrored each way and how this
//! machine's paths read on the node. The receiver writes it whole whenever any of it moves, and the
//! link reads it again.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::frame::Pull;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Spec {
  pub node: String,
  /// OpenSSH's arguments up to and including the destination, as the shared policy composes a batch
  /// login: every option and the account at the host.
  pub ssh: Vec<String>,
  /// The workplace's root on the node, and the link binary standing in it.
  pub root: String,
  pub binary: String,
  /// The node's herdr session socket and where it is forwarded to here.
  pub herdr: Forward,
  /// Ports of this machine's loopback the node's loopback reaches, the tool host's among them.
  #[serde(default)]
  pub ports: Vec<u16>,
  /// The socket here the node's home bridge reaches, and the tree the scripts it runs stand in.
  pub bridge: PathBuf,
  pub deploy: PathBuf,
  /// The socket here the bot asks the link on.
  pub control: PathBuf,
  /// This machine's roots as the node reads them, and the folders an account sharing the node keeps under a root
  /// of its own there, the longest first.
  #[serde(default)]
  pub rewrite: Vec<(String, String)>,
  #[serde(default)]
  pub push: Vec<Push>,
  /// What the node sends back, each with where it lands here.
  #[serde(default)]
  pub pull: Vec<Landing>,
  /// Where the node's proc files land, and where the link writes its own state.
  pub proc: PathBuf,
  pub state: PathBuf,
  /// The roots of this machine whose files a pushed program names go along with it; a file under
  /// one of `withheld` never does, which is where this machine keeps its logins.
  #[serde(default)]
  pub follow: Vec<String>,
  #[serde(default)]
  pub withheld: Vec<PathBuf>,
  /// The files under a withheld root that go along all the same where a pushed program names them: the model
  /// gateway's key, which a program on a gateway road reads on the node (docs/work-nodes.md, 「The model gateway
  /// on a node」).
  #[serde(default)]
  pub handed: Vec<PathBuf>,
  /// The directories put ahead of the node's own search path in every program pushed there.
  #[serde(default)]
  pub path_head: Vec<String>,
  /// The variables of a pushed program's environment that go with it, one ending in `*` keeping every
  /// name it begins, and the node's own laid over them.
  #[serde(default)]
  pub env_keep: Vec<String>,
  #[serde(default)]
  pub env_add: Vec<(String, String)>,
  /// Each program word a pushed command names first with the node's own program run in its place: the word
  /// `claude` with the workplace's own Claude Code, where the node's setup laid one (docs/work-nodes.md, 「The
  /// link」).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub programs: Vec<(String, String)>,
  /// The folders on the node the programs of the places pushed there work in, made where they are not: a
  /// supervisor's own folder or a run's, which nothing else of the place makes there.
  #[serde(default)]
  pub folders: Vec<String>,
  /// The verbs of the CLI's `bmo` the node may ask the home bridge to run, each the first word of its call, where
  /// it is a person's node; none where it may ask for any, as a node of the deployment's own may.
  #[serde(default)]
  pub bridge_verbs: Option<Vec<String>>,
  /// A person's PC, reached through the jake hub alone in place of SSH (docs/person-nodes.md); none for an SSH
  /// node.
  #[serde(default)]
  pub jake: Option<Jake>,
  /// A node on this machine running as a user of its own, reached with neither SSH nor a hub
  /// (docs/account-isolation.md); none for any other node.
  #[serde(default)]
  pub local: Option<Local>,
  /// On a person's PC: the socket here a personal tool's call is asked on, carried to a runner on the PC
  /// (docs/person-nodes.md, 「Personal tools on a PC」); none for any other node.
  #[serde(default)]
  pub runner: Option<PathBuf>,
  /// The one account whose personal tools the PC runs; none where several accounts share it or none owns it, and
  /// the runner's socket refuses every call.
  #[serde(default)]
  pub runner_owner: Option<String>,
  /// On a person's PC: each local AI of the account the PC runs personal tools for, by the port of this machine's
  /// loopback the near end listens on for it and the host and port the PC reaches it at (docs/person-nodes.md, 「A
  /// local AI on a PC」); none for any other node.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub reach: Vec<Reach>,
}

/// A local AI's address the link carries from this machine's loopback to the PC.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reach {
  pub port: u16,
  pub to: String,
}

/// How the link reaches a node on this machine that runs as a user of its own: the words its far end is started
/// under, ahead of the far end's own command line, which run it as that user with its standard input and output the
/// near end's; and the socket the far end joins to the node's herdr session, which the near end joins the herdr
/// socket here to. The bridge socket the spec names is the near end's own, which the node's programs reach.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Local {
  pub run_as: Vec<String>,
  pub herdr: PathBuf,
}

/// How the link reaches a person's PC through the jake hub: the agent, the words the hub's connect program runs
/// with ahead of the agent and a port, the hub's settings file its control key stands in, and the far end's
/// loopback ports on the PC for the mirror, over which the road home is dialed too, and for the herdr socket.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Jake {
  pub agent: String,
  pub connect: Vec<String>,
  pub hub_env: PathBuf,
  pub mirror_port: u16,
  pub herdr_port: u16,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Forward {
  pub local: PathBuf,
  pub remote: String,
  /// The herdr session the socket is, which the far end starts where it does not answer.
  #[serde(default)]
  pub session: Option<String>,
}

/// A directory here whose named files go to the node.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Push {
  pub local: PathBuf,
  pub files: Vec<String>,
  /// Whether this machine's paths in a JSON file are rewritten to the node's, and every file under
  /// this machine's data root the file names sent along with it.
  #[serde(default)]
  pub rewrite: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Landing {
  #[serde(flatten)]
  pub pull: Pull,
  pub local: PathBuf,
}

impl Spec {
  pub fn read(path: &Path) -> Result<Spec, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;

    serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
  }

  pub fn pulls(&self) -> Vec<Pull> {
    self.pull.iter().map(|landing| landing.pull.clone()).collect()
  }

  /// A path of this machine as the node reads it, or itself where no root of this machine holds it.
  pub fn to_node(&self, path: &str) -> String {
    rewrite(path, self.rewrite.iter().map(|(here, there)| (here.as_str(), there.as_str())))
  }

  /// A path of the node as this machine reads it.
  pub fn here(&self, path: &str) -> String {
    rewrite(path, self.rewrite.iter().map(|(here, there)| (there.as_str(), here.as_str())))
  }
}

/// A path with the first root that holds it replaced; a root holds a path equal to it or one under it.
pub fn rewrite<'a>(path: &str, roots: impl Iterator<Item = (&'a str, &'a str)>) -> String {
  for (from, to) in roots {
    if path == from {
      return to.to_owned();
    }

    if let Some(rest) = path.strip_prefix(from)
      && rest.starts_with('/')
    {
      return format!("{to}{rest}");
    }
  }

  path.to_owned()
}
