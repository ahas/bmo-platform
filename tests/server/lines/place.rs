use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use agency_server::lines::place::*;
use agent_host::place::ResidentCommand;
use harness_providers::Machine;
use serde_json::{Map, Value, json};

#[test]
fn a_program_whose_settings_came_to_call_other_hooks_is_another_program() {
  let dir = tempfile::tempdir().unwrap();
  let settings = dir.path().join("settings.json");
  let command = ResidentCommand {
    args: vec!["claude".into(), "--settings".into(), settings.to_string_lossy().into_owned()],
    cwd: dir.path().to_path_buf(),
    env: BTreeMap::new(),
    login_file: None,
  };

  std::fs::write(&settings, r#"{"statusLine":{"command":"old"}}"#).unwrap();

  let before = command_digest(&command);

  assert_eq!(command_digest(&command), before);
  std::fs::write(&settings, r#"{"statusLine":{"command":"new"}}"#).unwrap();
  assert_ne!(command_digest(&command), before);
}

#[test]
fn a_guard_the_place_s_verb_stands_is_the_program_s_and_another_hook_is_not() {
  let dir = tempfile::tempdir().unwrap();
  let settings = dir.path().join("settings.json");
  let command = ResidentCommand {
    args: vec!["claude".into(), "--settings".into(), settings.to_string_lossy().into_owned()],
    cwd: dir.path().to_path_buf(),
    env: BTreeMap::new(),
    login_file: None,
  };
  let hook = |command: &str| {
    json!({ "statusLine": { "command": "s" }, "hooks": { "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "command": command }] }] } })
      .to_string()
  };

  std::fs::write(&settings, r#"{"statusLine":{"command":"s"}}"#).unwrap();

  let bare = command_digest(&command);

  std::fs::write(&settings, hook("bmo job guard")).unwrap();
  assert_eq!(command_digest(&command), bare);
  std::fs::write(&settings, hook(&format!("bin/bmo-agency {STATUS_VERB} shell --place p"))).unwrap();
  assert_ne!(command_digest(&command), bare);
}

#[test]
fn a_command_opening_its_session_is_compared_by_the_form_that_resumes_it() {
  let machine = Machine::with_env([("HOME".to_owned(), "/fixture/home".to_owned())], "/data");
  let place = Path::new("/data/agent-places/worker");
  let env = BTreeMap::from([
    ("BMO_RUN_NAME".to_owned(), "bmo-standby-worker".to_owned()),
    ("BMO_TURN_ROOM".to_owned(), "5".to_owned()),
    ("PATH".to_owned(), "/bin".to_owned()),
  ]);
  let opening = ["claude".to_owned(), "--session-id".to_owned(), "s1".to_owned()];
  let program = resident_of(&machine, place, "/w", &opening, &env, "s1");
  let resumed = resident_of(
    &machine,
    place,
    "/w",
    &["claude".to_owned(), "--resume".to_owned(), "s1".to_owned()],
    &env,
    "s1",
  );

  assert_eq!(program.identity, resumed.identity);
  assert_eq!(program.restart.as_ref().unwrap().args, ["claude", "--resume", "s1"]);
  assert_eq!(
    program.command.env.get(PLACE_VARIABLE).map(String::as_str),
    Some("/data/agent-places/worker")
  );
  assert!(!program.command.env.contains_key("BMO_RUN_NAME"));
  assert!(!program.command.env.contains_key("BMO_TURN_ROOM"));
  assert_eq!(program.opened, Some(Some(PathBuf::from("/fixture/home/.claude/projects/-w/s1.jsonl"))));
  assert_eq!(resumed.opened, Some(None));
}

/// a standing program whose road took an advisor is another program, started again on the session it stood on
/// (docs/provider-order.md, 「The advisor」)
#[test]
fn a_standing_program_that_took_an_advisor_is_another_program_on_the_same_session() {
  let machine = Machine::with_env([("HOME".to_owned(), "/fixture/home".to_owned())], "/data");
  let place = Path::new("/data/agent-places/chat-agent");
  let env = BTreeMap::from([("PATH".to_owned(), "/bin".to_owned())]);
  let words = |advisor: Option<&str>| {
    let mut args: Vec<String> = ["claude", "--model", "claude-sonnet-5", "--effort", "medium"]
      .map(str::to_owned)
      .to_vec();

    if let Some(advisor) = advisor {
      args.extend(["--advisor".to_owned(), advisor.to_owned()]);
    }

    args.extend(["--resume".to_owned(), "s1".to_owned()]);
    args
  };
  let plain = resident_of(&machine, place, "/w", &words(None), &env, "s1");
  let advised = resident_of(&machine, place, "/w", &words(Some("opus")), &env, "s1");

  assert_ne!(plain.identity, advised.identity);
  assert!(advised.resumes("s1"));
  assert_eq!(
    advised.restart.as_ref().unwrap().args,
    [
      "claude",
      "--model",
      "claude-sonnet-5",
      "--effort",
      "medium",
      "--advisor",
      "opus",
      "--resume",
      "s1"
    ]
  );
}

#[test]
fn a_places_settings_keep_the_lines_own_hooks_and_add_the_status_line_and_the_four_hooks() {
  let dir = tempfile::tempdir().unwrap();
  let base = dir.path().join("settings.json");

  std::fs::write(
    &base,
    r#"{"env":{"A":"1"},"hooks":{"Stop":[{"hooks":[{"type":"command","command":"own"}]}]}}"#,
  )
  .unwrap();

  let place = dir.path().join("place");
  let path = write_place_settings(
    &place,
    read_settings(&base),
    Path::new("/deploy"),
    Holds {
      tasks: true,
      text_tool_call: true,
      wording: false,
      compose_typed: false,
      shell_guard: false,
      job_guard: false,
      carry_background: false,
    },
  )
  .unwrap();
  let settings: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
  let stop = settings["hooks"]["Stop"].as_array().unwrap();

  assert_eq!(settings["env"]["A"], "1");
  assert_eq!(stop[0]["hooks"][0]["command"], "own");
  assert_eq!(
    stop[1]["hooks"][0]["command"],
    format!(
      "'/deploy/bin/bmo-agency' 'agent-status' 'stop' '--place' '{}' '--hold-tasks' \
       '--hold-text-tool-call'",
      place.display()
    )
  );
  assert!(settings["statusLine"]["command"].as_str().unwrap().contains("'status'"));
  assert_eq!(settings["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
  assert_eq!(
    settings["hooks"]["StopFailure"][0]["hooks"][0]["command"],
    format!("'/deploy/bin/bmo-agency' 'agent-status' 'failure' '--place' '{}'", place.display())
  );
  assert_eq!(
    settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
    format!("'/deploy/bin/bmo-agency' 'agent-status' 'shell' '--place' '{}'", place.display()),
    "every place reads a shell call first to close its standard input, refusing nothing"
  );
}

// The plugins Claude Code builds in reach no program a place stands, a supervisor's or a run's, and a plugin the
// line's own settings name stands as they name it (docs/launch-lines.md, 「The CLI's own skills and plugins are
// switched off on every line」).
#[test]
fn a_places_settings_name_every_built_in_plugin_off() {
  let dir = tempfile::tempdir().unwrap();
  let place = dir.path().join("place");
  let mut given = Map::new();

  given.insert("enabledPlugins".into(), json!({ "cc-plugin-tips@builtin": true, "own@market": true }));

  let path = write_place_settings(&place, given, Path::new("/deploy"), Holds::of_line("chat")).unwrap();
  let settings: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
  let plugins = settings["enabledPlugins"].as_object().unwrap();
  let named = &policy::providers::claude_code::CLAUDE_CODE.launch.builtin_plugins;

  assert!(!named.is_empty());
  for plugin in named.iter().filter(|plugin| *plugin != "cc-plugin-tips") {
    assert_eq!(plugins[&format!("{plugin}@builtin")], json!(false), "{plugin}");
  }
  assert_eq!(plugins["cc-plugin-tips@builtin"], json!(true), "the line's own word stands");
  assert_eq!(plugins["own@market"], json!(true));
  assert_eq!(builtin_plugins_off().len(), named.len());
}

// The worker line's programs are jobs', so their place's shell hook is started as the job guard, and takes
// the file tools that write beside the shell.
#[test]
fn a_worker_s_place_guards_its_shell_as_a_job_s() {
  let dir = tempfile::tempdir().unwrap();
  let place = dir.path().join("place");
  let path =
    write_place_settings(&place, Map::new(), Path::new("/deploy"), Holds::of_line("worker")).unwrap();
  let settings: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();

  assert_eq!(
    settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
    format!(
      "'/deploy/bin/bmo-agency' 'agent-status' 'shell' '--place' '{}' '--guard-job'",
      place.display()
    )
  );
  assert_eq!(settings["hooks"]["PreToolUse"][0]["matcher"], "Bash|Edit|Write|MultiEdit|NotebookEdit");
  assert!(!Holds::of_line("chat").job_guard, "a supervisor's line is no job's");
}

#[test]
fn a_supervisor_s_place_guards_its_shell_and_carries_its_background_on_its_stop_and_submit() {
  let dir = tempfile::tempdir().unwrap();
  let place = dir.path().join("place");
  let path = write_place_settings(
    &place,
    Map::new(),
    Path::new("/deploy"),
    Holds {
      shell_guard: true,
      ..Holds::of_line("chat")
    },
  )
  .unwrap();
  let settings: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
  let guard = &settings["hooks"]["PreToolUse"][0];
  let hook = |kind: &str, flags: &str| {
    format!(
      "'/deploy/bin/bmo-agency' 'agent-status' '{kind}' '--place' '{}'{flags}",
      place.display()
    )
  };

  assert_eq!(guard["matcher"], "Bash");
  assert_eq!(guard["hooks"][0]["command"], hook("shell", " '--refuse-claude'"));
  assert_eq!(
    settings["hooks"]["Stop"][0]["hooks"][0]["command"],
    hook("stop", " '--carry-background'")
  );
  assert_eq!(
    settings["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"],
    hook("submit", " '--carry-background' '--compose-typed'")
  );
  assert!(carries_background(&place));
  assert!(!carries_background(dir.path()), "a place with no settings carries nothing");
}

#[test]
fn a_command_that_opts_in_carries_each_bmo_variable_under_a_name_no_scrub_drops() {
  let mut env = BTreeMap::from([
    (CARRY_VARIABLE.to_owned(), "1".to_owned()),
    ("BMO_MCP_TOKEN".to_owned(), "the token".to_owned()),
    ("PATH".to_owned(), "/bin".to_owned()),
    (format!("{CARRIER_PREFIX}00"), "left from before".to_owned()),
  ]);

  carry(&mut env);

  assert_eq!(carrier_of("BMO_MCP_TOKEN"), "BMO_CARRIED_424D4F5F4D43505F544F4B454E");
  assert_eq!(env.get(&carrier_of("BMO_MCP_TOKEN")).map(String::as_str), Some("the token"));
  assert!(!env.contains_key(&format!("{CARRIER_PREFIX}00")), "an old copy is dropped");
  assert!(!env.contains_key(&carrier_of("PATH")) && !env.contains_key(&carrier_of(CARRY_VARIABLE)));

  for word in ["KEY", "PASSWORD", "SECRET", "TOKEN"] {
    assert!(
      env
        .keys()
        .filter(|name| name.starts_with(CARRIER_PREFIX))
        .all(|name| !name.contains(word)),
      "{env:?}"
    );
  }

  let mut plain = BTreeMap::from([("BMO_MCP_TOKEN".to_owned(), "the token".to_owned())]);

  carry(&mut plain);
  assert_eq!(plain.len(), 1, "a command that does not opt in carries nothing");
}

#[test]
fn the_shell_start_up_file_puts_a_carried_variable_back_where_the_shell_lacks_it() {
  let data = tempfile::tempdir().unwrap();
  let script = carry_script(data.path()).unwrap();
  let shell = std::process::Command::new("bash")
    .args(["-c", r#"printf '%s|%s' "$BMO_MCP_TOKEN" "$BMO_DATA_DIR""#])
    .env_clear()
    .env("PATH", "/usr/bin:/bin")
    .env("BASH_ENV", &script)
    .env(carrier_of("BMO_MCP_TOKEN"), "a token with spaces")
    .env(carrier_of("BMO_DATA_DIR"), "the copy")
    .env("BMO_DATA_DIR", "the shell's own")
    .stdin(std::process::Stdio::null())
    .output()
    .unwrap();

  assert_eq!(String::from_utf8_lossy(&shell.stdout), "a token with spaces|the shell's own");
  assert_eq!(carry_script(data.path()).unwrap(), script, "the same file is written once");
}

#[tokio::test]
async fn a_program_written_into_its_place_carries_the_token_it_was_handed() {
  let data = tempfile::tempdir().unwrap();
  let place = Place::of(data.path(), "agent-a");
  let env = BTreeMap::from([
    (CARRY_VARIABLE.to_owned(), "1".to_owned()),
    ("BMO_MCP_TOKEN".to_owned(), "issued".to_owned()),
  ]);
  let command = ResidentCommand {
    args: vec!["term-client".into()],
    cwd: data.path().to_path_buf(),
    env,
    login_file: None,
  };
  let program = Program {
    identity: command_digest(&command),
    command,
    restart: None,
    opened: None,
  };

  place.replace(&program).await.unwrap();

  let written = place.program().unwrap();

  assert_eq!(written.identity, program.identity, "the copies are no part of the program's identity");
  assert_eq!(
    written.command.env.get(&carrier_of("BMO_MCP_TOKEN")).map(String::as_str),
    Some("issued")
  );
}

// "a program file names the files its commands need, which the agent host waits for before it starts it"
#[tokio::test]
async fn a_program_written_into_its_place_names_the_files_its_commands_need() {
  let data = tempfile::tempdir().unwrap();
  let place = Place::of(data.path(), "agent-b");
  let config = data.path().join("chat.agent-b.mcp.json");
  let prompt = data.path().join("prompt.md");
  let gone = data.path().join("gone.json");

  std::fs::write(&config, "{}").unwrap();
  std::fs::write(&prompt, "you are").unwrap();

  let command = |more: &[&Path]| ResidentCommand {
    args: ["claude", "--mcp-config", &config.to_string_lossy(), "--add-dir"]
      .into_iter()
      .map(str::to_owned)
      .chain([data.path().to_string_lossy().into_owned(), "relative.json".to_owned()])
      .chain(more.iter().map(|path| path.to_string_lossy().into_owned()))
      .collect(),
    cwd: data.path().to_path_buf(),
    env: BTreeMap::new(),
    login_file: None,
  };
  let program = Program {
    identity: "one".to_owned(),
    command: command(&[&gone]),
    restart: Some(command(&[&prompt])),
    opened: None,
  };

  // A folder, a relative word and a path to nothing are needed by nobody; each file of either command once.
  assert_eq!(needed_files(&program), vec![config.clone(), prompt.clone()]);

  place.replace(&program).await.unwrap();

  let written: Value =
    serde_json::from_str(&std::fs::read_to_string(place.path.join("program.json")).unwrap()).unwrap();

  assert_eq!(written["needs"], json!([config, prompt]));
  assert_eq!(
    place.program().unwrap(),
    program,
    "the files it needs are read back as the same program"
  );
}

// "a place standing on a node has its status line and hooks answered at home, through the node's link"
#[test]
fn a_place_standing_on_a_node_has_its_status_line_and_hooks_answered_at_home() {
  let dir = tempfile::tempdir().unwrap();
  let place = dir.path().join("place");

  agency_server::watches::links::mark_place_on(&place, Some("pc"), "/home/tester/folder").unwrap();

  let path =
    write_place_settings(&place, serde_json::Map::new(), Path::new("/deploy"), Holds::default()).unwrap();
  let settings: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();

  assert_eq!(
    settings["statusLine"]["command"],
    format!("'node-link' 'home' 'bin/bmo' 'agent-status' 'status' '--place' '{}'", place.display())
  );
  assert_eq!(
    settings["hooks"]["SessionStart"][0]["hooks"][0]["command"],
    format!("'node-link' 'home' 'bin/bmo' 'agent-status' 'start' '--place' '{}'", place.display())
  );
}
