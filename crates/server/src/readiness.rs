//! The server's readiness (docs/api.md, 「Health」): the process answering, the code tree it reads its code
//! from, and whether its places have stood once since it started. A deployment that started agency on a
//! pinned release waits for an answer naming its own process and the tree it pinned, and ready.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

/// Whether the places have stood once, kept for as long as the process stands.
#[derive(Debug, Default)]
pub struct Readiness {
  stood: AtomicBool,
}

/// What the health route answers of readiness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ready {
  pub pid: u32,
  pub code: PathBuf,
  pub ready: bool,
}

impl Readiness {
  /// The places stood once: every answer after this one says ready.
  pub fn stood(&self) {
    self.stood.store(true, Ordering::Release);
  }

  /// The answer of this process reading its code from the tree given.
  pub fn answer(&self, code: &Path) -> Ready {
    Ready {
      pid: std::process::id(),
      code: code.to_path_buf(),
      ready: self.stood.load(Ordering::Acquire),
    }
  }
}
