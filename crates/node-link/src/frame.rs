//! What the two ends of a node's link say to each other over the SSH session's standing input and
//! output (docs/work-nodes.md, 「The link」): one JSON object per line, file bytes in base64.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};

/// One file or tree the far end watches and sends back.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pull {
  /// The path on the node.
  pub remote: String,
  /// Only these names under it, where the pull is a directory of named files; none for every file.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub files: Vec<String>,
  /// Every file under it, at any depth, rather than the files directly in it.
  #[serde(default, skip_serializing_if = "is_false")]
  pub tree: bool,
  /// Files that only grow, sent as what they grew by.
  #[serde(default, skip_serializing_if = "is_false")]
  pub append: bool,
  /// The place's running record, whose processes are told alive or not.
  #[serde(default, skip_serializing_if = "is_false")]
  pub alive: bool,
}

// A switch left off is left out, as a spec's writer leaves it out.
fn is_false(on: &bool) -> bool {
  !on
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub enum Frame {
  /// The far end, up: the node's home, its own process and the search path its login carries.
  Hello { home: String, pid: u32, path: String },
  /// The near end's first words on a mirror session: the node its link is for, whose newest session alone the far end
  /// keeps, beside the sessions of other nodes (docs/person-nodes.md, 「The link over jake」).
  Near { node: String },
  /// What the far end is to watch, replacing whatever it watched before, and how much of each growing file the
  /// near end already holds, which the far end goes on from rather than sending it again. A near end that says
  /// nothing of what it holds is sent every file whole.
  ///
  /// A list the near end numbers goes on with what was sent of each pull it still names, every frame of what is
  /// watched naming the list it was read under, so a frame on its way when the list moved lands where its own list
  /// put it. A list numbered by none is sent from nothing whenever it changes, since its numbering may have moved.
  Watch {
    pulls: Vec<Pull>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    held: Vec<Held>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    list: Option<u64>,
  },
  /// A file written whole, at its path on the receiving end's side of the pull or push it belongs to.
  Put {
    pull: Option<usize>,
    path: String,
    data: String,
    mode: Option<u32>,
    /// The watch list the frame was read under, by the number the near end gave it; none from a far end that
    /// numbers no list, or for what the near end pushes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    list: Option<u64>,
  },
  /// Bytes a growing file grew by, from the offset they start at.
  Append {
    pull: usize,
    path: String,
    offset: u64,
    data: String,
    /// The watch list the frame was read under, by the number the near end gave it; none from a far end that
    /// numbers no list, or for what the near end pushes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    list: Option<u64>,
  },
  /// A file that is no longer there.
  Gone {
    pull: Option<usize>,
    path: String,
    /// The watch list the frame was read under, by the number the near end gave it; none from a far end that
    /// numbers no list, or for what the near end pushes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    list: Option<u64>,
  },
  /// The folders the programs placed on the node work in, each made where it is not.
  Folders { paths: Vec<String> },
  /// Which processes a place's running record names are alive, by the pull that watches it.
  Alive {
    pull: usize,
    alive: Vec<Identity>,
    /// The watch list the frame was read under, by the number the near end gave it; none from a far end that
    /// numbers no list, or for what the near end pushes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    list: Option<u64>,
  },
  /// The node's load, memory and pressure files, by their names under the proc root.
  Proc { files: Vec<(String, String)> },
  /// A signal for a process on the node, sent only where it is still the process recorded.
  Signal {
    pid: i32,
    birth: Option<String>,
    signal: i32,
  },
  /// The far end's answer to anything it could not do, said once.
  Trouble { said: String },
  /// The ports here a program on a person's PC reaches, which the far end listens on at its loopback and dials
  /// home, replacing whatever this session asked for before (docs/person-nodes.md, 「The link over jake」).
  Carry { ports: Vec<u16> },
  /// The far end's answer to what it was asked to carry: each port it could not listen on, with why.
  Carried { refused: Vec<Refused> },
  /// A connection the far end took and holds, to be dialed home by the number it gave it.
  Dial { id: u64, to: Home },
  /// The first words of the near end on a stream it opened to dial a held connection home: the stream is that
  /// connection's from here on.
  Back { id: u64 },
  /// The first words of the near end on a stream it opened for one call of a personal tool of the account named
  /// (docs/person-nodes.md, 「Personal tools on a PC」): the stream is that call's passage to a runner from here on,
  /// and its end the call's caller gone.
  Run { owner: String },
  /// The first words of the near end on a stream it opened for a person's local AI (docs/person-nodes.md, 「A local
  /// AI on a PC」): the host and port the PC reaches it at, which the far end connects to and joins the stream to.
  Reach { to: String },
  /// The far end's answer to a `reach`: none where it reached the address and joined the stream to it, else why not,
  /// and the stream ends.
  Reached {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    why: Option<String>,
  },
}

/// What a connection the far end took reaches at home.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Home {
  /// A port at this machine's loopback, one of those the near end asked the far end to carry.
  Port(u16),
  /// The home bridge's socket.
  Bridge,
}

/// How much of a growing file the near end holds: the file by the pull it belongs to and its path under it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Held {
  pub pull: usize,
  pub path: String,
  pub len: u64,
}

/// A port the far end could not listen on, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refused {
  pub port: u16,
  pub why: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
  pub pid: i32,
  pub birth: String,
}

pub fn encode(bytes: &[u8]) -> String {
  STANDARD.encode(bytes)
}

pub fn decode(text: &str) -> Option<Vec<u8>> {
  STANDARD.decode(text).ok()
}

impl Frame {
  pub fn line(&self) -> String {
    let mut line = serde_json::to_string(self).unwrap_or_default();

    line.push('\n');
    line
  }

  pub fn read(line: &str) -> Option<Frame> {
    serde_json::from_str(line.trim_end()).ok()
  }
}
