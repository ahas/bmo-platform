//! The file locks agency's server holds, as the TypeScript side's file lock in `packages/support` does
//! (docs/worker-bench.md, 「It stands only while a job of the worker runs」): one file per checkout, per agent's
//! program and per worker's dev pane under the data root's `locks/`, held by `flock`, so
//! a lock of a process that died is no lock at all.

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

// How much of a name a lock file keeps, and how many hex digits of its digest follow it.
const SLUG_MOST: usize = 64;
const DIGEST_CHARS: usize = 12;

/// The file a lock named by a checkout's path, or by a word and a path, stands in: the name made
/// safe for a file, cut from the front where it is long — the tail names the checkout — with the
/// digest of the whole name carrying what the cut threw away.
pub fn lock_file_name(name: &str) -> String {
  let mut slug = String::new();
  let mut replacing = false;

  // Each run of characters a file name does not take becomes one dash.
  for char in name.chars() {
    let kept = char.is_ascii_alphanumeric() || matches!(char, '.' | '_' | '-');

    if kept {
      slug.push(char);
    } else if !replacing {
      slug.push('-');
    }

    replacing = !kept;
  }

  let slug = slug.trim_matches('-');
  let kept = slug[slug.len().saturating_sub(SLUG_MOST)..].trim_start_matches('-');
  let digest = format!("{:x}", Sha256::digest(name.as_bytes()));

  format!("{}-{}.lock", if kept.is_empty() { "root" } else { kept }, &digest[..DIGEST_CHARS])
}

/// Where a lock of this name stands under a data root.
pub fn lock_path(data_root: &Path, name: &str) -> PathBuf {
  data_root.join("locks").join(lock_file_name(name))
}

/// The lock an agent's one program is held under while a turn or a stand-in of it runs.
pub fn agent_lock_path(data_root: &Path, agent: &str) -> PathBuf {
  lock_path(data_root, &format!("agent:{agent}"))
}

/// A worker's pane lock beside the checkouts' locks, named by the worker, so two workers pointed at one
/// checkout each hold their own (docs/agent-control.md, 「A worker's pane lock」).
pub fn pane_lock_path(data_root: &Path, worker: &str) -> PathBuf {
  data_root.join("locks").join(format!("pane-{}", lock_file_name(worker)))
}

/// The lock one worker's dev pane is set up under.
pub fn dev_pane_lock_path(data_root: &Path, worker: &str) -> PathBuf {
  lock_path(data_root, &format!("dev-pane:{worker}"))
}

/// A lock held for as long as this stands, and let go the moment it goes.
pub struct Held(File);

// The lock is let go by name rather than by closing the file: a child forked by another thread holds a copy
// of the descriptor until it execs, and the lock stands for as long as any copy is open, so a close alone
// would leave it held past this holder.
impl Drop for Held {
  fn drop(&mut self) {
    flock(&self.0, libc::LOCK_UN);
  }
}

fn flock(file: &File, how: libc::c_int) -> bool {
  // SAFETY: the descriptor is the open file's own, and flock reads nothing else.
  unsafe { libc::flock(file.as_raw_fd(), how) == 0 }
}

fn take(file: &File, wait: bool) -> bool {
  flock(file, if wait { libc::LOCK_EX } else { libc::LOCK_EX | libc::LOCK_NB })
}

fn open(path: &Path) -> std::io::Result<File> {
  if let Some(dir) = path.parent() {
    std::fs::create_dir_all(dir)?;
  }

  OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)
}

/// The lock taken where nobody holds it, none where somebody does or the file could not be opened.
pub fn try_hold(path: &Path) -> Option<Held> {
  let file = open(path).ok()?;

  take(&file, false).then_some(Held(file))
}

/// The lock taken once whoever holds it lets go, waited for off the async threads.
pub async fn hold(path: PathBuf) -> std::io::Result<Held> {
  tokio::task::spawn_blocking(move || {
    let file = open(&path)?;

    if take(&file, true) { Ok(Held(file)) } else { Err(std::io::Error::last_os_error()) }
  })
  .await
  .map_err(std::io::Error::other)?
}

/// Whether somebody holds the lock. A file that is not there is held by nobody; one that cannot be
/// opened is not taken for free, since nobody could say. The look takes the lock shared, which a holder's
/// exclusive lock refuses and another look at the same moment does not, so two looks never read each other
/// as a holder. The look lets its shared lock go by name, as a holder does, so a forked copy of its descriptor
/// never refuses a holder after it.
pub fn held(path: &Path) -> bool {
  if !path.exists() {
    return false;
  }

  match OpenOptions::new().read(true).open(path) {
    Ok(file) => {
      let free = flock(&file, libc::LOCK_SH | libc::LOCK_NB);

      if free {
        flock(&file, libc::LOCK_UN);
      }

      !free
    }
    Err(_) => true,
  }
}
