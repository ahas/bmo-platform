//! Git run for a test: a command under a fixed neutral author, its input closed, failing the test where git
//! fails and answering what it printed.

use std::path::Path;

/// What git printed for the arguments, trimmed; the test fails where git does.
pub fn git(args: &[&str]) -> String {
  let done = std::process::Command::new("git")
    .args(args)
    .env("GIT_AUTHOR_NAME", "tester")
    .env("GIT_AUTHOR_EMAIL", "tester@example.com")
    .env("GIT_COMMITTER_NAME", "tester")
    .env("GIT_COMMITTER_EMAIL", "tester@example.com")
    .stdin(std::process::Stdio::null())
    .output()
    .unwrap();

  assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
  String::from_utf8_lossy(&done.stdout).trim().to_owned()
}

/// A path as git's arguments take it.
pub fn at(path: &Path) -> String {
  path.to_string_lossy().into_owned()
}
