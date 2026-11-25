//! The Codex reader: the app-server's answer and a rollout's token count read into windows as the parity
//! cases record them, a rollout written within the last minute read without starting the program, the
//! app-server asked otherwise, and a failed ask carrying what the program said and leaving the slot as it
//! was.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cache_siren::codex::{CodexReader, SLOT, windows_of_answer, windows_of_rollout};
use cache_siren::{Keeping, Reader, Window};
use serde_json::{Value, json};

use crate::support::scratch::Scratch;

// The moment the rollout's token count was written, and every other moment from it, in epoch
// milliseconds; nothing reads the real clock.
const WRITTEN_MS: i64 = 1_791_349_200_000;
const WRITTEN: &str = "2026-10-07T05:00:00Z";

fn windows(made: &[Window]) -> Value {
  serde_json::to_value(made).unwrap()
}

#[test]
fn an_app_server_answer_is_read_into_the_windows_the_parity_cases_record() {
  let cases = [
    (
      json!({
        "rateLimitsByLimitId": {
          "codex": {
            "limitId": "codex",
            "limitName": "Codex",
            "primary": { "usedPercent": 41, "windowDurationMins": 300, "resetsAt": 1791400000.7 },
            "secondary": { "usedPercent": 3, "windowDurationMins": 0 },
          },
          "other": "x",
        },
        "rateLimits": { "primary": { "usedPercent": 99 } },
      }),
      Ok(json!([
        { "name": "codex:codex:primary", "utilization": 0.41, "resetsAt": 1791400000, "durationMinutes": 300, "limitName": "Codex" },
        { "name": "codex:codex:secondary", "utilization": 0.03, "resetsAt": null, "durationMinutes": null, "limitName": "Codex" },
      ])),
    ),
    (
      json!({ "rateLimits": { "limitId": " ", "primary": { "usedPercent": 10, "resetsAt": null }, "secondary": { "usedPercent": -1 } } }),
      Ok(json!([
        { "name": "codex:default:primary", "utilization": 0.1, "resetsAt": null, "durationMinutes": null, "limitName": null },
      ])),
    ),
    (
      json!({ "rateLimitsByLimitId": {}, "rateLimits": null }),
      Err("codex rate limits: app-server returned no usable windows".to_owned()),
    ),
    (Value::Null, Err("codex rate limits: app-server returned no result".to_owned())),
  ];

  for (answer, expected) in cases {
    assert_eq!(windows_of_answer(&answer).map(|made| windows(&made)), expected, "{answer}");
  }
}

#[test]
fn a_rollouts_token_count_is_read_into_the_windows_the_parity_cases_record() {
  let cases = [
    (
      json!({
        "limit_id": "codex",
        "limit_name": "",
        "primary": { "used_percent": 22.5, "window_minutes": 300, "resets_at": 1791400000 },
        "secondary": { "used_percent": 4, "window_minutes": 10080 },
      }),
      json!([
        { "name": "codex:codex:primary", "utilization": 0.225, "resetsAt": 1791400000, "durationMinutes": 300, "limitName": null },
        { "name": "codex:codex:secondary", "utilization": 0.04, "resetsAt": null, "durationMinutes": 10080, "limitName": null },
      ]),
    ),
    (json!({ "primary": { "used_percent": "5" }, "secondary": null }), json!([])),
    (json!("none"), json!([])),
  ];

  for (limits, expected) in cases {
    assert_eq!(windows(&windows_of_rollout(&limits)), expected, "{limits}");
  }
}

// A rollout under the sessions root whose last token count carries one window, written at `WRITTEN`.
fn rollout(root: &Path) {
  let day = root.join("2026").join("10").join("07");
  let line = json!({
    "timestamp": WRITTEN,
    "type": "event_msg",
    "payload": {
      "type": "token_count",
      "rate_limits": { "limit_id": "codex", "primary": { "used_percent": 12, "resets_at": 1791400000 } },
    },
  });

  std::fs::create_dir_all(&day).unwrap();
  std::fs::write(day.join("rollout-thread.jsonl"), format!("{{\"type\":\"session_meta\"}}\n{line}\n"))
    .unwrap();
}

// A stand-in program, written as a shell script that the reader starts in the app-server's place.
fn program(dir: &Path, name: &str, script: &str) -> PathBuf {
  let path = dir.join(name);

  std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
  std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
  path
}

const ANSWERS: &str = r#"while IFS= read -r line; do
  case "$line" in
    *rateLimits/read*) printf '%s\n' '{"id":2,"result":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":40}}}}' ;;
  esac
done"#;

#[test]
fn a_reading_stands_six_one_minute_passes() {
  let reader = CodexReader::new(PathBuf::from("/nowhere"), PathBuf::from("/nowhere"));

  assert_eq!(reader.cadence(), Duration::from_secs(60));
  assert_eq!(reader.fresh(), Duration::from_secs(360));
}

#[tokio::test]
async fn a_rollout_written_within_the_minute_is_read_and_the_app_server_is_asked_otherwise() {
  let dir = tempfile::tempdir().unwrap();
  let sessions = dir.path().join("sessions");

  rollout(&sessions);

  let reader = CodexReader::new(sessions, program(dir.path(), "codex", ANSWERS));
  let recent = reader.current(WRITTEN_MS + 60_000).await.unwrap();

  assert_eq!(recent[0].utilization, 0.12, "the rollout's reading, with no process started");
  assert_eq!(
    reader.current(WRITTEN_MS + 60_001).await.unwrap()[0].utilization,
    0.4,
    "the app-server's"
  );
}

#[tokio::test]
async fn a_pass_keeps_the_app_servers_windows_and_a_failed_one_carries_what_the_program_said() {
  let scratch = Scratch::new().await;
  let dir = tempfile::tempdir().unwrap();
  let answering =
    Arc::new(CodexReader::new(dir.path().join("none"), program(dir.path(), "codex", ANSWERS)));
  let keeping = Keeping::new(scratch.db().clone(), vec![answering.clone()]);

  answering.pass(&keeping, WRITTEN_MS).await;

  let read = keeping.read(SLOT).await.unwrap();

  assert_eq!(read.windows.len(), 1);
  assert_eq!(read.windows[0].window.name, "codex:codex:primary");
  assert_eq!(read.windows[0].seen_at, WRITTEN_MS / 1000);
  assert_eq!(read.fresh, Duration::from_secs(360));

  let failing = CodexReader::new(
    dir.path().join("none"),
    program(dir.path(), "failing", "echo 'not logged in' >&2\nexit 1"),
  );

  assert_eq!(
    failing.current(WRITTEN_MS).await.unwrap_err(),
    "app-server closed before returning rate limits: not logged in"
  );

  failing.pass(&keeping, WRITTEN_MS + 60_000).await;

  assert_eq!(keeping.read(SLOT).await.unwrap(), read, "a failed pass leaves the slot as it was");
}
