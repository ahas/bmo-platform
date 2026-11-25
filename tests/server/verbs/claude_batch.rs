use std::collections::BTreeMap;
use std::path::Path;

use agency_server::door::record::{ModelSpend, Reply};
use agency_server::door::turn::Ending;
use agency_server::verbs::claude_batch::*;
use harness_providers::claude_code::ClaudeCode;
use serde_json::{Value, json};

#[test]
fn a_batch_opens_its_own_session_bare_of_the_machine_and_never_in_print_mode() {
  let args = batch_args("claude-sonnet-5", "high", Path::new("/tmp/system.md"), "s-1");

  assert_eq!(args[0], ClaudeCode::PROGRAM);
  assert!(!args.iter().any(|arg| arg == "-p" || arg == "--print" || arg == "--output-format"));
  assert!(args.windows(2).any(|pair| pair == ["--tools", ""]));
  assert!(args.windows(2).any(|pair| pair == ["--system-prompt-file", "/tmp/system.md"]));
  assert!(args.windows(2).any(|pair| pair == ["--session-id", "s-1"]));
  assert!(args.contains(&"--safe-mode".to_owned()));
  assert!(args.contains(&"--strict-mcp-config".to_owned()));
}

#[test]
fn a_batch_reads_no_memory_file_so_it_never_stops_at_the_external_imports_question() {
  let args = batch_args("claude-sonnet-5", "high", Path::new("/tmp/system.md"), "s-1");

  assert!(args.windows(2).any(|pair| pair == ["--setting-sources", ""]));
}

#[test]
fn a_batch_is_pinned_to_the_login_named_and_no_other() {
  let process = BTreeMap::from([
    ("PATH".to_owned(), "/bin".to_owned()),
    ("ANTHROPIC_API_KEY".to_owned(), "key".to_owned()),
    ("CLAUDE_CONFIG_DIR".to_owned(), "/home/tester/.claude-other".to_owned()),
  ]);
  let env = batch_env(&process, Path::new("/home/tester/.claude"));

  assert_eq!(env["CLAUDE_CONFIG_DIR"].as_deref(), Some("/home/tester/.claude"));
  assert_eq!(env["PATH"].as_deref(), Some("/bin"));

  for variable in OTHER_LOGINS {
    assert_eq!(env[variable], None);
  }
}

#[test]
fn an_answer_prints_its_text_and_each_models_tokens() {
  let reply = Reply {
    text: "answer".to_owned(),
    spend: vec![ModelSpend {
      mode: String::new(),
      model: "claude-sonnet-5".to_owned(),
      tokens: 30,
      input_tokens: 10,
      output_tokens: 20,
      cache_read_tokens: 3,
      cache_write_tokens: 4,
      cost: 0.0,
    }],
    ..Reply::default()
  };
  let read: Value = serde_json::from_str(&printed(&reply)).unwrap_or_default();

  assert_eq!(
    read,
    json!({
      "text": "answer",
      "model": null,
      "spend": [{
        "model": "claude-sonnet-5",
        "inputTokens": 10,
        "outputTokens": 20,
        "cacheReadTokens": 3,
        "cacheWriteTokens": 4,
      }],
    })
  );
}

#[test]
fn a_spent_limit_says_so_beside_the_programs_own_words() {
  let failed = reply_of(Ending::Errored {
    error: "Claude AI usage limit reached|1791430000".to_owned(),
    kind: Some("rate_limit".to_owned()),
  });

  assert_eq!(
    failed,
    Err("claude usage limit reached — Claude AI usage limit reached|1791430000".to_owned())
  );
}
