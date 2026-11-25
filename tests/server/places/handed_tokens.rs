//! Agency's tokens handed as the files they stand in (docs/agent-control.md, 「A token is handed as the file it
//! stands in」, 「A record holds no secret」): a launch's environment and a run's carry each as its `_FILE`
//! variable, a place written before is rewritten by `hand-tokens`, and a place's records hold no token.

use std::collections::{BTreeMap, HashMap};
use std::os::unix::fs::PermissionsExt as _;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::door::place_run::{append, read_hooks};
use agency_server::lines::place::{Program, command_digest};
use agency_server::places::{Ground, hand_as_files, secret_file, token_or_file};
use agency_server::text::redact_tokens;
use agency_server::verbs::hand_tokens::hand_place;
use agent_host::place::ResidentCommand;
use harness_providers::Machine;
use sea_orm::DatabaseConnection;
use serde_json::json;

const CONTROL: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn ground(vars: &[(&str, &str)], data: &std::path::Path) -> Ground {
  let variables: HashMap<String, String> = HashMap::new();
  let service = Service::new(
    DatabaseConnection::Disconnected,
    Config::read(|name| variables.get(name).cloned()).unwrap(),
  );
  let machine =
    Machine::with_env(vars.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())), data);

  Ground::new(&service, &machine, None)
}

#[test]
fn a_launch_and_a_run_are_handed_agency_s_tokens_as_files_and_agency_keeps_the_values() {
  let data = tempfile::tempdir().unwrap();
  let own = data.path().join("chat.token");

  std::fs::write(&own, "chat-token\n").unwrap();

  let own = own.to_string_lossy().into_owned();
  let ground = ground(
    &[
      ("AGENCY_CONTROL_TOKEN", CONTROL),
      ("AGENCY_BRAIN_TOKEN", "brain-token"),
      ("BMO_JOB_TOKEN", "job-token"),
      ("BMO_JOB_CHAT_TOKEN_FILE", &own),
      ("BMO_JOB_URL", "http://[::1]:1"),
    ],
    data.path(),
  );

  for name in ["AGENCY_CONTROL_TOKEN", "AGENCY_BRAIN_TOKEN", "BMO_JOB_TOKEN"] {
    assert!(!ground.env().contains_key(name), "{name}");

    let file = secret_file(data.path(), name);

    assert_eq!(ground.env()[&format!("{name}_FILE")], file.to_string_lossy());
    assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600, "{name}");
  }

  assert_eq!(
    std::fs::read_to_string(secret_file(data.path(), "AGENCY_CONTROL_TOKEN")).unwrap(),
    CONTROL
  );
  assert_eq!(
    std::fs::metadata(data.path().join("secrets")).unwrap().permissions().mode() & 0o777,
    0o700
  );

  // A file the deployment names alone is handed as it is, and what agency calls with keeps the values.
  assert_eq!(ground.env()["BMO_JOB_CHAT_TOKEN_FILE"], own);
  assert_eq!(ground.vars()["BMO_JOB_TOKEN"], "job-token");
  assert_eq!(ground.env()["BMO_JOB_URL"], "http://[::1]:1");

  // A run's environment, its `BMO_` variables copied, carries the job token as its file too.
  let run = ground.run_env("a-run");

  assert!(!run.contains_key("BMO_JOB_TOKEN"), "{run:?}");
  assert_eq!(run["BMO_JOB_TOKEN_FILE"], secret_file(data.path(), "BMO_JOB_TOKEN").to_string_lossy());

  // A reader takes the value, else the file.
  let file = ground.env()["AGENCY_CONTROL_TOKEN_FILE"].clone();

  assert_eq!(token_or_file(None, Some(file.clone())).as_deref(), Some(CONTROL));
  assert_eq!(token_or_file(Some(" given ".into()), Some(file)).as_deref(), Some("given"));
  assert_eq!(token_or_file(None, Some("/no/such/file".into())), None);
}

#[test]
fn a_token_whose_file_cannot_be_written_is_handed_as_it_stands() {
  let data = tempfile::tempdir().unwrap();

  // The secrets directory's name taken by a file: nothing can be written under it.
  std::fs::write(data.path().join("secrets"), "").unwrap();

  let mut env = BTreeMap::from([("BMO_JOB_TOKEN".to_owned(), "job-token".to_owned())]);

  hand_as_files(&mut env, data.path());
  assert_eq!(env, BTreeMap::from([("BMO_JOB_TOKEN".to_owned(), "job-token".to_owned())]));
}

#[test]
fn hand_tokens_rewrites_a_place_written_before_without_overwriting_the_launcher_s_file() {
  let data = tempfile::tempdir().unwrap();
  let place = data.path().join("agent-places").join("worker");

  std::fs::create_dir_all(&place).unwrap();

  // The launcher already wrote the control token's file; the job token's it has not.
  let control = secret_file(data.path(), "AGENCY_CONTROL_TOKEN");

  std::fs::create_dir_all(control.parent().unwrap()).unwrap();
  std::fs::write(&control, "current").unwrap();

  let command = ResidentCommand {
    args: vec!["claude".into()],
    cwd: data.path().to_path_buf(),
    env: BTreeMap::from([
      ("AGENCY_CONTROL_TOKEN".to_owned(), "stale".to_owned()),
      ("BMO_JOB_TOKEN".to_owned(), "job-token".to_owned()),
      ("BMO_SHELL_CARRY".to_owned(), "1".to_owned()),
      ("BMO_CARRIED_424D4F5F4A4F425F544F4B454E".to_owned(), "job-token".to_owned()),
      ("HOME".to_owned(), "/".to_owned()),
    ]),
    login_file: None,
  };
  let program = Program {
    identity: command_digest(&command),
    command: command.clone(),
    restart: Some(command),
    opened: None,
  };

  std::fs::write(place.join("program.json"), serde_json::to_vec(&program).unwrap()).unwrap();
  assert!(hand_place(&place, data.path()).unwrap());

  let written: Program =
    serde_json::from_slice(&std::fs::read(place.join("program.json")).unwrap()).unwrap();
  let env = &written.command.env;

  assert_eq!(env.get("AGENCY_CONTROL_TOKEN"), None);
  assert_eq!(env.get("BMO_JOB_TOKEN"), None);
  assert_eq!(env["AGENCY_CONTROL_TOKEN_FILE"], control.to_string_lossy());
  assert_eq!(std::fs::read_to_string(&control).unwrap(), "current");
  assert_eq!(std::fs::read_to_string(secret_file(data.path(), "BMO_JOB_TOKEN")).unwrap(), "job-token");
  assert!(
    env.values().all(|value| value != "job-token"),
    "no carried copy keeps the value: {env:?}"
  );
  assert_eq!(written.restart.as_ref().unwrap().env, *env);
  assert_eq!(written.identity, command_digest(written.restart.as_ref().unwrap()));
  assert_ne!(written.identity, program.identity);

  // A place rewritten already is left as it is.
  assert!(!hand_place(&place, data.path()).unwrap());
}

#[test]
fn a_place_s_records_hold_no_token_shaped_value() {
  assert_eq!(redact_tokens(&format!("T={CONTROL}; curl")), "T=[redacted]; curl");
  assert_eq!(
    redact_tokens("-H 'Authorization: Bearer abcdefghijklmnopqrstuvwxyz' -d sk-or-v1-0123456789abcdef0123"),
    "-H 'Authorization: Bearer [redacted]' -d [redacted]"
  );
  // A session id, a commit and a short word after Bearer are no token.
  for kept in ["5f27643e-ae81-4401-83a3-bc4d195ba48b", "890a6c3 4878b11", "Bearer $T", &CONTROL[..40]] {
    assert_eq!(redact_tokens(kept), kept);
  }

  let place = tempfile::tempdir().unwrap();
  let hooks = place.path().join("hooks.jsonl");

  append(
    &hooks,
    &json!({ "at": 1, "kind": "pre-tool", "payload": { "tool_input": { "command": format!("T={CONTROL}") } } }),
  )
  .unwrap();

  let text = std::fs::read_to_string(&hooks).unwrap();

  assert!(!text.contains(CONTROL), "{text}");
  assert!(text.contains("T=[redacted]"), "{text}");
  assert_eq!(read_hooks(place.path(), 0).len(), 1);
}
