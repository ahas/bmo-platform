//! What a head pane is to become, as the agent host answers it on its socket (docs/agent-control.md,
//! 「One standing pane per agent」): the agent's program, the agent's view, or letting go of the pane.
//! The question and the answer are one JSON line each.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::disk::{now_ms, private_dir_below, private_file, publish, read_json};
use crate::identity::ProcessIdentity;
use crate::layout::{Layout, name_component};
use crate::place::{
  FILES_WAIT_MS, Place, ResidentProgram, RunningProgram, StartFailure, in_a_loop, next_start,
  recent_starts, restart_delay,
};
use crate::signals::signal_process;

/// A head's question, or its report of a start that failed.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "kebab-case")]
pub enum Request {
  Ask {
    agent: String,
    pane: Option<String>,
    pid: i32,
    #[serde(rename = "hostHead")]
    host_head: bool,
  },
  Failed {
    agent: String,
    pid: i32,
    identity: String,
    reason: String,
  },
}

/// What the head is to become.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "become", rename_all = "kebab-case")]
pub enum Answer {
  Program {
    identity: String,
    args: Vec<String>,
    cwd: PathBuf,
    env: BTreeMap<String, String>,
    #[serde(rename = "waitMs")]
    wait_ms: u64,
  },
  View {
    path: PathBuf,
  },
  LetGo {
    reason: String,
  },
  Recorded,
  Refused {
    reason: String,
  },
}

/// What a head pane holds, recorded under its pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Holds {
  Program,
  View,
}

/// A head's record: the process, which after its `exec` is what it became, and what it holds.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadRecord {
  pub pid: i32,
  pub birth: String,
  pub holds: Holds,
  pub pane_id: String,
  #[serde(default)]
  pub host_head: bool,
  /// Always false: no head this host records is retiring.
  #[serde(default)]
  pub retiring: bool,
}

impl HeadRecord {
  fn identity(&self) -> ProcessIdentity {
    ProcessIdentity {
      pid: self.pid,
      birth: self.birth.clone(),
    }
  }
}

// How long a program is left unheld before a follower is ended for it, longer than its own pane's
// loop takes to start it again; and how long before a head opened for a host takes it.
const VIEW_GIVES_WAY: Duration = Duration::from_secs(3);
const HOST_HEAD_TAKES: Duration = Duration::from_secs(10);

/// The answering side of the host, shared by its socket and its loop.
pub struct Asks {
  data_root: PathBuf,
  layout: Layout,
  // Since when each agent's written program has stood unheld.
  unheld_since: HashMap<String, Instant>,
}

impl Asks {
  pub fn new(data_root: &Path) -> Self {
    Self {
      data_root: data_root.to_owned(),
      layout: Layout::new(data_root),
      unheld_since: HashMap::new(),
    }
  }

  fn heads_dir(&self, agent: &str) -> PathBuf {
    self.layout.agent_heads(agent)
  }

  fn view_path(&self, agent: &str) -> PathBuf {
    self.layout.view(agent)
  }

  /// An agent's live head records, those of processes gone removed.
  pub fn live_heads(&self, agent_dir: &Path) -> Vec<(PathBuf, HeadRecord)> {
    let Ok(entries) = fs::read_dir(agent_dir) else {
      return Vec::new();
    };

    entries
      .filter_map(|entry| Some(entry.ok()?.path()))
      .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
      .filter_map(|path| match read_json::<HeadRecord>(&path) {
        Some(record) if record.identity().alive() => Some((path, record)),
        _ => {
          let _ = fs::remove_file(&path);
          None
        }
      })
      .collect()
  }

  // The records of an agent's heads whose processes are gone, removed.
  fn drop_gone_heads(&self, agent: &str) {
    self.live_heads(&self.heads_dir(agent));
  }

  fn record_head(
    &self,
    agent: &str,
    pane: Option<&str>,
    head: &ProcessIdentity,
    holds: Holds,
    host_head: bool,
  ) {
    let Some(pane) = pane else {
      return;
    };
    let dir = self.heads_dir(agent);
    let record = HeadRecord {
      pid: head.pid,
      birth: head.birth.clone(),
      holds,
      pane_id: pane.to_owned(),
      host_head,
      retiring: false,
    };

    if let Err(why) = private_dir_below(&self.layout.executions, &dir)
      .and_then(|()| publish(&dir.join(format!("{}.json", name_component(pane))), &record))
    {
      eprintln!("agent-host: the head of {agent} in {pane} could not be recorded: {why}");
    }
  }

  // How long the agent's written program has stood with no live head holding it, or none where no
  // program is written, one is held, it rests or it waits for its files. The agent is named as its place's
  // directory is.
  fn unheld_for(&mut self, agent: &str, place: &Place, now: u64) -> Option<Duration> {
    let waiting = place.program().is_some_and(|program| files_gone(agent, place, &program, now));

    if waiting || place.program().is_none() || place.resting() || place.held_elsewhere(None, now) {
      self.unheld_since.remove(agent);
      return None;
    }

    Some(self.unheld_since.entry(agent.to_owned()).or_insert_with(Instant::now).elapsed())
  }

  /// One request answered.
  pub fn answer(&mut self, request: Request) -> Answer {
    match request {
      Request::Ask {
        agent,
        pane,
        pid,
        host_head,
      } => self.ask(&agent, pane.as_deref(), pid, host_head),
      Request::Failed {
        agent,
        pid,
        identity,
        reason,
      } => self.failed(&agent, pid, &identity, &reason),
    }
  }

  fn ask(&mut self, agent: &str, pane: Option<&str>, pid: i32, host_head: bool) -> Answer {
    let Some(head) = ProcessIdentity::of(pid) else {
      return Answer::Refused {
        reason: format!("no process {pid} is asking"),
      };
    };

    if agent.is_empty() {
      return Answer::Refused {
        reason: "no agent named".to_owned(),
      };
    }

    self.drop_gone_heads(agent);

    let now = now_ms();
    let place = Place::of(&self.data_root, agent);
    let unheld = self.unheld_for(&name_component(agent), &place, now);
    let waiting = place.program().is_some_and(|program| !program.missing().is_empty());
    let held = place.program().is_some() && unheld.is_none() && !place.resting() && !waiting;

    // A head opened for a host beside a pane holding the program is a display left over: the pane
    // is the program's, and this one lets go.
    if host_head && held {
      return Answer::LetGo {
        reason: format!("{agent}'s program stands in another pane"),
      };
    }

    if let (Some(program), Some(unheld)) = (place.program(), unheld)
      && (!host_head || unheld >= HOST_HEAD_TAKES)
    {
      let before = place.running();
      let start = next_start(&program, before.as_ref());
      let recent = recent_starts(before.as_ref(), &start.identity, now);

      // A program in a loop is started no more: the place records that it rests, and this head and every
      // one after it follow the view until the place changes.
      if let Some(before) = before.filter(|_| in_a_loop(&recent, now)) {
        return self.rest(agent, pane, &head, &place, before, recent, now, host_head);
      }

      let wait_ms = restart_delay(&recent, now);
      let started_at = now + wait_ms;
      let mut recent = recent;

      recent.push(started_at);

      // Recorded before the head waits or becomes the program, so the place reads as held meanwhile
      // and the launcher finds the program's process: the head's own, which `exec` keeps.
      let running = RunningProgram {
        identity: start.identity.clone(),
        pid: head.pid,
        birth: Some(head.birth.clone()),
        head: head.clone(),
        starts: start.starts + 1,
        started_at,
        pane_id: pane.map(str::to_owned),
        recent,
        pending_until: (wait_ms > 0).then_some(started_at),
        resting_since: None,
      };

      if let Err(why) = place.record_running(&running) {
        return Answer::Refused {
          reason: format!("{agent}'s start could not be recorded: {why}"),
        };
      }

      self.unheld_since.remove(&name_component(agent));
      self.record_head(agent, pane, &head, Holds::Program, host_head);

      // The login the program goes out on readied where it starts, so a configuration directory new to this
      // machine stops it at neither its first-run screens nor the folder check.
      if let Some(file) = &start.command.login_file {
        crate::trust::ready_login(file, &start.command.cwd);
      }

      return Answer::Program {
        identity: start.identity,
        args: start.command.args,
        cwd: start.command.cwd,
        env: start.command.env,
        wait_ms,
      };
    }

    self.view(agent, pane, &head, host_head)
  }

  // The head answered the agent's view, which it follows.
  fn view(&mut self, agent: &str, pane: Option<&str>, head: &ProcessIdentity, host_head: bool) -> Answer {
    let path = self.view_path(agent);

    if let Err(why) = private_dir_below(&self.layout.executions, &self.layout.views())
      .and_then(|()| if path.exists() { Ok(()) } else { private_file(&path).map(drop) })
    {
      return Answer::Refused {
        reason: format!("{agent}'s view could not be made: {why}"),
      };
    }

    self.record_head(agent, pane, head, Holds::View, host_head);
    Answer::View { path }
  }

  // The program in a loop put to rest: its starts kept, no process standing for it, and the moment it
  // came to rest recorded; the head is answered the view.
  #[allow(clippy::too_many_arguments)]
  fn rest(
    &mut self,
    agent: &str,
    pane: Option<&str>,
    head: &ProcessIdentity,
    place: &Place,
    before: RunningProgram,
    recent: Vec<u64>,
    now: u64,
    host_head: bool,
  ) -> Answer {
    let resting = RunningProgram {
      pid: 0,
      birth: None,
      recent,
      pending_until: None,
      resting_since: Some(now),
      ..before
    };

    if let Err(why) = place.record_running(&resting) {
      return Answer::Refused {
        reason: format!("{agent}'s rest could not be recorded: {why}"),
      };
    }

    eprintln!(
      "agent-host: {agent}'s program started {} times within five minutes and rests until its place changes",
      resting.recent.len()
    );
    self.unheld_since.remove(&name_component(agent));
    self.view(agent, pane, head, host_head)
  }

  // A program the head could not start: written into the place for the turn waiting on it, and the
  // start it was recorded as left counting, standing for no process.
  fn failed(&mut self, agent: &str, pid: i32, identity: &str, reason: &str) -> Answer {
    let place = Place::of(&self.data_root, agent);
    let at = now_ms();

    if let Err(why) = place.record_failure(&StartFailure {
      identity,
      at,
      reason,
    }) {
      return Answer::Refused {
        reason: format!("{agent}'s failed start could not be recorded: {why}"),
      };
    }

    if let Some(mut running) = place.running().filter(|running| running.pid == pid) {
      running.pid = 0;
      running.birth = None;
      running.pending_until = None;
      let _ = place.record_running(&running);
    }

    Answer::Recorded
  }

  /// Every follower ended whose agent now has a program no live head holds, so its pane's loop
  /// starts the head again and the head becomes the program.
  pub fn give_way(&mut self) {
    let Ok(agents) = fs::read_dir(self.layout.heads()) else {
      return;
    };
    let now = now_ms();

    for agent_dir in agents.filter_map(|entry| Some(entry.ok()?.path())) {
      let heads = self.live_heads(&agent_dir);
      let Some(agent) = agent_dir.file_name().and_then(|name| name.to_str()).map(str::to_owned) else {
        continue;
      };
      let followers: Vec<HeadRecord> = heads
        .into_iter()
        .map(|(_, record)| record)
        .filter(|record| record.holds == Holds::View)
        .collect();

      if followers.is_empty() {
        continue;
      }

      // The directory is the agent's place's own name, so the place is found by it.
      let place = Place::named(&self.data_root, &agent);
      let Some(unheld) = self.unheld_for(&agent, &place, now) else {
        continue;
      };

      for follower in followers {
        let due = if follower.host_head { HOST_HEAD_TAKES } else { VIEW_GIVES_WAY };

        if unheld >= due {
          signal_process(follower.pid, libc::SIGTERM);
        }
      }
    }
  }
}

/// Whether a file the program needs is gone, which keeps it from starting: no head becomes it and no view gives
/// way to it, so a program whose configuration was taken costs nothing, and it starts by itself once the file is
/// back. Once the program has stood written past the wait for its files with one still gone, the place records
/// why it does not start, for the turn waiting on it (docs/agent-control.md, 「One standing pane per supervisor」).
fn files_gone(agent: &str, place: &Place, program: &ResidentProgram, now: u64) -> bool {
  let missing = program.missing();
  let Some(first) = missing.first() else {
    return false;
  };

  if place.program_written_at().is_some_and(|at| now.saturating_sub(at) >= FILES_WAIT_MS) {
    let reason = format!("{}: a file its command names is not there", first.display());

    if !place.failure_holds(&program.identity, &reason) {
      eprintln!("agent-host: {agent}'s program is not started: {reason}");

      if let Err(why) = place.record_failure(&StartFailure {
        identity: &program.identity,
        at: now,
        reason: &reason,
      }) {
        eprintln!("agent-host: {agent}'s unstarted program could not be recorded: {why}");
      }
    }
  }

  true
}

/// One connection answered: a request line in, an answer line out. Its asker's line is waited for until it
/// comes or the connection closes.
fn serve_one(asks: &Mutex<Asks>, stream: UnixStream) -> io::Result<()> {
  let mut line = String::new();

  BufReader::new(&stream).read_line(&mut line)?;

  let answer = match serde_json::from_str::<Request>(&line) {
    Ok(request) => asks.lock().map_or_else(
      |_| Answer::Refused {
        reason: "the host is broken".to_owned(),
      },
      |mut asks| asks.answer(request),
    ),
    Err(why) => Answer::Refused {
      reason: format!("not a question: {why}"),
    },
  };
  let mut said = serde_json::to_vec(&answer)?;

  said.push(b'\n');
  (&stream).write_all(&said)
}

/// The socket bound, readable by the owner alone, and answered on a thread of its own, each connection on
/// one of its own, so an asker slow to say its line holds up nobody else's.
pub fn listen(socket: &Path, asks: Arc<Mutex<Asks>>) -> io::Result<()> {
  let _ = fs::remove_file(socket);

  let listener = UnixListener::bind(socket)?;

  fs::set_permissions(socket, fs::Permissions::from_mode(0o600))?;
  thread::spawn(move || {
    for stream in listener.incoming().flatten() {
      let asks = Arc::clone(&asks);

      thread::spawn(move || {
        if let Err(why) = serve_one(&asks, stream) {
          eprintln!("agent-host: a question went unanswered: {why}");
        }
      });
    }
  });

  Ok(())
}

/// A question put to the host at `socket`, as a head puts it, and its answer waited for until it comes or
/// the host closes the connection.
pub fn ask(socket: &Path, request: &Request) -> io::Result<Answer> {
  let stream = UnixStream::connect(socket)?;
  let mut said = serde_json::to_vec(request)?;

  said.push(b'\n');
  (&stream).write_all(&said)?;

  let mut line = String::new();

  BufReader::new(&stream).read_line(&mut line)?;
  serde_json::from_str(&line).map_err(io::Error::other)
}
