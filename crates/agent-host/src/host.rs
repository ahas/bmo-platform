//! The agent host's own loop, the `serve` verb (docs/agent-control.md, 「The host」): it claims its
//! data root, takes the pointers launchers leave in its inbox, starts a supervisor for each envelope
//! and writes down what it took, and writes the lines it knows into each agent's view. It never
//! waits on what it started to end: a supervisor outlives the host, and the next host watches it.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::ask::{self, Asks};
use crate::disk::{now_ms, plain_name, private_dir, publish, read_json};
use crate::envelope::{Envelope, EnvelopeDir, Outcome};
use crate::identity::ProcessIdentity;
use crate::layout::Layout;
use crate::signals;
use crate::supervise::OWN_PROGRAM;
use crate::view::{Views, began_line, ended_line, failure_lines, resting_line};

pub struct HostOptions {
  pub data_root: PathBuf,
  pub code_root: PathBuf,
  pub poll: Duration,
}

/// Why the host ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Ended {
  /// Another live host holds the data root; this one served nothing.
  Duplicate,
  /// Asked to end, or its claim was taken from it.
  Stopped,
  /// The data root was taken away, leaving nothing to serve.
  RootGone,
}

/// The claim, in the fields the TypeScript launchers read.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claim {
  pid: i32,
  birth: String,
  pane_id: Option<String>,
  revision: Option<String>,
  code_root: PathBuf,
  started_at: u64,
}

/// A launcher's pointer: the agent an execution belongs to, and its envelope.
#[derive(Deserialize)]
struct Pointer {
  agent: String,
  envelope: String,
}

/// What the host took, written down so the next host carries on with it.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Taken {
  agent: String,
  envelope: PathBuf,
  title: String,
  since: u64,
  owner: Option<ProcessIdentity>,
  supervisor: Option<ProcessIdentity>,
}

struct Watched {
  taken: Taken,
  // The supervisor this host started, reaped once it ends; an adopted one has none here.
  child: Option<Child>,
}

// How often the host makes sure its data root and its claim are still there, and how often it looks
// for a follower to end for a program.
const LOOK_AROUND: Duration = Duration::from_secs(2);
const GIVE_WAY_EVERY: Duration = Duration::from_millis(500);

// The revision a tree stands on, or none for a tree that is not a repository.
fn revision(root: &Path) -> Option<String> {
  let probe = Command::new("git")
    .args(["rev-parse", "HEAD"])
    .current_dir(root)
    .stderr(Stdio::null())
    .output()
    .ok()?;
  let said = String::from_utf8(probe.stdout).ok()?.trim().to_owned();

  (probe.status.success() && !said.is_empty()).then_some(said)
}

// How much of an execution's error stream is read back for its end line, from its end.
const STREAM_TAIL: u64 = 16 * 1024;

// The newest part of a stream an execution wrote, its first line dropped where the cut fell inside it;
// nothing where the stream was never written.
fn stream_tail(path: &Path) -> String {
  use std::io::{Read, Seek, SeekFrom};

  let Ok(mut file) = fs::File::open(path) else {
    return String::new();
  };
  let length = file.metadata().map(|meta| meta.len()).unwrap_or_default();
  let from = length.saturating_sub(STREAM_TAIL);
  let mut bytes = Vec::new();

  if file.seek(SeekFrom::Start(from)).is_err() || file.read_to_end(&mut bytes).is_err() {
    return String::new();
  }

  let text = String::from_utf8_lossy(&bytes).into_owned();

  match (from > 0, text.split_once('\n')) {
    (true, Some((_, rest))) => rest.to_owned(),
    _ => text,
  }
}

struct Host {
  layout: Layout,
  views: Views,
  me: ProcessIdentity,
  watched: BTreeMap<String, Watched>,
}

impl Host {
  fn taken_path(&self, name: &str) -> PathBuf {
    self.layout.taken().join(format!("{name}.json"))
  }

  fn view(&self, agent: &str, lines: &[String]) {
    if let Err(why) = self.views.write(agent, lines) {
      eprintln!("agent-host: {agent}'s view could not be written: {why}");
    }
  }

  // The claim taken exclusively: written whole beside its name and linked into place, so it is never
  // seen half written and two hosts can never both hold it.
  fn claim(&self, code_root: &Path) -> io::Result<bool> {
    let path = self.layout.claim();
    let draft = self.layout.home().join(format!("claim.{}", self.me.pid));
    let claim = Claim {
      pid: self.me.pid,
      birth: self.me.birth.clone(),
      pane_id: None,
      revision: revision(code_root),
      code_root: code_root.to_owned(),
      started_at: now_ms(),
    };

    publish(&draft, &claim)?;

    let taken = (|| {
      for _ in 0..5 {
        match fs::hard_link(&draft, &path) {
          Ok(()) => return Ok(true),
          Err(why) if why.kind() == io::ErrorKind::AlreadyExists => {
            if read_json::<ProcessIdentity>(&path).is_some_and(|held| held.alive()) {
              return Ok(false);
            }

            let _ = fs::remove_file(&path);
          }
          Err(why) => return Err(why),
        }
      }

      Err(io::Error::other("the claim could not be taken"))
    })();

    let _ = fs::remove_file(&draft);

    taken
  }

  fn owns_claim(&self) -> bool {
    read_json::<ProcessIdentity>(&self.layout.claim()).is_some_and(|held| held == self.me)
  }

  fn release_claim(&self) {
    if self.owns_claim() {
      let _ = fs::remove_file(self.layout.claim());
    }
  }

  // A supervisor started for an envelope, in a process group of its own so nothing aimed at the host
  // reaches it.
  fn start_supervisor(&self, envelope: &Path) -> Option<Child> {
    Command::new(OWN_PROGRAM)
      .arg0("agent-host")
      .arg("supervise")
      .arg(envelope)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .process_group(0)
      .spawn()
      .map_err(|why| eprintln!("agent-host: no supervisor for {}: {why}", envelope.display()))
      .ok()
  }

  // A taken envelope handed to a supervisor, and the supervisor written down.
  fn hand_over(&mut self, name: String, mut taken: Taken) {
    let child = self.start_supervisor(&taken.envelope);

    taken.supervisor = child.as_ref().and_then(|child| ProcessIdentity::of(child.id() as i32));

    if let Err(why) = publish(&self.taken_path(&name), &taken) {
      eprintln!("agent-host: what it took of {name} could not be written down: {why}");
    }

    self.watched.insert(name, Watched { taken, child });
  }

  // One pointer read and removed, and its envelope taken where it is one to take.
  fn take(&mut self, pointer: &Path) {
    let read = fs::read(pointer);
    let _ = fs::remove_file(pointer);
    let Some(said) = read.ok().and_then(|bytes| serde_json::from_slice::<Pointer>(&bytes).ok()) else {
      eprintln!("agent-host: {} is not a pointer, and is dropped", pointer.display());
      return;
    };
    let Some((envelope, name)) = self.layout.envelope(&said.envelope) else {
      eprintln!("agent-host: {} is not an envelope it may run, and is dropped", said.envelope);
      return;
    };

    if said.agent.is_empty() || self.watched.contains_key(&name) || self.taken_path(&name).exists() {
      return;
    }

    let Some(command) = read_json::<Envelope>(&EnvelopeDir(envelope.clone()).command()) else {
      eprintln!("agent-host: {} holds no command, and is dropped", envelope.display());
      return;
    };
    let taken = Taken {
      agent: said.agent,
      title: command.title.clone().unwrap_or_else(|| name.clone()),
      envelope,
      since: now_ms(),
      owner: command.owner,
      supervisor: None,
    };

    // Written down before the supervisor starts, so a host gone in between leaves it to the next.
    if let Err(why) = publish(&self.taken_path(&name), &taken) {
      eprintln!("agent-host: {name} could not be written down, and is not started: {why}");
      return;
    }

    self.view(&taken.agent, &[began_line(&taken.title, &name, taken.since)]);
    self.hand_over(name, taken);
  }

  fn take_inbox(&mut self) {
    let Ok(entries) = fs::read_dir(self.layout.inbox()) else {
      return;
    };
    let mut pointers: Vec<PathBuf> = entries
      .filter_map(|entry| Some(entry.ok()?.path()))
      .filter(|path| path.extension().is_none_or(|extension| extension != "new"))
      .collect();

    pointers.sort();

    for pointer in pointers {
      self.take(&pointer);
    }
  }

  // What an earlier host took, carried on with.
  fn adopt(&mut self) {
    let Ok(entries) = fs::read_dir(self.layout.taken()) else {
      return;
    };

    for path in entries.filter_map(|entry| Some(entry.ok()?.path())) {
      let Some(name) = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|name| plain_name(name))
        .map(str::to_owned)
      else {
        // A record a host gone mid-write left staged beside its name.
        let _ = fs::remove_file(&path);
        continue;
      };
      let Some(mut taken) = read_json::<Taken>(&path) else {
        let _ = fs::remove_file(&path);
        continue;
      };
      let dir = EnvelopeDir(taken.envelope.clone());

      // A supervisor started by a host gone before it could write it down still holds the envelope.
      if taken.supervisor.is_none() {
        taken.supervisor = read_json(&dir.supervisor());
      }

      if taken.supervisor.is_none() && !dir.has("outcome.json") && taken.envelope.exists() {
        self.hand_over(name, taken);
      } else {
        self.watched.insert(name, Watched { taken, child: None });
      }
    }
  }

  // Whether a watched execution is over for the host: the collector has taken its envelope away; a
  // detached one has published its outcome; or nobody is left to finish it.
  fn over(watched: &mut Watched) -> bool {
    if let Some(child) = watched.child.as_mut() {
      let _ = child.try_wait();
    }

    let dir = EnvelopeDir(watched.taken.envelope.clone());
    let published = dir.has("outcome.json");
    let supervised = watched.taken.supervisor.as_ref().is_some_and(ProcessIdentity::alive);
    let collected = watched.taken.owner.as_ref().is_some_and(ProcessIdentity::alive);

    !dir.dir().exists() || (published && !collected) || (!published && !supervised)
  }

  fn finish(&mut self) {
    let over: Vec<String> = self
      .watched
      .iter_mut()
      .filter_map(|(name, one)| Self::over(one).then(|| name.clone()))
      .collect();

    for name in over {
      let Some(mut one) = self.watched.remove(&name) else {
        continue;
      };

      if let Some(child) = one.child.as_mut() {
        let _ = child.try_wait();
      }

      let at = now_ms();
      let mut lines = Vec::new();

      // A detached execution has no collector to say how it ended.
      if one.taken.owner.is_none() {
        let dir = EnvelopeDir(one.taken.envelope.clone());

        if let Some(outcome) = read_json::<Outcome>(&dir.outcome()) {
          lines.push(ended_line(&one.taken.title, at, &outcome));
          lines.extend(failure_lines(&outcome, &stream_tail(&dir.path("stderr"))));
        }
      }

      if !self.watched.values().any(|other| other.taken.agent == one.taken.agent) {
        lines.push(resting_line(Some(at)));
      }

      self.view(&one.taken.agent, &lines);
      let _ = fs::remove_file(self.taken_path(&name));
    }
  }
}

/// Serves the data root until asked to end, or until there is nothing to serve.
pub fn serve(options: &HostOptions) -> io::Result<Ended> {
  signals::listen();

  let layout = Layout::new(&options.data_root);

  for dir in [layout.inbox(), layout.taken(), layout.views()] {
    private_dir(&dir)?;
  }

  let mut host = Host {
    views: Views::new(&layout),
    layout,
    me: ProcessIdentity::own(),
    watched: BTreeMap::new(),
  };

  if !host.claim(&options.code_root)? {
    return Ok(Ended::Duplicate);
  }

  host.adopt();

  // The socket is bound only once the root is this host's, so a second host never takes it over.
  let asks = Arc::new(Mutex::new(Asks::new(&options.data_root)));
  let socket = host.layout.socket();

  if let Err(why) = ask::listen(&socket, asks.clone()) {
    host.release_claim();
    return Err(io::Error::other(format!("the socket {} could not be bound: {why}", socket.display())));
  }

  let mut looked_at = Instant::now();
  let mut gave_way_at = Instant::now();
  let ended = loop {
    if signals::asked() {
      break Ended::Stopped;
    }

    host.take_inbox();
    host.finish();

    if looked_at.elapsed() >= LOOK_AROUND {
      looked_at = Instant::now();

      if !host.layout.executions.exists() {
        break Ended::RootGone;
      }

      if !host.owns_claim() {
        break Ended::Stopped;
      }
    }

    if gave_way_at.elapsed() >= GIVE_WAY_EVERY {
      gave_way_at = Instant::now();

      if let Ok(mut asks) = asks.lock() {
        asks.give_way();
      }
    }

    thread::sleep(options.poll);
  };

  let _ = fs::remove_file(&socket);
  host.release_claim();

  Ok(ended)
}
