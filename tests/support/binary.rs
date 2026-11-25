//! A workspace binary as a test starts it: the root package's copy of it, which Cargo builds before it runs
//! the tests, placed under the binary's own name beside every other one.
//!
//! The root package compiles each binary a test starts from that crate's own entry point under a name of its
//! own (the root manifest, `test-binaries`), and Cargo names each one's path to the tests at compile time. A
//! binary that finds another beside itself, as the server finds the verbs, or that is told apart by the name
//! it was started under, needs both under their own names in one directory, so the first ask of a test
//! binary links every one into a directory of its own beside them, under the name the crate gives it. A
//! TypeScript test that starts one is handed that directory (`typescript_binaries`).

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Every binary a test starts: its own name and the root package's copy of it.
const BINARIES: [(&str, &str); 8] = [
  ("agency-server", env!("CARGO_BIN_EXE_test-agency-server")),
  ("bmo-agency", env!("CARGO_BIN_EXE_test-bmo-agency")),
  ("agent-host", env!("CARGO_BIN_EXE_test-agent-host")),
  ("pane-head", env!("CARGO_BIN_EXE_test-pane-head")),
  ("node-link", env!("CARGO_BIN_EXE_test-node-link")),
  ("llm-gateway", env!("CARGO_BIN_EXE_test-llm-gateway")),
  ("harness-providers", env!("CARGO_BIN_EXE_test-harness-providers")),
  ("procedures-tool", env!("CARGO_BIN_EXE_test-procedures-tool")),
];

// The directory the binaries stand in under their own names. It is not named `bin`, since a binary standing
// in a `bin` directory takes the directory above it for the tree it was deployed in.
const PLACED: &str = "checked-binaries";

static PLACED_DIR: OnceLock<PathBuf> = OnceLock::new();

// One binary linked under `name` into `dir`, replacing whatever an earlier test binary linked there: a new
// link beside it renamed over it, so a test binary of another run reading the old one meanwhile still finds
// a whole file.
fn place(dir: &Path, name: &str, built: &Path) {
  let placed = dir.join(name);
  let fresh = dir.join(format!(".{name}.{}", std::process::id()));
  let _ = fs::remove_file(&fresh);

  if fs::hard_link(built, &fresh).is_err() {
    fs::copy(built, &fresh).unwrap_or_else(|error| panic!("{name} could not be placed: {error}"));
  }

  fs::rename(&fresh, &placed).unwrap_or_else(|error| panic!("{name} could not be placed: {error}"));

  // Renaming one link of a file over another link of the same file does nothing and leaves both, so where the
  // binary placed is still the one built the fresh link stands on and is taken away here.
  let _ = fs::remove_file(&fresh);
}

// The fresh links a test binary that is gone left in `dir`: each is named for the process that made it, and
// one whose process no longer stands is no run's any more, so it goes. Left standing, each keeps a whole
// binary of an older build on the disk.
fn sweep(dir: &Path) {
  let Ok(entries) = fs::read_dir(dir) else { return };

  for entry in entries.flatten() {
    let name = entry.file_name().to_string_lossy().into_owned();
    let made_by = name
      .strip_prefix('.')
      .and_then(|rest| rest.rsplit_once('.'))
      .map(|(_, pid)| pid.to_owned());

    if made_by.is_some_and(|pid| pid.parse::<u32>().is_ok() && !Path::new("/proc").join(&pid).exists()) {
      let _ = fs::remove_file(entry.path());
    }
  }
}

/// The directory every workspace binary stands in under its own name.
pub fn placed_dir() -> &'static Path {
  PLACED_DIR.get_or_init(|| {
    let (_, first) = BINARIES[0];
    let dir = Path::new(first).parent().expect("a built binary stands in a directory").join(PLACED);

    fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("{} could not be made: {error}", dir.display()));

    sweep(&dir);

    for (name, built) in BINARIES {
      place(&dir, name, Path::new(built));
    }

    dir
  })
}

/// The path of the workspace binary named `binary`, which Cargo built before the tests began.
pub fn path(binary: &str) -> PathBuf {
  assert!(
    BINARIES.iter().any(|(name, _)| *name == binary),
    "{binary} is no binary the root package builds for its tests"
  );

  placed_dir().join(binary)
}
