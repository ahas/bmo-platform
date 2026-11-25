//! The providers this build carries, and a program found only where it may be run.

use harness_providers::*;

#[test]
fn every_provider_this_build_carries_is_one_the_archive_stores() {
  let names: Vec<&str> = providers().iter().map(|one| one.name()).collect();

  assert_eq!(names, ["claude-code", "registered", "codex", "openrouter", "deepseek"]);
}

#[test]
fn a_program_is_found_only_where_it_may_be_run() {
  use std::os::unix::fs::PermissionsExt;

  let dir = tempfile::tempdir().unwrap();
  let plain = dir.path().join("plain");

  std::fs::write(&plain, "").unwrap();
  std::fs::write(dir.path().join("runs"), "").unwrap();
  std::fs::set_permissions(dir.path().join("runs"), std::fs::Permissions::from_mode(0o755)).unwrap();

  let machine = Machine::with_env([("PATH".to_owned(), dir.path().display().to_string())], dir.path());

  assert_eq!(machine.program("runs"), Some(dir.path().join("runs")));
  assert_eq!(machine.program("plain"), None);
  assert_eq!(Machine::with_env([], dir.path()).program("runs"), None);
}
