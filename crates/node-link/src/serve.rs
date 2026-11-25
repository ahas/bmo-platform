//! The far end of a node's link (docs/work-nodes.md, 「The link」): started by the SSH session on the
//! node, it watches what the near end asks for and sends it back as it changes, writes what the near end
//! sends, tells which processes of a place are alive, carries a signal to one, and reads the node's
//! load and memory. It ends with the session.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

use crate::frame::{Frame, Held, Identity, Pull, decode, encode};
use crate::home::{Carrier, listen_again, take_bridge};

/// How often what is watched is looked at, how often a place's processes are told without a change,
/// and how often the node's load is read.
const LOOK_EVERY: Duration = Duration::from_millis(500);
const ALIVE_EVERY: Duration = Duration::from_secs(5);
const PROC_EVERY: Duration = Duration::from_secs(5);
/// The most of a file one frame carries.
const CHUNK: u64 = 512 * 1024;
/// The most files one tree is walked for.
const TREE_MOST: usize = 5000;
/// The most frames of files read ahead of what the connection has carried: enough to keep it busy, and few
/// enough that what the link says of its own waits behind no more than a few chunks.
const FILES_AHEAD: usize = 4;

/// The proc files a machine's standing is read from (docs/agent-control.md, 「Room on the machine」).
const PROC_FILES: [&str; 5] = ["loadavg", "meminfo", "stat", "pressure/memory", "pressure/cpu"];

#[derive(Clone, Copy, PartialEq, Eq)]
struct Seen {
  len: u64,
  modified: i64,
  modified_ns: i64,
  inode: u64,
}

fn seen(meta: &fs::Metadata) -> Seen {
  Seen {
    len: meta.len(),
    modified: meta.mtime(),
    modified_ns: meta.mtime_nsec(),
    inode: meta.ino(),
  }
}

pub struct Serve {
  pub root: PathBuf,
  pub bridge: Option<PathBuf>,
  /// The node's herdr session, by its name and its socket: started where its socket does not answer.
  pub herdr_session: Option<String>,
  pub herdr_socket: Option<PathBuf>,
  /// On a person's PC (docs/person-nodes.md): the loopback address the mirror is spoken on in place of standard
  /// input and output, one session after another, over which the road home is dialed too; and the loopback address
  /// the herdr socket is reached at.
  pub listen: Option<String>,
  pub herdr_listen: Option<String>,
  /// On a node of this machine running as a user of its own (docs/account-isolation.md): the socket the herdr
  /// socket is reached at by the owner's user, in the folder the two share.
  pub herdr_unix: Option<PathBuf>,
}

/// The mode of the socket the owner's user reaches a node's herdr session at, on a node of this machine running as a
/// user of its own: open to every user that reaches its folder, which the folder's own mode leaves to the two.
const SHARED_SOCKET_MODE: u32 = 0o666;

/// How often what must stand on the node is made sure of.
const STANDING_EVERY: Duration = Duration::from_secs(30);

/// A program started apart from this session, so it outlives it: in a session of its own, its output
/// into a log under the data root.
fn start_apart(program: &Path, args: &[&str], log: &Path) {
  let Ok(out) = fs::OpenOptions::new().create(true).append(true).open(log) else {
    return;
  };
  let Ok(err) = out.try_clone() else { return };
  let mut command = std::process::Command::new(program);

  command.args(args).stdin(std::process::Stdio::null()).stdout(out).stderr(err);

  // SAFETY: setsid touches nothing of the parent's memory and is async-signal-safe.
  unsafe {
    std::os::unix::process::CommandExt::pre_exec(&mut command, || {
      libc::setsid();
      Ok(())
    });
  }

  let _ = command.spawn();
}

#[derive(Deserialize)]
struct Claimed {
  pid: i32,
  birth: String,
}

/// What must stand on the node whether or not a link holds it: the agent host of its data root, which
/// the heads there ask, and its herdr session. Each is started only where it is not standing, and a
/// second agent host stands down by itself where one already holds the root.
fn make_standing(options: &Serve) {
  let data = options.root.join("data");
  let claim = data.join("executions").join("agent-host").join("claim");
  let held = fs::read_to_string(&claim)
    .ok()
    .and_then(|text| serde_json::from_str::<Claimed>(&text).ok());

  if !held.is_some_and(|one| process_birth(one.pid).as_deref() == Some(one.birth.as_str())) {
    let host = options.root.join("deploy").join("bin").join("agent-host");
    let data_arg = data.to_string_lossy().into_owned();
    let code_arg = options.root.join("deploy").to_string_lossy().into_owned();

    start_apart(
      &host,
      &["serve", "--data-root", &data_arg, "--code-root", &code_arg],
      &data.join("agent-host.log"),
    );
  }

  if let (Some(session), Some(socket)) = (&options.herdr_session, &options.herdr_socket)
    && std::os::unix::net::UnixStream::connect(socket).is_err()
  {
    start_apart(Path::new("herdr"), &["--session", session, "server"], &data.join("herdr-server.log"));
  }
}

/// The name the CLI's verbs are called by on the node, written beside the link's own binary.
pub const VERBS_NAME: &str = "bmo";

/// The CLI's `bmo` on the node (docs/work-nodes.md, 「The link」): a script in the workplace's own binaries, which
/// every program there finds on its search path, handing its whole call to the home bridge through the link's
/// binary beside it. Written afresh, by a rename over whatever stood, so a program calling it never reads half a
/// script; answered with where it stands.
pub fn place_verbs(root: &Path) -> std::io::Result<PathBuf> {
  let bin = root.join("deploy").join("bin");
  let link = bin.join("node-link");
  let script = format!(
    "#!/bin/sh\n# The CLI's verbs on this node, each asked of the home bridge.\nexec {} home {} \"$@\"\n",
    shell_quoted(&link.to_string_lossy()),
    crate::bridge::VERBS_BINARY
  );
  let path = bin.join(VERBS_NAME);
  let fresh = bin.join(format!(".{VERBS_NAME}.{}", std::process::id()));

  fs::create_dir_all(&bin)?;
  fs::write(&fresh, script)?;
  fs::set_permissions(&fresh, fs::Permissions::from_mode(0o755))?;
  fs::rename(&fresh, &path)?;

  Ok(path)
}

/// A word a POSIX shell reads back as exactly this text: single-quoted, each single quote closed, escaped and
/// opened again. Writing a quoted word needs no parser, and no shell-quoting library is a dependency of the link.
fn shell_quoted(text: &str) -> String {
  format!("'{}'", text.replace('\'', r"'\''"))
}

/// Where the home bridge stands on the node: one name every program is handed, pointing at the socket
/// this session's forward made, which carries the session's own name so no earlier session's leftover
/// stands in its way.
fn point_bridge(root: &Path, bridge: &Path) {
  let link = root.join("data").join("bridge.sock");
  let _ = fs::remove_file(&link);
  let _ = symlink(bridge, &link);

  if let (Some(dir), Some(own)) = (bridge.parent(), bridge.file_name())
    && let Ok(entries) = fs::read_dir(dir)
  {
    for entry in entries.flatten() {
      let name = entry.file_name();

      if name != own && name.to_string_lossy().starts_with("bridge-") {
        let _ = fs::remove_file(entry.path());
      }
    }
  }
}

pub async fn serve(options: Serve) -> std::io::Result<()> {
  if let Some(bridge) = &options.bridge {
    point_bridge(&options.root, bridge);
  }

  // Every far end points a bridge, its own or the one its near end forwards, so its programs' `bmo` asks there.
  // A workplace whose binaries cannot be written still serves; its programs reach the bridge by the link alone.
  if let Err(error) = place_verbs(&options.root) {
    eprintln!("node-link: the node's bmo could not be written: {error}");
  }

  // On a person's PC the far end carries home what the programs reach there: the ports the near end names, at the
  // loopback address the mirror is spoken on, and the node's bridge socket, which is its own.
  let carrier = options
    .listen
    .as_deref()
    .map(|address| Carrier::new(address.rsplit_once(':').map_or(address, |(host, _)| host)));
  let bridging = match &carrier {
    Some(carrier) => {
      let socket = options.root.join("data").join("run").join("bridge-jake.sock");
      if let Some(dir) = socket.parent() {
        fs::create_dir_all(dir)?;
      }
      let _ = fs::remove_file(&socket);
      let listener = tokio::net::UnixListener::bind(&socket)?;
      point_bridge(&options.root, &socket);
      let again = tokio::spawn(listen_again(carrier.clone()));
      Some((tokio::spawn(take_bridge(listener, carrier.clone())), again))
    }
    None => None,
  };
  let herdr = match (&options.herdr_listen, &options.herdr_unix, &options.herdr_socket) {
    (Some(address), _, Some(socket)) => {
      let listener = tokio::net::TcpListener::bind(address.as_str()).await?;
      Some(tokio::spawn(join_tcp_to_unix(listener, socket.clone())))
    }
    (None, Some(shared), Some(socket)) => {
      let listener = crate::connect::bind_mode(shared, SHARED_SOCKET_MODE)?;
      Some(tokio::spawn(join_unix_to_unix(listener, socket.clone())))
    }
    _ => None,
  };

  let standing_options = Serve {
    root: options.root.clone(),
    bridge: options.bridge.clone(),
    herdr_session: options.herdr_session.clone(),
    herdr_socket: options.herdr_socket.clone(),
    listen: None,
    herdr_listen: None,
    herdr_unix: None,
  };
  let standing = tokio::spawn(async move {
    loop {
      make_standing(&standing_options);
      tokio::time::sleep(STANDING_EVERY).await;
    }
  });

  match (&options.listen, &carrier) {
    // Each connection a mirror session, each node's newest near end's alone, or a stream dialing a held connection
    // home.
    (Some(address), Some(carrier)) => {
      let listener = tokio::net::TcpListener::bind(address.as_str()).await?;
      let mirror = Arc::new(Mirror::default());
      let tools = Arc::new(options.root.join("data").join(crate::runner::PERSONAL_TOOLS));
      let places = Arc::new(places_of(&options.root));
      loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(connection(socket, carrier.clone(), mirror.clone(), tools.clone(), places.clone()));
      }
    }
    _ => {
      // Over standard input and output one near end stands for the whole of the far end's life.
      let (_standing, superseded) = oneshot::channel();
      let keeping = Keeping {
        mirror: Arc::new(Mirror::default()),
        node: String::new(),
        places: places_of(&options.root),
      };

      session(
        BufReader::new(tokio::io::stdin()),
        tokio::io::stdout(),
        Greeting::Owed,
        None,
        superseded,
        keeping,
      )
      .await
    }
  }

  standing.abort();
  if let Some((bridging, again)) = bridging {
    bridging.abort();
    again.abort();
  }
  if let Some(herdr) = herdr {
    herdr.abort();
  }

  Ok(())
}

// The far end's greeting: who it is, its home and its search path.
fn hello() -> Frame {
  Frame::Hello {
    home: std::env::var("HOME").unwrap_or_default(),
    pid: std::process::id(),
    path: std::env::var("PATH").unwrap_or_default(),
  }
}

// One connection on a person's PC, greeted, then told apart by the near end's first words: a stream dialing a held
// connection home, joined to it; a passage for one call of a personal tool, handed to a runner; or a mirror session
// of the node it names, which ends that node's session before it and takes its turn once it has. A session that names
// no node is the nameless node's.
async fn connection(
  socket: tokio::net::TcpStream,
  carrier: Arc<Carrier>,
  mirror: Arc<Mirror>,
  tools: Arc<PathBuf>,
  places: Arc<PathBuf>,
) {
  let (reader, mut writer) = socket.into_split();

  if writer.write_all(hello().line().as_bytes()).await.is_err() {
    return;
  }

  let mut reader = BufReader::new(reader);
  let mut first = String::new();

  if !matches!(reader.read_line(&mut first).await, Ok(read) if read > 0) {
    return;
  }

  match Frame::read(&first) {
    Some(Frame::Back { id }) => {
      if let Some(mut held) = carrier.take(id) {
        let mut stream = tokio::io::join(reader, writer);
        let _ = tokio::io::copy_bidirectional(&mut stream, &mut held).await;
      }
    }
    Some(Frame::Run { owner }) => run_tool(reader, writer, &tools, &owner).await,
    Some(Frame::Reach { to }) => reach(reader, writer, &to).await,
    first => {
      let (node, first) = match first {
        Some(Frame::Near { node }) => (node, None),
        first => (String::new(), first),
      };
      let superseded = mirror.newest(&node);
      let _turn = mirror.turn(&node).lock_owned().await;
      let keeping = Keeping {
        mirror: mirror.clone(),
        node,
        places: places.as_ref().clone(),
      };

      session(reader, writer, Greeting::Said(first), Some(carrier), superseded, keeping).await;
    }
  }
}

/// How long the far end tries to reach a local AI's address before it says it could not.
const REACH_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// A stream named for a person's local AI (docs/person-nodes.md, 「A local AI on a PC」): the host and port connected
/// to from the PC, the near end told whether it was reached, and the stream joined to the connection, each side's
/// close passed on to the other.
pub async fn reach(
  reader: impl tokio::io::AsyncRead + Unpin,
  mut writer: impl tokio::io::AsyncWrite + Unpin,
  to: &str,
) {
  let reached = match tokio::time::timeout(REACH_WAIT, tokio::net::TcpStream::connect(to)).await {
    Ok(Ok(reached)) => reached,
    Ok(Err(why)) => {
      let _ = writer
        .write_all(
          Frame::Reached {
            why: Some(format!("{to}: {why}")),
          }
          .line()
          .as_bytes(),
        )
        .await;
      return;
    }
    Err(_) => {
      let why = format!("{to} did not answer within {} seconds", REACH_WAIT.as_secs());
      let _ = writer.write_all(Frame::Reached { why: Some(why) }.line().as_bytes()).await;
      return;
    }
  };

  if writer.write_all(Frame::Reached { why: None }.line().as_bytes()).await.is_err() {
    return;
  }

  let mut stream = tokio::io::join(reader, writer);
  let mut reached = reached;
  let _ = tokio::io::copy_bidirectional(&mut stream, &mut reached).await;
}

/// One call's passage handed to a runner of this far end's own binary, run for the one owner the near end named,
/// in a process group of its own (docs/person-nodes.md, 「Personal tools on a PC」): what the stream says goes to the
/// runner's input, and what the runner answers back on the stream. The stream ending first is the call's caller gone,
/// and ends the runner's whole group, the confined tool with it; the runner answering first ends the stream.
pub async fn run_tool(
  mut reader: impl tokio::io::AsyncRead + Unpin,
  mut writer: impl tokio::io::AsyncWrite + Unpin,
  tools: &Path,
  owner: &str,
) {
  let mut command = tokio::process::Command::new("/proc/self/exe");
  // The workplace's own tree's screen kit, which an app's build sees (docs/person-nodes.md, 「Personal screens」).
  let kit = tools
    .parent()
    .and_then(Path::parent)
    .map(|root| root.join("deploy").join(crate::runner::SCREEN_KIT))
    .filter(|kit| kit.is_dir());

  command.arg("runner").arg("--root").arg(tools).args(["--owner", owner]);

  if let Some(kit) = kit {
    command.arg("--kit").arg(kit);
  }

  command
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::null())
    .process_group(0)
    .kill_on_drop(true);

  let Ok(mut child) = command.spawn() else {
    let refused = crate::runner::Answered::Refused {
      why: "the PC's runner could not be started".to_owned(),
    };
    let _ = writer.write_all(refused.line().as_bytes()).await;
    return;
  };
  let group = child.id().and_then(|pid| i32::try_from(pid).ok());
  let (Some(mut input), Some(mut output)) = (child.stdin.take(), child.stdout.take()) else {
    return;
  };

  tokio::select! {
    _ = tokio::io::copy(&mut reader, &mut input) => {
      if let Some(group) = group {
        // SAFETY: a signal to the runner's own process group, which this far end started and has not waited on.
        unsafe { libc::killpg(group, libc::SIGKILL) };
      }
    }
    _ = tokio::io::copy(&mut output, &mut writer) => {
      let _ = writer.flush().await;
      let _ = writer.shutdown().await;
    }
  }

  let _ = child.start_kill();
  let _ = child.wait().await;
}

/// The mirror sessions of one far end (docs/person-nodes.md, 「The link over jake」), by the node each names: one at a
/// time of a node, its newest near end's, beside those of other nodes. A near end that went away may leave its stream
/// standing here with nothing to close it, so the session behind it is ended by its node's next connection rather
/// than waited for.
#[derive(Default)]
pub struct Mirror {
  nodes: Mutex<HashMap<String, Turns>>,
  /// The places each node's latest watch list names, by their folder's name, kept for the far end's life.
  placed: Mutex<HashMap<String, BTreeSet<String>>>,
}

/// The file a place's running record stands in, which every watch list pulls of each place it names.
pub const PLACE_RECORD: &str = "running.json";

/// The places a watch list names, by their folder's name: each pull that watches a place's running record.
pub fn places_named(pulls: &[Pull]) -> BTreeSet<String> {
  pulls
    .iter()
    .filter(|pull| pull.files.iter().any(|file| file == PLACE_RECORD))
    .filter_map(|pull| Path::new(&pull.remote).file_name().map(|name| name.to_string_lossy().into_owned()))
    .collect()
}

/// Every place under `places` that `kept` does not name let go (docs/work-nodes.md, 「The link」): its program file
/// removed, so its head is answered the view and nothing starts it again, and the program its running record names
/// ended where it is still that process. Answers the places let go.
pub fn let_go_unnamed(places: &Path, kept: &BTreeSet<String>) -> Vec<String> {
  let Ok(entries) = fs::read_dir(places) else { return Vec::new() };
  let mut let_go = Vec::new();

  for entry in entries.flatten() {
    let name = entry.file_name().to_string_lossy().into_owned();
    let dir = entry.path();

    if kept.contains(&name) || fs::remove_file(dir.join("program.json")).is_err() {
      continue;
    }

    let running = fs::read_to_string(dir.join(PLACE_RECORD))
      .ok()
      .and_then(|text| serde_json::from_str::<Running>(&text).ok());

    if let Some(running) = running
      && running.pid > 1
      && running
        .birth
        .is_some_and(|birth| process_birth(running.pid).as_deref() == Some(birth.as_str()))
    {
      // SAFETY: a signal to the process the record names, checked a moment ago to be that process.
      unsafe { libc::kill(running.pid, libc::SIGTERM) };
    }

    let_go.push(name);
  }

  let_go
}

/// One node's mirror sessions.
#[derive(Default)]
struct Turns {
  /// Held by the node's session speaking now.
  turn: Arc<tokio::sync::Mutex<()>>,
  /// What ends the node's newest session, once a newer one comes.
  newest: Option<oneshot::Sender<()>>,
}

impl Mirror {
  /// A new session of the node made its newest: the node's one before it told to end, and what tells this one, in
  /// turn.
  pub fn newest(&self, node: &str) -> oneshot::Receiver<()> {
    let (ends, superseded) = oneshot::channel();
    let older = self.nodes().entry(node.to_owned()).or_default().newest.replace(ends);

    if let Some(older) = older {
      let _ = older.send(());
    }

    superseded
  }

  /// What the node's session speaking now holds, and the next waits on.
  pub fn turn(&self, node: &str) -> Arc<tokio::sync::Mutex<()>> {
    self.nodes().entry(node.to_owned()).or_default().turn.clone()
  }

  fn nodes(&self) -> std::sync::MutexGuard<'_, HashMap<String, Turns>> {
    self.nodes.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  /// The places a node's newest watch list names, kept as that node's, and every place under `places` that no
  /// node's latest list names let go. A node heard from earlier keeps its places for as long as the far end stands,
  /// so a node whose link dropped loses nothing to another's list. Answers the places let go.
  pub fn keep_places(&self, node: &str, named: BTreeSet<String>, places: &Path) -> Vec<String> {
    let kept: BTreeSet<String> = {
      let mut placed = self.placed.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

      placed.insert(node.to_owned(), named);
      placed.values().flatten().cloned().collect()
    };

    let_go_unnamed(places, &kept)
  }
}

// Whether a session still owes its greeting, or said it already and read the near end's first frame.
enum Greeting {
  Owed,
  Said(Option<Frame>),
}

// One mirror session over a reader and a writer: the near end's frames done, and what is watched sent. On a person's
// PC the session dials home what the carrier holds for as long as it stands.
async fn session(
  reader: BufReader<impl tokio::io::AsyncRead + Unpin + Send + 'static>,
  writer: impl tokio::io::AsyncWrite + Unpin + Send + 'static,
  greeting: Greeting,
  carrier: Option<Arc<Carrier>>,
  mut superseded: oneshot::Receiver<()>,
  keeping: Keeping,
) {
  let (send, mut sending) = mpsc::unbounded_channel::<Frame>();
  let (files, mut files_sending) = mpsc::channel::<Frame>(FILES_AHEAD);
  let asked: Arc<Mutex<Asked>> = Arc::new(Mutex::new(Asked::default()));

  // What the link says of its own first, a file's bytes only where nothing of it waits.
  let writer = tokio::spawn(async move {
    let mut out = writer;

    loop {
      let frame = tokio::select! {
        biased;
        Some(frame) = sending.recv() => frame,
        Some(frame) = files_sending.recv() => frame,
        else => break,
      };

      if out.write_all(frame.line().as_bytes()).await.is_err() || out.flush().await.is_err() {
        break;
      }
    }
  });

  let first = match greeting {
    Greeting::Owed => {
      let _ = send.send(hello());
      None
    }
    Greeting::Said(first) => first,
  };

  if let Some(carrier) = &carrier {
    carrier.attach(send.clone());
  }

  let watching = [
    tokio::spawn(watch_files(asked.clone(), files)),
    tokio::spawn(watch_standing(asked.clone(), send.clone())),
  ];

  let mut lines = reader.lines();
  let mut first = first;
  let mut overtaken = false;

  loop {
    let frame = match first.take() {
      Some(frame) => frame,
      None => tokio::select! {
        line = lines.next_line() => match line {
          Ok(Some(line)) => match Frame::read(&line) {
            Some(frame) => frame,
            None => continue,
          },
          _ => break,
        },
        _ = &mut superseded => {
          overtaken = true;
          break;
        }
      },
    };

    if let Frame::Carry { ports } = frame {
      if let Some(carrier) = &carrier {
        carrier.carry(Some((&send, &ports))).await;
      }
      continue;
    }

    if let Err(said) = take(frame, &asked, &keeping) {
      let _ = send.send(Frame::Trouble { said });
    }
  }

  if let Some(carrier) = &carrier {
    carrier.detach(&send);
  }

  for watch in &watching {
    watch.abort();
  }
  drop(send);

  // A session a newer near end overtook may be writing into a stream nobody reads, so its writer is ended rather
  // than waited on.
  if overtaken {
    writer.abort();
  }

  let _ = writer.await;
}

// Each connection to a unix socket here joined to a fresh connection to the unix socket `socket`.
async fn join_unix_to_unix(listener: tokio::net::UnixListener, socket: PathBuf) {
  while let Ok((mut client, _)) = listener.accept().await {
    let socket = socket.clone();
    tokio::spawn(async move {
      if let Ok(mut far) = tokio::net::UnixStream::connect(&socket).await {
        let _ = tokio::io::copy_bidirectional(&mut client, &mut far).await;
      }
    });
  }
}

// Each TCP connection here joined to a fresh connection to the unix socket `socket`.
async fn join_tcp_to_unix(listener: tokio::net::TcpListener, socket: PathBuf) {
  while let Ok((client, _)) = listener.accept().await {
    let socket = socket.clone();
    tokio::spawn(async move {
      if let Ok(mut far) = tokio::net::UnixStream::connect(&socket).await {
        let mut client = client;
        let _ = tokio::io::copy_bidirectional(&mut client, &mut far).await;
      }
    });
  }
}

// What the near end sent, done.
/// The folders the programs placed on the node work in, each made where it is not, readable by the account alone.
pub fn make_folders(paths: &[String]) -> Result<(), String> {
  for path in paths {
    std::fs::DirBuilder::new()
      .recursive(true)
      .mode(0o700)
      .create(path)
      .map_err(|error| format!("{path}: {error}"))?;
  }

  Ok(())
}

/// What the near end asked to be watched, and how much of each growing file it said it holds, which the files'
/// watch takes once.
#[derive(Default)]
struct Asked {
  pulls: Vec<Pull>,
  held: Vec<Held>,
  list: Option<u64>,
}

impl Asked {
  fn take(&mut self) -> (Vec<Pull>, Vec<Held>, Option<u64>) {
    (self.pulls.clone(), std::mem::take(&mut self.held), self.list)
  }
}

// Where a session keeps the places its node's watch lists name: the far end's sessions, the node it speaks for, and
// the folder the places stand in under the workplace's data root.
struct Keeping {
  mirror: Arc<Mirror>,
  node: String,
  places: PathBuf,
}

/// The folder the places of a workplace stand in, under its data root.
pub fn places_of(root: &Path) -> PathBuf {
  root.join("data").join("agent-places")
}

fn take(frame: Frame, asked: &Mutex<Asked>, keeping: &Keeping) -> Result<(), String> {
  match frame {
    Frame::Watch { pulls, held, list } => {
      // A place no node's watch list names any more stands nothing on this node: an account moved to another node,
      // or a node whose agent went away on a machine another node's link now holds.
      for place in keeping.mirror.keep_places(&keeping.node, places_named(&pulls), &keeping.places) {
        eprintln!("node-link: {place} is placed here by no node's link any more, so its program is let go");
      }

      *asked.lock().map_err(|_| "the watch list is poisoned".to_owned())? = Asked { pulls, held, list };
      Ok(())
    }
    Frame::Put {
      path, data, mode, ..
    } => {
      let bytes = decode(&data).ok_or_else(|| format!("{path}: not base64"))?;

      write_whole(Path::new(&path), &bytes, mode.unwrap_or(0o600))
        .map_err(|error| format!("{path}: {error}"))
    }
    Frame::Folders { paths } => make_folders(&paths),
    Frame::Gone { path, .. } => match fs::remove_file(&path) {
      Ok(()) => Ok(()),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
      Err(error) => Err(format!("{path}: {error}")),
    },
    Frame::Signal { pid, birth, signal } => {
      if pid <= 1
        || birth
          .as_ref()
          .is_some_and(|birth| process_birth(pid).as_deref() != Some(birth.as_str()))
      {
        return Ok(());
      }

      // SAFETY: kill takes a plain pid and signal number and touches no memory of ours.
      unsafe { libc::kill(pid, signal) };
      Ok(())
    }
    _ => Ok(()),
  }
}

/// A file written whole and put in place in one step, its directory made private where it is new.
pub fn write_whole(path: &Path, bytes: &[u8], mode: u32) -> std::io::Result<()> {
  if let Some(dir) = path.parent() {
    fs::create_dir_all(dir)?;
  }

  let temporary = path.with_extension(format!("link-{}", std::process::id()));

  fs::write(&temporary, bytes)?;
  fs::set_permissions(&temporary, fs::Permissions::from_mode(mode))?;
  fs::rename(&temporary, path)
}

/// A process's start time as the kernel counts it, field 22 of its stat line.
pub fn process_birth(pid: i32) -> Option<String> {
  let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
  let (_, rest) = stat.rsplit_once(") ")?;
  let fields: Vec<&str> = rest.split(' ').collect();

  if fields.first().is_some_and(|state| state.starts_with('Z')) {
    return None;
  }

  fields.get(19).map(|birth| (*birth).to_owned())
}

#[derive(Deserialize)]
struct Recorded {
  pid: i32,
  birth: Option<String>,
}

#[derive(Deserialize)]
struct Running {
  pid: i32,
  birth: Option<String>,
  #[serde(rename = "loop")]
  head: Option<Recorded>,
}

/// The processes a running record names that are alive: the program and the head that became it.
pub fn alive_in(dir: &Path) -> Vec<Identity> {
  let Ok(text) = fs::read_to_string(dir.join("running.json")) else { return Vec::new() };
  let Ok(running) = serde_json::from_str::<Running>(&text) else { return Vec::new() };
  let named = [
    Some(Recorded {
      pid: running.pid,
      birth: running.birth,
    }),
    running.head,
  ];

  named
    .into_iter()
    .flatten()
    .filter_map(|one| {
      let birth = one.birth?;

      (one.pid > 1 && process_birth(one.pid).as_deref() == Some(birth.as_str())).then_some(Identity {
        pid: one.pid,
        birth,
      })
    })
    .collect()
}

/// The files one pull covers, by their path under it.
pub fn files_of(pull: &Pull) -> Vec<String> {
  let root = Path::new(&pull.remote);

  if !pull.files.is_empty() {
    return pull.files.clone();
  }

  let mut found = Vec::new();
  let mut dirs = vec![PathBuf::new()];

  while let Some(dir) = dirs.pop() {
    let Ok(entries) = fs::read_dir(root.join(&dir)) else { continue };

    for entry in entries.flatten() {
      let Ok(kind) = entry.file_type() else { continue };
      let relative = dir.join(entry.file_name());

      if kind.is_dir() && pull.tree {
        dirs.push(relative);
      } else if kind.is_file() {
        found.push(relative.to_string_lossy().into_owned());
      }

      if found.len() >= TREE_MOST {
        return found;
      }
    }
  }

  found
}

fn read_range(path: &Path, from: u64, to: u64) -> std::io::Result<Vec<u8>> {
  let mut file = fs::File::open(path)?;
  let mut bytes = vec![0; (to - from) as usize];

  file.seek(SeekFrom::Start(from))?;
  file.read_exact(&mut bytes)?;

  Ok(bytes)
}

/// Where a frame of what is watched goes: ahead with what the link says of its own, or behind it in the files'
/// queue, which the far end fills no faster than the connection carries it.
enum Out {
  Link(mpsc::UnboundedSender<Frame>),
  Files(mpsc::Sender<Frame>),
}

impl Out {
  /// The frame sent, waiting for room where it goes behind; false once the session no longer reads.
  async fn send(&self, frame: Frame) -> bool {
    match self {
      Self::Link(link) => link.send(frame).is_ok(),
      Self::Files(files) => files.send(frame).await.is_ok(),
    }
  }

  fn is_closed(&self) -> bool {
    match self {
      Self::Link(link) => link.is_closed(),
      Self::Files(files) => files.is_closed(),
    }
  }
}

/// One look at a pull's files: each new or changed one sent, a growing one from where it was last sent or, where
/// none of it was sent yet, from what the near end holds of it, and each one gone said gone. False once the session
/// no longer reads.
async fn send_pull(
  (index, list): (usize, Option<u64>),
  pull: &Pull,
  sent: &mut HashMap<String, Seen>,
  holds: &mut HashMap<String, u64>,
  out: &Out,
) -> bool {
  let mut here: Vec<String> = Vec::new();

  for relative in files_of(pull) {
    let path = Path::new(&pull.remote).join(&relative);
    let Ok(meta) = fs::metadata(&path) else { continue };

    if !meta.is_file() {
      continue;
    }

    here.push(relative.clone());

    let now = seen(&meta);
    let before = sent.get(&relative).copied();

    if before == Some(now) {
      continue;
    }

    let from = match before {
      Some(was) if pull.append && was.inode == now.inode && was.len <= now.len => was.len,
      Some(_) => 0,
      None => holds.remove(&relative).filter(|held| pull.append && *held <= now.len).unwrap_or(0),
    };

    if pull.append {
      let mut offset = from;

      while offset < now.len {
        let to = (offset + CHUNK).min(now.len);
        let Ok(bytes) = read_range(&path, offset, to) else { break };
        let frame = Frame::Append {
          pull: index,
          path: relative.clone(),
          offset,
          data: encode(&bytes),
          list,
        };

        if !out.send(frame).await {
          return false;
        }

        offset = to;
      }

      if offset == 0 && now.len == 0 {
        let frame = Frame::Append {
          pull: index,
          path: relative.clone(),
          offset: 0,
          data: String::new(),
          list,
        };

        if !out.send(frame).await {
          return false;
        }
      }
    } else if let Ok(bytes) = fs::read(&path) {
      let frame = Frame::Put {
        pull: Some(index),
        path: relative.clone(),
        data: encode(&bytes),
        mode: Some(meta.mode() & 0o777),
        list,
      };

      if !out.send(frame).await {
        return false;
      }
    }

    sent.insert(relative, now);
  }

  let gone: Vec<String> = sent.keys().filter(|relative| !here.contains(relative)).cloned().collect();

  for relative in gone {
    sent.remove(&relative);

    if !out
      .send(Frame::Gone {
        pull: Some(index),
        path: relative,
        list,
      })
      .await
    {
      return false;
    }
  }

  true
}

/// What was sent of each watched file, by the pull it belongs to and its path under it. A pull is known by what it
/// is rather than by where the list puts it, so a list asked anew, as one is whenever a place comes or goes, goes on
/// with every pull it still names and sends from nothing only those it newly names — where the near end numbers its
/// lists, so each frame names the list it was read under. A list numbered by none is sent from nothing as it
/// changes, since a frame on its way could not say which list's numbering it was read under.
#[derive(Default)]
struct Sent {
  watched: Vec<Pull>,
  list: Option<u64>,
  files: HashMap<Pull, HashMap<String, Seen>>,
  holds: HashMap<Pull, HashMap<String, u64>>,
}

impl Sent {
  /// The list asked for now, what was sent of a pull it no longer names forgotten, and what the near end holds of
  /// each growing file kept until the file is first sent.
  fn follow(&mut self, pulls: Vec<Pull>, held: Vec<Held>, list: Option<u64>) {
    if pulls != self.watched {
      if list.is_none() {
        self.files.clear();
        self.holds.clear();
      }

      self.files.retain(|pull, _| pulls.contains(pull));
      self.holds.retain(|pull, _| pulls.contains(pull));
      self.watched = pulls;
    }

    self.list = list;

    for one in held {
      if let Some(pull) = self.watched.get(one.pull).filter(|pull| pull.append) {
        self.holds.entry(pull.clone()).or_default().insert(one.path, one.len);
      }
    }
  }

  /// One look at every pull `which` picks; false once the session no longer reads.
  async fn send(&mut self, which: impl Fn(&Pull) -> bool, out: &Out) -> bool {
    for (index, pull) in self.watched.iter().enumerate().filter(|(_, pull)| which(pull)) {
      let sent = self.files.entry(pull.clone()).or_default();
      let holds = self.holds.entry(pull.clone()).or_default();

      if !send_pull((index, self.list), pull, sent, holds, out).await {
        return false;
      }
    }

    true
  }
}

/// The files of every pull that does not tell a place's processes, sent behind what the link says of its own, as
/// fast as the connection carries them and no faster.
async fn watch_files(asked: Arc<Mutex<Asked>>, files: mpsc::Sender<Frame>) {
  let out = Out::Files(files);
  let mut sent = Sent::default();

  loop {
    let (pulls, held, list) = asked.lock().map(|mut asked| asked.take()).unwrap_or_default();

    sent.follow(pulls, held, list);

    if !sent.send(|pull| !pull.alive, &out).await || out.is_closed() {
      return;
    }

    tokio::time::sleep(LOOK_EVERY).await;
  }
}

/// Each place's running record and which of its processes are alive, and the node's load, sent with what the link
/// says of its own, so a transcript on its way never holds them back.
async fn watch_standing(asked: Arc<Mutex<Asked>>, link: mpsc::UnboundedSender<Frame>) {
  let out = Out::Link(link.clone());
  let mut sent = Sent::default();
  let mut told: HashMap<Pull, (Vec<Identity>, Instant)> = HashMap::new();
  let mut proc_at: Option<Instant> = None;

  loop {
    let (pulls, list) = asked.lock().map(|asked| (asked.pulls.clone(), asked.list)).unwrap_or_default();

    // A list numbered by none is told from nothing as it changes, as its files are.
    if list.is_none() && pulls != sent.watched {
      told.clear();
    }

    told.retain(|pull, _| pulls.contains(pull));
    sent.follow(pulls, Vec::new(), list);

    if !sent.send(|pull| pull.alive, &out).await {
      return;
    }

    for (index, pull) in sent.watched.iter().enumerate().filter(|(_, pull)| pull.alive) {
      let alive = alive_in(Path::new(&pull.remote));
      let due = told.get(pull).is_none_or(|(was, at)| *was != alive || at.elapsed() >= ALIVE_EVERY);

      if due {
        let _ = link.send(Frame::Alive {
          pull: index,
          alive: alive.clone(),
          list: sent.list,
        });
        told.insert(pull.clone(), (alive, Instant::now()));
      }
    }

    if proc_at.is_none_or(|at| at.elapsed() >= PROC_EVERY) {
      let files = PROC_FILES
        .iter()
        .filter_map(|name| {
          fs::read_to_string(Path::new("/proc").join(name))
            .ok()
            .map(|text| ((*name).to_owned(), text))
        })
        .collect();

      let _ = link.send(Frame::Proc { files });
      proc_at = Some(Instant::now());
    }

    if link.is_closed() {
      return;
    }

    tokio::time::sleep(LOOK_EVERY).await;
  }
}
