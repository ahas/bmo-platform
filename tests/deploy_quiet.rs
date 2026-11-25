//! The declared pre-flip quiet, `deploy/quiet`, run from a release whose `bin/bmo-agency` is a stand-in
//! (docs/sessions.md, 「A restart hands the session over」): the release's own `bmo-agency quiet` is asked, and
//! its code and what it printed are the quiet's, whatever the code. What the verb answers is its own test's.

use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// How long the script, which runs one stand-in that ends at once, is given before it fails the test.
const BOUND: Duration = Duration::from_secs(10);

// The script run from a release whose verb prints `lines` and ends with `code`, as long as it was asked for
// `quiet`: the script's code and what it printed.
fn quiet_over(lines: &[&str], code: i32) -> (i32, String) {
  let release = tempfile::tempdir().unwrap();
  let bin = release.path().join("bin");
  std::fs::create_dir_all(&bin).unwrap();
  let verb = bin.join("bmo-agency");
  let printed: String = lines.iter().map(|line| format!("echo '{line}'\n")).collect();
  std::fs::write(
    &verb,
    format!("#!/bin/sh\n[ \"$1\" = quiet ] || {{ echo \"asked $*\"; exit 9; }}\n{printed}exit {code}\n"),
  )
  .unwrap();
  std::fs::set_permissions(&verb, std::fs::Permissions::from_mode(0o755)).unwrap();

  run(release.path())
}

fn run(release: &Path) -> (i32, String) {
  let script = concat!(env!("CARGO_MANIFEST_DIR"), "/deploy/quiet");
  let mut child = Command::new("sh")
    .arg(script)
    .current_dir(release)
    .env_clear()
    .env("PATH", std::env::var("PATH").unwrap_or_default())
    .env("HOME", release)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .spawn()
    .unwrap();
  let deadline = Instant::now() + BOUND;

  while child.try_wait().unwrap().is_none() {
    if Instant::now() > deadline {
      child.kill().unwrap();
      child.wait().unwrap();
      panic!("the quiet did not end");
    }
    std::thread::sleep(Duration::from_millis(5));
  }

  let done = child.wait_with_output().unwrap();

  (done.status.code().unwrap_or(-1), String::from_utf8_lossy(&done.stdout).into_owned())
}

#[test]
fn the_quiet_is_the_release_s_own_verb_its_code_and_its_lines_handed_on() {
  assert_eq!(quiet_over(&[], 0), (0, String::new()));
  assert_eq!(
    quiet_over(&["agency carries turn chat:-5:1 of bmo"], 1),
    (1, "agency carries turn chat:-5:1 of bmo\n".to_owned())
  );
  assert_eq!(quiet_over(&["the quiet route refused"], 2), (2, "the quiet route refused\n".to_owned()));
}
