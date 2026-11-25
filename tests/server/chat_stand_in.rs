//! A turn on a road other than Claude Code's own session (docs/cache-fallback.md, 「A supervisor's turn on a road
//! other than Claude Code」): the turn composed for Codex on the kept thread and for OpenRouter's loop through a
//! tap, the turn's variables handed to the run and kept out of the program's own, none of Claude Code's launch
//! variables; a role's run in a place of its own on Claude Code, on its login and its catalogue's model; and the
//! chat's turn across rooms on the supervisor's one road — failed with why where the road cannot answer,
//! nothing standing in, left to Claude Code's own session where the road is Claude Code's, and not sent out
//! once stopped from outside.

use std::collections::{BTreeMap, HashMap};
use std::os::unix::fs::PermissionsExt;

use crate::support::grounds::{OrderEntry, set_provider_order};
use crate::support::rows::supervisor;
use agency_server::Service;
use agency_server::chat_stand_in::road_turn;
use agency_server::config::Config;
use agency_server::control::claude_accounts::{Account, Credential};
use agency_server::lines::chat::{ChatRun, Door, RoomContext, claude_folder, compose_stand_in};
use agency_server::lines::standing::GatewayModel;
use agency_server::lines::worker::Reasoning;
use agency_server::places::Ground;
use agency_server::run_log::{RunLog, Target};
use agency_server::stand_in::{Step, held_thinking};
use agency_server::supervisor_turn::run::NoAnswer;
use harness_providers::Machine;
use policy::providers::claude_code::CLAUDE_CODE;
use sea_orm::DatabaseConnection;
use sha2::{Digest, Sha256};

use crate::support::scratch::Scratch;

fn ground_on(db: DatabaseConnection, home: &std::path::Path, set: &[(&str, &str)]) -> Ground {
  let variables: HashMap<String, String> = HashMap::new();
  let service = Service::new(db, Config::read(|name| variables.get(name).cloned()).unwrap());
  let mut env = vec![
    ("HOME".to_owned(), home.to_string_lossy().into_owned()),
    ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
  ];

  env.extend(set.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())));

  Ground::new(&service, &Machine::with_env(env, home.join("data")), None)
}

// The chat's run as the chat line composed it for a turn, with one of Claude Code's launch variables and
// its thinking switch among its variables.
fn chat_run(home: &std::path::Path) -> ChatRun {
  let launch = CLAUDE_CODE.launch.variables.keys().next().cloned().unwrap_or_default();

  // The chat line's one MCP configuration, which Codex's command reads its servers out of, and the
  // system prompt dsh's overlay carries as its persona.
  std::fs::write(
    home.join("chat.mcp.json"),
    r#"{"mcpServers":{"bmo":{"type":"http","url":"http://127.0.0.1:3290/mcp/bmo","headers":{"Authorization":"Bearer ${BMO_MCP_TOKEN}"}}}}"#,
  )
  .unwrap();
  let system = "You are the supervisor.\n";
  std::fs::write(home.join("system.md"), system).unwrap();
  let system_hash = format!("{:x}", Sha256::digest(system.as_bytes()));

  ChatRun {
    line: "chat",
    agent: "chat-agent".into(),
    model: "claude-model".into(),
    effort: "high".into(),
    advisor: None,
    tier: None,
    reasoning: Reasoning::default(),
    thinking: true,
    system_path: home.join("system.md"),
    system_hash,
    system_parts: None,
    room: RoomContext {
      chat_id: -5,
      text: "the room's block".into(),
      mark: "[a]".into(),
    },
    tools_shown: "Bash,Read".into(),
    tools_allowed: "Bash,Read".into(),
    tools_denied: Some(agency_server::lines::chat::denied()),
    tools_kept: None,
    mcp_config: home.join("chat.mcp.json"),
    plugin_dir: None,
    mcp_tools: None,
    prompt: "what is new".into(),
    persona_unused: None,
    fold: false,
    ping: false,
    resume: Some("claude-session".into()),
    cwd: home.to_string_lossy().into_owned(),
    env: BTreeMap::from([
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
      ("BMO_TURN_CHAT".to_owned(), "-5".to_owned()),
      (launch, "1".to_owned()),
      ("MAX_THINKING_TOKENS".to_owned(), "0".to_owned()),
    ]),
    cards: Default::default(),
    worker: None,
  }
}

fn step(provider: &str, model: &str) -> Step {
  Step {
    provider: provider.into(),
    model: model.into(),
    effort: "medium".into(),
    thinking: true,
    answer_tokens: None,
  }
}

#[tokio::test]
async fn a_codex_step_goes_out_on_the_kept_thread_with_the_turns_variables_beside_the_command() {
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(DatabaseConnection::Disconnected, home.path(), &[]);
  let run = chat_run(home.path());
  let thread = Door::Codex {
    thread: Some("thread-1".into()),
  };
  let composed =
    compose_stand_in(&ground, &ground.machine(), &run, &step("codex", "gpt-x"), &thread, &[7, 8], 1_000)
      .await
      .unwrap();
  let args = &composed.command.args;
  let handed = composed.run_env.clone().unwrap();
  let launch = CLAUDE_CODE.launch.variables.keys().next().unwrap();

  assert_eq!(args[0], "codex");
  assert!(args.windows(2).any(|pair| pair == ["resume", "thread-1"]), "{args:?}");
  assert!(args.windows(2).any(|pair| pair == ["--model", "gpt-x"]), "{args:?}");
  assert_eq!(composed.session, "thread-1");
  assert_eq!((composed.model.as_str(), composed.effort.as_deref()), ("gpt-x", Some("medium")));
  assert_eq!(handed.get("BMO_TURN_CHAT").map(String::as_str), Some("-5"));
  assert_eq!(handed.get("BMO_TURN_MESSAGES").map(String::as_str), Some("7,8"));
  assert_eq!(handed.get("BMO_TURN_BEGAN").map(String::as_str), Some("1000"));
  assert!(!handed.contains_key(launch) && !handed.contains_key("MAX_THINKING_TOKENS"));
  assert!(
    !composed.command.env.contains_key("BMO_TURN_CHAT"),
    "the program's identity holds no turn"
  );

  let given = composed.given.unwrap();

  assert_eq!(
    (given["prompt"].as_str(), given["resumeSessionId"].as_str()),
    (Some("what is new"), Some("thread-1"))
  );
  assert_eq!(
    (given["model"].as_str(), given["promptDelivery"].as_str()),
    (Some("gpt-x"), Some("input"))
  );
}

#[tokio::test]
async fn a_codex_step_with_no_kept_thread_opens_one() {
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(DatabaseConnection::Disconnected, home.path(), &[]);
  let composed = compose_stand_in(
    &ground,
    &ground.machine(),
    &chat_run(home.path()),
    &step("codex", "gpt-x"),
    &Door::Codex { thread: None },
    &[],
    1,
  )
  .await
  .unwrap();

  assert!(!composed.command.args.iter().any(|arg| arg == "resume"));
  assert_eq!(composed.given.unwrap()["resumeSessionId"], serde_json::Value::Null);
}

#[tokio::test]
async fn an_openrouter_member_step_stands_claude_code_on_the_gateway() {
  let home = tempfile::tempdir().unwrap();
  // A launch on the gateway asks the store whose run it is, for the key its requests carry.
  let scratch = Scratch::new().await;
  let ground = ground_on(scratch.db().clone(), home.path(), &[]);
  let home_dir = ground.roots.data.join("claude-roads/openrouter/vendor-model");
  let key = ground.roots.data.join("env/llm-gateway.key");

  std::fs::create_dir_all(key.parent().unwrap()).unwrap();
  std::fs::write(&key, "sk-gateway\n").unwrap();
  std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();

  let held = GatewayModel {
    name: "vendor/model".into(),
    label: "Vendor Model".into(),
    home: home_dir.clone(),
    context_tokens: 131_072,
    answer_tokens: 16_384,
    thinking: true,
    registration: None,
    road: Some("openrouter"),
  };
  let composed = compose_stand_in(
    &ground,
    &ground.machine(),
    &chat_run(home.path()),
    &step("openrouter", "vendor/model"),
    &Door::Gateway {
      held: Box::new(held),
      placement: None,
    },
    &[3],
    1,
  )
  .await
  .unwrap();
  let args = &composed.command.args;
  let env = &composed.command.env;

  assert_eq!(args.first().map(String::as_str), Some("claude"));
  assert_eq!(env.get("ANTHROPIC_MODEL").map(String::as_str), Some("vendor/model"));
  assert_eq!(env.get("ANTHROPIC_BASE_URL").map(String::as_str), Some("http://127.0.0.1:18030"));
  assert_eq!(
    env.get("CLAUDE_CONFIG_DIR").map(String::as_str),
    Some(home_dir.to_string_lossy().as_ref())
  );
  assert_eq!(composed.window.map(|window| window.context_tokens), Some(131_072));
}

// A role's run in a place of its own on Claude Code (docs/person-nodes.md, 「Which node is a person's」): Claude
// Code itself on the step's model and effort, signed in by the login the door names, with nothing of the gateway.
#[tokio::test]
async fn a_claude_code_step_stands_claude_code_on_its_login_and_not_on_the_gateway() {
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(DatabaseConnection::Disconnected, home.path(), &[]);
  let login_home = home.path().join("login");
  let login = Account {
    id: "main".into(),
    name: "main".into(),
    place: 0,
    enabled: true,
    credential: Credential::Directory {
      home: login_home.clone(),
    },
    needs_login_at: None,
    owner_account: None,
    owner_unit: None,
    node: None,
    lendable: false,
    base_prompt: None,
    node_token_gone: None,
  };
  let composed = compose_stand_in(
    &ground,
    &ground.machine(),
    &chat_run(home.path()),
    &step("claude-code", "claude-sonnet-5"),
    &Door::Claude {
      login: Some(Box::new(login)),
    },
    &[4],
    1,
  )
  .await
  .unwrap();
  let args = &composed.command.args;
  let env = &composed.command.env;
  let launch = CLAUDE_CODE.launch.variables.keys().next().unwrap();

  assert_eq!(args.first().map(String::as_str), Some("claude"));
  assert!(args.windows(2).any(|pair| pair == ["--model", "claude-sonnet-5"]), "{args:?}");
  assert!(args.windows(2).any(|pair| pair == ["--effort", "medium"]), "{args:?}");
  assert!(
    !env.contains_key("ANTHROPIC_BASE_URL") && !env.contains_key("ANTHROPIC_MODEL"),
    "{env:?}"
  );
  assert!(env.contains_key(launch), "{env:?}");
  assert_eq!(
    env.get("CLAUDE_CONFIG_DIR").map(String::as_str),
    Some(login_home.to_string_lossy().as_ref())
  );
  assert_eq!(env.get("BMO_TURN_MESSAGES").map(String::as_str), Some("4"));
  assert_eq!(composed.account.as_deref(), Some("main"));
  // The session the run names stands nowhere under the login's records, so a new one is opened.
  assert_ne!(composed.session, "claude-session");
}

// A Claude Code model is held to the catalogue the Claude CLI keeps, never to the gateway's members, and a
// catalogue that cannot be read holds nothing against it (docs/provider-order.md, 「Models a provider holds」).
#[tokio::test]
async fn a_claude_code_model_is_held_to_the_claude_catalogue() {
  let home = tempfile::tempdir().unwrap();
  let scratch = Scratch::new().await;
  let ground = ground_on(scratch.db().clone(), home.path(), &[]);
  let machine = ground.machine();

  assert_eq!(held_thinking(&ground, &machine, "claude-code", "claude-sonnet-5").await, Ok(None));

  let catalogue = home.path().join(".claude/cache/model-catalog");

  std::fs::create_dir_all(&catalogue).unwrap();
  std::fs::write(
    catalogue.join("main-cc.json"),
    serde_json::json!({
      "version": 2,
      "fetchedAt": 1_000,
      "staleAt": 3_601_000,
      "catalog": { "surface": "cc", "config": { "id": "cc", "models": [
        { "id": "claude-sonnet-5", "name": "Sonnet 5", "section": "overflow" },
      ] } },
    })
    .to_string(),
  )
  .unwrap();

  assert_eq!(held_thinking(&ground, &machine, "claude-code", "claude-sonnet-5").await, Ok(None));

  let refused = held_thinking(&ground, &machine, "claude-code", "claude-unknown").await;

  assert!(matches!(&refused, Err(said) if said.contains("claude-unknown")), "{refused:?}");
}

fn log() -> RunLog {
  RunLog::new(Target {
    agent: "chat-agent".into(),
    run: "chat:-5:1".into(),
    chat_id: Some(-5),
    job_id: None,
  })
}

// A store whose supervisor stands on the road of `provider`, each provider named switched as the test says.
async fn store_on(provider: &str, switches: &[(&str, bool)]) -> Scratch {
  let scratch = Scratch::new().await;
  let entries: Vec<OrderEntry> = switches
    .iter()
    .map(|(name, enabled)| OrderEntry {
      name: (*name).to_owned(),
      enabled: *enabled,
    })
    .collect();
  let models = [
    ("claude-code", "claude-opus-5"),
    ("codex", "gpt-5.6-sol"),
    ("openrouter", "vendor/model"),
  ];
  let model = models.iter().find(|(name, _)| *name == provider).map_or("a-model", |(_, model)| *model);

  supervisor(scratch.db(), "chat-agent", &[("provider", provider.into()), ("model", model.into())]).await;
  set_provider_order(scratch.db(), &entries).await.unwrap();
  scratch
}

// The supervisor's road is Codex, switched off, while Claude Code and every other provider is on: the turn fails
// saying why, and no other provider stands in for the road.
#[tokio::test]
async fn a_road_that_cannot_answer_fails_the_turn_and_nothing_stands_in() {
  let home = tempfile::tempdir().unwrap();
  let scratch = store_on("codex", &[("claude-code", true), ("codex", false), ("openrouter", true)]).await;
  let ground = ground_on(scratch.db().clone(), home.path(), &[]);
  let turned =
    road_turn(&ground, &mut log(), &chat_run(home.path()), "chat-agent", &[], 1, None, None).await;

  assert!(
    matches!(&turned, Err(NoAnswer::Failed(said)) if said.contains("codex") || said.contains("Codex")),
    "{turned:?}"
  );
}

#[tokio::test]
async fn a_road_on_claude_code_leaves_the_turn_to_claude_s_own_session() {
  let home = tempfile::tempdir().unwrap();
  let scratch = store_on("claude-code", &[("claude-code", true), ("codex", true)]).await;
  let ground = ground_on(scratch.db().clone(), home.path(), &[]);
  let turned =
    road_turn(&ground, &mut log(), &chat_run(home.path()), "chat-agent", &[], 1, None, None).await;

  assert!(matches!(turned, Ok(None)), "{turned:?}");
}

// A turn of the receiver's handoff goes out on the road the session last answered on, not the road the row now
// names (docs/sessions.md, 「A restart hands the session over」): the row names Claude Code, whose own session would
// carry the turn, but the turn named onto Codex goes there.
#[tokio::test]
async fn a_handoff_turn_goes_out_on_the_road_it_is_named_onto() {
  let home = tempfile::tempdir().unwrap();
  let scratch = store_on("claude-code", &[("claude-code", true), ("codex", false)]).await;
  let ground = ground_on(scratch.db().clone(), home.path(), &[]);
  let last = agency_server::road::Road {
    provider: "codex".to_owned(),
    model: "gpt-5".to_owned(),
    account: None,
  };
  let turned =
    road_turn(&ground, &mut log(), &chat_run(home.path()), "chat-agent", &[], 1, None, Some(&last)).await;

  assert!(!matches!(turned, Ok(None)), "the turn did not stay on Claude Code: {turned:?}");
}

// A turn interrupted before it goes out does not go out (docs/cache-fallback.md, 「Automatic walks only on
// providers, in order」).
#[tokio::test]
async fn a_turn_stopped_from_outside_does_not_go_out() {
  let home = tempfile::tempdir().unwrap();
  let scratch = store_on("claude-code", &[("claude-code", true)]).await;
  let ground = ground_on(scratch.db().clone(), home.path(), &[]);

  agency_server::queue::settings::set(
    scratch.db(),
    &format!("{}chat:-5:1", agency_server::queue::running::EXECUTION_CANCEL_PREFIX),
    "1",
  )
  .await
  .unwrap();

  let turned =
    road_turn(&ground, &mut log(), &chat_run(home.path()), "chat-agent", &[], 1, None, None).await;

  assert!(
    matches!(&turned, Err(NoAnswer::Failed(said)) if said.contains("stopped from outside")),
    "{turned:?}"
  );
}

#[test]
fn the_chat_s_own_launch_stands_on_claude_code_in_its_agent_s_home_and_a_named_folder_in_that_folder() {
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(DatabaseConnection::Disconnected, home.path(), &[]);
  let named = chat_run(home.path());

  // A caller's folder is the caller's.
  assert_eq!(claude_folder(&ground, &named).unwrap(), named.cwd);

  // The chat's own launch names the deploy tree's link, which on Claude Code is the agent's own home instead, a
  // real folder made as it is first needed.
  let own = ChatRun {
    cwd: ground.roots.deploy.to_string_lossy().into_owned(),
    ..named
  };
  let folder = claude_folder(&ground, &own).unwrap();

  assert_eq!(
    std::path::PathBuf::from(&folder),
    ground.roots.data.join("supervisor-homes").join("chat-agent")
  );
  assert!(std::path::Path::new(&folder).is_dir());
}
