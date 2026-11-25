use std::collections::BTreeMap;

use agency_server::door::apart::*;
use agency_server::door::turn::Ending;
use agency_server::job_run::claude::failure::Failure;
use agency_server::lines::claude::Command;

#[test]
fn the_pane_is_handed_what_the_line_set_and_none_of_the_launchers_own_environment() {
  let process = BTreeMap::from([
    ("PATH".to_owned(), "/bin".to_owned()),
    ("HOME".to_owned(), "/home/tester".to_owned()),
    ("ANTHROPIC_BASE_URL".to_owned(), "https://elsewhere".to_owned()),
  ]);
  let mut env: BTreeMap<String, Option<String>> =
    process.iter().map(|(name, value)| (name.clone(), Some(value.clone()))).collect();

  env.insert("ANTHROPIC_BASE_URL".to_owned(), Some("http://127.0.0.1:3291".to_owned()));
  env.insert("MAX_THINKING_TOKENS".to_owned(), Some("0".to_owned()));
  env.insert("BMO_TURN_CHAT".to_owned(), Some("-7".to_owned()));
  env.insert("CLAUDE_CODE_OAUTH_TOKEN".to_owned(), None);

  let command = Command {
    args: vec![],
    cwd: "/w".into(),
    env,
  };
  let pane = pane_environment(&process, &command, "search-1234abcd");

  assert_eq!(pane["ANTHROPIC_BASE_URL"], "http://127.0.0.1:3291");
  assert_eq!(pane["MAX_THINKING_TOKENS"], "0");
  assert_eq!(pane["BMO_TURN_CHAT"], "-7");
  assert_eq!(pane["BMO_RUN_NAME"], "search-1234abcd");
  assert!(!pane.contains_key("HOME"));
  assert!(!pane.contains_key("CLAUDE_CODE_OAUTH_TOKEN"));
}

#[test]
fn a_full_window_in_the_programs_own_error_is_the_walks_limit_and_a_stop_is_a_stop() {
  let limit = answered(
    Ending::Errored {
      error: "Claude AI usage limit reached|1791430000".to_owned(),
      kind: Some("rate_limit".to_owned()),
    },
    "search",
  );

  assert!(matches!(
    limit,
    Err(Failure::Limit {
      resets_at: Some(1_791_430_000),
      ..
    })
  ));
  assert_eq!(
    answered(
      Ending::Stopped {
        screen: String::new()
      },
      "search"
    ),
    Err(Failure::Stopped)
  );
}
