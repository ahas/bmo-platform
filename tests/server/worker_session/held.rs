//! The session a worker holds: set aside only where the worker's own session turns to another model.

use std::path::PathBuf;

use agency_server::lines::worker::WorkerRun;
use agency_server::worker_session::held::*;

const CODEX: &str = "codex";

fn kept(mode: &str, model: &str, at: i64) -> Held {
  Held {
    mode: mode.to_owned(),
    model: model.to_owned(),
    at,
    ..Held::default()
  }
}

fn run_on(mode: &'static str, model: &str) -> SessionRun {
  SessionRun {
    mode,
    line: WorkerRun {
      model: model.to_owned(),
      effort: "high".to_owned(),
      tier: None,
      thinking: true,
      advisor: None,
      reasoning: None,
      system_prompt: None,
      harnessed: PathBuf::new(),
      rules: PathBuf::new(),
      mcp_config: PathBuf::new(),
      settings: PathBuf::new(),
      plugin_dir: PathBuf::new(),
      tools_denied: String::new(),
      cwd: "/work".to_owned(),
    },
    prompt: String::new(),
    resume: None,
    account: None,
    placement: None,
  }
}

#[test]
fn only_the_workers_own_session_turned_to_another_model_is_set_aside() {
  assert!(turns_from(&kept(MODE, "model-a", 0), &run_on(MODE, "model-b")));
  assert!(!turns_from(&kept(MODE, "model-a", 0), &run_on(MODE, "model-a")));
  assert!(!turns_from(&kept(CODEX, "model-a", 0), &run_on(MODE, "model-b")));
  assert!(!turns_from(&kept(MODE, "model-a", 0), &run_on(CODEX, "model-b")));
}
