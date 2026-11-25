//! A program's paths and variables read as the node's.

use node_link::rewrite::*;
use serde_json::Value;

fn roots() -> Vec<(String, String)> {
  vec![
    ("/h/a/.local/share/bmo/data".into(), "/n/w/data".into()),
    ("/h/a/.local/share/bmo-agency/deploy".into(), "/n/w/deploy".into()),
    ("/h/a".into(), "/n".into()),
  ]
}

#[test]
fn every_root_in_a_string_reads_as_the_nodes() {
  assert_eq!(
    rewrite_text("--settings=/h/a/.local/share/bmo/data/p/settings.json", &roots()),
    "--settings=/n/w/data/p/settings.json"
  );
  assert_eq!(
    rewrite_text("/h/a/bin:/usr/bin:/h/a/.local/share/bmo-agency/deploy/bin", &roots()),
    "/n/bin:/usr/bin:/n/w/deploy/bin"
  );
  assert_eq!(rewrite_text("/h/ab/x", &roots()), "/h/ab/x");
  assert_eq!(rewrite_text("echo /h/a", &roots()), "echo /n");
}

#[test]
fn a_program_reads_as_the_nodes_with_its_files_gathered_and_no_token_of_this_machine() {
  let mut program: Value = serde_json::json!({
    "command": {
      "args": ["claude", "--settings", "/h/a/.local/share/bmo/data/agent-places/w/settings.json"],
      "cwd": "/n/repo",
      "env": { "PATH": "/h/a/bin", "BMO_AGENT_PLACE": "/h/a/.local/share/bmo/data/agent-places/w", "BMO_CONTROL_TOKEN": "secret", "CLAUDE_CODE_X": "1" }
    },
    "opened": "/h/a/.claude/projects/x/s.jsonl"
  });
  let keep = vec!["BMO_AGENT_PLACE".to_owned(), "CLAUDE_CODE_*".to_owned()];
  let add = vec![("BMO_HOME_BRIDGE".to_owned(), "/n/w/data/bridge.sock".to_owned())];
  let env = NodeEnv {
    keep: &keep,
    path: Some("/n/.bun/bin:/usr/bin"),
    add: &add,
    programs: &[],
  };
  let mut named = Vec::new();

  rewrite_json(&mut program, &roots(), &env, &["/h/a/.local/share/bmo/data".to_owned()], &mut named);

  assert_eq!(program["command"]["args"][2], "/n/w/data/agent-places/w/settings.json");
  assert_eq!(
    program["command"]["env"],
    serde_json::json!({ "PATH": "/n/.bun/bin:/usr/bin", "BMO_AGENT_PLACE": "/n/w/data/agent-places/w", "CLAUDE_CODE_X": "1", "BMO_HOME_BRIDGE": "/n/w/data/bridge.sock" })
  );
  assert_eq!(program["opened"], "/n/.claude/projects/x/s.jsonl");
  assert_eq!(
    named,
    vec![
      "/h/a/.local/share/bmo/data/agent-places/w/settings.json".to_owned(),
      "/h/a/.local/share/bmo/data/agent-places/w".to_owned()
    ]
  );
}

#[test]
fn a_path_between_single_quotes_reads_as_the_nodes_and_is_gathered() {
  let mut settings: Value =
    serde_json::json!({ "apiKeyHelper": "cat '/h/a/.local/share/bmo/data/env/llm-gateway.key'" });
  let mut named = Vec::new();

  rewrite_json(
    &mut settings,
    &roots(),
    &NodeEnv::default(),
    &["/h/a/.local/share/bmo/data".to_owned()],
    &mut named,
  );

  assert_eq!(settings["apiKeyHelper"], "cat '/n/w/data/env/llm-gateway.key'");
  assert_eq!(named, ["/h/a/.local/share/bmo/data/env/llm-gateway.key"]);
}

#[test]
fn a_command_whose_program_the_node_keeps_its_own_of_runs_that_one_and_no_other_word_moves() {
  let mut program: Value = serde_json::json!({
    "command": { "args": ["claude", "--name", "claude"], "cwd": "/n/repo" },
    "restart": { "args": ["claude", "--resume", "s"], "cwd": "/n/repo" },
    "other": { "args": ["codex", "claude"] }
  });
  let programs = vec![("claude".to_owned(), "/n/w/bin/bmo-claude".to_owned())];
  let env = NodeEnv {
    programs: &programs,
    ..NodeEnv::default()
  };

  rewrite_json(&mut program, &roots(), &env, &[], &mut Vec::new());

  assert_eq!(
    program["command"]["args"],
    serde_json::json!(["/n/w/bin/bmo-claude", "--name", "claude"])
  );
  assert_eq!(program["restart"]["args"][0], "/n/w/bin/bmo-claude");
  assert_eq!(program["other"]["args"], serde_json::json!(["codex", "claude"]));
}

#[test]
fn a_node_that_keeps_no_program_of_its_own_runs_the_word_as_it_is() {
  let mut program: Value = serde_json::json!({ "command": { "args": ["claude", "-p"] } });

  rewrite_json(&mut program, &roots(), &NodeEnv::default(), &[], &mut Vec::new());

  assert_eq!(program["command"]["args"][0], "claude");
}
