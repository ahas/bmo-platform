use std::collections::BTreeMap;
use std::path::Path;

use agency_server::control::claude_accounts::{Account, Credential, EndpointModels};
use agency_server::lines::claude::*;

fn account(id: &str, credential: Credential) -> Account {
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
    base_prompt: None,
    node_token_gone: None,
  }
}

#[test]
fn a_directory_login_clears_any_token_and_a_token_login_hands_its_token() {
  let dir = tempfile::tempdir().unwrap();
  let token = dir.path().join("token");

  std::fs::write(&token, "sk-test\n").unwrap();

  let directory = |home: &Path| account("a", Credential::Directory { home: home.into() });
  let tokened = |file: &Path| account("a", Credential::Token { file: file.into() });
  let directory = account_env(Some(&directory(dir.path())), &Runs::OwnLogin).unwrap();
  let handed = account_env(Some(&tokened(&token)), &Runs::OwnLogin).unwrap();

  assert_eq!(directory["CLAUDE_CODE_OAUTH_TOKEN"], None);
  assert_eq!(directory["CLAUDE_CONFIG_DIR"].as_deref(), dir.path().to_str());
  assert_eq!(handed["CLAUDE_CODE_OAUTH_TOKEN"].as_deref(), Some("sk-test"));
  assert_eq!(handed.len(), 1);
  assert!(account_env(Some(&tokened(&dir.path().join("gone"))), &Runs::OwnLogin).is_err());
}

#[test]
fn an_endpoint_login_hands_its_home_its_url_its_key_and_its_models_and_clears_every_other_credential() {
  let dir = tempfile::tempdir().unwrap();
  let key = dir.path().join("endpoint.key");
  let endpoint = |key_file: &Path| {
    account(
      "endpoint",
      Credential::Endpoint {
        home: dir.path().join("home"),
        url: "https://endpoint.test/anthropic".into(),
        key_file: key_file.into(),
        models: EndpointModels {
          opus: Some("big-model".into()),
          sonnet: Some("small-model".into()),
          haiku: None,
        },
      },
    )
  };

  std::fs::write(&key, "sk-endpoint\n").unwrap();

  let env = account_env(Some(&endpoint(&key)), &Runs::OwnLogin).unwrap();

  assert_eq!(env["CLAUDE_CONFIG_DIR"].as_deref(), dir.path().join("home").to_str());
  assert_eq!(env["ANTHROPIC_BASE_URL"].as_deref(), Some("https://endpoint.test/anthropic"));
  assert_eq!(env["ANTHROPIC_AUTH_TOKEN"].as_deref(), Some("sk-endpoint"));
  assert_eq!(env["ANTHROPIC_DEFAULT_OPUS_MODEL"].as_deref(), Some("big-model"));
  assert_eq!(env["ANTHROPIC_DEFAULT_SONNET_MODEL"].as_deref(), Some("small-model"));
  assert_eq!(env["ANTHROPIC_DEFAULT_HAIKU_MODEL"], None);
  assert_eq!(env["ANTHROPIC_API_KEY"], None);
  assert_eq!(env["CLAUDE_CODE_OAUTH_TOKEN"], None);

  // A key that cannot be read is refused, and so is a run from a work node.
  std::fs::write(dir.path().join("empty.key"), "\n").unwrap();
  assert!(account_env(Some(&endpoint(&dir.path().join("empty.key"))), &Runs::OwnLogin).is_err());
  assert!(account_env(Some(&endpoint(&dir.path().join("gone.key"))), &Runs::OwnLogin).is_err());

  let node = Runs::OnNode {
    data_root: dir.path().into(),
  };

  assert!(account_env(Some(&endpoint(&key)), &node).is_err());

  // Over the launching process's own, the endpoint's credentials are what the program is handed.
  let process = BTreeMap::from([
    ("ANTHROPIC_API_KEY".to_owned(), "sk-process".to_owned()),
    ("CLAUDE_CODE_OAUTH_TOKEN".to_owned(), "sk-ant-oat01-process".to_owned()),
  ]);
  let program = command_env(&process, Some(&endpoint(&key)), &Runs::OwnLogin, true).unwrap();

  assert_eq!(program["ANTHROPIC_API_KEY"], None);
  assert_eq!(program["CLAUDE_CODE_OAUTH_TOKEN"], None);
  assert_eq!(program["ANTHROPIC_AUTH_TOKEN"].as_deref(), Some("sk-endpoint"));
}

#[test]
fn a_pane_on_an_endpoint_account_is_handed_its_url_its_key_and_its_models_by_name() {
  let dir = tempfile::tempdir().unwrap();
  let key = dir.path().join("endpoint.key");

  std::fs::write(&key, "sk-endpoint\n").unwrap();

  let endpoint = account(
    "endpoint",
    Credential::Endpoint {
      home: dir.path().join("home"),
      url: "https://endpoint.test/anthropic".into(),
      key_file: key.clone(),
      models: EndpointModels {
        opus: Some("big-model".into()),
        sonnet: None,
        haiku: Some("tiny-model".into()),
      },
    },
  );
  let process = BTreeMap::from([("PATH".to_owned(), "/bin".to_owned())]);
  let command = Command {
    args: vec![],
    cwd: "/w".into(),
    env: command_env(&process, Some(&endpoint), &Runs::OwnLogin, true).unwrap(),
  };
  let env = line_environment(&command, "bmo-standby-worker", Some(&endpoint));

  assert_eq!(env["CLAUDE_CONFIG_DIR"], dir.path().join("home").to_string_lossy());
  assert_eq!(env["ANTHROPIC_BASE_URL"], "https://endpoint.test/anthropic");
  assert_eq!(env["ANTHROPIC_AUTH_TOKEN"], "sk-endpoint");
  assert_eq!(env["ANTHROPIC_DEFAULT_OPUS_MODEL"], "big-model");
  assert_eq!(env["ANTHROPIC_DEFAULT_HAIKU_MODEL"], "tiny-model");
  assert!(!env.contains_key("ANTHROPIC_DEFAULT_SONNET_MODEL"));
  assert!(!env.contains_key("ANTHROPIC_API_KEY"));
  assert!(!env.contains_key("CLAUDE_CODE_OAUTH_TOKEN"));
}

#[test]
fn a_directory_login_on_a_node_is_handed_its_token_for_the_nodes_beside_its_directory() {
  let data = tempfile::tempdir().unwrap();
  let login = tempfile::tempdir().unwrap();
  let account = account(
    "spare",
    Credential::Directory {
      home: login.path().into(),
    },
  );
  let place = data.path().join("agent-places").join("worker");
  let runs = || Runs::of_place(data.path(), &place);

  std::fs::create_dir_all(&place).unwrap();
  assert_eq!(
    runs(),
    Runs::Here {
      data_root: data.path().into()
    }
  );
  agency_server::watches::links::mark_place_node(&place, Some("node-a")).unwrap();

  // No token made for the nodes yet: the directory alone, the node's own login in it.
  let bare = account_env(Some(&account), &runs()).unwrap();

  assert_eq!(bare["CLAUDE_CODE_OAUTH_TOKEN"], None);

  let file = account.node_token_file(data.path()).unwrap();

  std::fs::create_dir_all(file.parent().unwrap()).unwrap();
  std::fs::write(&file, "sk-ant-oat01-node\n").unwrap();

  let on_node = account_env(Some(&account), &runs()).unwrap();
  let here = account_env(
    Some(&account),
    &Runs::Here {
      data_root: data.path().into(),
    },
  )
  .unwrap();

  assert_eq!(on_node["CLAUDE_CODE_OAUTH_TOKEN"].as_deref(), Some("sk-ant-oat01-node"));
  assert_eq!(on_node["CLAUDE_CONFIG_DIR"].as_deref(), login.path().to_str());
  assert_eq!(here["CLAUDE_CODE_OAUTH_TOKEN"], None);
}

#[test]
fn the_pane_is_handed_the_lines_variables_by_name_and_never_the_launchers_whole_environment() {
  let process = BTreeMap::from([
    ("PATH".to_owned(), "/bin".to_owned()),
    ("BMO_DATA_DIR".to_owned(), "/data".to_owned()),
    ("BMO_DEPLOY_DIR".to_owned(), "/srv/deploy-releases/0123456789ab".to_owned()),
    ("BMO_TASK_LOCKED".to_owned(), String::new()),
    ("CLAUDECODE".to_owned(), "1".to_owned()),
    ("HOME".to_owned(), "/fixture/home".to_owned()),
  ]);
  let command = Command {
    args: vec![],
    cwd: "/w".into(),
    env: command_env(&process, None, &Runs::OwnLogin, false).unwrap(),
  };
  let env = line_environment(&command, "bmo-standby-worker", None);

  assert_eq!(env["PATH"], "/bin");
  assert_eq!(env["BMO_DATA_DIR"], "/data");
  // The job's release reaches the pane, so its hooks and the scripts it calls read the same tree.
  assert_eq!(env["BMO_DEPLOY_DIR"], "/srv/deploy-releases/0123456789ab");
  assert_eq!(env["BMO_RUN_NAME"], "bmo-standby-worker");
  assert_eq!(env["CLAUDE_CODE_DISABLE_GIT_INSTRUCTIONS"], "1");
  assert_eq!(env["CLAUDE_CODE_DISABLE_AUTO_MEMORY"], "1", "the CLI's memory reaches no run");
  assert_eq!(env["CLAUDE_CODE_DISABLE_BUNDLED_SKILLS"], "1", "nor the skills the CLI bundles");
  assert!(!env.contains_key("CLAUDECODE"));
  assert!(!env.contains_key("HOME"));
  assert!(!env.contains_key("BMO_TASK_LOCKED"));
  assert!(!env.contains_key("MAX_THINKING_TOKENS"));
}

#[test]
fn an_endpoint_launch_names_the_model_in_the_environment_and_refuses_an_unknown_one() {
  let dir = tempfile::tempdir().unwrap();
  let key = dir.path().join("gateway.key");
  std::fs::write(&key, "sk-gateway\n").unwrap();
  let gateway = account(
    "gateway-members",
    Credential::Endpoint {
      home: dir.path().join("home"),
      url: "http://127.0.0.1:18030".into(),
      key_file: key,
      models: EndpointModels {
        opus: Some("deepseek-flash".into()),
        sonnet: Some("openrouter-deepseek-flash".into()),
        haiku: None,
      },
    },
  );
  let command = endpoint_launch(&gateway, "deepseek-flash", "high").unwrap();

  assert_eq!(command.args, ["claude", "--effort", "high", "--dangerously-skip-permissions"]);
  assert_eq!(command.env["ANTHROPIC_BASE_URL"].as_deref(), Some("http://127.0.0.1:18030"));
  assert_eq!(command.env["ANTHROPIC_AUTH_TOKEN"].as_deref(), Some("sk-gateway"));
  // The CLI checks `--model` against the names it ships with, so the endpoint's own id travels in the
  // environment and the family map is read only to refuse an id it does not hold.
  assert_eq!(command.env["ANTHROPIC_MODEL"].as_deref(), Some("deepseek-flash"));
  assert_eq!(command.env["ANTHROPIC_DEFAULT_OPUS_MODEL"].as_deref(), Some("deepseek-flash"));
  assert_eq!(
    command.env["ANTHROPIC_DEFAULT_SONNET_MODEL"].as_deref(),
    Some("openrouter-deepseek-flash")
  );
  assert_eq!(command.cwd, dir.path().join("home").to_string_lossy());

  let unknown = endpoint_launch(&gateway, "not-in-the-map", "high").unwrap_err();
  assert!(unknown.detail.contains("maps no family"), "{}", unknown.detail);
  let directory = account(
    "a-directory",
    Credential::Directory {
      home: dir.path().into(),
    },
  );
  assert!(endpoint_launch(&directory, "deepseek-flash", "high").is_err());
}

// A subscription login's credential file whose access token ends at `access` and whose refresh stops being
// taken at `refresh`, both in milliseconds.
fn subscription(access: i64, refresh: i64) -> String {
  serde_json::json!({
    "claudeAiOauth": {
      "accessToken": "a",
      "refreshToken": "r",
      "expiresAt": access,
      "refreshTokenExpiresAt": refresh,
    }
  })
  .to_string()
}

// A directory account runs here on its token for the work nodes, beside its configuration directory, at each of
// the three moments its login here cannot be counted on — past its end, marked as waiting for a person, inside
// the policy's span before its end — and on its login alone otherwise: while its login stands, where it holds
// no token, where that token is marked gone, for the run that renews the login, and once it is signed in again.
#[test]
fn a_directory_account_carries_on_by_its_token_where_its_login_here_cannot_be_counted_on() {
  const HOUR: i64 = 3_600_000;
  let data = tempfile::tempdir().unwrap();
  let home = tempfile::tempdir().unwrap();
  let credentials = home.path().join(".credentials.json");
  let now = agency_server::now_ms();
  let mut held = account(
    "spare",
    Credential::Directory {
      home: home.path().into(),
    },
  );
  let here = Runs::Here {
    data_root: data.path().into(),
  };
  let handed = |account: &Account, runs: &Runs| {
    let env = account_env(Some(account), runs).unwrap();

    assert_eq!(env["CLAUDE_CONFIG_DIR"].as_deref(), home.path().to_str(), "the directory stays handed");
    env["CLAUDE_CODE_OAUTH_TOKEN"].clone()
  };

  // Its login stands: a month of refresh ahead.
  std::fs::write(&credentials, subscription(now + HOUR, now + 720 * HOUR)).unwrap();
  assert_eq!(handed(&held, &here), None);

  // Its login has ended, but this machine holds no token for the nodes: nothing to carry on by.
  std::fs::write(&credentials, subscription(now - HOUR, now - HOUR)).unwrap();
  assert_eq!(handed(&held, &here), None);

  let token = held.node_token_file(data.path()).unwrap();

  std::fs::create_dir_all(token.parent().unwrap()).unwrap();
  std::fs::write(&token, "sk-ant-oat01-node\n").unwrap();

  // Past its end.
  assert_eq!(handed(&held, &here).as_deref(), Some("sk-ant-oat01-node"));
  // The run that renews the login is never handed the token.
  assert_eq!(handed(&held, &Runs::OwnLogin), None);

  // Inside the policy's span before its end, and not before it.
  let span = policy::providers::claude_code::CLAUDE_CODE.login.hand_over_ahead_hours * HOUR;

  std::fs::write(&credentials, subscription(now + HOUR, now + span - HOUR)).unwrap();
  assert_eq!(handed(&held, &here).as_deref(), Some("sk-ant-oat01-node"));
  std::fs::write(&credentials, subscription(now + HOUR, now + span + HOUR)).unwrap();
  assert_eq!(handed(&held, &here), None);

  // Marked as waiting for a person, its file's moments still ahead.
  held.needs_login_at = Some(now / 1000);
  assert_eq!(handed(&held, &here).as_deref(), Some("sk-ant-oat01-node"));

  // A token marked gone answers for nothing, and one made since stands clear of the mark.
  held.node_token_gone = Some(held.node_token_stamp(data.path()));
  assert_eq!(handed(&held, &here), None);
  held.node_token_gone = Some("1".into());
  assert_eq!(handed(&held, &here).as_deref(), Some("sk-ant-oat01-node"));

  // While it carries on by its token, a credential that merely still states a moment ahead lifts no mark
  // made after it was written; one written after the mark, by a person signing in again, does.
  held.needs_login_at = Some(now / 1000 + 5);
  assert!(held.login_back(now));
  assert!(!held.login_back_here(data.path(), now));
  held.needs_login_at = Some(now / 1000 - 5);
  std::fs::write(&credentials, subscription(now + HOUR, now + 720 * HOUR)).unwrap();
  assert!(held.login_back_here(data.path(), now));

  // Signed in again: the judgement lifts the mark off that credential, and the login is handed alone.
  let held = agency_server::control::claude_accounts::judged(held, data.path(), now);

  assert_eq!(held.needs_login_at, None);
  assert_eq!(handed(&held, &here), None);

  // A token account's own token is its login here, never a token for the nodes beside a directory.
  let tokened = account(
    "tokened",
    Credential::Token {
      file: token.clone(),
    },
  );

  assert_eq!(tokened.carrying_token(data.path(), now), None);
}

// A subscription login's end is the moment its file states its refresh stops being taken, and a file that
// states none has no end this side can read.
#[test]
fn a_subscription_login_ends_when_its_refresh_stops_being_taken() {
  let home = tempfile::tempdir().unwrap();
  let held = account(
    "spare",
    Credential::Directory {
      home: home.path().into(),
    },
  );
  let credentials = home.path().join(".credentials.json");

  std::fs::write(&credentials, subscription(1_900_000_000_000, 1_950_000_000_000)).unwrap();
  assert_eq!(agency_server::control::claude_accounts::login_end(&held), Some(1_950_000_000_000.0));

  std::fs::write(
    &credentials,
    serde_json::json!({ "claudeAiOauth": { "refreshToken": "r", "expiresAt": 1_900_000_000_000_i64 } })
      .to_string(),
  )
  .unwrap();
  assert_eq!(agency_server::control::claude_accounts::login_end(&held), None);
}

#[test]
fn an_api_key_login_hands_its_home_and_its_key_and_clears_every_other_credential() {
  let dir = tempfile::tempdir().unwrap();
  let key = dir.path().join("anthropic");

  std::fs::write(&key, "  sk-ant-api-test\n").unwrap();

  let keyed = Credential::ApiKey {
    home: dir.path().join("home"),
    key_file: key.clone(),
  };
  let platform = account("keyed", keyed.clone());
  let process = BTreeMap::from([
    ("ANTHROPIC_AUTH_TOKEN".to_owned(), "sk-process".to_owned()),
    ("CLAUDE_CODE_OAUTH_TOKEN".to_owned(), "sk-ant-oat01-process".to_owned()),
  ]);
  let env = command_env(&process, Some(&platform), &Runs::OwnLogin, true).unwrap();

  assert_eq!(env["CLAUDE_CONFIG_DIR"].as_deref(), dir.path().join("home").to_str());
  assert_eq!(env["ANTHROPIC_API_KEY"].as_deref(), Some("sk-ant-api-test"));
  assert_eq!(env["ANTHROPIC_AUTH_TOKEN"], None);
  assert_eq!(env["CLAUDE_CODE_OAUTH_TOKEN"], None);

  // A key that cannot be read is refused.
  let gone = account(
    "gone",
    Credential::ApiKey {
      home: dir.path().join("home"),
      key_file: dir.path().join("missing"),
    },
  );

  assert!(account_env(Some(&gone), &Runs::OwnLogin).is_err());

  // The platform's key never goes out to a work node; a person's goes out on theirs.
  let node = Runs::OnNode {
    data_root: dir.path().into(),
  };
  let persons = Account {
    user_id: Some(7),
    unit_id: None,
    ..account("persons", keyed)
  };

  assert!(account_env(Some(&platform), &node).is_err());
  assert!(!platform.runs_on_nodes());
  assert_eq!(
    account_env(Some(&persons), &node).unwrap()["ANTHROPIC_API_KEY"].as_deref(),
    Some("sk-ant-api-test")
  );
  assert!(persons.runs_on_nodes());
}

#[test]
fn a_person_s_login_on_their_node_is_handed_its_directory_there_and_no_token() {
  let data = tempfile::tempdir().unwrap();
  let held = Account {
    user_id: Some(7),
    unit_id: None,
    node: Some("pc-7".into()),
    ..account(
      "theirs",
      Credential::Directory {
        home: "/node/data/env/theirs".into(),
      },
    )
  };
  let token = held.node_token_file(data.path()).unwrap();

  // A token for the nodes standing here is never handed to it.
  std::fs::create_dir_all(token.parent().unwrap()).unwrap();
  std::fs::write(&token, "sk-ant-oat01-node\n").unwrap();

  let env = account_env(
    Some(&held),
    &Runs::OnNode {
      data_root: data.path().into(),
    },
  )
  .unwrap();

  assert_eq!(env["CLAUDE_CONFIG_DIR"].as_deref(), Some("/node/data/env/theirs"));
  assert_eq!(env["CLAUDE_CODE_OAUTH_TOKEN"], None);
}
