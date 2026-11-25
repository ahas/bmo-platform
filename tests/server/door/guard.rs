//! The road a turn goes out on (docs/agent-control.md, 「The registered models through Claude Code on the
//! gateway」): a program under a registration's home or at the gateway is that road's and needs no Claude Code
//! on, where a DeepSeek account's own endpoint is Claude Code's road and is refused while Claude Code is off.

use std::collections::BTreeMap;
use std::path::Path;

use crate::support::grounds::{OrderEntry, set_provider_order};
use agency_server::door::guard::{launch_refusal, prompt_refusal, socket_refusal, turn_refusal};
use harness_providers::Machine;
use harness_providers::registered::Registered;
use policy::door_words::DOOR_WORDS;

use crate::support::scratch::Scratch;

const WORK_KEY: &str = "job-worker";

// What a turn on a Claude Code program is refused with while Claude Code is off.
fn refused(work_key: &str) -> String {
  format!("{work_key}: selected claude-code route did not answer: {}", DOOR_WORDS.provider_off)
}

// The owner's provider order with Claude Code on or off, and every other provider on.
async fn order(scratch: &Scratch, claude_code: bool) {
  let entries = [
    OrderEntry {
      name: "claude-code".into(),
      enabled: claude_code,
    },
    OrderEntry {
      name: "codex".into(),
      enabled: true,
    },
    OrderEntry {
      name: "openrouter".into(),
      enabled: true,
    },
    OrderEntry {
      name: "registered".into(),
      enabled: true,
    },
  ];

  set_provider_order(scratch.db(), &entries).await.unwrap();
}

fn program_env(home: &Path, base_url: Option<&str>) -> BTreeMap<String, String> {
  let mut env = BTreeMap::from([("CLAUDE_CONFIG_DIR".to_owned(), home.to_string_lossy().into_owned())]);

  if let Some(url) = base_url {
    env.insert("ANTHROPIC_BASE_URL".to_owned(), url.to_owned());
  }

  env
}

#[tokio::test]
async fn a_deepseek_program_is_claude_codes_and_is_refused_while_it_is_off_where_a_registered_one_is_not() {
  let scratch = Scratch::new().await;
  let dir = tempfile::tempdir().unwrap();
  let gateway = "http://gateway.test:9000";
  let machine = Machine::with_env([("BMO_LLM_GATEWAY_URL".to_owned(), gateway.to_owned())], dir.path());
  let deepseek =
    program_env(&dir.path().join("env").join("deepseek"), Some("https://api.deepseek.com/anthropic"));
  let registration = program_env(&Registered::homes(&machine).join("member"), Some(gateway));

  order(&scratch, false).await;

  assert_eq!(turn_refusal(scratch.db(), &machine, WORK_KEY, &deepseek).await, Some(refused(WORK_KEY)));
  assert_ne!(
    turn_refusal(scratch.db(), &machine, WORK_KEY, &registration).await,
    Some(refused(WORK_KEY))
  );

  order(&scratch, true).await;

  assert_ne!(turn_refusal(scratch.db(), &machine, WORK_KEY, &deepseek).await, Some(refused(WORK_KEY)));
}

fn machine_of(vars: &[(&str, &str)]) -> Machine {
  Machine::with_env(vars.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())), "/nowhere")
}

// A test run from inside a job's shell carries both of a job's marks in its process; the guard reads them
// off the machine it is handed, so a test's machine, which carries neither, is not a job's.
#[test]
fn a_jobs_marks_are_read_off_the_machine_handed_in_and_a_tests_home_lifts_them() {
  let launch = ["claude".to_owned(), "-p".to_owned()];
  let job = machine_of(&[("CLAUDECODE", "1"), ("BMO_TASK_LOCKED", "1")]);

  assert!(prompt_refusal(&job).is_some());
  assert!(launch_refusal("worker", &launch, &BTreeMap::new(), &job).is_some());

  for not_a_job in [
    machine_of(&[]),
    machine_of(&[("CLAUDECODE", "1")]),
    machine_of(&[("CLAUDECODE", "1"), ("BMO_TASK_LOCKED", "1"), ("BMO_TEST_HOME", "/a/test/home")]),
  ] {
    assert_eq!(prompt_refusal(&not_a_job), None);
    assert_eq!(launch_refusal("worker", &launch, &BTreeMap::new(), &not_a_job), None);
  }
}

#[test]
fn a_tests_home_holds_it_to_a_herdr_under_that_home() {
  let home = tempfile::tempdir().unwrap();
  let test = machine_of(&[("BMO_TEST_HOME", &home.path().to_string_lossy())]);

  assert_eq!(socket_refusal(&home.path().join("herdr.sock"), &test), None);
  assert!(socket_refusal(Path::new("/run/herdr/herdr.sock"), &test).is_some());
  assert_eq!(socket_refusal(Path::new("/run/herdr/herdr.sock"), &machine_of(&[])), None);
}
