//! A run a procedure requires opening a job's way to a Claude Code session (docs/live-claude-guard.md,
//! 「Second layer: the guard in agency's code」). Alone in a process of its own, since the declaration stands
//! for the whole process.

use std::collections::BTreeMap;

use crate::integration::common::alone::alone;
use agency_server::door::guard::{declare_required_run, launch_refusal, prompt_refusal};
use harness_providers::Machine;

// "a declaration opens a job's launch"
#[test]
fn a_declaration_opens_a_jobs_launch() {
  let Some(_run) = alone(module_path!(), "a_declaration_opens_a_jobs_launch") else {
    return;
  };

  let job = Machine::with_env(
    [("CLAUDECODE", "1"), ("BMO_TASK_LOCKED", "1")]
      .map(|(name, value)| (name.to_owned(), value.to_owned())),
    "/nowhere",
  );
  let line = ["claude".to_owned(), "-p".to_owned()];

  assert!(launch_refusal("worker", &line, &BTreeMap::new(), &job).is_some());
  assert!(prompt_refusal(&job).is_some());

  declare_required_run("this test");

  assert_eq!(launch_refusal("worker", &line, &BTreeMap::new(), &job), None);
  assert_eq!(prompt_refusal(&job), None);
}
