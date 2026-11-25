//! LiteLLM on one table: the table written beside the hooks it names, at `0600`, the keys and the front's
//! own address in the child's environment alone, and the child ready once it answers its liveliness path.

use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use http_body_util::Empty;
use hyper::body::Bytes;
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::front::{BoxFuture, Child};
use crate::keys::read_key;
use crate::table::{GATEWAY_KEY_VARIABLE, Table};

/// How often a child coming up is asked whether it answers.
const READINESS_LOOK: Duration = Duration::from_millis(500);
// How many free ports a start tries where another process takes the port picked before the child binds it.
const PORT_TRIES: usize = 5;

/// Where a launcher keeps its tables and reads its keys, and the hooks it copies beside each table.
pub struct Place {
  /// The data root's `llm-gateway/`, private to the account.
  pub folder: PathBuf,
  /// The data root's `env/`.
  pub env_dir: PathBuf,
  /// The gateway's own key, read once as the launcher starts: the one the front checks, handed to each
  /// child as LiteLLM's own.
  pub gateway_key: String,
  pub hooks: Vec<PathBuf>,
  /// The front's own address, which the costs hook hands each request's cost to.
  pub front: String,
}

/// The variable the child is handed the front's own address in.
pub const FRONT_VARIABLE: &str = "LLM_GATEWAY_FRONT";

impl Place {
  pub fn table_path(&self, digest: &str) -> PathBuf {
    self.folder.join(format!("config-{digest}.yaml"))
  }

  /// Each hook's name and what its file holds now, which the table's digest covers.
  pub fn hook_texts(&self) -> Vec<(String, Vec<u8>)> {
    self
      .hooks
      .iter()
      .map(|path| {
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        (name, std::fs::read(path).unwrap_or_default())
      })
      .collect()
  }

  /// A table left by a launcher that ended without stopping its children is nobody's.
  pub fn clear_leftovers(&self) {
    let Ok(entries) = std::fs::read_dir(&self.folder) else {
      return;
    };
    for entry in entries.flatten() {
      let name = entry.file_name().to_string_lossy().into_owned();
      if name.starts_with("config-") && name.contains(".yaml") {
        let _ = std::fs::remove_file(entry.path());
      }
    }
  }
}

// A file written private and renamed into place, so no reader sees half of it.
pub fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
  let staged = PathBuf::from(format!("{}.{}", path.display(), std::process::id()));
  std::fs::write(&staged, text)?;
  std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o600))?;
  std::fs::rename(&staged, path)
}

struct Litellm {
  port: u16,
  process: Mutex<Option<tokio::process::Child>>,
  table: PathBuf,
}

impl Child for Litellm {
  fn port(&self) -> u16 {
    self.port
  }

  fn stop(&self) -> BoxFuture<'_, ()> {
    Box::pin(async move {
      if let Some(mut process) = self.process.lock().await.take() {
        stop_process(&mut process).await;
      }
      let _ = std::fs::remove_file(&self.table);
    })
  }
}

// A child asked to end, and waited for until it has.
async fn stop_process(process: &mut tokio::process::Child) {
  if let Some(pid) = process.id() {
    unsafe {
      libc::kill(pid as i32, libc::SIGTERM);
    }
  }
  let _ = process.wait().await;
}

fn free_port() -> std::io::Result<u16> {
  Ok(std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

// Whether a process outside the child's own family is seen holding the loopback listener on the port. A port
// picked free stands open to any process until the child binds it, so an answer there is not the child's where
// another process is seen listening on it; where no listener's holder can be read, the answer is taken as the
// child's, so a reading that fails never stops a start.
fn held_by_another(pid: u32, port: u16) -> bool {
  let Ok(table) = procfs::net::tcp() else { return false };
  let sockets: BTreeSet<u64> = table
    .iter()
    .filter(|entry| entry.state == procfs::net::TcpState::Listen && entry.local_address.port() == port)
    .map(|entry| entry.inode)
    .collect();

  if sockets.is_empty() {
    return false;
  }

  let Ok(all) = procfs::process::all_processes() else { return false };
  // Each process is read and its handle let go before the next is opened, since a handle holds a
  // descriptor: the look holds a few files open however many processes the machine runs.
  let processes: Vec<(i32, Option<i32>, bool)> = all
    .filter_map(Result::ok)
    .map(|one| {
      let parent = one.stat().ok().map(|stat| stat.ppid);
      let listens = one.fd().is_ok_and(|fds| {
        fds.filter_map(Result::ok).any(
          |fd| matches!(fd.target, procfs::process::FDTarget::Socket(inode) if sockets.contains(&inode)),
        )
      });

      (one.pid, parent, listens)
    })
    .collect();
  let mut family = BTreeSet::from([pid as i32]);

  loop {
    let before = family.len();

    for (child, parent, _) in &processes {
      if parent.is_some_and(|parent| family.contains(&parent)) {
        family.insert(*child);
      }
    }

    if family.len() == before {
      break;
    }
  }

  processes.iter().any(|(one, _, listens)| *listens && !family.contains(one))
}

// Whether the child answers its liveliness path with a success: a refused or broken connection is no, and
// an answer is waited for.
async fn lively(port: u16) -> bool {
  let asked = async {
    let stream = TcpStream::connect(("127.0.0.1", port)).await.ok()?;
    let (mut sender, connection) =
      hyper::client::conn::http1::handshake(TokioIo::new(stream)).await.ok()?;
    tokio::spawn(connection);
    let request = hyper::Request::get("/health/liveliness")
      .header(hyper::header::HOST, format!("127.0.0.1:{port}"))
      .body(Empty::<Bytes>::new())
      .ok()?;
    Some(sender.send_request(request).await.ok()?.status().is_success())
  };
  asked.await.unwrap_or(false)
}

/// LiteLLM started on the table and waited for until it answers or ends, however long it takes.
pub async fn start(place: &Place, table: &Table, digest: &str) -> Result<Box<dyn Child>, String> {
  let table_path = place.table_path(digest);
  write_private(&table_path, &table.text)
    .map_err(|fault| format!("the table could not be written: {fault}"))?;
  // LiteLLM loads a callback from a file of its name beside the table.
  for hook in &place.hooks {
    if let Some(name) = hook.file_name() {
      std::fs::copy(hook, place.folder.join(name))
        .map_err(|fault| format!("{} could not be copied: {fault}", hook.display()))?;
    }
  }

  let mut env = BTreeMap::new();
  env.insert("PATH".to_owned(), std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_owned()));
  env.insert("HOME".to_owned(), std::env::var("HOME").unwrap_or_default());
  env.insert(FRONT_VARIABLE.to_owned(), place.front.clone());
  env.insert(GATEWAY_KEY_VARIABLE.to_owned(), place.gateway_key.clone());
  for (variable, key_file) in &table.key_files {
    let key = read_key(&place.env_dir, key_file);
    if key.is_none() {
      eprintln!(
        "llm-gateway: the key file {key_file} is missing or not 0600; its addresses will be refused"
      );
    }
    env.insert(variable.clone(), key.unwrap_or_default());
  }

  for _ in 0..PORT_TRIES {
    if let Some(started) = started_on_a_free_port(place, &table_path, &env).await {
      return started;
    }
  }

  let _ = std::fs::remove_file(&table_path);
  Err(format!("another process took each of {PORT_TRIES} free ports before LiteLLM bound it"))
}

// LiteLLM started on a port picked free and waited for: its answer, or none where another process holds the
// port, the child then ended, so the start is tried again on another.
async fn started_on_a_free_port(
  place: &Place,
  table_path: &Path,
  env: &BTreeMap<String, String>,
) -> Option<Result<Box<dyn Child>, String>> {
  let port = match free_port() {
    Ok(port) => port,
    Err(fault) => return Some(Err(format!("no free port: {fault}"))),
  };
  let spawned = Command::new("litellm")
    .args([
      "--config",
      &table_path.to_string_lossy(),
      "--host",
      "127.0.0.1",
      "--port",
      &port.to_string(),
    ])
    .args(["--num_workers", "1"])
    .current_dir(&place.folder)
    .env_clear()
    .envs(env)
    .stdin(Stdio::null())
    .spawn();
  let mut process = match spawned {
    Ok(process) => process,
    Err(fault) => {
      let _ = std::fs::remove_file(table_path);
      return Some(Err(format!("LiteLLM could not be started: {fault}")));
    }
  };
  let pid = process.id().unwrap_or_default();

  loop {
    if let Ok(Some(ended)) = process.try_wait() {
      // A child that could not bind the port another process holds is started again on another.
      if lively(port).await {
        eprintln!("llm-gateway: another process holds port {port}; LiteLLM starts again on another");
        return None;
      }

      let _ = std::fs::remove_file(table_path);
      let code = ended.code().map_or_else(|| "a signal".to_owned(), |code| code.to_string());
      return Some(Err(format!("LiteLLM ended with {code} before it answered")));
    }
    if lively(port).await {
      if !held_by_another(pid, port) {
        return Some(Ok(Box::new(Litellm {
          port,
          process: Mutex::new(Some(process)),
          table: table_path.to_path_buf(),
        })));
      }

      eprintln!("llm-gateway: another process holds port {port}; LiteLLM starts again on another");
      stop_process(&mut process).await;
      return None;
    }
    tokio::time::sleep(READINESS_LOOK).await;
  }
}
