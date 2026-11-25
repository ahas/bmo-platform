//! The runner of a person's personal tools (docs/person-nodes.md, 「Personal tools on a PC」): `node-link runner`,
//! started for one call, speaking the tool host's runner passage on its standard input and output (bmo-pocket's
//! docs/api.md, 「The runner」). It runs the version of the owner's tool the call names, confined by bubblewrap to
//! that version's folder, read alone, and the tool's own data folder, with a clean environment, a network of its own
//! whose one road out is the call's proxy where the call names addresses, and the secrets the call carries as files
//! of the confinement's own (docs/person-nodes.md, 「The network and secrets of a call」), and answers what the tool
//! wrote and how it ended, every secret's value masked in it. Its standard input closing before the tool has
//! answered is its caller gone, and ends the tool.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

/// The folder under a workplace's data root a PC's runner keeps its owners' tools in.
pub const PERSONAL_TOOLS: &str = "personal-tools";

/// The folder beside it, under the same data root, the person's apps' sources stand in, one folder an app.
pub const MY_APPS: &str = "my-apps";

/// The folder of agency's tree the screen kit stands in (docs/person-nodes.md, 「Personal screens」).
pub const SCREEN_KIT: &str = "screen-kit";

/// The variable a test names a stand-in for bubblewrap in.
pub const BWRAP_VARIABLE: &str = "NODE_LINK_BWRAP";

/// Where a confined tool sees its version folder and its data folder.
const TOOL_AT: &str = "/tool";
const DATA_AT: &str = "/data";

/// Where a confined run finds the runner's own binary — which forwards its loopback to the proxy where it reaches the
/// network, and which speaks MCP for an MCP server's version (docs/person-nodes.md, 「MCP servers」) — and the proxy's
/// socket.
pub const OWN_BINARY_AT: &str = "/.bmo/node-link";
const PROXY_AT: &str = "/.bmo/proxy.sock";

/// Where a confined tool finds the secrets its call carries, one file a secret by its name, and the variable naming
/// the folder.
const SECRETS_AT: &str = "/secrets";
const SECRETS_VARIABLE: &str = "BMO_SECRETS";

/// The certificates a confined tool reaching the network reads, where they stand.
const CERTIFICATES: [&str; 3] = ["/etc/ssl", "/etc/ca-certificates", "/etc/pki"];

/// The fewest characters a secret's value is masked at in what the tool writes.
const MASKED_FROM: usize = 4;

/// The search path a confined tool runs with: the machine's own programs, read alone.
const CONFINED_PATH: &str = "/usr/local/bin:/usr/bin:/bin";

/// The machine's own folders a confined tool reads, where they stand as folders; where one is a link, as `/bin` is
/// to `usr/bin` on most systems, the link is made again inside.
const SYSTEM_FOLDERS: [&str; 5] = ["/bin", "/sbin", "/lib", "/lib32", "/lib64"];

/// The machine's own files a confined tool reads where they stand: what the dynamic loader and the clock read, and
/// the alternatives a program of `/usr` is often named through.
const SYSTEM_FILES: [&str; 5] = [
  "/etc/ld.so.cache",
  "/etc/ld.so.conf",
  "/etc/ld.so.conf.d",
  "/etc/localtime",
  "/etc/alternatives",
];

/// What the runner is asked on the passage's first line (bmo-pocket's docs/api.md, 「The runner」).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asked {
  pub owner: String,
  pub tool: String,
  pub version: String,
  pub run: Vec<String>,
  pub call: Value,
  /// The addresses the call may reach, as the tool's manifest writes them; none reaches none.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub network: Vec<String>,
  /// The secrets the call is handed, each file's bytes in base64 by its name; never written down or said.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub secrets: BTreeMap<String, String>,
}

/// One line the runner answers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub enum Answered {
  /// A line the tool wrote on its standard error.
  Said { line: String },
  /// How the tool ended, none where a signal ended it, and what it wrote on its standard output.
  End { code: Option<i32>, out: String },
  /// Why the runner did not run the tool, or did not do what it was asked.
  Refused { why: String },
  /// An MCP server's version placed nothing, since it asks for addresses or secrets the place does not carry: every
  /// address and every secret's name it asks for.
  Wants {
    network: Vec<String>,
    secrets: Vec<String>,
  },
  /// An app placed as a new version, pointed at by nothing yet, and its description by path.
  Placed {
    version: String,
    files: BTreeMap<String, String>,
  },
  /// A version's description by path.
  Described {
    version: String,
    files: BTreeMap<String, String>,
  },
  /// The pointer moved onto the version, and the versions kept older than it, newest first.
  Pointed { version: String, kept: Vec<String> },
  /// Each tool's version pointed at and those kept beside it, by the tool's name.
  Versions { tools: BTreeMap<String, Kept> },
  /// The tool's pointer and versions taken away.
  Taken {},
  /// One file of a version's screen, its bytes in base64.
  File { bytes: String },
  /// The source a version was placed from, by path.
  Source {
    version: String,
    files: BTreeMap<String, String>,
  },
  /// A shared app's source written into its folder of 'my apps'.
  Received {},
  /// A received app made the person's own.
  Adopted {},
}

/// What a tool keeps: the version pointed at, none where no pointer stands, and the others older than it, newest
/// first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kept {
  pub current: Option<String>,
  pub kept: Vec<String>,
}

impl Answered {
  pub fn line(&self) -> String {
    let mut line = serde_json::to_string(self).unwrap_or_default();

    line.push('\n');
    line
  }
}

/// How a runner is started: the folder its owners' tools stand in, the one owner it runs for where it is told one,
/// the bubblewrap program it confines with, the screen kit a place's build sees, where it is told one, and the
/// stand-in addresses a test names for the names a call reaches (`crate::proxy::RESOLVE_VARIABLE`).
#[derive(Clone, Debug)]
pub struct Runner {
  pub root: PathBuf,
  pub owner: Option<String>,
  pub bwrap: String,
  pub kit: Option<PathBuf>,
  pub stand_ins: BTreeMap<String, IpAddr>,
}

/// A name a folder of the runner's holds: an owner's, a tool's or a version's, never one that climbs or hides.
pub fn plain_name(name: &str) -> bool {
  !name.is_empty()
    && !name.starts_with('.')
    && name.chars().all(|one| one.is_ascii_alphanumeric() || matches!(one, '-' | '_' | '.'))
}

/// The folder an owner's placed version of a tool stands in, which the tool reads alone.
pub fn version_dir(root: &Path, owner: &str, tool: &str, version: &str) -> PathBuf {
  root.join("owners").join(owner).join("tools").join(tool).join(version)
}

/// The folder the apps' sources stand in, one folder an app: the runner's root's sibling `my-apps`, under the same
/// data root (docs/person-nodes.md, 「Personal apps」).
pub fn apps_dir(root: &Path) -> PathBuf {
  root.parent().map_or_else(|| PathBuf::from(MY_APPS), |parent| parent.join(MY_APPS))
}

/// The folder an owner's tool keeps its data in, the one it writes.
pub fn data_dir(root: &Path, owner: &str, tool: &str) -> PathBuf {
  root.join("owners").join(owner).join("data").join(tool)
}

/// The words bubblewrap is started with to run `run` confined: every namespace its own, the network among them;
/// ended with its parent; in a session of its own; a clean environment; the machine's `/usr` and the few files its
/// programs read, read alone; a process table, devices and a scratch folder of its own; the version folder read alone
/// at `/tool`, where it starts, and the data folder written at `/data`, its home. The first run word is read in the
/// version folder where a file of that name stands there, and found on the search path otherwise.
pub fn confined(version: &Path, data: &Path, run: &[String]) -> Vec<String> {
  let mut words = machine_words();

  words.extend(["--ro-bind".into(), version.to_string_lossy().into_owned(), TOOL_AT.into()]);
  words.extend(["--bind".into(), data.to_string_lossy().into_owned(), DATA_AT.into()]);
  words.extend(["--chdir".into(), TOOL_AT.into()]);

  for (name, value) in [("PATH", CONFINED_PATH), ("HOME", DATA_AT), ("TMPDIR", "/tmp"), ("LANG", "C.UTF-8")]
  {
    words.extend(["--setenv".into(), name.into(), value.into()]);
  }

  words.push("--".into());

  if let Some((first, rest)) = run.split_first() {
    let own = version.join(first);

    words.push(if plain_name(first) && own.is_file() {
      format!("{TOOL_AT}/{first}")
    } else {
      first.clone()
    });
    words.extend(rest.iter().cloned());
  }

  words
}

/// What one call's confinement adds to [`confined`]: the runner's own binary, read alone at `/.bmo/node-link`, where
/// the call reaches the network or its run words open on it; the proxy's socket, which the binary forwards the
/// confinement's loopback to, where the call names addresses; and each secret the call carries, read from the
/// descriptor given, by its name.
#[derive(Debug, Default)]
pub struct Opened {
  pub binary: Option<PathBuf>,
  pub proxy: Option<PathBuf>,
  pub secrets: Vec<(String, i32)>,
}

/// The words bubblewrap is started with for one call: those of [`confined`], with what `opened` adds. A call
/// reaching the network runs its command behind the forwarder, its proxy named in every proxy variable, and reads the
/// machine's certificates; a call carrying secrets finds each read alone in `/secrets`, a file of the confinement's
/// own memory, named in `BMO_SECRETS`.
pub fn confined_call(version: &Path, data: &Path, run: &[String], opened: &Opened) -> Vec<String> {
  let mut words = confined(version, data, run);
  let command_at = words.iter().position(|word| word == "--").unwrap_or(words.len());
  let mut added: Vec<String> = Vec::new();

  if let Some(binary) = &opened.binary {
    added.extend(["--ro-bind".into(), binary.to_string_lossy().into_owned(), OWN_BINARY_AT.into()]);
  }

  let forwarded = opened.binary.is_some() && opened.proxy.is_some();

  if let Some(socket) = opened.proxy.as_ref().filter(|_| forwarded) {
    added.extend(["--bind".into(), socket.to_string_lossy().into_owned(), PROXY_AT.into()]);

    for certificates in CERTIFICATES {
      added.extend(["--ro-bind-try".into(), certificates.into(), certificates.into()]);
    }

    let proxy = format!("http://{}", crate::proxy::LISTEN_AT);

    for name in crate::proxy::PROXY_VARIABLES {
      added.extend(["--setenv".into(), name.into(), proxy.clone()]);
    }
  }

  if !opened.secrets.is_empty() {
    for (name, descriptor) in &opened.secrets {
      added.extend(["--perms".into(), "0400".into(), "--ro-bind-data".into(), descriptor.to_string()]);
      added.push(format!("{SECRETS_AT}/{name}"));
    }

    added.extend(["--setenv".into(), SECRETS_VARIABLE.into(), SECRETS_AT.into()]);
  }

  let command = words.split_off(command_at);

  words.extend(added);

  if forwarded {
    words.push("--".into());
    words.extend([
      OWN_BINARY_AT.to_owned(),
      "forward".to_owned(),
      "--listen".to_owned(),
      crate::proxy::LISTEN_AT.to_owned(),
      "--to".to_owned(),
      PROXY_AT.to_owned(),
      "--".to_owned(),
    ]);
    words.extend(command.into_iter().skip(1));
  } else {
    words.extend(command);
  }

  words
}

/// The words every confinement starts with: every namespace its own, the network among them; ended with its
/// parent; in a session of its own; a clean environment; the machine's `/usr` and the few files its programs read,
/// read alone; and a process table, devices and a scratch folder of its own.
pub fn machine_words() -> Vec<String> {
  let mut words: Vec<String> = [
    "--unshare-all",
    "--die-with-parent",
    "--new-session",
    "--clearenv",
    "--ro-bind",
    "/usr",
    "/usr",
  ]
  .map(str::to_owned)
  .to_vec();

  for folder in SYSTEM_FOLDERS {
    let path = Path::new(folder);

    match std::fs::read_link(path) {
      Ok(target) => {
        words.extend(["--symlink".into(), target.to_string_lossy().into_owned(), folder.into()])
      }
      Err(_) if path.is_dir() => words.extend(["--ro-bind".into(), folder.into(), folder.into()]),
      Err(_) => {}
    }
  }

  for file in SYSTEM_FILES {
    words.extend(["--ro-bind-try".into(), file.into(), file.into()]);
  }

  words.extend(["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"].map(str::to_owned));
  words
}

/// The search path a confined program runs with.
pub fn confined_path() -> &'static str {
  CONFINED_PATH
}

/// Where a program named on the search path stands, its links followed; none where it is not found.
pub fn program_path(program: &str) -> Option<PathBuf> {
  std::env::var_os("PATH")
    .and_then(|path| {
      std::env::split_paths(&path).map(|dir| dir.join(program)).find(|found| found.is_file())
    })
    .and_then(|found| std::fs::canonicalize(found).ok())
}

/// Whether a program stands where it is named: a path that is a file, or a name found on the search path.
pub fn program_stands(program: &str) -> bool {
  if program.contains('/') {
    return Path::new(program).is_file();
  }

  std::env::var_os("PATH")
    .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

/// What the runner is asked, held to what it runs: an owner it runs for, plain names, a version that stands and a
/// run command; the version and data folders, the data folder made where it is not.
fn placed(runner: &Runner, asked: &Asked) -> Result<(PathBuf, PathBuf), String> {
  if runner.owner.as_deref().is_some_and(|owner| owner != asked.owner) {
    return Err(format!("this runner runs no personal tool of the account {}", asked.owner));
  }

  for (what, name) in [("owner", &asked.owner), ("tool", &asked.tool), ("version", &asked.version)] {
    if !plain_name(name) {
      return Err(format!("{name:?} is no {what}'s name"));
    }
  }

  if asked.run.first().is_none_or(|word| word.trim().is_empty()) {
    return Err("the call names no run command".to_owned());
  }

  let version = version_dir(&runner.root, &asked.owner, &asked.tool, &asked.version);

  if !version.is_dir() {
    return Err(format!(
      "version {} of the tool {} is not placed on this machine for the account {}",
      asked.version, asked.tool, asked.owner
    ));
  }

  let data = data_dir(&runner.root, &asked.owner, &asked.tool);

  std::fs::DirBuilder::new()
    .recursive(true)
    .mode(0o700)
    .create(&data)
    .map_err(|why| format!("{}: {why}", data.display()))?;

  if !program_stands(&runner.bwrap) {
    return Err(format!(
      "bubblewrap ({}) is not on this machine, and a personal tool runs confined or not at all",
      runner.bwrap
    ));
  }

  Ok((version, data))
}

/// One line answered on `output` and flushed. The process's standard output writes on a blocking thread of the
/// runtime, which is let go unwritten where the runtime ends first, as it does once the runner returns: an answer
/// unflushed would be lost to a caller waiting on it.
async fn answer(output: &mut (impl AsyncWrite + Unpin), answered: &Answered) {
  if output.write_all(answered.line().as_bytes()).await.is_ok() {
    let _ = output.flush().await;
  }
}

/// One call run: its first line read from `input`, the tool run confined, each line it writes on its errors and its
/// end answered on `output`. `input` closing before the tool ends is its caller gone: the tool's process is ended
/// and nothing more is answered.
pub async fn run(
  runner: Runner,
  input: impl AsyncRead + Unpin + Send + 'static,
  mut output: impl AsyncWrite + Unpin + Send + 'static,
) {
  let mut input = BufReader::new(input);
  let mut first = String::new();

  if !matches!(input.read_line(&mut first).await, Ok(read) if read > 0) {
    return;
  }

  // A line naming no call asks the runner to keep the owner's versions (docs/person-nodes.md, 「Personal apps」).
  if serde_json::from_str::<Value>(first.trim_end()).is_ok_and(|line| line.get("call").is_none()) {
    crate::keeping::keep(runner, first.trim_end(), input, output).await;
    return;
  }

  let asked = match serde_json::from_str::<Asked>(first.trim_end()) {
    Ok(asked) => asked,
    Err(why) => {
      let refused = Answered::Refused {
        why: format!("the call does not read: {why}"),
      };

      answer(&mut output, &refused).await;
      return;
    }
  };
  let (version, data) = match placed(&runner, &asked) {
    Ok(folders) => folders,
    Err(why) => {
      answer(&mut output, &Answered::Refused { why }).await;
      return;
    }
  };
  let mut launched =
    match launch(&runner, &version, &data, &asked.run, &asked.network, &asked.secrets, Stdio::piped()) {
      Ok(launched) => launched,
      Err(why) => {
        answer(&mut output, &Answered::Refused { why }).await;
        return;
      }
    };
  let masks = launched.masks.clone();
  let mut child = launched.take_child();
  let call = serde_json::to_vec(&asked.call).unwrap_or_default();

  if let Some(mut stdin) = child.stdin.take() {
    tokio::spawn(async move {
      let _ = stdin.write_all(&call).await;
    });
  }

  let (lines, mut answering) = mpsc::unbounded_channel::<Answered>();
  let writer = tokio::spawn(async move {
    while let Some(answered) = answering.recv().await {
      if output.write_all(answered.line().as_bytes()).await.is_err() || output.flush().await.is_err() {
        break;
      }
    }
  });
  let said = child.stderr.take().map(|stderr| {
    let lines = lines.clone();
    let masks = masks.clone();

    tokio::spawn(async move {
      let mut read = BufReader::new(stderr).lines();

      while let Ok(Some(line)) = read.next_line().await {
        let _ = lines.send(Answered::Said {
          line: masked(&masks, line),
        });
      }
    })
  });
  let mut stdout = child.stdout.take();
  let reading = async move {
    let mut out = Vec::new();

    if let Some(stdout) = stdout.as_mut() {
      let _ = stdout.read_to_end(&mut out).await;
    }

    out
  };
  // Nothing more comes on the passage after its first line, so its end is the caller gone.
  let gone = async move {
    let mut rest = Vec::new();
    let _ = input.read_to_end(&mut rest).await;
  };

  // The call's proxy and its folder go as the tool ends, before the answer, so nothing of the call is left on the PC
  // when its caller, answered, closes the passage and the runner with it.
  tokio::select! {
    out = reading => {
      let ended = child.wait().await.ok();

      if let Some(said) = said {
        let _ = said.await;
      }

      launched.end();

      let _ = lines.send(Answered::End {
        code: ended.and_then(|status| status.code()),
        out: masked(&masks, String::from_utf8_lossy(&out).into_owned()),
      });
    }
    () = gone => {
      let _ = child.start_kill();
      let _ = child.wait().await;
      launched.end();
    }
  }

  drop(lines);
  let _ = writer.await;
}

/// One confined run started: bubblewrap's process until it is taken, the call's proxy served while it runs, and what
/// is masked in what it writes, each with its secret's name.
pub struct Launched {
  child: Option<tokio::process::Child>,
  proxy: Option<(crate::proxy::CallFolder, tokio::task::JoinHandle<()>)>,
  pub masks: std::sync::Arc<Vec<(String, String)>>,
}

impl Launched {
  /// Bubblewrap's process, its standard input, output and errors piped.
  pub fn take_child(&mut self) -> tokio::process::Child {
    self.child.take().expect("a launched run's process is taken once")
  }

  /// The run's proxy and its folder taken away.
  pub fn end(&mut self) {
    if let Some((folder, serving)) = self.proxy.take() {
      serving.abort();
      drop(folder);
    }
  }
}

impl Drop for Launched {
  fn drop(&mut self) {
    self.end();
  }
}

/// `run` started confined in the version folder, writing the data folder: reaching the addresses `network` lists
/// through a proxy of its own, handed the secrets `secrets` carries, each by its name and its bytes in base64, and
/// the runner's own binary bound where it reaches the network or its run words open on it. Bubblewrap's standard
/// output and errors are piped, its input `input`; it is killed as it is dropped.
pub fn launch(
  runner: &Runner,
  version: &Path,
  data: &Path,
  run: &[String],
  network: &[String],
  secrets: &BTreeMap<String, String>,
  input: Stdio,
) -> Result<Launched, String> {
  let handed = handed_secrets(secrets)?;
  let masks = std::sync::Arc::new(masks(&handed));
  let mut opened = Opened::default();
  let mut proxy = None;

  if !network.is_empty() || run.first().is_some_and(|first| first == OWN_BINARY_AT) {
    opened.binary = Some(own_binary()?);
  }

  if !network.is_empty() {
    let (folder, serving) = serve_proxy(runner, network)?;

    opened.proxy = Some(folder.socket());
    proxy = Some((folder, serving));
  }

  let readers = secret_pipes(&handed)?;

  opened.secrets = readers.iter().map(|(name, reader)| (name.clone(), reader.as_raw_fd())).collect();

  let inherited: Vec<i32> = opened.secrets.iter().map(|(_, descriptor)| *descriptor).collect();
  let mut command = tokio::process::Command::new(&runner.bwrap);

  command
    .args(confined_call(version, data, run, &opened))
    .env_clear()
    .stdin(input)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);

  // SAFETY: between the fork and the exec only `fcntl` runs, on descriptors this process holds open, so bubblewrap
  // inherits the secrets' reading ends and nothing else of this process's.
  unsafe {
    command.pre_exec(move || {
      for descriptor in &inherited {
        if libc::fcntl(*descriptor, libc::F_SETFD, 0) == -1 {
          return Err(std::io::Error::last_os_error());
        }
      }

      Ok(())
    });
  }

  let spawned = command.spawn();

  // This process's own reading ends go: bubblewrap holds its copies, and reads each to its end as it starts.
  drop(readers);

  let child =
    spawned.map_err(|why| format!("bubblewrap ({}) could not be started: {why}", runner.bwrap))?;

  Ok(Launched {
    child: Some(child),
    proxy,
    masks,
  })
}

// The secrets a call carries, each decoded, by its name; or why not, naming the secret and never what it holds.
fn handed_secrets(carried: &BTreeMap<String, String>) -> Result<BTreeMap<String, Vec<u8>>, String> {
  carried
    .iter()
    .map(|(name, encoded)| {
      if !plain_name(name) {
        return Err(format!("{name:?} is no secret's name"));
      }

      base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map(|bytes| (name.clone(), bytes))
        .map_err(|_| format!("the secret {name} does not decode"))
    })
    .collect()
}

// What is masked in what the tool writes, each with the secret's name: every value of at least four characters,
// the file whole, each of its lines and each `KEY=VALUE` line's value, longest first.
fn masks(handed: &BTreeMap<String, Vec<u8>>) -> Vec<(String, String)> {
  let mut masks = Vec::new();

  for (name, bytes) in handed {
    let text = String::from_utf8_lossy(bytes);
    let mut values = vec![text.trim().to_owned()];

    for line in text.lines() {
      values.push(line.trim().to_owned());

      if let Some((_, value)) = line.split_once('=') {
        values.push(value.trim().to_owned());
      }
    }

    for value in values {
      if value.chars().count() >= MASKED_FROM {
        masks.push((value, format!("[secret {name}]")));
      }
    }
  }

  masks.sort_by_key(|(value, _)| std::cmp::Reverse(value.len()));
  masks.dedup();
  masks
}

/// A line the tool wrote with every secret's value in it masked.
pub fn masked(masks: &[(String, String)], mut text: String) -> String {
  for (value, mask) in masks {
    if text.contains(value.as_str()) {
      text = text.replace(value.as_str(), mask);
    }
  }

  text
}

// The runner's own binary, which forwards a confinement's loopback to its proxy: where it stands now, refused where
// a new tree has taken its place since the runner started.
fn own_binary() -> Result<PathBuf, String> {
  let binary =
    std::env::current_exe().map_err(|why| format!("the runner's own binary is not found: {why}"))?;

  if !binary.is_file() {
    return Err("the runner's own binary was replaced; ask the call again".to_owned());
  }

  Ok(binary)
}

// The call's proxy: its folder made, its socket listened on, and the proxy served there for the addresses listed.
fn serve_proxy(
  runner: &Runner,
  listed: &[String],
) -> Result<(crate::proxy::CallFolder, tokio::task::JoinHandle<()>), String> {
  let reach = crate::proxy::Reach::of(listed, runner.stand_ins.clone())
    .map_err(|why| format!("the call's network does not read: {why}"))?;
  let folder = crate::proxy::CallFolder::make()?;
  let listener = tokio::net::UnixListener::bind(folder.socket())
    .map_err(|why| format!("the call's proxy could not listen: {why}"))?;

  Ok((folder, tokio::spawn(crate::proxy::serve(listener, reach))))
}

// A pipe for each secret, its bytes written into it by a thread of its own and its reading end answered, which the
// confinement reads the secret from.
fn secret_pipes(handed: &BTreeMap<String, Vec<u8>>) -> Result<Vec<(String, OwnedFd)>, String> {
  let mut readers = Vec::new();

  for (name, bytes) in handed {
    let (reader, mut writer) =
      std::io::pipe().map_err(|why| format!("the secret {name} could not be handed: {why}"))?;
    let bytes = bytes.clone();

    std::thread::spawn(move || {
      use std::io::Write;

      let _ = writer.write_all(&bytes);
    });
    readers.push((name.clone(), OwnedFd::from(reader)));
  }

  Ok(readers)
}
