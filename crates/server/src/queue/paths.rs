//! Directories as a worker takes them: absolute, with every `.` and `..` worked out on the path's own
//! words, so a worker moved between machines is wrong loudly rather than quietly running somewhere
//! else.

use std::path::{Component, Path, PathBuf};

/// The path made absolute against the directory the process runs in, and normalised.
pub fn resolve(path: &str) -> PathBuf {
  let given = Path::new(path);
  let joined = if given.is_absolute() {
    given.to_path_buf()
  } else {
    std::env::current_dir().unwrap_or_default().join(given)
  };
  let mut resolved = PathBuf::from("/");

  for component in joined.components() {
    match component {
      Component::ParentDir => {
        resolved.pop();
      }
      Component::Normal(part) => resolved.push(part),
      Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
    }
  }

  resolved
}
