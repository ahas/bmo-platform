//! The records as the binary writes and rewrites them (docs/agent-control.md, 「A record holds no secret」):
//! `bmo-agency agent-status` reading a hook's call for the saved values under its data root and the secret
//! shapes before the hooks record keeps it, and `bmo-agency redact-records` rewriting a record written before.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use agency_server::door::place_run::read_hooks;
use serde_json::json;

use crate::support::binary;

const BEARER: &str = "abcdefghijklmnopqrstuvwxyz0123";
const API_KEY: &str = "key0123456789abcdef";
const SAVED: &str = "saved-0123456789-abcdef";
const FOLDED: &str = "folded-0123456789-abcdef";
const URL: &str = "https://example.com/docs/page?id=42&lang=en";

// The binary run on `args` under `home`, its data root `home/data` and `input` on its standard input; killed and
// failed where it has not ended within ten seconds.
fn run(home: &Path, args: &[&str], input: &str) -> Output {
  let mut command = Command::new(binary::path("bmo-agency"));

  command
    .args(args)
    .current_dir(home)
    .env("HOME", home)
    .env("BMO_DATA_DIR", home.join("data"))
    .env_remove("CLAUDECODE")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

  let mut child = command.spawn().unwrap();

  child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();

  let until = Instant::now() + Duration::from_secs(10);

  while child.try_wait().unwrap().is_none() {
    if Instant::now() >= until {
      let _ = child.kill();
      let _ = child.wait();
      panic!("bmo-agency {} did not end", args.join(" "));
    }

    std::thread::sleep(Duration::from_millis(20));
  }

  child.wait_with_output().unwrap()
}

// A home whose data root holds two saved secrets, one in a folder, and the place of one agent under it.
fn home() -> (tempfile::TempDir, std::path::PathBuf) {
  let home = tempfile::tempdir().unwrap();
  let env = home.path().join("data").join("env");
  let place = home.path().join("data").join("agent-places").join("worker");

  std::fs::create_dir_all(&env).unwrap();
  std::fs::write(env.join("service.env"), format!("SERVICE_URL={URL}\nSERVICE_TOKEN={SAVED}\n")).unwrap();
  std::fs::create_dir_all(env.join("hooks")).unwrap();
  std::fs::write(env.join("hooks").join("point.json"), json!({ "token": FOLDED }).to_string()).unwrap();
  std::fs::create_dir_all(&place).unwrap();

  (home, place)
}

#[test]
fn a_secret_in_a_tool_argument_is_redacted_in_the_hooks_record_and_an_ordinary_url_stays_whole() {
  let (home, place) = home();
  let session = "5f27643e-ae81-4401-83a3-bc4d195ba48b";
  let command = format!("curl -H 'Authorization: Bearer {BEARER}' -H 'x-api-key: {API_KEY}' '{URL}'");
  let payload = json!({
    "session_id": session,
    "transcript_path": home.path().join(format!("{session}.jsonl")),
    "hook_event_name": "Stop",
    "background_tasks": [{ "id": "b7x2k9", "type": "local_bash", "status": "running", "description": "Fetch", "command": command }],
    "last_assistant_message": format!("The saved ones are {SAVED} and {FOLDED}."),
  });
  let place_arg = place.to_string_lossy().into_owned();
  let output = run(home.path(), &["agent-status", "stop", "--place", &place_arg], &payload.to_string());

  assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

  let text = std::fs::read_to_string(place.join("hooks.jsonl")).unwrap();

  for secret in [BEARER, API_KEY, SAVED, FOLDED] {
    assert!(!text.contains(secret), "{text}");
  }

  let hooks = read_hooks(&place, 0);
  let payload = &hooks[0].payload;

  assert_eq!(
    payload["background_tasks"][0]["command"],
    format!("curl -H 'Authorization: Bearer [redacted]' -H 'x-api-key: [redacted]' '{URL}'")
  );
  assert_eq!(payload["last_assistant_message"], "The saved ones are [redacted] and [redacted].");
  assert_eq!(payload["session_id"], session);
  assert_eq!(payload["background_tasks"][0]["id"], "b7x2k9");
}

#[test]
fn redact_records_rewrites_a_record_written_before_and_says_only_how_many_by_source() {
  let (home, place) = home();
  let line = json!({ "at": 1, "kind": "submit", "payload": { "prompt": format!("use api_key={API_KEY} or {FOLDED} at {URL}") } });

  std::fs::write(place.join("hooks.jsonl"), format!("{line}\n{}\n", json!({ "at": 2, "kind": "stop" })))
    .unwrap();
  let output = run(home.path(), &["redact-records"], "");
  let said = String::from_utf8_lossy(&output.stdout);

  assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
  assert!(
    said.contains("1 record(s) of 1 place(s) read; 1 rewritten, 1 line(s) changed"),
    "{said}"
  );
  assert!(said.contains("  saved value in hooks/point.json: 1"), "{said}");
  assert!(said.contains("  secret shape named-value: 1"), "{said}");
  assert!(!said.contains(API_KEY) && !said.contains(FOLDED), "{said}");

  let text = std::fs::read_to_string(place.join("hooks.jsonl")).unwrap();

  assert!(!text.contains(API_KEY), "{text}");
  assert!(text.contains(&format!("use api_key=[redacted] or [redacted] at {URL}")), "{text}");
  assert_eq!(read_hooks(&place, 0).len(), 2);
}
