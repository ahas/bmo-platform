//! Files as the envelope protocol writes them.

use std::fs;

use agent_host::disk::private_dir_below;

#[test]
fn a_directory_is_made_below_a_standing_base_and_never_makes_a_base_taken_away() {
  let scratch = tempfile::tempdir().unwrap();
  let base = scratch.path().join("base");

  fs::create_dir(&base).unwrap();
  private_dir_below(&base, &base.join("a/b")).unwrap();
  private_dir_below(&base, &base.join("a/b")).unwrap();
  assert!(base.join("a/b").is_dir());

  fs::remove_dir_all(&base).unwrap();
  assert!(private_dir_below(&base, &base.join("a")).is_err());
  assert!(!base.exists());
}
