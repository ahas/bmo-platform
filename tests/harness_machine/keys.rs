//! The registered models' key files: written private, refused when not one line, and taken away only
//! where agency wrote them.

use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;

use harness_machine::keys::*;
use harness_machine::{Fault, Refusal};

#[test]
fn a_key_is_written_private_under_its_address_name_and_counted_usable() {
  let root = tempfile::tempdir().unwrap();
  let file = write(root.path(), "local", "a", "  sk-local-0123 \n").unwrap();
  let written = path(root.path(), &file);

  assert_eq!(file, "registered/local-a.key");
  assert_eq!(std::fs::read_to_string(&written).unwrap(), "sk-local-0123\n");
  assert_eq!(std::fs::metadata(&written).unwrap().permissions().mode() & 0o777, 0o600);
  assert_eq!(
    std::fs::metadata(written.parent().unwrap()).unwrap().permissions().mode() & 0o777,
    0o700
  );
  assert_eq!(state(root.path(), Some(&file)), KeyState::Usable);
  assert_eq!(read(root.path(), &file).as_deref(), Some("sk-local-0123"));

  write(root.path(), "local", "a", "sk-second").unwrap();
  assert_eq!(std::fs::read_to_string(&written).unwrap(), "sk-second\n");
  assert_eq!(std::fs::metadata(&written).unwrap().permissions().mode() & 0o777, 0o600);
}

#[test]
fn a_key_that_is_not_one_line_is_refused_and_nothing_is_written() {
  let root = tempfile::tempdir().unwrap();

  for key in ["", "   ", "two words", &"k".repeat(KEY_MOST + 1)] {
    match write(root.path(), "local", "a", key) {
      Err(Fault::Refused(refusal)) => {
        assert_eq!(refusal, Refusal::new("key", "A key must be one line with no whitespace."))
      }
      other => panic!("{key:?} was not refused: {other:?}"),
    }
  }

  assert!(!root.path().join("env").exists());
}

#[test]
fn a_key_file_is_usable_only_while_private_and_holding_a_key() {
  let root = tempfile::tempdir().unwrap();
  let file = path(root.path(), "node-b.key");

  assert_eq!(state(root.path(), None), KeyState::None);
  assert_eq!(state(root.path(), Some("node-b.key")), KeyState::Unusable);

  std::fs::create_dir_all(file.parent().unwrap()).unwrap();
  std::fs::write(&file, "token\n").unwrap();
  std::fs::set_permissions(&file, Permissions::from_mode(0o644)).unwrap();
  assert_eq!(state(root.path(), Some("node-b.key")), KeyState::Unusable);

  std::fs::set_permissions(&file, Permissions::from_mode(0o600)).unwrap();
  assert_eq!(state(root.path(), Some("node-b.key")), KeyState::Usable);
}

#[test]
fn only_a_key_file_agency_wrote_is_taken_away() {
  let root = tempfile::tempdir().unwrap();
  let file = write(root.path(), "local", "a", "sk-1").unwrap();

  std::fs::write(path(root.path(), "node-b.key"), "token").unwrap();
  remove(root.path(), Some("node-b.key"), false);
  remove(root.path(), Some(&file), true);

  assert!(path(root.path(), "node-b.key").exists());
  assert!(!path(root.path(), &file).exists());
}
