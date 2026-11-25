//! The job worker's line: an account naming a base prompt hands the worker a whole system prompt of its
//! own with the worker's rules after it, and an account naming none hands the platform's harness opening
//! with the same rules after it (docs/base-prompts.md, 「What each agent is sent」).

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use crate::support::grounds::{OrderEntry, set_provider_order};
use agency_server::Service;
use agency_server::config::Config;
use agency_server::control::claude_accounts::{Account, Credential, EndpointModels};
use agency_server::lines::Trees;
use agency_server::lines::claude::{
  COMPACT_SHARE, Command, Runs, command_env, line_environment, worker_args,
};
use agency_server::lines::standing::registered_program;
use agency_server::lines::worker::{configured_pinned, pinned_model_on, role_run, worker_run};
use agency_server::places::Ground;
use harness_providers::Machine;
use sea_orm::Value;

use crate::support::rows::repository;
use crate::support::scratch::Scratch;

// No line sets the share of its model's window a session compacts at: a chat, an editor, a gate and a
// job worker alike compact where Claude Code does by itself (docs/sessions.md, 「Session compaction」).
#[test]
fn no_line_sets_the_compact_share() {
  let process = BTreeMap::from([("PATH".to_owned(), "/bin".to_owned())]);
  let command = Command {
    args: vec![],
    cwd: "/w".into(),
    env: command_env(&process, None, &Runs::OwnLogin, true).unwrap(),
  };

  assert!(!line_environment(&command, "bmo-any-line", None).contains_key(COMPACT_SHARE));
}

// The worker line's own settings name every plugin Claude Code builds in off, the same list its places name, and
// carry no list of the CLI's bundled skills, which the line's variables switch off whole (docs/launch-lines.md,
// 「The CLI's own skills and plugins are switched off on every line」).
#[test]
fn the_worker_settings_switch_off_the_cli_s_own_plugins_and_name_no_bundled_skill() {
  let settings: serde_json::Value = serde_json::from_str(
    &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/prompts/agents/worker/settings.json"))
      .unwrap(),
  )
  .unwrap();

  assert_eq!(
    settings["enabledPlugins"].as_object().unwrap(),
    &agency_server::lines::place::builtin_plugins_off()
  );
  assert!(settings.get("skillOverrides").is_none());
}

// A ground with a store of its own, its home and data root under one directory the test drops.
async fn ground(scratch: &Scratch, home: &tempfile::TempDir) -> Ground {
  let machine = Machine::with_env(
    [("HOME".to_owned(), home.path().to_string_lossy().into_owned())],
    home.path().join("data"),
  );
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());

  Ground::new(&service, &machine, None)
}

// The tracer, a role whose order holds Claude Code's Sonnet at low effort and, on the registry, a
// registration of its own that thinks, and whose Claude Code runs are given a provider prompt.
async fn tracer(scratch: &Scratch) {
  scratch
    .run(
      r#"INSERT INTO "roles" ("name", "label", "bundles", "max_concurrent", "prompt", "position", "created_at",
           "updated_at", "description", "line", "provider_order")
         VALUES ('tracer', 'Tracer', '[]', 1, 'Trace the road.', 0, 0, 0, 'Traces a road', 'worker',
           '[{"provider":"claude-code","model":"claude-sonnet-5","effort":"low"},
             {"provider":"registered","model":"coder-registration","effort":"thinking"}]')
         ON CONFLICT ("name") DO UPDATE SET "prompt" = excluded."prompt", "description" = excluded."description",
           "provider_order" = excluded."provider_order""#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "role_provider_prompts" ("role", "provider", "prompt", "updated_at")
         VALUES ('tracer', 'claude-code', 'Open your subagents as you judge.', 0)
         ON CONFLICT ("role", "provider") DO NOTHING"#,
      vec![],
    )
    .await;
  // One agent of the tracer on each road, so the role is defined on either provider a caller stands on.
  crate::support::rows::seated_on_order(scratch.db(), "tracer").await;
}

// An account of a kind, on no base prompt unless it is given one.
fn account(id: &str, credential: Credential, base_prompt: Option<&str>) -> Account {
  Account {
    id: id.into(),
    name: id.into(),
    place: 0,
    enabled: true,
    credential,
    needs_login_at: None,
    user_id: None,
    unit_id: None,
    node: None,
    lendable: false,
    base_prompt: base_prompt.map(str::to_owned),
    node_token_gone: None,
  }
}

// The file the words after the flag hand over, as the program is handed it.
fn handed_after(args: &[String], flag: &str) -> PathBuf {
  let at = args
    .iter()
    .position(|arg| arg == flag)
    .unwrap_or_else(|| panic!("{flag} is not among {args:?}"));

  PathBuf::from(&args[at + 1])
}

#[tokio::test]
async fn an_endpoint_accounts_base_stands_where_the_programs_prompt_would_and_the_rules_follow() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let dsh = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/prompts/base/dsh.md")).unwrap();

  scratch
    .run(
      r#"INSERT INTO "base_prompts" ("name", "label", "text") VALUES ('dsh', 'DeepSeek Harness based', $1)"#,
      vec![Value::from(dsh.trim())],
    )
    .await;

  let ground = ground(&scratch, &home).await;
  let trees = Trees::of(&ground);
  let endpoint = account(
    "deep",
    Credential::Endpoint {
      home: home.path().join("deep"),
      url: "https://api.deepseek.com/anthropic".into(),
      key_file: home.path().join("deep.key"),
      models: EndpointModels {
        opus: Some("deepseek-v4-pro".into()),
        sonnet: Some("deepseek-flash".into()),
        haiku: Some("deepseek-flash".into()),
      },
    },
    Some("dsh"),
  );
  let run = worker_run(&ground, &trees, Some("worker"), Some(&endpoint), "claude-opus-5", "/work")
    .await
    .unwrap();

  let args = worker_args(&run, &run.settings, None).unwrap();
  let whole = handed_after(&args, "--system-prompt-file");

  assert!(!args.iter().any(|arg| arg == "--append-system-prompt-file"));
  assert_eq!(Some(&whole), run.system_prompt.as_ref());

  // The dsh base, its model named by the account's map for the run's family, then the worker's rules.
  let filled = dsh.trim().replace("{{model}}", "deepseek-v4-pro").replace("{{cwd}}", "/work");
  let rules = std::fs::read_to_string(&run.rules).unwrap();

  assert!(filled.starts_with("You are an AI agent powered by DeepSeek Harness."));
  assert_eq!(std::fs::read_to_string(&whole).unwrap(), format!("{filled}\n\n{rules}"));
}

// A worker that chooses no base on an account that names none opens on the platform's harness opening, read
// out of the tree the line is composed from, with its rules after it, handed whole in the program's own place.
#[tokio::test]
async fn a_directory_account_with_no_choice_hands_the_harness_and_the_rules_whole() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  tracer(&scratch).await;

  let ground = ground(&scratch, &home).await;
  let trees = Trees::of(&ground);
  let directory = account(
    "main",
    Credential::Directory {
      home: home.path().join("main"),
    },
    None,
  );
  let run = worker_run(&ground, &trees, Some("worker"), Some(&directory), "claude-opus-5", "/work")
    .await
    .unwrap();

  let args = worker_args(&run, &run.settings, None).unwrap();
  let harness = std::fs::read_to_string(trees.harness_path()).unwrap();
  let rules = std::fs::read_to_string(&run.rules).unwrap();

  assert!(run.system_prompt.is_none());
  assert!(!args.iter().any(|arg| arg == "--append-system-prompt-file"));
  assert_eq!(handed_after(&args, "--system-prompt-file"), run.harnessed);
  assert_eq!(
    std::fs::read_to_string(&run.harnessed).unwrap(),
    format!("{}\n\n{rules}", harness.trim())
  );
  assert!(!args.iter().any(|arg| arg == "--agents"), "no subagent definition is handed: {args:?}");
}

// A worker's prefix documents stand last in its rules file, in the list's order, each headed by its path
// as written, a relative one read from its checkout and an absolute one where it stands; one that cannot
// be read is left out (docs/worker-bench.md, 「Prefix documents」).
#[tokio::test]
async fn a_worker_s_prefix_documents_stand_last_in_its_rules() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let checkout = home.path().join("checkout");
  let umbrella = home.path().join("STRUCTURE.md");

  std::fs::create_dir_all(checkout.join("docs")).unwrap();
  std::fs::write(checkout.join("docs").join("area.md"), "# The area\n\nIts canonical statement.\n")
    .unwrap();
  std::fs::write(&umbrella, "# The structure\n").unwrap();

  let listed = serde_json::json!([umbrella.to_string_lossy(), "docs/area.md", "docs/gone.md"]).to_string();

  repository(
    scratch.db(),
    "helper",
    &checkout.to_string_lossy(),
    &[("prefix_documents", Value::from(listed))],
  )
  .await;

  let ground = ground(&scratch, &home).await;
  let trees = Trees::of(&ground);
  let cwd = checkout.to_string_lossy().into_owned();
  let run = worker_run(&ground, &trees, Some("helper"), None, "claude-opus-5", &cwd).await.unwrap();
  let rules = std::fs::read_to_string(&run.rules).unwrap();
  let expected = format!(
    "DOCUMENT {}:\n# The structure\n\nDOCUMENT docs/area.md:\n# The area\n\nIts canonical statement.",
    umbrella.display()
  );

  assert!(rules.trim_end().ends_with(&expected), "{rules}");
  assert!(!rules.contains("docs/gone.md"), "{rules}");
}

// A worker's rules file, the session's cached prefix, carries the report trailer every job is held to, its
// marks filled, after the worker's own rules and before its prefix documents; a co-work branch's carries
// none (docs/launch-lines.md, the job worker's rules file).
#[tokio::test]
async fn a_worker_s_rules_carry_the_report_trailer_before_its_documents_and_a_branch_s_carry_none() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let checkout = home.path().join("checkout");

  std::fs::create_dir_all(checkout.join("docs")).unwrap();
  std::fs::write(checkout.join("docs").join("area.md"), "# The area\n").unwrap();

  repository(
    scratch.db(),
    "helper",
    &checkout.to_string_lossy(),
    &[("prefix_documents", r#"["docs/area.md"]"#.into())],
  )
  .await;

  let ground = ground(&scratch, &home).await;
  let trees = Trees::of(&ground);
  let cwd = checkout.to_string_lossy().into_owned();
  let worker = worker_run(&ground, &trees, Some("helper"), None, "claude-opus-5", &cwd).await.unwrap();
  let rules = std::fs::read_to_string(&worker.rules).unwrap();
  let opening = "Rules a job needs only in some situations stand in the worker's own skills";
  let trailer_at = rules.find(opening).unwrap_or_else(|| panic!("no trailer in {rules}"));

  assert!(trailer_at < rules.find("DOCUMENT docs/area.md:").unwrap(), "{rules}");
  assert!(!rules.contains("{{"), "every mark of the trailer is filled: {rules}");
  // A job's result for a person is written in the language its brief names (docs/worker-language.md).
  assert!(
    rules.contains("A result written for a person is written in the language of the person who asked"),
    "{rules}"
  );
  assert!(
    rules.contains("in the language the item's brief names for the person who asked"),
    "{rules}"
  );
  assert!(rules.contains("plain-writing.md"), "{rules}");

  let branch = worker_run(&ground, &trees, None, None, "claude-opus-5", &cwd).await.unwrap();

  assert!(!std::fs::read_to_string(&branch.rules).unwrap().contains(opening));
}

// A role's run opens on the platform's harness and the role's own prompt, whatever the provider, and where it
// works in a repository's checkout carries the repository's own rules, found by the prompt folder its row
// names rather than by its name (docs/repositories.md, 「What a run in a repository is handed」).
#[tokio::test]
async fn a_role_s_run_opens_on_its_role_s_prompt_and_reads_its_repository_s_rules_by_the_row_s_folder() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let checkout = home.path().join("checkout");
  let code = home.path().join("code");

  std::fs::create_dir_all(&checkout).unwrap();
  std::fs::create_dir_all(&code).unwrap();
  // A code tree of the test's own: the prompts as they stand, and a prompt folder holding a repository's rules.
  let copied = std::process::Command::new("cp")
    .args(["-r", &format!("{}/prompts", env!("CARGO_MANIFEST_DIR")), &code.to_string_lossy()])
    .status()
    .unwrap();

  assert!(copied.success());
  std::fs::create_dir_all(code.join("prompts/agents/kept")).unwrap();
  std::fs::write(code.join("prompts/agents/kept/own-rules.md"), "KEEP THE TREE TIDY.\n").unwrap();
  scratch
    .run(
      r#"INSERT INTO "roles" ("name", "label", "bundles", "max_concurrent", "prompt", "position", "created_at",
           "updated_at", "line", "provider_order")
         VALUES ('making', 'Making', '[]', 1, 'MAKE IT WELL.', 0, 1, 1, 'worker',
           '[{"provider":"deepseek","model":"deepseek-flash"}]')"#,
      vec![],
    )
    .await;
  crate::support::rows::reseated(scratch.db()).await;
  repository(scratch.db(), "repo-a", &checkout.to_string_lossy(), &[("prompts", "kept".into())]).await;
  repository(scratch.db(), "kept", &home.path().join("other").to_string_lossy(), &[]).await;

  let ground = ground(&scratch, &home).await;
  let trees = Trees {
    code,
    ..Trees::of(&ground)
  };
  let cwd = checkout.to_string_lossy().into_owned();
  let run = role_run(&ground, &trees, "making", Some("repo-a"), None, "deepseek-flash", &cwd, "deepseek")
    .await
    .unwrap();
  let whole = std::fs::read_to_string(&run.harnessed).unwrap();

  assert!(whole.contains("MAKE IT WELL."), "{whole}");
  assert!(
    whole.find("MAKE IT WELL.") < whole.find("KEEP THE TREE TIDY."),
    "the role's prompt stands before the repository's rules: {whole}"
  );
  assert!(std::fs::read_to_string(&run.rules).unwrap().contains("KEEP THE TREE TIDY."));

  // A repository named like the folder but naming none on its row hands no rules of its own.
  let bare = role_run(&ground, &trees, "making", Some("kept"), None, "deepseek-flash", &cwd, "deepseek")
    .await
    .unwrap();

  assert!(!std::fs::read_to_string(&bare.rules).unwrap().contains("KEEP THE TREE TIDY."));
}

// A role's provider prompt stands after its prompt on a run whose program stands on that provider and on no
// other (docs/agent-roles.md, 「A role's provider prompts」), and a worker standing on the registry sends every
// model slot of its run, its subagents' among them, to its own registration (docs/own-llm.md, 「How requests
// go」).
#[tokio::test]
async fn a_role_s_provider_prompt_reaches_its_runs_on_that_provider_alone() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  tracer(&scratch).await;

  for sql in [
    r#"INSERT INTO "registered_models" ("name", "label", "model", "context_tokens", "answer_tokens",
          "thinking", "walked", "added_at", "keeps_research")
       VALUES ('run-registration', 'Run', 'served', 200000, 8000, 1, 1, 1, 0)"#,
    r#"INSERT INTO "registered_model_addresses" ("registration", "node", "position", "url", "key_file", "key_owned")
       VALUES ('run-registration', 'n1', 1, 'http://[::1]:9/v1/chat/completions', NULL, 0)"#,
  ] {
    scratch.run(sql, vec![]).await;
  }

  let ground = ground(&scratch, &home).await;
  let trees = Trees::of(&ground);
  let on_claude =
    role_run(&ground, &trees, "tracer", None, None, "claude-sonnet-5", "/work", "claude-code")
      .await
      .unwrap();
  let on_deepseek = role_run(&ground, &trees, "tracer", None, None, "deepseek-flash", "/work", "deepseek")
    .await
    .unwrap();
  let claude_prompt = std::fs::read_to_string(&on_claude.harnessed).unwrap();
  let deepseek_prompt = std::fs::read_to_string(&on_deepseek.harnessed).unwrap();

  assert!(
    claude_prompt.contains("Trace the road.\n\nOpen your subagents as you judge."),
    "{claude_prompt}"
  );
  assert!(deepseek_prompt.contains("Trace the road."), "{deepseek_prompt}");
  assert!(!deepseek_prompt.contains("Open your subagents"), "{deepseek_prompt}");

  let standing =
    registered_program(&ground, &ground.machine(), "worker", "/work", Some("run-registration"))
      .await
      .unwrap();
  let command = standing.program.command;

  assert!(!command.args.iter().any(|arg| arg == "--agents"), "{:?}", command.args);

  for slot in [
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "CLAUDE_CODE_SUBAGENT_MODEL",
  ] {
    assert_eq!(command.env.get(slot).map(String::as_str), Some("run-registration"), "{slot}");
  }
}

// A job's turn and its standing program on Claude Code go out on the model its repository's pin names there
// (docs/repositories.md, 「A pinned model」), a support checkout's on its repository's, and an unpinned
// repository's on the job role's own entry; a pin to Claude Code that the owner turned off reads as none.
#[tokio::test]
async fn a_worker_s_claude_run_goes_out_on_the_model_its_pin_names() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, &home).await;

  store::seed::settle(scratch.db(), &agency_server::seeds::deployment(&ground.machine()))
    .await
    .unwrap();
  crate::support::rows::seated_on_order(scratch.db(), "worker").await;
  repository(
    scratch.db(),
    "helper",
    "/checkouts/helper",
    &[
      ("support_count", 1i64.into()),
      ("pinned_provider", "claude-code".into()),
      ("pinned_model", "claude-fable-5-1".into()),
    ],
  )
  .await;
  repository(scratch.db(), "plain", "/checkouts/plain", &[]).await;

  let on = |enabled: bool| {
    vec![
      OrderEntry {
        name: "claude-code".to_owned(),
        enabled,
      },
      OrderEntry {
        name: "registered".to_owned(),
        enabled: true,
      },
    ]
  };

  set_provider_order(scratch.db(), &on(true)).await.unwrap();

  let trees = Trees::of(&ground);
  let machine = ground.machine();
  let mapped = agency_server::order::role_entry(scratch.db(), "worker", "claude-code").await.unwrap();

  assert_ne!(mapped.model, "claude-fable-5-1");

  for (worker, model) in [
    ("helper", "claude-fable-5-1"),
    ("helper-w2", "claude-fable-5-1"),
    ("plain", mapped.model.as_str()),
  ] {
    let run = worker_run(&ground, &trees, Some(worker), None, model, "/work").await.unwrap();
    let laid = configured_pinned(&ground, &machine, run, worker, "claude-code").await.unwrap();

    assert_eq!(laid.model, model, "{worker}");
  }

  set_provider_order(scratch.db(), &on(false)).await.unwrap();

  assert_eq!(pinned_model_on(scratch.db(), "helper", "claude-code").await.unwrap(), None);
}

// A run on the worker line on a registration that names the languages it writes carries the result-language
// piece, filled with them, right after the standing rules, on a role's run and on a pinned repository's standing
// program alike; a registration naming none and every other road carry none (docs/own-llm.md, 「A run on the
// worker line is composed as on Claude」).
#[tokio::test]
async fn a_run_on_a_registration_naming_its_languages_is_told_them_after_the_standing_rules() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  tracer(&scratch).await;

  for sql in [
    r#"INSERT INTO "registered_models" ("name", "label", "model", "context_tokens", "answer_tokens",
          "thinking", "walked", "added_at", "keeps_research", "languages")
       VALUES ('narrow', 'Narrow', 'served', 200000, 8000, 1, 1, 1, 0, '["English","Japanese"]'),
              ('wide', 'Wide', 'served', 200000, 8000, 1, 1, 2, 0, NULL)"#,
    r#"INSERT INTO "registered_model_addresses" ("registration", "node", "position", "url", "key_file", "key_owned")
       VALUES ('narrow', 'n1', 1, 'http://[::1]:9/v1/chat/completions', NULL, 0)"#,
  ] {
    scratch.run(sql, vec![]).await;
  }

  let ground = ground(&scratch, &home).await;
  let trees = Trees::of(&ground);
  let standing = std::fs::read_to_string(trees.agent_dir("worker").join("standing-rules.md")).unwrap();
  let run = |model: &'static str, provider: &'static str| {
    let (ground, trees) = (&ground, &trees);

    async move { role_run(ground, trees, "tracer", None, None, model, "/work", provider).await.unwrap() }
  };
  let narrow = run("narrow", "registered").await;
  let rules = std::fs::read_to_string(&narrow.rules).unwrap();
  let piece = "# The languages this run writes its results in\n\nThe model this run stands on writes a result \
               for a person well in English and Japanese alone. Where the brief\nnames a result language outside \
               English and Japanese, write every result for a person in English instead";

  assert!(rules.starts_with(&format!("{}\n\n{piece}", standing.trim_end())), "{rules}");
  assert!(
    rules.contains("Where the brief names one of English and Japanese, or names no language"),
    "{rules}"
  );
  assert!(rules.contains("\n\nRULEBOOK prompts/rules/self-preservation.md"), "{rules}");
  assert!(!rules.contains("{{"), "{rules}");
  assert!(std::fs::read_to_string(&narrow.harnessed).unwrap().contains(piece));

  // A registration naming no languages, and Claude Code's own road, carry no piece, and share no rules file
  // with a run that does.
  for other in [run("wide", "registered").await, run("claude-sonnet-5", "claude-code").await] {
    let rules = std::fs::read_to_string(&other.rules).unwrap();

    assert!(!rules.contains("The languages this run writes its results in"), "{rules}");
    assert_ne!(other.rules, narrow.rules);
  }

  // A pinned repository's standing program on the registration is composed on it, so its whole prompt carries
  // the piece from its first turn.
  let standing_program = registered_program(&ground, &ground.machine(), "worker", "/work", Some("narrow"))
    .await
    .unwrap();
  let args = standing_program.program.command.args;
  let prompt = args
    .iter()
    .position(|arg| arg == "--system-prompt-file")
    .map(|at| &args[at + 1])
    .unwrap();

  assert!(std::fs::read_to_string(prompt).unwrap().contains(piece), "{args:?}");
}
