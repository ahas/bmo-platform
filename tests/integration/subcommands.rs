//! The verbs as a shell reaches them: each subcommand of `bmo-agency` the deployment, the hooks and the panes
//! call, hidden ones among them, read by the binary's own command line, and the status line's shell guard
//! answered through it.

use std::io::Write as _;
use std::process::{Command, Output, Stdio};

use crate::support::binary;

/// Every subcommand path the deployment, an agent's settings, a hook or a pane runs.
const SUBCOMMANDS: &[&[&str]] = &[
  &["agent"],
  &["agent", "status"],
  &["agent-status"],
  &["node", "claude-token"],
  &["node", "pane"],
  &["node", "work"],
  &["dev-server", "start"],
  &["dev-server", "restart"],
  &["dev-server", "stop"],
  &["dev-server", "list"],
  &["dev-server", "pane"],
  &["model"],
  &["reload"],
  &["supervisor", "follow"],
  &["supervisor", "reset"],
  &["run", "list"],
  &["run", "stop"],
  &["machine", "memory"],
  &["claude", "batch"],
  &["live-claude"],
  &["prompts"],
  &["dev-pane"],
  &["node-pane"],
  &["work-node"],
];

// The binary run on `args` in a home and a data root of the test's own, `input` on its standard input.
fn run(args: &[&str], input: &str) -> Output {
  let home = tempfile::tempdir().unwrap();
  let mut child = Command::new(binary::path("bmo-agency"))
    .args(args)
    .current_dir(home.path())
    .env("HOME", home.path())
    .env("BMO_DATA_DIR", home.path().join("data"))
    .env_remove("BMO_ARCHIVE_URL")
    .env_remove("CLAUDECODE")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();

  child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
  child.wait_with_output().unwrap()
}

#[test]
fn every_subcommand_a_caller_runs_is_read_by_the_binary() {
  let mut unread = Vec::new();

  for path in SUBCOMMANDS {
    let mut args = path.to_vec();

    args.push("--help");

    let output = run(&args, "");
    let said = String::from_utf8_lossy(&output.stdout);

    if !output.status.success() || !said.contains("Usage:") {
      unread.push(format!("{}: {}", path.join(" "), String::from_utf8_lossy(&output.stderr)));
    }
  }

  assert!(unread.is_empty(), "subcommands the binary does not read: {unread:#?}");
}

#[test]
fn the_status_lines_shell_hook_refuses_a_launch_of_claude_code_and_closes_another_command_s_input() {
  let place = tempfile::tempdir().unwrap();
  let at = place.path().to_string_lossy().into_owned();
  let guarded = ["agent-status", "shell", "--place", at.as_str(), "--refuse-claude"];
  let refused = run(&guarded, r#"{"tool_name":"Bash","tool_input":{"command":"claude -p hello"}}"#);
  // The shell hook reads only the call, so it answers with no place named.
  let let_through = run(
    &["agent-status", "shell"],
    r#"{"tool_name":"Bash","tool_input":{"command":"ls -la ."}}"#,
  );

  assert!(refused.status.success());
  assert!(String::from_utf8_lossy(&refused.stdout).contains("deny"), "{refused:?}");
  assert!(let_through.status.success());

  let answer: serde_json::Value = serde_json::from_slice(&let_through.stdout).unwrap();

  assert_eq!(answer["hookSpecificOutput"]["updatedInput"]["command"], "exec </dev/null\nls -la .");
}

#[test]
fn a_subcommand_nobody_runs_is_refused() {
  let output = run(&["no-such-verb"], "");

  assert!(!output.status.success());
}
