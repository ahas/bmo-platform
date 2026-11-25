//! The home bridge's near end (docs/work-nodes.md, 「The home bridge」): a script that only this machine
//! can run — it reads the archive or the data root — asked for from a node, run here out of the deploy
//! tree the spec names, the CLI's, its output, errors and exit carried back.
//!
//! One connection is one run. The asker writes one JSON line naming the script, its arguments, the run's
//! own variables and the directory it was asked from, then its standing input as frames of a channel
//! byte and a length — 1 for a piece of input, 0 for its end — since an asker that half-closed its
//! socket could no longer read the answer. The answer is frames the same way: 1 for output, 2 for
//! errors, and 3 for the exit code, which ends it. A call refused, or a request line that cannot be read, is
//! answered on the request line alone and the connection closed with the asker's input unread, so an asker
//! keeps reading the answer where writing its input fails.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;

use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::Command;
use tokio::sync::mpsc;

/// A piece of the asker's input; a frame on any other channel, the end frame among them, ends it.
pub const IN: u8 = 1;
pub const OUT: u8 = 1;
pub const ERR: u8 = 2;
pub const EXIT: u8 = 3;

#[derive(Debug, Deserialize)]
pub struct Asked {
  pub script: String,
  #[serde(default)]
  pub args: Vec<String>,
  #[serde(default)]
  pub env: BTreeMap<String, String>,
  #[serde(default)]
  pub cwd: Option<String>,
}

/// The CLI's verbs' binary under the deploy tree, which the bridge runs as it runs a script.
pub const VERBS_BINARY: &str = "bin/bmo";

/// The script asked for, where it is one of the deploy tree's own: a path under its `scripts/` with no
/// step out of it, or the verbs' binary.
pub fn script_in(deploy: &Path, asked: &str) -> Option<PathBuf> {
  if asked == VERBS_BINARY {
    return Some(deploy.join(VERBS_BINARY));
  }

  let path = Path::new(asked);

  if path.is_absolute() || path.components().any(|part| !matches!(part, Component::Normal(_))) {
    return None;
  }

  let mut parts = path.components();

  if parts.next() != Some(Component::Normal("scripts".as_ref())) || !asked.ends_with(".ts") {
    return None;
  }

  Some(deploy.join(path))
}

/// Why a call is not run here, where the node may ask only for `verbs`: a script, or a call of the verbs' binary
/// whose first word is none of them; none where the node may ask for any.
pub fn refusal(asked: &Asked, verbs: Option<&[String]>) -> Option<String> {
  let verbs = verbs?;
  let verb = asked.args.first().map(String::as_str).unwrap_or_default();

  if asked.script == VERBS_BINARY && verbs.iter().any(|allowed| allowed == verb) {
    return None;
  }

  Some(format!(
    "{} {} is not run for this node, which asks the bridge for {} alone, each its call's first word",
    asked.script,
    asked.args.join(" "),
    verbs.join(", ")
  ))
}

/// The variables a run hands over that this machine reads as the node meant them: the run's own and
/// its place, every path in them read back as this machine's.
pub fn carried_env(
  asked: &BTreeMap<String, String>,
  from_node: impl Fn(&str) -> String,
) -> BTreeMap<String, String> {
  asked
    .iter()
    .filter(|(name, _)| name.starts_with("BMO_") || name.starts_with("CLAUDE"))
    .filter(|(name, _)| {
      !matches!(
        name.as_str(),
        "BMO_HOME_BRIDGE"
          | "BMO_DATA_DIR"
          | "BMO_SOURCE_DIR"
          | "BMO_DB_PATH"
          | "BMO_ARCHIVE_URL"
          | "CLAUDE_CONFIG_DIR"
      )
    })
    .map(|(name, value)| (name.clone(), from_node(value)))
    .collect()
}

async fn frame(out: &mpsc::UnboundedSender<(u8, Vec<u8>)>, channel: u8, mut from: impl AsyncRead + Unpin) {
  let mut buffer = vec![0; 64 * 1024];

  loop {
    match from.read(&mut buffer).await {
      Ok(0) | Err(_) => return,
      Ok(read) => {
        if out.send((channel, buffer[..read].to_vec())).is_err() {
          return;
        }
      }
    }
  }
}

pub async fn write_frame(
  to: &mut (impl AsyncWrite + Unpin),
  channel: u8,
  bytes: &[u8],
) -> std::io::Result<()> {
  to.write_all(&[channel]).await?;
  to.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
  to.write_all(bytes).await
}

/// One run asked over the bridge, answered on the same connection; where `verbs` names the verbs the node may
/// ask for, any other call is refused.
pub async fn answer(
  stream: UnixStream,
  deploy: PathBuf,
  node: String,
  verbs: Option<Vec<String>>,
  from_node: impl Fn(&str) -> String,
) -> std::io::Result<()> {
  let (reading, mut writing) = stream.into_split();
  let mut reading = BufReader::new(reading);
  let mut header = String::new();

  reading.read_line(&mut header).await?;

  let asked: Asked = match serde_json::from_str(header.trim_end()) {
    Ok(asked) => asked,
    Err(error) => {
      write_frame(&mut writing, ERR, format!("node-link bridge: unreadable request: {error}\n").as_bytes())
        .await?;
      return write_frame(&mut writing, EXIT, &2i32.to_be_bytes()).await;
    }
  };

  if let Some(why) = refusal(&asked, verbs.as_deref()) {
    write_frame(&mut writing, ERR, format!("node-link bridge: {why}\n").as_bytes()).await?;
    return write_frame(&mut writing, EXIT, &2i32.to_be_bytes()).await;
  }

  let Some(script) = script_in(&deploy, &asked.script) else {
    write_frame(
      &mut writing,
      ERR,
      format!("node-link bridge: {} is no script of the deploy tree\n", asked.script).as_bytes(),
    )
    .await?;
    return write_frame(&mut writing, EXIT, &2i32.to_be_bytes()).await;
  };

  // The verbs' binary runs on its own; a script, on bun.
  let mut command = if asked.script == VERBS_BINARY {
    Command::new(&script)
  } else {
    let mut on_bun = Command::new("bun");

    on_bun.arg(&script);
    on_bun
  };

  command
    // A path of the node among the arguments — the place a hook names — is read as this machine's.
    .args(asked.args.iter().map(|arg| from_node(arg)))
    .current_dir(&deploy)
    .envs(carried_env(&asked.env, &from_node))
    .env("BMO_NODE", &node)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);

  if let Some(cwd) = &asked.cwd {
    command.env("BMO_NODE_CWD", cwd);
  }

  let mut child = match command.spawn() {
    Ok(child) => child,
    Err(error) => {
      write_frame(
        &mut writing,
        ERR,
        format!("node-link bridge: {}: {error}\n", script.display()).as_bytes(),
      )
      .await?;
      return write_frame(&mut writing, EXIT, &127i32.to_be_bytes()).await;
    }
  };

  let (send, mut frames) = mpsc::unbounded_channel();

  if let Some(mut input) = child.stdin.take() {
    tokio::spawn(async move {
      // Each piece of input as it comes, until the asker says it ended or goes away.
      loop {
        let mut head = [0u8; 5];

        if reading.read_exact(&mut head).await.is_err() || head[0] != IN {
          break;
        }

        let mut piece = vec![0; u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize];

        if reading.read_exact(&mut piece).await.is_err() || input.write_all(&piece).await.is_err() {
          break;
        }
      }

      let _ = input.shutdown().await;
    });
  }

  let outputs = [
    child.stdout.take().map(|out| tokio::spawn(frame_of(send.clone(), OUT, out))),
    child.stderr.take().map(|err| tokio::spawn(frame_of(send.clone(), ERR, err))),
  ];

  drop(send);

  while let Some((channel, bytes)) = frames.recv().await {
    if write_frame(&mut writing, channel, &bytes).await.is_err() {
      let _ = child.kill().await;
      return Ok(());
    }
  }

  for output in outputs.into_iter().flatten() {
    let _ = output.await;
  }

  let code = child.wait().await.ok().and_then(|status| status.code()).unwrap_or(1);

  write_frame(&mut writing, EXIT, &code.to_be_bytes()).await
}

async fn frame_of(send: mpsc::UnboundedSender<(u8, Vec<u8>)>, channel: u8, from: impl AsyncRead + Unpin) {
  frame(&send, channel, from).await;
}

/// The variables of a run on the node that go with what it asks of the home bridge.
fn asked_env() -> BTreeMap<String, String> {
  std::env::vars()
    .filter(|(name, _)| name.starts_with("BMO_") || name.starts_with("CLAUDE"))
    .collect()
}

/// One run asked of the home bridge from the node (docs/person-nodes.md, 「The link over jake」), as the CLI's
/// own asker asks it: the request line naming the script, its arguments, the run's variables and the folder it
/// was asked from; standing input carried as frames and its end said; the answer's output and errors written
/// into the writers given as they come; and the exit code the answer ends with, 2 where the bridge cannot be
/// reached or drops the run. The connection stays open both ways until the answer ends, since a half-closed
/// asker reads no answer.
pub async fn ask(
  socket: &Path,
  script: &str,
  args: &[String],
  input: impl AsyncRead + Unpin + Send + 'static,
  mut stdout: impl AsyncWrite + Unpin,
  mut stderr: impl AsyncWrite + Unpin,
) -> i32 {
  let stream = match UnixStream::connect(socket).await {
    Ok(stream) => stream,
    Err(error) => {
      eprintln!("node-link home: the home bridge at {} is out of reach: {error}", socket.display());
      return 2;
    }
  };
  let (mut reading, mut writing) = stream.into_split();
  let cwd = std::env::current_dir().ok().map(|dir| dir.display().to_string());
  let request = serde_json::json!({ "script": script, "args": args, "env": asked_env(), "cwd": cwd });

  if writing.write_all(format!("{request}\n").as_bytes()).await.is_err() {
    eprintln!("node-link home: the home bridge dropped the request");
    return 2;
  }

  let feeding = tokio::spawn(async move {
    let mut input = input;
    let mut buffer = vec![0; 64 * 1024];

    loop {
      match input.read(&mut buffer).await {
        Ok(0) | Err(_) => break,
        Ok(read) => {
          if write_frame(&mut writing, IN, &buffer[..read]).await.is_err() {
            return writing;
          }
        }
      }
    }

    let _ = write_frame(&mut writing, 0, &[]).await;
    writing
  });
  let code = loop {
    let mut head = [0u8; 5];

    if reading.read_exact(&mut head).await.is_err() {
      eprintln!("node-link home: the home bridge dropped the run before it ended");
      break 2;
    }

    let length = u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize;
    let mut body = vec![0; length];

    if reading.read_exact(&mut body).await.is_err() {
      eprintln!("node-link home: the home bridge dropped the run before it ended");
      break 2;
    }

    match head[0] {
      OUT => {
        let _ = stdout.write_all(&body).await;
        let _ = stdout.flush().await;
      }
      ERR => {
        let _ = stderr.write_all(&body).await;
        let _ = stderr.flush().await;
      }
      EXIT => {
        let code = <[u8; 4]>::try_from(body.as_slice()).map_or(1, i32::from_be_bytes);

        break if (0..=255).contains(&code) { code } else { 1 };
      }
      _ => {}
    }
  };

  feeding.abort();
  code
}
