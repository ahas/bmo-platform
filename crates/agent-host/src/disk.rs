//! Files as the envelope protocol writes them: readable by the owner alone, and published whole by a
//! rename, so a reader never sees one half written.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Milliseconds since the epoch, as every time in the protocol is written.
pub fn now_ms() -> u64 {
  SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis() as u64)
}

/// A directory made, with its parents, readable by the owner alone.
pub fn private_dir(path: &Path) -> io::Result<()> {
  fs::DirBuilder::new().recursive(true).mode(0o700).create(path)
}

/// A directory made, with its parents, below `base`, which must still stand: a base taken away is
/// never made again.
pub fn private_dir_below(base: &Path, path: &Path) -> io::Result<()> {
  let below = path
    .strip_prefix(base)
    .map_err(|_| io::Error::other(format!("{} is not below {}", path.display(), base.display())))?;
  let mut at = base.to_owned();

  for part in below.components() {
    at.push(part);

    match fs::DirBuilder::new().mode(0o700).create(&at) {
      Err(why) if why.kind() != io::ErrorKind::AlreadyExists => return Err(why),
      _ => {}
    }
  }

  Ok(())
}

/// A file created or emptied, readable by the owner alone.
pub fn private_file(path: &Path) -> io::Result<fs::File> {
  OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)
}

/// A name no other write in flight stands under: the process and a count of its own.
pub fn staged_suffix() -> String {
  static WRITES: AtomicU64 = AtomicU64::new(0);

  format!("{}.{}", std::process::id(), WRITES.fetch_add(1, Ordering::Relaxed))
}

/// Bytes written beside `path`, under a name of this write's own so two writers of one path never take
/// each other's, and renamed onto it.
pub fn publish_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
  let mut staged = path.as_os_str().to_owned();

  staged.push(format!(".{}.new", staged_suffix()));

  let staged = Path::new(&staged);

  private_file(staged)?.write_all(bytes)?;
  fs::rename(staged, path)
}

/// A value published as JSON.
pub fn publish<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
  publish_bytes(path, &serde_json::to_vec(value)?)
}

/// A JSON file read, or none where it is missing or not that shape.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
  serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// A name that may stand as one path component: letters, digits, `-` and `_`.
pub fn plain_name(name: &str) -> bool {
  !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}
