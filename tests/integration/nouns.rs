//! The nouns `bmo` dispatches to `bmo-agency` (bmo-cli's `docs/api.md`, 「The nouns a unit declares」; agency's
//! `docs/api.md`, 「The command」): what `nouns` prints from any directory and with no settings, and each
//! noun's own help read by the command.

use std::process::{Command, Output, Stdio};

use crate::support::binary;

// The nouns the command declares, as `nouns` prints them, `home` marks included.
const NOUNS: [&str; 25] = [
  "agent\thome",
  "agents\thome",
  "agent status\thome",
  "agent-status\thome",
  "model",
  "models",
  "reload",
  "node work\thome",
  "work-node\thome",
  "node claude-token\thome",
  "node person\thome",
  "node pane",
  "node-pane",
  "dev-server",
  "dev-pane",
  "supervisor follow",
  "supervisor reset\thome",
  "claude batch",
  "live-claude",
  "job borrow\thome",
  "repository\thome",
  "run list\thome",
  "run say\thome",
  "run stop\thome",
  "machine memory",
];

// The binary run on `args` in a home and a working directory of the test's own, with nothing on its standard
// input and no variable of the machine's telling it where agency stands.
fn run(args: &[&str]) -> Output {
  let home = tempfile::tempdir().unwrap();
  let elsewhere = tempfile::tempdir().unwrap();

  Command::new(binary::path("bmo-agency"))
    .args(args)
    .current_dir(elsewhere.path())
    .env_clear()
    .env("HOME", home.path())
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .output()
    .unwrap()
}

#[test]
fn the_nouns_are_printed_one_a_line_with_their_marks() {
  let printed = run(&["nouns"]);

  assert!(printed.status.success(), "{}", String::from_utf8_lossy(&printed.stderr));
  assert_eq!(String::from_utf8_lossy(&printed.stdout), NOUNS.map(|noun| format!("{noun}\n")).join(""));
}

#[test]
fn every_noun_answers_its_own_help() {
  let mut unread = Vec::new();

  for noun in NOUNS {
    let words: Vec<&str> = noun.split('\t').next().unwrap().split(' ').collect();
    let mut args = words.clone();

    args.push("--help");

    let asked = run(&args);
    let said = String::from_utf8_lossy(&asked.stdout);

    if !asked.status.success() || !said.contains("Usage:") {
      unread.push(format!("{noun}: {}", String::from_utf8_lossy(&asked.stderr).trim()));
    }
  }

  assert!(unread.is_empty(), "nouns whose help does not answer: {unread:#?}");
}
