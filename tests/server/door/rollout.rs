use std::collections::{BTreeMap, HashSet};

use agency_server::door::record::RequestUsage;
use agency_server::door::rollout::*;
use agency_server::door::surface::Run;
use agency_server::now_ms;
use agent_host::place::ResidentCommand;
use serde_json::{Value, json};

fn run(resume: Option<&str>) -> Run {
  serde_json::from_value(json!({
    "line": "chat", "model": "gpt-5.6-luna", "effort": "high", "prompt": "hello", "cwd": "/w",
    "resumeSessionId": resume,
  }))
  .unwrap()
}

// The lines one turn writes to its thread's rollout, in the shape a recorded rollout carries them.
fn turn_lines(text: &str) -> Vec<Value> {
  vec![
    json!({ "type": "turn_context", "payload": { "model": "gpt-5.6-luna" } }),
    json!({ "type": "event_msg", "payload": { "type": "task_started" } }),
    json!({ "type": "event_msg", "payload": { "type": "agent_message", "message": text } }),
    json!({ "type": "event_msg", "payload": { "type": "token_count", "info": null } }),
    json!({
      "type": "event_msg",
      "payload": {
        "type": "token_count",
        "info": { "last_token_usage": { "input_tokens": 1000, "cached_input_tokens": 900, "cache_write_input_tokens": 0, "output_tokens": 12 } },
        "rate_limits": {
          "limit_id": "codex",
          "primary": { "used_percent": 12, "window_minutes": 300, "resets_at": 1790000000 },
          "secondary": { "used_percent": 3, "window_minutes": 10080, "resets_at": 1790500000 },
        },
      },
    }),
    json!({ "type": "event_msg", "payload": { "type": "task_complete", "last_agent_message": text } }),
  ]
}

#[test]
fn a_completed_task_is_its_last_message_with_its_requests_and_the_newest_windows() {
  let mut lines = vec![json!({ "type": "session_meta", "payload": { "id": "thread-1", "cwd": "/w" } })];

  lines.extend(turn_lines(" first answer "));

  let read = read_rollout(&lines, &run(None), "codex");

  assert_eq!(read.state, State::Ended);
  assert_eq!(
    (read.reply.text.as_str(), read.reply.session_id.as_deref()),
    ("first answer", Some("thread-1"))
  );
  assert_eq!(
    read.reply.last_request_usage,
    Some(RequestUsage {
      input_tokens: 100,
      cache_read_tokens: 900,
      cache_write_tokens: 0,
      output_tokens: 12
    })
  );
  assert_eq!(read.reply.spend.iter().map(|one| one.tokens).collect::<Vec<_>>(), vec![1012]);
  assert_eq!(
    serde_json::to_value(&read.reply.windows).unwrap(),
    json!([
      { "name": "codex:codex:primary", "utilization": 0.12, "resetsAt": 1790000000, "durationMinutes": 300, "limitName": null },
      { "name": "codex:codex:secondary", "utilization": 0.03, "resetsAt": 1790500000, "durationMinutes": 10080, "limitName": null },
    ])
  );
}

#[test]
fn only_the_last_task_is_read_and_one_still_going_works() {
  let mut lines = turn_lines("old");

  lines.push(json!({ "type": "event_msg", "payload": { "type": "task_started" } }));
  lines.push(json!({ "type": "event_msg", "payload": { "type": "agent_message", "message": "half" } }));

  let read = read_rollout(&lines, &run(Some("thread-2")), "codex");

  assert_eq!((read.state, read.reply.text.as_str()), (State::Working, "half"));
  assert_eq!(read.reply.session_id.as_deref(), Some("thread-2"));
  assert!(read.reply.spend.is_empty() && read.reply.request_usages.is_none());
}

#[test]
fn a_failed_task_names_its_code_and_message_and_an_aborted_one_reads_interrupted() {
  let failed = read_rollout(
    &[
      json!({ "type": "event_msg", "payload": { "type": "task_started" } }),
      json!({ "type": "event_msg", "payload": { "type": "task_complete", "error": { "codex_error_info": "usage_limit_exceeded", "message": "You've hit your usage limit" } } }),
    ],
    &run(None),
    "codex",
  );
  let aborted = read_rollout(
    &[
      json!({ "type": "event_msg", "payload": { "type": "task_started" } }),
      json!({ "type": "event_msg", "payload": { "type": "turn_aborted", "reason": "interrupted" } }),
    ],
    &run(None),
    "codex",
  );

  assert_eq!(failed.error.as_deref(), Some("usage_limit_exceeded: You've hit your usage limit"));
  assert_eq!(failed.reply.text, "");
  assert_eq!(error_text(&json!("plain")), "plain");
  assert_eq!(aborted.state, State::Interrupted);
}

#[test]
fn the_resident_home_shares_the_users_login_and_threads_and_trusts_each_folder_once() {
  let dir = tempfile::tempdir().unwrap();
  let homes = Homes {
    resident: dir.path().join("own"),
    user: dir.path().join("user"),
  };

  std::fs::create_dir_all(&homes.user).unwrap();
  std::fs::write(homes.user.join("auth.json"), "{}").unwrap();
  std::fs::write(homes.user.join("config.toml"), "model = \"gpt-5.6-terra\"\n").unwrap();

  prepare_home("/work/one", &homes).unwrap();
  prepare_home("/work/one", &homes).unwrap();
  prepare_home("/work/two", &homes).unwrap();

  assert_eq!(
    std::fs::read_link(homes.resident.join("auth.json")).unwrap(),
    homes.user.join("auth.json")
  );
  assert_eq!(
    std::fs::read_link(homes.resident.join("sessions")).unwrap(),
    homes.user.join("sessions")
  );
  assert_eq!(
    std::fs::read_to_string(homes.resident.join("config.toml")).unwrap(),
    "[projects.\"/work/one\"]\ntrust_level = \"trusted\"\n[projects.\"/work/two\"]\ntrust_level = \"trusted\"\n"
  );
  assert_eq!(
    std::fs::read_to_string(homes.user.join("config.toml")).unwrap(),
    "model = \"gpt-5.6-terra\"\n"
  );
}

#[test]
fn a_thread_is_found_wherever_its_day_filed_it_and_a_new_one_only_where_it_began_here() {
  let dir = tempfile::tempdir().unwrap();
  let day = dir.path().join("2026").join("09").join("21");
  let thread = "019a-0000-c0de";
  let path = day.join(format!("rollout-2026-09-21T12-03-04-{thread}.jsonl"));

  std::fs::create_dir_all(&day).unwrap();
  std::fs::write(
    &path,
    format!("{}\n", json!({ "type": "session_meta", "payload": { "id": thread, "cwd": "/w" } })),
  )
  .unwrap();

  assert_eq!(rollout_of(dir.path(), thread), Some(path.clone()));
  assert_eq!(rollout_of(dir.path(), "../escape"), None);
  assert_eq!(opened_rollout(dir.path(), &HashSet::new(), "/w", now_ms()), Some(path.clone()));
  assert_eq!(opened_rollout(dir.path(), &HashSet::new(), "/elsewhere", now_ms()), None);
  assert_eq!(opened_rollout(dir.path(), &HashSet::from([path]), "/w", now_ms()), None);
}

#[test]
fn a_program_on_the_thread_it_opened_resumes_it() {
  let command = ResidentCommand {
    args: vec!["codex".into(), "--model".into(), "gpt-5.6-luna".into()],
    cwd: "/w".into(),
    env: BTreeMap::new(),
    login_file: None,
  };

  assert_eq!(resumed(&command, "t1").args, vec!["codex", "resume", "t1", "--model", "gpt-5.6-luna"]);
}

#[test]
fn the_watch_is_handed_only_the_lines_the_program_has_ended() {
  let working = "{\"type\":\"session_meta\"}\n\n{\"type\":\"response_item\"}\n{\"type\":\"event_m";
  let ended = format!("{working}sg\"}}\n");

  assert_eq!(
    whole_lines(working),
    vec!["{\"type\":\"session_meta\"}", "{\"type\":\"response_item\"}"]
  );
  assert_eq!(whole_lines(&ended).len(), 3);
  assert_eq!(whole_lines(&ended)[2], "{\"type\":\"event_msg\"}");
  assert!(whole_lines("").is_empty());
}
