//! The TypeScript test files that start a workspace binary, run under Bun from here.
//!
//! No test runs Cargo, and Bun builds no Rust, so a TypeScript test that starts a workspace binary is named
//! `*.cargo-test.ts`, outside Bun's own naming, and this harness runs it: Cargo built the root package's
//! copies before the harness began, and each file is handed the directory they stand in under their own
//! names in `AGENCY_TEST_BINARIES` (`tests/packages/support/binaries.ts`). A file runs as the suite runs a
//! Bun file, from the folder of its `bunfig.toml` with Bun's own per-test limit lifted.

use std::fs::{self, File};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::support::binary;

// The longest one file runs before it fails as hung, well past the slowest file's run on a loaded machine.
const BOUND: Duration = Duration::from_secs(900);

// How much of a failed file's output its failure carries.
const TAIL_LINES: usize = 120;

fn packages() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("packages")
}

// Every `*.cargo-test.ts` under `dir`, by its path from it.
fn cargo_test_files(dir: &Path, from: &Path, found: &mut Vec<PathBuf>) {
  let entries =
    fs::read_dir(dir).unwrap_or_else(|error| panic!("{} cannot be read: {error}", dir.display()));

  for entry in entries {
    let path = entry.unwrap().path();
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();

    if path.is_dir() && name != "node_modules" {
      cargo_test_files(&path, from, found);
    } else if name.ends_with(".cargo-test.ts") {
      found.push(path.strip_prefix(from).unwrap().to_owned());
    }
  }
}

// Bun's process, which leads a group of its own, taken down with everything in the group whatever happens.
struct Group(Child);

impl Drop for Group {
  fn drop(&mut self) {
    // SAFETY: a signal to a process group this test made; nothing is read or written through a pointer.
    unsafe {
      libc::kill(-(self.0.id() as i32), libc::SIGKILL);
    }
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

// One file run to its end, or failed once it outruns the bound; what it said, whichever way it went.
fn run(file: &Path) -> Result<(), String> {
  let out = tempfile::tempdir().unwrap();
  let said = out.path().join("bun.log");
  let log = File::create(&said).unwrap();
  let started = Command::new("bun")
    .arg("test")
    .arg("--timeout")
    .arg("0")
    .arg(Path::new(".").join(file))
    .current_dir(packages())
    .env("AGENCY_TEST_BINARIES", binary::placed_dir())
    .stdin(Stdio::null())
    .stdout(log.try_clone().unwrap())
    .stderr(log)
    .process_group(0)
    .spawn()
    .unwrap_or_else(|error| panic!("bun could not start: {error}"));
  let mut bun = Group(started);
  let deadline = Instant::now() + BOUND;
  let status: Option<ExitStatus> = loop {
    if let Some(status) = bun.0.try_wait().unwrap() {
      break Some(status);
    }
    if Instant::now() >= deadline {
      break None;
    }
    thread::sleep(Duration::from_millis(100));
  };

  drop(bun);

  let text = fs::read_to_string(&said).unwrap_or_default();
  let lines: Vec<&str> = text.lines().collect();
  let tail = lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n");

  match status {
    Some(status) if status.success() => Ok(()),
    Some(status) => Err(format!("{} failed ({status}):\n{tail}", file.display())),
    None => Err(format!("{} ran past {} s:\n{tail}", file.display(), BOUND.as_secs())),
  }
}

#[test]
fn every_typescript_file_that_starts_a_built_binary_passes() {
  let mut files = Vec::new();

  cargo_test_files(&packages(), &packages(), &mut files);
  files.sort();
  assert!(!files.is_empty(), "no *.cargo-test.ts stands under tests/packages");

  let failures: Vec<String> = thread::scope(|scope| {
    let runs: Vec<_> = files.iter().map(|file| scope.spawn(move || run(file))).collect();

    runs.into_iter().filter_map(|run| run.join().unwrap().err()).collect()
  });

  assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
