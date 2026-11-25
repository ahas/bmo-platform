//! The key files the gateway reads, each under the data root's `env/`: a file only its owner may read
//! or write, holding something, or no key at all.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Whether a secret file may be used: a plain file at `0600` that is not empty.
pub fn usable_secret_file(path: &Path) -> bool {
  std::fs::metadata(path)
    .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o777 == 0o600 && meta.len() > 0)
}

/// A key file's key, trimmed; absent where the file may not be used.
pub fn read_key(env_dir: &Path, key_file: &str) -> Option<String> {
  let path = env_dir.join(key_file);
  if !usable_secret_file(&path) {
    return None;
  }
  std::fs::read_to_string(path).ok().map(|key| key.trim().to_owned())
}
