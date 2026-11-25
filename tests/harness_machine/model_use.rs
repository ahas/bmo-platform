//! A model's use on its engine: the unit's line, the installer's log and the log's tail.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use harness_machine::model_use::*;

#[test]
fn the_unit_runs_the_catalogue_verb_of_the_binary_on_the_choice_and_its_archive() {
  let start = Start {
    engine: "test-llm".into(),
    use_id: 7,
    archive: "/data/archive.db".into(),
    path: "/usr/bin".into(),
    source_root: "/src".into(),
    code_root: "/deploy".into(),
  };

  assert_eq!(
    start.line()[4..],
    [
      "--unit=bmo-model-use-test-llm",
      "--setenv=PATH=/usr/bin",
      "--setenv=BMO_SOURCE_DIR=/src",
      "--working-directory=/deploy",
      "/deploy/bin/bmo-agency",
      "model",
      "run-use",
      "7",
      "--db",
      "/data/archive.db",
    ]
  );
}

#[test]
fn a_character_split_between_two_reads_waits_for_its_end() {
  let text = "€€€".as_bytes();

  assert_eq!(decodable(&text[..4]), 3);
  assert_eq!(decodable(text), text.len());
  assert_eq!(decodable(&[0xff, b'a']), 2);
}

#[test]
fn the_log_tail_is_its_last_twenty_lines_and_none_without_a_log() {
  let dir = tempfile::tempdir().unwrap();
  let log = dir.path().join("use.log");

  std::fs::write(&log, (1..=25).map(|n| format!("line {n}\n")).collect::<String>()).unwrap();

  let tail = log_tail(Some(&log));

  assert_eq!((tail.len(), tail[0].as_str(), tail[19].as_str()), (20, "line 6", "line 25"));
  assert!(log_tail(Some(&dir.path().join("gone.log"))).is_empty());
  assert!(log_tail(None).is_empty());
}

// A stand-in installer: a script the stand-in bun runs, printing its arguments and the archive.
fn source_with_installer(dir: &Path, body: &str) -> (PathBuf, PathBuf) {
  let source = dir.join("source");
  let bun = dir.join("bun");

  std::fs::create_dir_all(source.join("gate/training/tooling/scripts")).unwrap();
  std::fs::write(source.join("gate/training/tooling/scripts/test-llm-install.ts"), "").unwrap();
  std::fs::write(&bun, format!("#!/bin/sh\n{body}\n")).unwrap();
  std::fs::set_permissions(&bun, std::fs::Permissions::from_mode(0o755)).unwrap();

  (source, bun)
}

#[tokio::test]
async fn the_installer_writes_its_output_into_the_log_and_answers_its_last_line() {
  let dir = tempfile::tempdir().unwrap();
  let (source, bun) = source_with_installer(
    dir.path(),
    "echo \"$1 $2 $3 at $BMO_DB_PATH\"; echo 'setup failed' >&2; echo '  '; exit 3",
  );
  let install = Install {
    engine: "test-llm".into(),
    entry: "rvn".into(),
    log: dir.path().join("use.log"),
    archive: "/data/archive.db".into(),
    source_root: source,
    bun,
  };
  let done = install.run().await.unwrap();
  let log = std::fs::read_to_string(dir.path().join("use.log")).unwrap();

  assert!(!done.ok);
  assert!(log.contains("scripts/test-llm-install.ts --model rvn at /data/archive.db"));
  assert!(log.contains("setup failed"));
  assert!(
    ["setup failed", "scripts/test-llm-install.ts --model rvn at /data/archive.db"]
      .contains(&done.last.as_str())
  );
}

#[tokio::test]
async fn an_engine_with_no_installer_fails_naming_it() {
  let dir = tempfile::tempdir().unwrap();
  let install = Install {
    engine: "no-such-engine".into(),
    entry: "rvn".into(),
    log: dir.path().join("use.log"),
    archive: "a".into(),
    source_root: dir.path().into(),
    bun: "bun".into(),
  };

  assert_eq!(
    install.run().await.unwrap(),
    Installed {
      ok: false,
      last: "The no-such-engine engine has no installer at gate/training/tooling/scripts/no-such-engine-install.ts.".into(),
    }
  );
}
