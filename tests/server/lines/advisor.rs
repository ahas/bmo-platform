//! The advisor on a Claude Code line (docs/provider-order.md, 「The advisor」; docs/launch-lines.md): the flag
//! right after the effort where the road names one and the program would attach it, none otherwise, and no word
//! of a session's key.

use std::collections::BTreeMap;
use std::path::Path;

use agency_server::apart_sessions;
use agency_server::lines::chat::{Hands, Words, words};
use agency_server::lines::claude::{ApartRun, Command, Session, apart_args, without_advisor, worker_args};
use agency_server::lines::worker::WorkerRun;

// The words after `--effort`, up to the next flag but the advisor's.
fn after_effort(args: &[String]) -> Vec<String> {
  let at = args.iter().position(|word| word == "--effort").unwrap() + 2;

  args[at..].iter().take(2).cloned().collect()
}

fn chat(dir: &Path, model: &'static str, advisor: Option<&'static str>) -> Vec<String> {
  let system = dir.join("system.md");

  std::fs::write(&system, "a supervisor").unwrap();

  words(&Words {
    line: "chat",
    model,
    effort: "medium",
    advisor,
    system: &system,
    mcp: dir,
    plugin: None,
    hands: Hands::AllBut {
      denied: "WebSearch",
      kept: None,
    },
    host_commands: true,
  })
  .unwrap()
}

/// a supervisor's line names its advisor right after its effort
#[test]
fn a_supervisors_line_names_its_advisor_right_after_its_effort() {
  let dir = tempfile::tempdir().unwrap();

  assert_eq!(after_effort(&chat(dir.path(), "claude-sonnet-5", Some("opus"))), ["--advisor", "opus"]);
  assert!(!chat(dir.path(), "claude-sonnet-5", None).iter().any(|word| word == "--advisor"));
}

/// a model the program would not pair the advisor with goes out without it, a gateway member's name among them
#[test]
fn a_model_the_program_would_not_pair_goes_out_without_the_advisor() {
  let dir = tempfile::tempdir().unwrap();

  for model in ["flash-next", "claude-opus-5-5"] {
    let args = chat(dir.path(), model, Some("claude-sonnet-5"));

    assert!(!args.iter().any(|word| word == "--advisor"), "{model}: {args:?}");
  }
}

/// a runner line's run names its advisor right after its effort
#[test]
fn a_runner_lines_run_names_its_advisor_right_after_its_effort() {
  let run = |advisor| ApartRun {
    line: "search",
    model: "claude-sonnet-5",
    effort: "low",
    system_prompt: None,
    mcp_config: None,
    tools_shown: "",
    tools_allowed: "",
    host_commands: false,
    advisor,
  };

  assert_eq!(after_effort(&apart_args(&run(Some("fable")), "s-1").unwrap()), ["--advisor", "fable"]);
  assert!(!apart_args(&run(None), "s-1").unwrap().iter().any(|word| word == "--advisor"));
}

/// a worker line's run names its advisor right after its effort
#[test]
fn a_worker_lines_run_names_its_advisor_right_after_its_effort() {
  let dir = tempfile::tempdir().unwrap();
  let file = |name: &str| {
    let path = dir.path().join(name);

    std::fs::write(&path, "{}").unwrap();
    path
  };
  let run = WorkerRun {
    model: "claude-sonnet-5".to_owned(),
    effort: "high".to_owned(),
    tier: None,
    thinking: true,
    reasoning: None,
    system_prompt: None,
    harnessed: file("harnessed.md"),
    rules: file("rules.md"),
    mcp_config: file("mcp.json"),
    settings: file("settings.json"),
    plugin_dir: dir.path().to_path_buf(),
    tools_denied: "WebSearch".to_owned(),
    cwd: dir.path().to_string_lossy().into_owned(),
    advisor: Some("opus".to_owned()),
  };
  let args = worker_args(&run, &run.settings, Some(&Session::Open("s-1".into()))).unwrap();

  assert_eq!(after_effort(&args), ["--advisor", "opus"]);
}

/// another advisor is no other session: the key of a role's session leaves it out
#[test]
fn another_advisor_is_no_other_session() {
  let home = tempfile::tempdir().unwrap();
  let command = |args: &[&str]| Command {
    args: args.iter().map(|word| (*word).to_owned()).collect(),
    cwd: "/work".to_owned(),
    env: BTreeMap::new(),
  };
  let plain = command(&["claude", "--model", "claude-sonnet-5", "--effort", "high", "--session-id", "a"]);
  let advised = command(&[
    "claude",
    "--model",
    "claude-sonnet-5",
    "--effort",
    "high",
    "--advisor",
    "opus",
    "--session-id",
    "b",
  ]);
  let other = command(&["claude", "--model", "claude-sonnet-5", "--effort", "low", "--session-id", "a"]);

  assert_eq!(
    apart_sessions::key("worker", &plain, home.path()),
    apart_sessions::key("worker", &advised, home.path())
  );
  assert_ne!(
    apart_sessions::key("worker", &plain, home.path()),
    apart_sessions::key("worker", &other, home.path())
  );
  assert_eq!(without_advisor(&advised.args).len(), advised.args.len() - 2);
}
