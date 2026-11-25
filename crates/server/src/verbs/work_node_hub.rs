//! A person's PC reached through its hub for the workplace's setup (docs/person-nodes.md, 「The workplace on a
//! PC」): each command a transient unit of the person's own user manager, started and watched through the hub's
//! commands, each of which ends well inside the hub's ceiling of time on one; and the copy of the code and of
//! herdr made through the hub's uploads. Every request carries a credential that reaches the node's agent alone
//! (「Who reaches a node」).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use node_link::hub;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sse_stream::SseStream;

use super::work_node_setup::{
  COPY_LEFT_OUT, FarEnd, NO_CODE, SetupHands, SetupShell, ShellRun, shell_word,
};
use crate::peers::node::HubReach;

/// What a Windows PC's Linux shell is asked of its networking: WSL's networking mode, then the range of ports Linux
/// takes for its own connections, which mirrored networking keeps from every Windows program.
const WSL_NETWORKING: &str =
  "wslinfo --networking-mode 2>/dev/null || echo unknown; cat /proc/sys/net/ipv4/ip_local_port_range";

/// What a PC's Linux shell is asked of the machine it is: the id systemd keeps for it, one for every agent that
/// reaches that shell.
const MACHINE_ID: &str = "cat /etc/machine-id";

/// A command as the hub is handed it (docs/person-nodes.md, 「The workplace on a PC」): one line that decodes it and
/// gives it to the Linux shell, holding nothing a shell on the way would read in the command's place.
pub fn carried(command: &str) -> String {
  format!("printf %s {} | base64 -d | /bin/bash", STANDARD.encode(command))
}

/// The shells the hub's agent may offer for Linux, the agent's own first: where the agent runs inside the
/// Linux shell, and WSL's where it runs on Windows.
const WSL: &str = "wsl";
const LINUX_SHELLS: [&str; 2] = ["linux", WSL];

/// A file's place as an agent on Windows writes it: the folder as WSL's Windows view names it, and the file's
/// name under it.
pub fn windows_view(folder_view: &str, name: &str) -> String {
  format!(r"{}\{name}", folder_view.trim().trim_end_matches('\\'))
}

/// How long one command through the hub waits on a step before it answers that the step still runs, in tenths
/// of a second: well inside the hub's ceiling of time on one command.
const WATCH_TENTHS: u32 = 2400;

/// How many times, and how far apart, a step's watch is asked again while the hub cannot reach the agent: long
/// enough for the agent to restart and attach again, the step running on meanwhile.
const AGAIN_TRIES: u32 = 60;
const AGAIN_EVERY: std::time::Duration = std::time::Duration::from_secs(2);

/// The most of a step's output read back, from its end, per stream.
const OUTPUT_TAIL_BYTES: u32 = 1 << 20;

/// What runs a step on the PC, given the base path of its files: the step itself with nothing on its input, its
/// output and errors into files beside it, and the code it ended with written last and whole.
const STEP_WRAPPER: &str = r#"b=$1
bash "$b.cmd" </dev/null >"$b.out" 2>"$b.err"
echo $? >"$b.code.part" && mv "$b.code.part" "$b.code"
"#;

/// What a command through the hub is run with beside its words: WSL's own messages asked for as UTF-8, since its
/// launcher otherwise writes them as UTF-16, which reads here as nothing legible. A shell of the agent's own is
/// handed nothing.
fn shell_env(shell: &str) -> Value {
  if shell == WSL { json!({ "WSL_UTF8": "1" }) } else { json!({}) }
}

/// The directory a step's files stand in on the PC.
const STEP_DIR: &str = r#"${XDG_CACHE_HOME:-$HOME/.cache}/bmo-node-setup"#;

/// One hub and one agent attached to it, as the setup reaches the PC through them.
pub struct HubDoor {
  client: reqwest::Client,
  address: String,
  /// The control credential that reaches this agent alone.
  credential: String,
  agent: String,
  shell: Option<&'static str>,
  capabilities: Vec<String>,
  /// What names this run's steps apart from any other run's, and the count of steps so far.
  run: String,
  steps: AtomicU64,
}

// A file's path as written, a leading `~/` meaning the home.
fn expand_home(path: &str, home: &Path) -> PathBuf {
  path.strip_prefix("~/").map_or_else(|| PathBuf::from(path), |rest| home.join(rest))
}

fn failed(why: impl std::fmt::Display) -> ShellRun {
  ShellRun {
    code: NO_CODE,
    stdout: String::new(),
    stderr: why.to_string(),
  }
}

impl HubDoor {
  /// The door to the agent the reach names, with what the agent offers as the hub last heard it; refused where
  /// the hub cannot be asked or the agent is not attached.
  pub async fn open(reach: &HubReach, home: &Path) -> Result<Self, String> {
    let settings = expand_home(&reach.token_file, home);
    let credential = hub::agent_credential(&settings, &reach.agent)?;
    let address = reach.address.trim_end_matches('/').to_owned();
    let client = reqwest::Client::new();
    let status: Value = client
      .get(format!("{address}/status"))
      .bearer_auth(&credential)
      .send()
      .await
      .and_then(reqwest::Response::error_for_status)
      .map_err(|why| format!("the hub at {address} did not answer: {why}"))?
      .json()
      .await
      .map_err(|why| format!("the hub at {address} answered no status: {why}"))?;
    let attached = status
      .get("agents")
      .and_then(Value::as_array)
      .and_then(|agents| {
        agents.iter().find(|agent| agent.get("name").and_then(Value::as_str) == Some(reach.agent.as_str()))
      })
      .ok_or_else(|| {
        format!(
          "the hub has no agent {} attached; a machine just turned on attaches after its redial wait, so ask again",
          reach.agent
        )
      })?;
    let capabilities: Vec<String> = attached
      .get("capabilities")
      .and_then(Value::as_array)
      .map(|all| all.iter().filter_map(Value::as_str).map(str::to_owned).collect())
      .unwrap_or_default();
    let shell = LINUX_SHELLS
      .into_iter()
      .find(|shell| capabilities.iter().any(|offered| offered == &format!("exec:{shell}")));
    let started = chrono::Utc::now().timestamp_millis();

    Ok(Self {
      client,
      address,
      credential,
      agent: reach.agent.clone(),
      shell,
      capabilities,
      run: format!("{}-{started}", std::process::id()),
      steps: AtomicU64::new(0),
    })
  }

  /// What the hub's agent does not offer that the workplace and its link need, one thing per line: a Linux shell,
  /// and the far end's mirror and herdr ports to stream to.
  pub fn wanting(&self, far_end: &FarEnd) -> Vec<String> {
    let offers = |capability: String| self.capabilities.contains(&capability);
    let agent = &self.agent;
    let mut wanting = Vec::new();

    if self.shell.is_none() {
      wanting.push(format!("jake: the agent {agent} offers no Linux shell"));
    }

    for (port, what) in [(far_end.mirror_port, "the mirror"), (far_end.herdr_port, "herdr")] {
      if !offers(format!("stream:{port}")) {
        wanting.push(format!(
          "jake: the agent {agent} does not offer port {port} for {what} to stream to; its JAKE_STREAM_PORTS must list it"
        ));
      }
    }

    wanting
  }

  /// One command through the hub, as its output and its end came back, or why the hub told no end of it.
  async fn exec(&self, command: &str) -> Result<ShellRun, String> {
    let Some(shell) = self.shell else {
      return Err(format!("the agent {} offers no Linux shell", self.agent));
    };
    let sent = self
      .client
      .post(format!("{}/exec", self.address))
      .query(&[("agent", self.agent.as_str())])
      .bearer_auth(&self.credential)
      .json(&json!({ "shell": shell, "command": carried(command), "env": shell_env(shell) }))
      .send()
      .await
      .and_then(reqwest::Response::error_for_status);
    let answer = match sent {
      Ok(answer) => answer,
      Err(why) => return Err(format!("the hub did not run the command: {why}")),
    };
    let mut events = std::pin::pin!(SseStream::from_bytes_stream(answer.bytes_stream()));
    let mut run = ShellRun::default();

    while let Some(event) = events.next().await {
      let Ok(event) = event else { break };
      let data: Value = event
        .data
        .as_deref()
        .and_then(|data| serde_json::from_str(data).ok())
        .unwrap_or_default();
      let chunk = || {
        data
          .pointer("/chunk/base64")
          .and_then(Value::as_str)
          .and_then(|text| STANDARD.decode(text).ok())
          .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
          .unwrap_or_default()
      };

      match event.event.as_deref() {
        Some("stdout") => run.stdout.push_str(&chunk()),
        Some("stderr") => run.stderr.push_str(&chunk()),
        Some("exit") => {
          run.code = data.get("code").and_then(Value::as_i64).map_or(255, |code| code as i32);

          return Ok(run);
        }
        Some("error" | "ended") => {
          let said = data.get("message").or_else(|| data.get("reason")).and_then(Value::as_str);

          return Err(format!("the hub: {}", said.unwrap_or("the command failed")));
        }
        _ => {}
      }
    }

    Err("the hub's answer ended before the command did".into())
  }

  /// WSL's networking on a Windows PC, said plainly (docs/person-nodes.md, 「The workplace on a PC」): its mode, and
  /// in mirrored mode the range of ports its Linux side takes for its own connections, with every port of the far
  /// end's that lies in it; nothing where the agent runs inside the Linux shell itself.
  pub async fn wsl_networking(&self, ports: &[(u16, &str)]) -> Vec<String> {
    if self.shell != Some("wsl") {
      return Vec::new();
    }

    match self.exec_again(WSL_NETWORKING).await {
      Ok(read) if read.code == 0 => wsl_said(&read.stdout, ports),
      Ok(read) => vec![format!("wsl: its networking could not be read: {}", read.said())],
      Err(why) => vec![format!("wsl: its networking could not be read: {why}")],
    }
  }

  /// The machine id the PC's Linux shell reads, which tells one far end from another whichever agent reaches it
  /// (docs/person-nodes.md, 「The workplace on a PC」); or why it could not be read.
  pub async fn machine(&self) -> Result<String, String> {
    match self.exec_again(MACHINE_ID).await {
      Ok(read) if read.code == 0 && !read.stdout.trim().is_empty() => Ok(read.stdout.trim().to_owned()),
      Ok(read) => Err(read.said()),
      Err(why) => Err(why),
    }
  }

  /// A command that reads and may run again, asked again while the hub cannot reach the agent.
  async fn exec_again(&self, command: &str) -> Result<ShellRun, String> {
    let mut tries = 1;

    loop {
      match self.exec(command).await {
        Err(_) if tries < AGAIN_TRIES => {
          tries += 1;
          tokio::time::sleep(AGAIN_EVERY).await;
        }
        done => return done,
      }
    }
  }

  /// One step on the PC as a transient unit of the person's own, watched until it ends: its files written
  /// beside each other under the cache directory, its unit started with the user manager, and what it wrote
  /// read back once its code stands. A program the step starts apart outlives its unit.
  async fn step(&self, command: &str) -> ShellRun {
    let number = self.steps.fetch_add(1, Ordering::Relaxed);
    let unit = format!("bmo-node-step-{}-{number}", self.run);
    let base = format!(r#""{STEP_DIR}/{unit}""#);
    let start = format!(
      r#"mkdir -p "{STEP_DIR}" && printf %s {} | base64 -d >{base}.cmd && printf %s {} | base64 -d >{base}.sh && systemd-run --user --collect --quiet --unit={unit} -p KillMode=process /bin/bash {base}.sh {base}"#,
      STANDARD.encode(command),
      STANDARD.encode(STEP_WRAPPER),
    );
    let watch = format!(
      r#"i=0; while [ ! -f {base}.code ] && [ $i -lt {WATCH_TENTHS} ]; do sleep 0.1; i=$((i+1)); done; if [ -f {base}.code ]; then echo ended; elif systemctl --user is-active -q {unit}; then echo running; elif [ -f {base}.code ]; then echo ended; else echo lost; fi"#
    );
    // A start the hub told no end of is not asked again, since the step may already run.
    let started = match self.exec(&format!("{start} && {watch}")).await {
      Ok(started) if started.code == 0 => started,
      Ok(started) => return failed(format!("the step's unit did not start: {}", started.said())),
      Err(why) => return failed(format!("the step's unit may not have started: {why}")),
    };
    let mut state = started.stdout.trim().to_owned();

    while state == "running" {
      match self.exec_again(&watch).await {
        Ok(watched) if watched.code == 0 => state = watched.stdout.trim().to_owned(),
        Ok(watched) => return failed(format!("the step's unit could not be watched: {}", watched.said())),
        Err(why) => return failed(format!("the step's unit could not be watched: {why}")),
      }
    }

    if state != "ended" {
      return failed(format!("the step's unit {unit} ended without saying how"));
    }

    // The files are removed only by a read that came back, so a read asked again finds them still there.
    let read = format!(
      r#"tail -c {OUTPUT_TAIL_BYTES} {base}.out; tail -c {OUTPUT_TAIL_BYTES} {base}.err >&2; exit "$(cat {base}.code)""#
    );
    let ran = match self.exec_again(&read).await {
      Ok(ran) => ran,
      Err(why) => return failed(format!("what the step wrote could not be read back: {why}")),
    };
    let _ = self
      .exec(&format!("rm -f {base}.cmd {base}.sh {base}.out {base}.err {base}.code"))
      .await;

    ran
  }

  /// The path the agent writes a Linux path's file at: the path itself where the agent runs inside the Linux
  /// shell, and WSL's Windows view of it where the agent runs on Windows, whose own filesystem holds no Linux
  /// path. The folder is made first, since the view is asked of a folder that stands.
  async fn written_at(&self, path: &str) -> Result<String, String> {
    if self.shell != Some(WSL) {
      return Ok(path.to_owned());
    }

    let (folder, name) =
      path.rsplit_once('/').ok_or_else(|| format!("{path} names no folder to upload into"))?;
    let folder = shell_word(folder);
    let asked = self.exec(&format!("mkdir -p {folder} && wslpath -w {folder}")).await?;

    if asked.code != 0 || asked.stdout.trim().is_empty() {
      return Err(format!("WSL gave no Windows view of {path}: {}", asked.stderr.trim()));
    }

    Ok(windows_view(&asked.stdout, name))
  }

  /// A file of this machine written at a path of the PC through the hub's uploads, its length and checksum
  /// declared, so the agent moves it into place only whole.
  async fn upload(&self, file: &Path, path: &str) -> Result<(), String> {
    let path = &self.written_at(path).await?;
    let shown = |why: std::io::Error| format!("{}: {why}", file.display());
    let total = std::fs::metadata(file).map_err(shown)?.len();
    let mut hash = Sha256::new();

    std::io::copy(&mut std::fs::File::open(file).map_err(shown)?, &mut hash).map_err(shown)?;

    let checksum = format!("{:x}", hash.finalize());
    let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(
      tokio::fs::File::open(file).await.map_err(shown)?,
    ));
    let answer = self
      .client
      .post(format!("{}/upload", self.address))
      .query(&[
        ("agent", self.agent.as_str()),
        ("path", path),
        ("total", &total.to_string()),
        ("checksum", &checksum),
      ])
      .bearer_auth(&self.credential)
      .body(body)
      .send()
      .await
      .and_then(reqwest::Response::error_for_status)
      .map_err(|why| format!("the hub took no upload: {why}"))?;
    let mut events = std::pin::pin!(SseStream::from_bytes_stream(answer.bytes_stream()));

    while let Some(event) = events.next().await {
      let event = event.map_err(|why| format!("the upload's answer broke off: {why}"))?;
      let data: Value = event
        .data
        .as_deref()
        .and_then(|data| serde_json::from_str(data).ok())
        .unwrap_or_default();

      match event.event.as_deref() {
        Some("completed") => return Ok(()),
        Some("error" | "ended") => {
          let said = data.get("message").or_else(|| data.get("reason")).and_then(Value::as_str);

          return Err(format!("the upload failed: {}", said.unwrap_or("no reason given")));
        }
        _ => {}
      }
    }

    Err("the upload's answer ended before it completed".into())
  }
}

impl SetupShell for HubDoor {
  fn run<'a>(&'a self, command: &'a str) -> BoxFuture<'a, ShellRun> {
    Box::pin(self.step(command))
  }
}

async fn captured(mut command: tokio::process::Command) -> (bool, String) {
  match command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).output().await {
    Ok(done) => (done.status.success(), String::from_utf8_lossy(&done.stderr).trim().to_owned()),
    Err(why) => (false, why.to_string()),
  }
}

/// What the setup may do on this machine's side for a PC: copy through the hub's uploads; a PC is never saved
/// as a machine of this herdr, which reaches a machine over SSH alone.
pub struct HubHands<'a> {
  pub install: bool,
  pub copy_code: bool,
  pub door: &'a HubDoor,
  /// The directory here the copy is gathered and packed in, under the node's own.
  pub staging: PathBuf,
}

impl HubHands<'_> {
  // A tree of this machine laid over a tree of the PC: the files an SSH copy sends gathered into a mirror here,
  // packed, uploaded beside the tree there, unpacked and laid over it with the same files left out, so what is
  // built or installed there stays.
  async fn copy_tree(&self, from: &str, to: &str) -> Result<(), String> {
    let mirror = self.staging.join("copy");
    let archive = self.staging.join("copy.tar.gz");
    let excluded: Vec<String> = COPY_LEFT_OUT
      .iter()
      .flat_map(|pattern| ["--exclude".to_owned(), (*pattern).to_owned()])
      .collect();

    std::fs::create_dir_all(&mirror).map_err(|why| format!("{}: {why}", mirror.display()))?;

    let mut gather = tokio::process::Command::new("rsync");

    gather
      .args(["-a", "--delete"])
      .args(&excluded)
      .arg(from)
      .arg(format!("{}/", mirror.display()));

    match captured(gather).await {
      (true, _) => {}
      (false, said) => return Err(format!("the copy could not be gathered here: {said}")),
    }

    let mut pack = tokio::process::Command::new("tar");

    pack.arg("-czf").arg(&archive).arg("-C").arg(&mirror).arg(".");

    match captured(pack).await {
      (true, _) => {}
      (false, said) => return Err(format!("the copy could not be packed: {said}")),
    }

    let tree = to.trim_end_matches('/');
    let incoming = format!("{tree}.incoming");
    let uploaded = format!("{incoming}.tar.gz");

    self.door.upload(&archive, &uploaded).await?;

    let excluded = excluded.iter().map(|word| shell_word(word)).collect::<Vec<_>>().join(" ");
    let laid = self
      .door
      .run(&format!(
        "rm -rf {incoming} && mkdir -p {incoming} {tree} && tar -xzf {uploaded} -C {incoming} && rsync -a --delete {excluded} {incoming}/ {tree}/ && rm -rf {incoming} {uploaded}",
        incoming = shell_word(&incoming),
        uploaded = shell_word(&uploaded),
        tree = shell_word(tree),
      ))
      .await;

    if laid.code != 0 {
      return Err(format!("the copy could not be laid out on the node: {}", laid.said()));
    }

    Ok(())
  }

  // One file of this machine written on the PC with its mode.
  async fn copy_file(&self, from: &str, to: &str) -> Result<(), String> {
    let mode =
      std::fs::metadata(from).map_err(|why| format!("{from}: {why}"))?.permissions().mode() & 0o777;

    self.door.upload(Path::new(from), to).await?;

    let changed = self.door.run(&format!("chmod {mode:o} {}", shell_word(to))).await;

    if changed.code != 0 {
      return Err(format!("{to}: {}", changed.said()));
    }

    Ok(())
  }
}

impl SetupHands for HubHands<'_> {
  fn install(&self) -> bool {
    self.install
  }

  fn copy_code(&self) -> bool {
    self.copy_code
  }

  fn copy<'a>(&'a self, from: &'a str, to: &'a str) -> BoxFuture<'a, (bool, String)> {
    Box::pin(async move {
      let copied = if Path::new(from).is_dir() {
        self.copy_tree(from, to).await
      } else {
        self.copy_file(from, to).await
      };

      match copied {
        Ok(()) => (true, String::new()),
        Err(why) => (false, why),
      }
    })
  }

  fn add_machine(&self) -> BoxFuture<'_, (bool, String)> {
    Box::pin(async {
      (
        true,
        "already saved: a PC is reached through its hub, never as a herdr machine".to_owned(),
      )
    })
  }

  fn say(&self, line: &str) {
    super::work_node::say(line);
  }
}

/// What WSL's networking reading says, a line each: the mode, and in mirrored mode the range Windows keeps for the
/// Linux side's own connections and each of the ports given that lies in it. The reading is the mode on its first
/// line and the kernel's two bounds on its second, as the kernel writes them, separated by white space.
pub fn wsl_said(reading: &str, ports: &[(u16, &str)]) -> Vec<String> {
  let mut lines = reading.lines().map(str::trim).filter(|line| !line.is_empty());
  let mode = lines.next().unwrap_or("unknown").to_owned();
  let range: Vec<u16> = lines
    .next()
    .unwrap_or_default()
    .split_whitespace()
    .filter_map(|n| n.parse().ok())
    .collect();
  let mut said = vec![format!("wsl: networking mode {mode}")];

  if let ([low, high], "mirrored") = (range.as_slice(), mode.as_str()) {
    said[0] = format!(
      "wsl: networking mode mirrored; Windows keeps ports {low}-{high} for the Linux side's own connections"
    );

    for (port, what) in ports.iter().filter(|(port, _)| (low..=high).contains(&port)) {
      said.push(format!(
        "wsl: port {port} for {what} lies in that range, where a Linux connection's own port may hold it when the far end starts again"
      ));
    }
  }

  said
}
