//! What the integration tests stand on: a store of the test's own, the fixed ids they name the owner and a
//! job's unit by, the writer of a stand-in program, and a systemd stand-in.

#![allow(dead_code)]

pub mod alone;
pub mod herdr;
pub mod queue;

use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub use crate::support::scratch::Scratch;

/// The owner's user id, which is also the chat id of the owner's private chat.
pub const OWNER: i64 = 42;
/// A job's unit name.
pub const UNIT: &str = "bmo-task-1";

/// Writes `script` at `path` as a program a test runs at once, its folder made where missing.
///
/// A test binary runs its tests on many threads, and a process one of them starts holds a copy of every
/// descriptor open at that moment until it execs; the kernel refuses to run a file still open for writing
/// in any process ("Text file busy"). So the file is written by `tee`, a child of its own that starts
/// nothing, and is marked runnable only once that child has ended.
pub fn executable(path: &Path, script: &str) {
  if let Some(folder) = path.parent() {
    std::fs::create_dir_all(folder).unwrap();
  }

  let mut tee = Command::new("tee")
    .arg(path)
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .spawn()
    .expect("tee starts");

  tee.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
  assert!(tee.wait().unwrap().success(), "tee wrote {}", path.display());
  std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A systemd stand-in written into a folder that is the whole of a machine's PATH: it answers
/// `systemctl --user is-active` for each unit asked, `active` for a unit named on a line of `alive`
/// beside it, or for every unit while a file `all` stands beside it, and `inactive` for any other; and
/// nothing at all while a file `silent` stands beside it, as a bus it could not reach answers. The
/// folder, to put on a machine's PATH.
pub fn systemd_stand_in(folder: &Path) -> PathBuf {
  executable(
    &folder.join("systemctl"),
    r#"#!/bin/sh
here=${0%/*}
[ -e "$here/silent" ] && exit 1
shift 2
for unit in "$@"; do
  name=${unit%.service}
  state=inactive
  [ -e "$here/all" ] && state=active
  if [ -e "$here/alive" ]; then
    while IFS= read -r one; do [ "$one" = "$name" ] && state=active; done < "$here/alive"
  fi
  echo "$state"
done
"#,
  );

  folder.to_path_buf()
}

/// Agency's service on the scratch store, its configuration read from the variables named alone.
pub fn service(scratch: &Scratch, set: &[(&str, String)]) -> agency_server::Service {
  let variables: std::collections::HashMap<String, String> =
    set.iter().map(|(name, value)| ((*name).to_owned(), value.clone())).collect();

  agency_server::Service::new(
    scratch.db().clone(),
    agency_server::config::Config::read(|name| variables.get(name).cloned()).unwrap(),
  )
}
