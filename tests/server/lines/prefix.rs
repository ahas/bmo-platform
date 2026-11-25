//! A session's prefix: what it is read out of, and which committed paths move it.

use std::path::PathBuf;

use agency_server::lines::prefix::*;

#[test]
fn a_commit_moves_the_prefix_through_a_memory_file_a_worker_s_own_rules_or_a_prompt_shaping_folder() {
  let paths = [
    "AGENTS.md",
    "server/bot.ts",
    "prompts/harness.md",
    "prompts/agents/pieces/own-rules.md",
    "prompts/agents/worker/plugin/skills/x/SKILL.md",
    "prompts/agents/worker/trailer.md",
    "prompts/agents/worker/request.md",
  ]
  .map(str::to_owned);

  // The report trailer stands in the worker's rules file, so it moves the prefix like the rules do; the
  // platform's harness opening opens the worker's whole prompt.
  assert_eq!(
    touches_prefix(&paths),
    [
      "AGENTS.md",
      "prompts/harness.md",
      "prompts/agents/pieces/own-rules.md",
      "prompts/agents/worker/plugin/skills/x/SKILL.md",
      "prompts/agents/worker/trailer.md"
    ]
    .map(str::to_owned)
  );
}

#[test]
fn a_memory_file_that_only_points_brings_in_what_it_points_at() {
  let home = tempfile::tempdir().unwrap();
  let cwd = home.path().join("repo");

  std::fs::create_dir_all(&cwd).unwrap();
  std::fs::write(cwd.join("CLAUDE.md"), "@AGENTS.md\n").unwrap();
  std::fs::write(cwd.join("AGENTS.md"), "@gone.md\nthe rules\n").unwrap();

  let settings = home.path().join("settings.json");

  std::fs::write(&settings, format!(r#"{{"claudeMdExcludes":["{}/*/CLAUDE.md"]}}"#, "/nowhere")).unwrap();

  let none = PathBuf::from("/none");
  let reading = PrefixReading {
    cwd: &cwd,
    args: &["claude".to_owned(), format!("{}/x", cwd.display())],
    code_root: &none,
    source_root: &cwd,
    data_root: &none,
    home: home.path(),
    plugin_dir: &none,
    settings: &settings,
    rules: &none,
    mcp_config: &none,
  };
  let sources = prefix_sources(&reading);
  let keys: Vec<&str> = sources.iter().map(|(key, _)| key.as_str()).collect();

  assert_eq!(sources[0].1, "claude <source>/x");
  assert_eq!(
    keys,
    [
      "args",
      "memory:repo/CLAUDE.md",
      "memory:repo/AGENTS.md",
      "memory:repo/gone.md",
      "settings"
    ]
  );
  assert_eq!(sources[3].1, UNRESOLVED_IMPORT);
  assert_eq!(front_matter("---\nname: a\n---\nbody"), "name: a");
  assert_eq!(front_matter("no front"), "no front");
}

#[test]
fn a_line_read_out_of_another_release_has_the_same_word() {
  let home = tempfile::tempdir().unwrap();
  let none = PathBuf::from("/none");
  let word = |release: &str| {
    let code = home.path().join("deploy-releases").join(release);
    let plugin = code.join("prompts/agents/worker/plugin");

    prefix_digest(&PrefixReading {
      cwd: &none,
      args: &["claude".to_owned(), "--plugin-dir".to_owned(), plugin.to_string_lossy().into_owned()],
      code_root: &code,
      source_root: &none,
      data_root: &none,
      home: home.path(),
      plugin_dir: &plugin,
      settings: &none,
      rules: &none,
      mcp_config: &none,
    })
  };

  assert_eq!(word("0123456789ab"), word("ba9876543210"));
}
