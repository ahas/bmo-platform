//! The registered models' key files (docs/harness-machine.md, 「The keys」): written by agency under the
//! data root's `env/registered/` at 0600, taken away only where agency wrote them, and counted usable
//! only while private and not empty. Nothing here answers a key but to the engine health that sends
//! it.

use std::fs::{DirBuilder, OpenOptions, Permissions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{Fault, Refusal};

// The folder under `env/` the keys agency is given are written in.
const FOLDER: &str = "registered";

/// The longest key worth taking: a bearer key is a line, and a pasted document is not one.
pub const KEY_MOST: usize = 4_096;

/// What a screen is shown of an address's key: usable, none named, or named but not usable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyState {
  Usable,
  None,
  Unusable,
}

/// Where a key file stands, by the name an address row keeps.
pub fn path(data_root: &Path, key_file: &str) -> PathBuf {
  data_root.join("env").join(key_file)
}

/// Whether a secret file may be used: a file of its own, at exactly 0600, not empty. A file with
/// looser permissions counts as absent.
pub fn usable(path: &Path) -> bool {
  std::fs::metadata(path)
    .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o777 == 0o600 && meta.len() > 0)
}

/// The state of the key an address names, or of none.
pub fn state(data_root: &Path, key_file: Option<&str>) -> KeyState {
  match key_file {
    None => KeyState::None,
    Some(file) if usable(&path(data_root, file)) => KeyState::Usable,
    Some(_) => KeyState::Unusable,
  }
}

/// The key a file holds, trimmed, or none where it cannot be read.
pub fn read(data_root: &Path, key_file: &str) -> Option<String> {
  std::fs::read_to_string(path(data_root, key_file)).ok().map(|key| key.trim().to_owned())
}

/// The key given for one address, written into agency's own file for it, replacing what stood there.
/// Answers the file's name as the address row keeps it, never the key.
pub fn write(data_root: &Path, registration: &str, node: &str, key: &str) -> Result<String, Fault> {
  let trimmed = key.trim();

  if trimmed.is_empty() || crate::length(trimmed) > KEY_MOST || trimmed.chars().any(char::is_whitespace) {
    return Err(Fault::Refused(Refusal::new("key", "A key must be one line with no whitespace.")));
  }

  let key_file = format!("{FOLDER}/{registration}-{node}.key");
  let place = path(data_root, &key_file);
  let staged = PathBuf::from(format!("{}.{}.new", place.display(), std::process::id()));

  if let Some(folder) = place.parent() {
    DirBuilder::new().recursive(true).mode(0o700).create(folder)?;
  }

  let mut file = OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&staged)?;

  file.write_all(format!("{trimmed}\n").as_bytes())?;
  drop(file);
  std::fs::set_permissions(&staged, Permissions::from_mode(0o600))?;
  std::fs::rename(&staged, &place)?;

  Ok(key_file)
}

/// The key file of an address that is going, taken away where agency wrote it; a file agency did not write
/// is left where it is.
pub fn remove(data_root: &Path, key_file: Option<&str>, owned: bool) {
  if let (Some(file), true) = (key_file, owned) {
    let _ = std::fs::remove_file(path(data_root, file));
  }
}
