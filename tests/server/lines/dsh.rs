use std::collections::BTreeMap;

use agency_server::lines::dsh::*;
use agency_server::lines::ordered::Ordered;
use agency_server::lines::token::TOKEN_VARIABLE;
use policy::providers::deepseek::DEEPSEEK;
use policy::providers::openrouter::OPENROUTER;
use serde_json::Value;

#[test]
fn a_long_task_is_cut_at_spaces_that_join_it_again() {
  // Three bytes and one UTF-16 unit a letter, so the first stretch alone runs past one argument's bytes.
  let task = format!("{} flag -x {}", "€".repeat(WORD_UNITS - 3), "b".repeat(10));
  let words = task_words(&task);

  assert_eq!(words.len(), 2);
  assert!(words.iter().all(|word| word.len() <= WORD_BYTES && !word.starts_with('-')));
  assert_eq!(words.join(" "), task);
  assert_eq!(task_words("€ task\0"), vec!["€ task".to_owned()]);
}

#[test]
fn a_failure_is_dsh_s_own_word_before_the_last_line() {
  assert_eq!(
    failure("dsh: reasoning about it\nError: dsh: no route\nbanner", 1),
    "Error: dsh: no route"
  );
  assert_eq!(failure("dsh: reasoning\nnpm ERR! fetch failed\n", 2), "exit 2: npm ERR! fetch failed");
  assert_eq!(failure("", 3), "exit 3");
}

// The overlay a standing session reads, as a line that grants tools and as one shown none.
fn session_overlay(shown: &'static str) -> Vec<Value> {
  let dir = tempfile::tempdir().unwrap();
  let system = dir.path().join("system.md");

  std::fs::write(&system, "You are the agent.\n").unwrap();

  let run = Composed {
    line: "chat",
    provider: "openrouter",
    system: Some(&system),
    rules: None,
    shown: Some(shown),
    allowed: Some(shown),
    cwd: "/work",
    model: "vendor/model",
    effort: "",
    chosen: false,
    thinking: true,
    mcp: None,
    compact_at: None,
  };
  let patch =
    session_patch(&run, &BTreeMap::new(), dir.path(), OPENROUTER.declared.dsh.as_ref().unwrap()).unwrap();

  serde_json::from_str(&std::fs::read_to_string(&patch).unwrap()).unwrap()
}

#[test]
fn a_session_granting_tools_keeps_dsh_s_defaults_but_the_web_and_the_loops() {
  let overlay = session_overlay("Bash,Read");
  let disabled = |id: &str| overlay.iter().any(|row| row["id"] == id && row["disabled"] == true);

  for id in ["tool-web", "tool-goal"] {
    assert!(disabled(id), "{id}");
  }

  for id in ["tool-bash", "tool-fs", "tool-fs-search", "plan-mode", "user-questions"] {
    assert!(!disabled(id), "{id}");
  }

  assert_eq!(overlay[0]["id"], "system-prompt");

  // The chat's role and persona follow dsh's own prompt whole.
  assert_eq!(
    overlay[0]["config"]["personaPrefix"],
    "You are a coding agent powered by the {{model}} model."
  );
  assert_eq!(overlay[0]["config"]["personaSuffix"], "You are the agent.\n");
  assert!(disabled("agent-instructions"));
  assert!(disabled("session-title-llm"));
}

#[test]
fn a_worker_s_session_on_deepseek_writes_its_rules_after_the_opening_line_on_deepseek_s_own_route() {
  let dir = tempfile::tempdir().unwrap();
  let rules = dir.path().join("rules.md");

  std::fs::write(&rules, "Keep the {{name}} mark as it stands.\n").unwrap();

  let run = Composed {
    line: "worker",
    provider: "deepseek",
    system: None,
    rules: Some(&rules),
    shown: None,
    allowed: None,
    cwd: "/work",
    model: "deepseek-flash",
    effort: "",
    chosen: false,
    thinking: true,
    mcp: None,
    compact_at: None,
  };
  let patch =
    session_patch(&run, &BTreeMap::new(), dir.path(), DEEPSEEK.declared.dsh.as_ref().unwrap()).unwrap();
  let overlay: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&patch).unwrap()).unwrap();
  let row = |id: &str| overlay.iter().find(|row| row["id"] == id).unwrap();
  let config = &row("system-prompt")["config"];

  // dsh's own prompt stands whole, its opening line as the profile writes it, and the rules follow its
  // guidance as the persona suffix. dsh reads a `{{name}}` as its own variable and refuses the prompt on one it
  // does not hold, so the rules' braces reach it broken.
  assert_eq!(config["personaPrefix"], "You are a coding agent powered by the {{model}} model.");
  assert_eq!(config["personaSuffix"], "Keep the {\u{2060}{name}} mark as it stands.\n");
  assert!(config.get("persona").is_none());

  let route = &row("llm-pi-ai")["config"]["providers"]["deepseek"];

  assert_eq!(route["apiKeyEnv"], "DEEPSEEK_API_KEY");
  assert_eq!(route["baseURL"], "https://api.deepseek.com/v1");
  assert_eq!(row("agent-default-model")["config"]["provider"], "deepseek");
  assert_eq!(row("agent-default-model")["config"]["model"], "deepseek-flash");
}

#[test]
fn a_session_shown_no_tools_switches_every_tool_row_off() {
  let overlay = session_overlay("");
  let disabled = |id: &str| overlay.iter().any(|row| row["id"] == id && row["disabled"] == true);

  for id in [
    "tool-bash",
    "tool-pwsh",
    "tool-jobs",
    "tool-fs",
    "tool-fs-search",
    "tool-skill",
    "tool-subagent-control",
    "tool-subagent-list-agents",
    "tool-subagent",
    "tool-subagent-fork",
    "tool-workflow",
    "tool-todo",
    "tool-goal",
    "tool-str-replace-editor",
    "tool-working-directory",
    "tool-web",
    "plan-mode",
    "user-questions",
  ] {
    assert!(disabled(id), "{id}");
  }
}

#[test]
fn the_overlay_writes_the_bearer_as_dsh_reads_it() {
  let overlay =
    Ordered::map(vec![("Authorization", Ordered::Text(format!("{ENVIRONMENT_MARK}{TOKEN_VARIABLE}")))]);

  assert_eq!(
    overlay_text(&overlay).unwrap(),
    "{\n  \"Authorization\": !!js \"`Bearer ${process.env.BMO_MCP_TOKEN}`\"\n}\n"
  );

  // A server's variable is read from dsh's environment by its name, never written into the file.
  let env = Ordered::map(vec![("BMO_JOB_TOKEN", Ordered::Text(format!("{VALUE_MARK}BMO_JOB_TOKEN")))]);

  assert_eq!(
    overlay_text(&env).unwrap(),
    "{\n  \"BMO_JOB_TOKEN\": !!js \"process.env.BMO_JOB_TOKEN\"\n}\n"
  );
}

#[test]
fn the_same_session_composes_the_same_overlay_and_a_changed_prompt_another() {
  let dir = tempfile::tempdir().unwrap();
  let system = dir.path().join("system.md");
  let mcp = dir.path().join("mcp.json");

  std::fs::write(&system, "You are the agent.\n").unwrap();
  std::fs::write(
    &mcp,
    r#"{"mcpServers": {
      "bmo": {"type": "http", "url": "http://[::1]:1/mcp/bmo", "headers": {"Authorization": "Bearer ${BMO_MCP_TOKEN}"}},
      "web": {"type": "http", "url": "http://[::1]:1/mcp/web", "headers": {"Authorization": "Bearer ${BMO_MCP_TOKEN}"}},
      "job": {"command": "bmo-job", "args": ["mcp"], "env": {"B_TOKEN": "b", "A_TOKEN": "a"}}
    }}"#,
  )
  .unwrap();

  let run = Composed {
    line: "chat",
    provider: "deepseek",
    system: Some(&system),
    rules: None,
    shown: Some("Bash,Read"),
    allowed: Some("Bash,Read"),
    cwd: "/work",
    model: "deepseek-flash",
    effort: "max",
    chosen: true,
    thinking: true,
    mcp: Some(&mcp),
    compact_at: None,
  };
  let env = BTreeMap::from([("B".to_owned(), "2".to_owned()), ("A".to_owned(), "1".to_owned())]);
  let route = DEEPSEEK.declared.dsh.as_ref().unwrap();
  let first = session_patch(&run, &env, dir.path(), route).unwrap();
  let first_text = std::fs::read_to_string(&first).unwrap();
  let second = session_patch(&run, &env, dir.path(), route).unwrap();

  // Composed again on the same words, the overlay is the same file to the byte, so the pane's command and
  // its identity stand.
  assert_eq!(first, second);
  assert_eq!(std::fs::read_to_string(&second).unwrap(), first_text);

  std::fs::write(&system, "You are another agent.\n").unwrap();

  assert_ne!(session_patch(&run, &env, dir.path(), route).unwrap(), first);
}

// A stdio server's value the program holds under the same name is read by that name; one it holds otherwise
// — the chat's own job token beside the shared one — is carried under the server's own variable, which the
// overlay reads (docs/agent-control.md, the overlay's MCP servers).
#[test]
fn a_server_s_own_value_is_carried_under_its_own_variable_where_the_program_holds_another() {
  let dir = tempfile::tempdir().unwrap();
  let system = dir.path().join("system.md");
  let mcp = dir.path().join("mcp.json");

  std::fs::write(&system, "You are the agent.\n").unwrap();
  std::fs::write(
    &mcp,
    r#"{"mcpServers": {
      "job": {"command": "bmo-job", "args": ["mcp"], "env": {"BMO_JOB_TOKEN": "chat-token"}},
      "agency": {"command": "bmo-agency", "args": ["mcp"], "env": {"AGENCY_CONTROL_TOKEN": "control"}}
    }}"#,
  )
  .unwrap();

  let env = BTreeMap::from([
    ("BMO_JOB_TOKEN".to_owned(), "shared-token".to_owned()),
    ("AGENCY_CONTROL_TOKEN".to_owned(), "control".to_owned()),
  ]);
  let carried = server_values(Some(&mcp), &env).unwrap();

  assert_eq!(server_variable("job", "BMO_JOB_TOKEN"), "BMO_MCP_JOB_BMO_JOB_TOKEN");
  assert_eq!(
    carried,
    BTreeMap::from([("BMO_MCP_JOB_BMO_JOB_TOKEN".to_owned(), "chat-token".to_owned())])
  );

  let mut program = env.clone();

  program.extend(carried);

  let run = Composed {
    line: "chat",
    provider: "deepseek",
    system: Some(&system),
    rules: None,
    shown: Some("Bash,Read"),
    allowed: Some("Bash,Read"),
    cwd: "/work",
    model: "deepseek-flash",
    effort: "max",
    chosen: true,
    thinking: true,
    mcp: Some(&mcp),
    compact_at: None,
  };
  let route = DEEPSEEK.declared.dsh.as_ref().unwrap();
  let text = std::fs::read_to_string(session_patch(&run, &program, dir.path(), route).unwrap()).unwrap();

  assert!(
    text.contains("\"BMO_JOB_TOKEN\": !!js \"process.env.BMO_MCP_JOB_BMO_JOB_TOKEN\""),
    "{text}"
  );
  assert!(
    text.contains("\"AGENCY_CONTROL_TOKEN\": !!js \"process.env.AGENCY_CONTROL_TOKEN\""),
    "{text}"
  );
  assert!(
    !text.contains("chat-token") && !text.contains("\"control\""),
    "no value in the file: {text}"
  );
}

#[test]
fn a_session_that_folds_itself_compacts_at_its_fold_ratio_and_another_at_dsh_s_own() {
  let dir = tempfile::tempdir().unwrap();
  let system = dir.path().join("system.md");

  std::fs::write(&system, "You are the agent.\n").unwrap();

  let overlay = |compact_at: Option<f64>| -> Vec<Value> {
    let run = Composed {
      line: "chat",
      provider: "deepseek",
      system: Some(&system),
      rules: None,
      shown: Some("Bash"),
      allowed: Some("Bash"),
      cwd: "/work",
      model: "deepseek-flash",
      effort: "",
      chosen: false,
      thinking: true,
      mcp: None,
      compact_at,
    };
    let patch =
      session_patch(&run, &BTreeMap::new(), dir.path(), DEEPSEEK.declared.dsh.as_ref().unwrap()).unwrap();

    serde_json::from_str(&std::fs::read_to_string(&patch).unwrap()).unwrap()
  };
  let folding = overlay(Some(0.5));
  let row = folding.iter().find(|row| row["id"] == COMPACTION_ROW).expect("the compaction row");

  assert_eq!(row["config"], serde_json::json!({ "thresholdRatio": 0.5 }));
  assert!(
    overlay(None).iter().all(|row| row["id"] != COMPACTION_ROW),
    "dsh's own threshold stands"
  );
}
