//! A thrown-away session over the session contract (docs/api.md, 「Sessions」): started on Claude Code alone
//! under a number of its own, its state its scope's provider, model and folder at the first generation,
//! named in the list until it ends, refusing a fold and a turn that names no work parts, and gone once
//! ended; its turn placed in the standing branch checkout its folder is, under the co-work task's label, or
//! in a pane of its own; the folder a branch's place is taken in, as the take answers it; and a session
//! handing a runner line its instructions whole, with the labels and the capabilities its record keeps, which
//! goes out on that line in a pane of its own, on any road whose program carries the line; a start passes
//! over a number whose place the process before left standing; and a job's final answer that is a tool call
//! written as text held by its stop hook on a registered model's road and in a pane of its own.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::control::session_ops::scope_asked;
use agency_server::herdr_backend::{HerdrSessions, TurnEnded, apart_id, is_apart};
use agency_server::job_run::claude::turn::{ApartSeat, apart_seat, runner_command};
use agency_server::lines::Trees;
use agency_server::order_run::places::slot_cwd;
use agency_server::places::branch::{place_agent, pool_of, slot_dir};
use agency_server::places::{Ground, Places};
use agency_server::seeds::deployment;
use agent_session::{
  Apart, FaultKind, GateAsk, Mapping, Resume, Scope, SessionId, Sessions, Start, Turn, TurnContext,
  WorkParts,
};
use harness_providers::Machine;
use serde_json::{Value, json};

use crate::support::scratch::Scratch;

fn machine(home: &std::path::Path) -> Machine {
  let at = |path: std::path::PathBuf| path.to_string_lossy().into_owned();

  Machine::with_env(
    [
      ("HOME".to_owned(), at(home.to_path_buf())),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
    ],
    home.join("data"),
  )
}

fn ground(scratch: &Scratch, machine: &Machine, herdr: Option<herdr::Herdr>) -> Ground {
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());

  Ground::new(&service, machine, herdr)
}

fn apart(provider: &str, cwd: &str) -> Apart {
  Apart {
    provider: provider.into(),
    model: "claude-sonnet-5".into(),
    cwd: cwd.into(),
    todo: Some(1816),
    role: Some("scout".into()),
    ..Apart::default()
  }
}

fn start(agent: &str, apart: Apart) -> Start {
  Start {
    agent: agent.into(),
    scope: Scope::Apart(apart),
  }
}

#[test]
fn a_thrown_away_sessions_id_names_its_agent_and_its_number() {
  assert_eq!(apart_id("branch-a", 3), SessionId("branch-a@run:3".into()));
  assert!(is_apart(&apart_id("branch-a", 3)));
  assert!(!is_apart(&SessionId("branch-a@run".into())));
  assert!(!is_apart(&SessionId("branch-a@worker".into())));
  assert!(!is_apart(&SessionId("branch-a@-7".into())));
}

#[tokio::test]
async fn a_thrown_away_session_stands_from_its_start_to_its_end_and_keeps_nothing_to_fold() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = machine(home.path());

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  let places = Places::new(ground(&scratch, &machine, None), Arc::default());
  let mut ended = places.ground.events.subscribe::<TurnEnded>();
  let sessions = HerdrSessions::new(places);
  let first = sessions.start(start("branch-a", apart("claude-code", "/tmp/run-a"))).await.unwrap();
  let second = sessions.start(start("branch-a", apart("claude-code", "/tmp/run-b"))).await.unwrap();

  assert_eq!(first.id, SessionId("branch-a@run:1".into()));
  assert_eq!(second.id, SessionId("branch-a@run:2".into()), "every start is a session of its own");
  assert_eq!(first.scope, Scope::Apart(apart("claude-code", "/tmp/run-a")));

  let listed = sessions.list().await.unwrap();

  assert!(listed.contains(&first) && listed.contains(&second), "{listed:?}");

  let state = sessions.read_state(&first.id, None).await.unwrap();

  assert_eq!(
    (state.provider.as_str(), state.model.as_str(), state.place.as_str(), state.generation),
    ("claude-code", "claude-sonnet-5", "/tmp/run-a", 1)
  );
  assert_eq!((state.context, state.run, state.went_out), (None, None, None));
  assert_eq!(sessions.fold(&first.id).await.unwrap_err().kind, FaultKind::Refused);

  let bare = Turn {
    session: first.id.clone(),
    prompt: "look around".into(),
    context: TurnContext::default(),
  };

  assert_eq!(sessions.send_turn(bare).await.unwrap_err().kind, FaultKind::Refused);
  assert!(ended.ready().is_none(), "a turn refused before it starts publishes no end");

  // A turn carrying a gate's ask goes out on the gate road even in a session apart: its refusal is the
  // gate's own, that a gate's turn names its system prompt, not the apart turn's that names no work.
  let gate = Turn {
    session: first.id.clone(),
    prompt: "classify".into(),
    context: TurnContext {
      gate: Some(GateAsk::default()),
      ..TurnContext::default()
    },
  };
  let refused = sessions.send_turn(gate).await.unwrap_err();

  assert_eq!(refused.kind, FaultKind::Refused);
  assert!(
    refused.message.contains("a gate's turn names its system prompt"),
    "an apart turn carrying a gate's ask takes the gate road: {}",
    refused.message
  );

  sessions.end(&first.id).await.unwrap();

  assert_eq!(sessions.read_state(&first.id, None).await.unwrap_err().kind, FaultKind::Unknown);
  assert_eq!(sessions.end(&first.id).await.unwrap_err().kind, FaultKind::Unknown);
  assert!(!sessions.list().await.unwrap().contains(&first));
  assert!(sessions.list().await.unwrap().contains(&second));
  assert_eq!(
    sessions.read_state(&SessionId("branch-a@run:9".into()), None).await.unwrap_err().kind,
    FaultKind::Unknown,
    "a number never given names no session"
  );
  let elsewhere = sessions.start(start("branch-a", apart("codex", "/tmp/run-c"))).await.unwrap();

  assert_eq!(
    elsewhere.scope,
    agent_session::Scope::Apart(apart("codex", "/tmp/run-c")),
    "a role's session goes out on whichever provider it names"
  );
  sessions.end(&elsewhere.id).await.unwrap();
  assert_eq!(
    sessions.start(start("a@b", apart("claude-code", "/tmp/run-c"))).await.unwrap_err().kind,
    FaultKind::Refused
  );
}

#[tokio::test]
async fn a_thrown_away_turn_goes_into_the_standing_checkout_its_folder_is_or_a_pane_of_its_own() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = machine(home.path());
  let herdr = herdr::Herdr::there(home.path().join("herdr.sock"));
  let with_herdr = ground(&scratch, &machine, Some(herdr));
  let without = ground(&scratch, &machine, None);
  let place = slot_dir(&with_herdr.roots.data, "agency-0123456789", 3);
  let inside = place.join("crates").to_string_lossy().into_owned();

  match apart_seat(&with_herdr, &apart("claude-code", &inside)) {
    Some(ApartSeat::Standing(home)) => {
      assert_eq!(home.agent, place_agent("agency-0123456789", 3));
      assert!(home.label.contains("#1816 scout 03"), "{}", home.label);
      assert!(home.resting.is_some(), "the place rests on its own label once the run ends");
    }
    other => panic!("a standing checkout's folder is its place: {other:?}"),
  }

  assert!(matches!(
    apart_seat(&with_herdr, &apart("claude-code", "/tmp/run-7")),
    Some(ApartSeat::Own(_))
  ));
  assert!(apart_seat(&without, &apart("claude-code", &inside)).is_none());
  assert!(apart_seat(&without, &apart("claude-code", "/tmp/run-7")).is_none());
}

#[tokio::test]
async fn a_branchs_place_is_taken_in_the_folder_of_its_number_under_its_checkouts_pool() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = machine(home.path());
  let ground = ground(&scratch, &machine, None);
  let checkout = home.path().join("checkout").to_string_lossy().into_owned();

  assert_eq!(
    slot_cwd(&ground, &checkout, 2).await,
    slot_dir(&ground.roots.data, &pool_of(&checkout).await, 2)
  );
}

// A machine whose tool host is the socket named, its code read from this checkout.
fn hosted(home: &std::path::Path, socket: &std::path::Path) -> Machine {
  let at = |path: &std::path::Path| path.to_string_lossy().into_owned();

  Machine::with_env(
    [
      ("HOME".to_owned(), at(home)),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
      ("BMO_DEPLOY_DIR".to_owned(), env!("CARGO_MANIFEST_DIR").to_owned()),
      ("BMO_MCP_HOST_SOCKET".to_owned(), at(socket)),
      ("BMO_MCP_HOST_ISSUER_TOKEN".to_owned(), "issuer".to_owned()),
    ],
    home.join("data"),
  )
}

// A turn's work parts on a runner line's work key, its variables the run's own.
fn work(variables: &[(&str, &str)]) -> WorkParts {
  WorkParts {
    provider: "claude-code".into(),
    model: "claude-sonnet-5".into(),
    mapping: Mapping {
      model: "claude-sonnet-5".into(),
      effort: Some("medium".into()),
      tier: Some("standard".into()),
      answer_tokens: None,
    },
    resume: Resume::Fresh,
    cwd: String::new(),
    name: "run-7".into(),
    title: "a branch".into(),
    chat: 0,
    unit: None,
    user: None,
    work_key: "search".into(),
    hears_words: false,
    variables: variables
      .iter()
      .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
      .collect(),
  }
}

#[tokio::test]
async fn a_session_handing_no_instructions_keeps_the_worker_line_and_its_folder() {
  assert_eq!(
    scope_asked(&json!({ "agentName": "a", "scope": "apart", "model": "claude-sonnet-5" })).unwrap(),
    Scope::Apart(Apart {
      model: "claude-sonnet-5".into(),
      ..Apart::default()
    }),
    "a session handing no instructions keeps the worker line's folder"
  );

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("nobody.sock");
  let ground = ground(&scratch, &hosted(home.path(), &socket), None);

  assert!(
    runner_command(&ground, &apart("claude-code", ""), &work(&[]), None, "s").await.is_err(),
    "a session on the worker line names no runner line"
  );
}

#[tokio::test]
async fn a_session_handing_its_instructions_takes_a_pane_of_its_own_and_its_line_s_folder() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = machine(home.path());
  let herdr = herdr::Herdr::there(home.path().join("herdr.sock"));
  let with_herdr = ground(&scratch, &machine, Some(herdr));
  let place = slot_dir(&with_herdr.roots.data, "agency-0123456789", 3);
  let inside = place.join("crates").to_string_lossy().into_owned();

  assert!(matches!(
    apart_seat(
      &with_herdr,
      &Apart {
        cwd: inside.clone(),
        ..handed("claude-code", "claude-sonnet-5")
      }
    ),
    Some(ApartSeat::Own(_))
  ));
  assert!(
    matches!(apart_seat(&with_herdr, &apart("claude-code", &inside)), Some(ApartSeat::Standing(_))),
    "a session handing none still goes into the standing checkout its folder is"
  );

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  let places = Places::new(ground(&scratch, &machine, None), Arc::default());
  let agents = Trees::of(&places.ground).agents;
  let sessions = HerdrSessions::new(places);
  let started = sessions.start(start("search", handed("claude-code", "claude-sonnet-5"))).await.unwrap();

  assert_eq!(
    started.scope,
    Scope::Apart(Apart {
      cwd: agents.join("search").to_string_lossy().into_owned(),
      ..handed("claude-code", "claude-sonnet-5")
    }),
    "a session on a runner line naming no folder leaves it to the line"
  );

  let worker = sessions.start(start("scout", apart("claude-code", ""))).await.unwrap();
  let own = agents.join("worker");

  assert_eq!(
    worker.scope,
    Scope::Apart(apart("claude-code", &own.to_string_lossy())),
    "a session on the worker line naming no folder goes out in the worker line's own, by its absolute path"
  );
  assert!(own.is_dir(), "the worker line's folder is made where it is missing");
}

fn handed(provider: &str, model: &str) -> Apart {
  Apart {
    provider: provider.into(),
    model: model.into(),
    cwd: String::new(),
    line: Some("search".into()),
    system_prompt: Some("THE INSTRUCTIONS HANDED IN".into()),
    labels: Some(json!({ "research": { "run": 3, "closesAt": 9 } }).to_string()),
    tools_allowed: Some(vec!["room.tools".into()]),
    ..Apart::default()
  }
}

#[tokio::test]
async fn a_session_handing_its_runner_line_instructions_reads_its_line_road_labels_and_tools() {
  let read = scope_asked(&json!({
    "agentName": "search", "scope": "apart", "provider": "registered", "model": "flash-next",
    "line": "search", "systemPrompt": "THE INSTRUCTIONS HANDED IN",
    "labels": { "research": { "run": 3, "closesAt": 9 } }, "toolsAllowed": ["room.tools"],
  }))
  .unwrap();

  assert_eq!(read, Scope::Apart(handed("registered", "flash-next")));

  for (body, why) in [
    (json!({ "systemPrompt": "x" }), "instructions with no line"),
    (json!({ "line": "worker", "systemPrompt": "x" }), "a line that is no runner's"),
    (
      json!({ "line": "search", "systemPrompt": "x", "labels": { "line": "chat" } }),
      "a label agency writes",
    ),
    (
      json!({ "line": "search", "systemPrompt": "x", "labels": ["research"] }),
      "labels that are no object",
    ),
    (
      json!({ "line": "search", "systemPrompt": "x", "toolsAllowed": "web.read" }),
      "tools that are no list",
    ),
  ] {
    let mut asked = json!({ "agentName": "search", "scope": "apart", "model": "claude-sonnet-5" });

    for (field, value) in body.as_object().unwrap() {
      asked[field] = value.clone();
    }

    assert!(scope_asked(&asked).is_err(), "{why} is refused");
  }
}

#[tokio::test]
async fn a_session_handing_its_instructions_goes_out_on_its_line_with_them_and_the_record_it_named() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("mcp-host.sock");
  let machine = hosted(home.path(), &socket);
  let ground = ground(&scratch, &machine, None);
  let trees = Trees::of(&ground);
  let apart = handed("claude-code", "claude-sonnet-5");
  let (run, command) = runner_command(&ground, &apart, &work(&[]), None, "a-session").await.unwrap();
  let name = run.system_prompt.file_name().unwrap().to_string_lossy().into_owned();

  assert_eq!(
    (run.line, run.model.as_str(), run.effort.as_str()),
    ("search", "claude-sonnet-5", "medium")
  );
  assert!(
    name.starts_with("search-given-"),
    "{name}: written under a name of its own, never the line's"
  );

  let system = std::fs::read_to_string(&run.system_prompt).unwrap();

  assert!(system.trim_end().ends_with("THE INSTRUCTIONS HANDED IN"), "{system}");
  assert_eq!(
    command.cwd,
    trees.agents.join("search").to_string_lossy(),
    "no folder named is the line's"
  );

  let again = runner_command(&ground, &apart, &work(&[]), None, "b-session").await.unwrap().0;

  assert_eq!(again.system_prompt, run.system_prompt, "the same instructions read one prefix");

  let env: BTreeMap<String, String> = command
    .env
    .iter()
    .filter_map(|(name, value)| Some((name.clone(), value.clone()?)))
    .collect();

  assert_eq!(
    serde_json::from_str::<serde_json::Value>(&env["BMO_RECORD_LABELS"]).unwrap(),
    json!({ "research": { "run": 3, "closesAt": 9 } })
  );
  assert_eq!(env["BMO_RECORD_TOOLS"], r#"["room.tools"]"#);
}

#[tokio::test]
async fn a_plain_session_is_told_its_caller_s_instructions_alone_and_reaches_nothing() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("mcp-host.sock");
  let machine = hosted(home.path(), &socket);
  let ground = ground(&scratch, &machine, None);
  let trees = Trees::of(&ground);
  let instructions = "You are the note assistant.\n\nAnswer in one fenced block.";
  let apart = Apart {
    line: Some("plain".into()),
    system_prompt: Some(instructions.into()),
    labels: None,
    tools_allowed: Some(Vec::new()),
    ..handed("claude-code", "claude-opus-5-5")
  };
  let (run, command) = runner_command(&ground, &apart, &work(&[]), None, "a-session").await.unwrap();
  let args = &command.args;

  assert_eq!(run.line, "plain");
  assert_eq!(
    std::fs::read_to_string(&run.system_prompt).unwrap().trim_end(),
    instructions,
    "the system prompt is the caller's instructions exactly: no harness opening, no role prompt, no persona"
  );
  assert_eq!(after(args, "--system-prompt-file"), Some(run.system_prompt.to_string_lossy().as_ref()));
  assert!(
    !args
      .iter()
      .any(|arg| arg == "--append-system-prompt" || arg == "--append-system-prompt-file"),
    "{args:?}: nothing is laid beside it"
  );
  assert_eq!(
    after(args, "--setting-sources"),
    Some(""),
    "no CLAUDE.md, memory, setting or hook is read"
  );
  assert!(args.iter().any(|arg| arg == "--strict-mcp-config"), "{args:?}");

  let config: serde_json::Value =
    serde_json::from_str(&std::fs::read_to_string(after(args, "--mcp-config").unwrap()).unwrap()).unwrap();

  assert_eq!(config, json!({ "mcpServers": {} }), "no MCP server of any kind");
  assert_eq!(after(args, "--tools"), Some(""), "no built-in tool is shown");
  assert_eq!(after(args, "--allowedTools"), Some(""), "and none is allowed");
  assert!(
    args.iter().any(|arg| arg == "--disable-slash-commands"),
    "no skill and no command: {args:?}"
  );
  assert!(
    !args
      .iter()
      .any(|arg| arg == "--settings" || arg == "--plugin-dir" || arg == "--add-dir"),
    "{args:?}: no settings file, plugin or further folder"
  );
  assert_eq!(command.cwd, trees.agents.join("plain").to_string_lossy(), "the line's own folder");
}

#[test]
fn a_start_names_the_program_session_its_turns_continue_and_the_words_that_stand_in_for_it() {
  let read = scope_asked(&json!({
    "agentName": "note-assistant", "scope": "role", "line": "plain", "systemPrompt": "THE INSTRUCTIONS",
    "continues": "the-program-session", "continuesElse": "the whole words",
  }))
  .unwrap();
  let Scope::Apart(apart) = read else { panic!("a role's session") };

  assert_eq!(apart.resume.as_deref(), Some("the-program-session"));
  assert_eq!(apart.resume_else.as_deref(), Some("the whole words"));

  for (body, why) in [
    (json!({ "continues": "" }), "an empty session"),
    (json!({ "continues": 7 }), "a session that is no text"),
    (json!({ "continuesElse": "words" }), "words standing in for no session"),
    (json!({ "continues": "s", "continuesElse": [] }), "words that are no text"),
  ] {
    let mut asked =
      json!({ "agentName": "note-assistant", "scope": "role", "line": "plain", "systemPrompt": "x" });

    for (field, value) in body.as_object().unwrap() {
      asked[field] = value.clone();
    }

    assert!(scope_asked(&asked).is_err(), "{why} is refused");
  }
}

// A tool host that keeps the record each token is issued for, answering it as issued.
fn issuing_host(socket: &std::path::Path) -> Arc<std::sync::Mutex<Vec<serde_json::Value>>> {
  let issued = Arc::new(std::sync::Mutex::new(Vec::new()));
  let kept = issued.clone();
  let listener = tokio::net::UnixListener::bind(socket).unwrap();
  let app = axum::Router::new().route(
    "/tool-sessions",
    axum::routing::post(move |axum::Json(mut record): axum::Json<serde_json::Value>| {
      let kept = kept.clone();

      async move {
        kept.lock().unwrap().push(record.clone());
        record["token"] = json!("a-token");
        record["createdAt"] = json!(1);
        record["expiresAt"] = serde_json::Value::Null;

        (axum::http::StatusCode::CREATED, axum::Json(record))
      }
    }),
  );

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  issued
}

#[tokio::test]
async fn a_run_of_a_session_naming_its_record_is_issued_with_its_labels_and_only_the_tools_it_keeps() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("mcp-host.sock");
  let issued = issuing_host(&socket);
  let machine = hosted(home.path(), &socket);

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  let ground = ground(&scratch, &machine, None);
  let env: BTreeMap<String, String> =
    agency_server::lines::token::record_variables(&handed("registered", "flash-next"))
      .into_iter()
      .collect();

  assert!(agency_server::lines::token::names_record(&env));

  let token =
    agency_server::lines::token::issue_for_run(&ground, "search", "search", "/tmp", &env, None, u64::MAX)
      .await
      .unwrap();

  assert_eq!(token.as_deref(), Some("a-token"));

  let record = issued.lock().unwrap().pop().expect("a record was issued");

  assert_eq!(
    record["capabilities"],
    json!(["room.tools"]),
    "the list narrows the line's and the role's"
  );
  assert_eq!(record["labels"]["research"], json!({ "run": 3, "closesAt": 9 }));
  assert_eq!(
    record["labels"]["line"],
    json!("search"),
    "agency's own labels stand beside the caller's"
  );
}

#[tokio::test]
async fn a_session_on_a_road_carrying_its_own_turns_stands_in_a_place_of_its_own() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = machine(home.path());

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  let sessions = HerdrSessions::new(Places::new(ground(&scratch, &machine, None), Arc::default()));
  let worker_line = sessions.start(start("search", apart("registered", "/tmp/run-a"))).await.unwrap();

  assert_eq!(
    worker_line.scope,
    agent_session::Scope::Apart(apart("registered", "/tmp/run-a")),
    "a role's run on the worker line goes out on any provider"
  );
  sessions.end(&worker_line.id).await.unwrap();

  let placed = sessions.start(start("search", handed("registered", "flash-next"))).await.unwrap();

  assert_eq!(placed.id, SessionId("search@run:2".into()));
  assert_eq!(
    agency_server::apart_places::place_name(&placed.id),
    "apart-search-run-2",
    "its place is named after it"
  );

  sessions.end(&placed.id).await.unwrap();
}

#[tokio::test]
async fn a_start_passes_over_a_number_whose_place_still_stands() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = machine(home.path());

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  let ground = ground(&scratch, &machine, None);
  let kept = SessionId("search@run:1".into());

  // The process before left this session's place standing, kept for a turn picked up across the restart.
  agency_server::apart_places::mark(&ground, &agency_server::apart_places::place_name(&kept), &kept, None)
    .unwrap();

  let sessions = HerdrSessions::new(Places::new(ground, Arc::default()));
  let started = sessions.start(start("search", handed("registered", "flash-next"))).await.unwrap();

  assert_eq!(started.id, SessionId("search@run:2".into()), "no new session goes into the kept place");

  sessions.end(&started.id).await.unwrap();
}

#[tokio::test]
async fn a_start_on_a_road_whose_program_does_not_carry_the_line_says_why() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = machine(home.path());

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  let ground = ground(&scratch, &machine, None);
  let carried = async |provider: &str| {
    agency_server::control::session_ops::line_not_carried(&ground, &handed(provider, "a-model"))
      .await
      .unwrap()
  };

  assert_eq!(carried("claude-code").await, None, "Claude Code carries the search line");
  assert!(carried("nobody").await.unwrap().contains("no provider this build carries"));
  assert!(carried("codex").await.is_some(), "Codex's program does not carry the search line");
  assert_eq!(
    agency_server::control::session_ops::line_not_carried(&ground, &apart("codex", "/tmp"))
      .await
      .unwrap(),
    None,
    "a session on the worker line is the backend's to judge"
  );
}

// A role's run names its role in its variables, and its record is given that role's bundles whatever name it
// is carried under: a job's run under its checkout's name holds the bundles of the role its item names
// (docs/agent-roles.md, 「Where the role is read」).
#[tokio::test]
async fn a_run_naming_its_role_is_issued_that_role_s_bundles_under_any_name() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("mcp-host.sock");
  let issued = issuing_host(&socket);
  let machine = hosted(home.path(), &socket);

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  let ground = ground(&scratch, &machine, None);
  let producer = Apart {
    provider: "deepseek".into(),
    model: "deepseek-flash".into(),
    cwd: "/tmp".into(),
    role: Some("producer".into()),
    ..Apart::default()
  };
  let env: BTreeMap<String, String> =
    agency_server::lines::token::record_variables(&producer).into_iter().collect();

  assert_eq!(env[agency_server::lines::token::RUN_ROLE], "producer");
  assert!(agency_server::lines::token::names_record(&env));

  agency_server::lines::token::issue_for_run(
    &ground,
    "worker",
    "some-checkout",
    "/tmp",
    &env,
    None,
    u64::MAX,
  )
  .await
  .unwrap()
  .expect("a token");

  let record = issued.lock().unwrap().pop().expect("a record was issued");
  let capabilities: Vec<String> = serde_json::from_value(record["capabilities"].clone()).unwrap();

  assert_eq!(record["labels"]["role"], json!("producer"), "{record}");
  assert!(
    capabilities.contains(&"room.send".to_owned()),
    "the producer role's own bundle: {capabilities:?}"
  );
  assert!(capabilities.contains(&"brain.lessons".to_owned()), "{capabilities:?}");
  assert!(
    record["variables"].get("BMO_BRAIN_AGENT").is_none(),
    "no supervisor's brain where the research setting names none: {record}"
  );
  assert!(
    !capabilities.contains(&"graph.read".to_owned()),
    "no bundle the role does not give: {capabilities:?}"
  );
  assert!(record["labels"].get("reads").is_none(), "a text-only model reads no media: {record}");
}

// A run's record names the media its run model reads natively, so a placed tool hands its material only to
// a run that can read it (docs/provider-order.md, 「The media a model reads」).
#[tokio::test]
async fn a_run_on_a_road_whose_model_reads_media_is_issued_with_its_reads_label() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("mcp-host.sock");
  let issued = issuing_host(&socket);
  let machine = hosted(home.path(), &socket);

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  let ground = ground(&scratch, &machine, None);
  let producer = Apart {
    provider: "claude-code".into(),
    model: "claude-sonnet-5".into(),
    cwd: "/tmp".into(),
    role: Some("producer".into()),
    ..Apart::default()
  };
  let env: BTreeMap<String, String> =
    agency_server::lines::token::record_variables(&producer).into_iter().collect();

  agency_server::lines::token::issue_for_run(
    &ground,
    "worker",
    "some-checkout",
    "/tmp",
    &env,
    Some(("claude-code", "claude-sonnet-5")),
    u64::MAX,
  )
  .await
  .unwrap()
  .expect("a token");

  let record = issued.lock().unwrap().pop().expect("a record was issued");

  assert_eq!(record["labels"]["reads"], json!(["image", "pdf"]), "{record}");
}

// A record issued with no road of its own takes the one the standing supervisor's row names.
#[tokio::test]
async fn a_supervisor_s_record_reads_the_media_of_its_own_road() {
  use crate::support::rows::supervisor;
  use std::path::Path;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("mcp-host.sock");
  let issued = issuing_host(&socket);
  let machine = hosted(home.path(), &socket);

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();
  supervisor(scratch.db(), "bmo", &[]).await;

  let ground = ground(&scratch, &machine, None);

  agency_server::lines::token::issue(
    &ground,
    "chat",
    "bmo",
    Path::new("/tmp/place"),
    Path::new("/tmp"),
    None,
  )
  .await
  .unwrap()
  .expect("a token");

  let record = issued.lock().unwrap().pop().expect("a record was issued");

  assert_eq!(
    record["labels"]["reads"],
    json!(["image", "pdf"]),
    "the supervisor's own row stands on Claude Code: {record}"
  );
}

// The registered road judges a registration by the engine its row names, so every alias of one engine reads
// the same media, and a model no rule reaches leaves the label off.
#[tokio::test]
async fn a_registration_takes_the_reads_of_the_engine_its_row_names() {
  use policy::providers::Media;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = machine(home.path());

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();
  scratch
    .run(
      r#"INSERT INTO "registered_models"
           ("name", "label", "model", "context_tokens", "answer_tokens", "thinking", "walked", "added_at")
         VALUES ('bong-q38fn', 'bong', 'qwen3.8-flash-next', 524288, 16000, 1, 1, 0)"#,
      vec![],
    )
    .await;

  let ground = ground(&scratch, &machine, None);

  assert_eq!(agency_server::road::reads(&ground, "registered", "bong-q38fn").await, [Media::Image]);
  assert!(agency_server::road::reads(&ground, "registered", "hy-mt2-7b").await.is_empty());
  assert!(agency_server::road::reads(&ground, "deepseek", "deepseek-chat").await.is_empty());
  assert!(agency_server::road::reads(&ground, "nobody", "anything").await.is_empty());
}

// A run of the worker line works for the brain of the supervisor agency's research setting names, which its record names apart from the turn's agent, so its other tools act as the run itself
// (docs/agent-roles.md, 「The bundles」).
#[tokio::test]
async fn a_worker_line_run_works_for_the_research_supervisor_s_brain_and_stays_its_own() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("mcp-host.sock");
  let issued = issuing_host(&socket);
  let machine = hosted(home.path(), &socket);

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();
  agency_server::queue::settings::set(
    scratch.db(),
    agency_server::lines::token::RESEARCH_SUPERVISOR,
    "agent-a",
  )
  .await
  .unwrap();

  let ground = ground(&scratch, &machine, None);
  let coder = Apart {
    provider: "deepseek".into(),
    model: "deepseek-flash".into(),
    cwd: "/tmp".into(),
    role: Some("coder".into()),
    ..Apart::default()
  };
  let env: BTreeMap<String, String> =
    agency_server::lines::token::record_variables(&coder).into_iter().collect();

  agency_server::lines::token::issue_for_run(
    &ground,
    "worker",
    "some-checkout",
    "/tmp",
    &env,
    None,
    u64::MAX,
  )
  .await
  .unwrap()
  .expect("a token");

  let record = issued.lock().unwrap().pop().expect("a record was issued");
  let capabilities: Vec<String> = serde_json::from_value(record["capabilities"].clone()).unwrap();
  let namespaces: Vec<String> = serde_json::from_value(record["namespaces"].clone()).unwrap();

  assert!(capabilities.contains(&"brain.lessons".to_owned()), "{capabilities:?}");
  assert!(namespaces.contains(&"bmo".to_owned()), "{namespaces:?}");
  assert_eq!(record["variables"]["BMO_BRAIN_AGENT"], json!("agent-a"), "{record}");
  assert_ne!(
    record["variables"].get("BMO_TURN_AGENT"),
    Some(&json!("agent-a")),
    "the run stays its own: {record}"
  );
  assert_ne!(record["holderName"], json!("agent-a"), "{record}");
}

// A standing program's environment holds no turn's variables, the role its run holds among them: the record it
// is handed is issued with the run's variables laid over the program's own, so a role's session on a road whose
// program stands in a place holds its role's bundles (docs/agent-roles.md, 「Where the role is read」).
#[tokio::test]
async fn a_standing_program_of_a_role_s_session_is_issued_its_role_s_bundles() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("mcp-host.sock");
  let issued = issuing_host(&socket);
  let machine = hosted(home.path(), &socket);

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  let ground = ground(&scratch, &machine, None);
  let voyager = Apart {
    provider: "registered".into(),
    model: "flash-next".into(),
    cwd: "/tmp".into(),
    role: Some("voyager".into()),
    line: Some("search".into()),
    system_prompt: Some("THE INSTRUCTIONS".into()),
    ..Apart::default()
  };
  let run_env: BTreeMap<String, String> =
    agency_server::lines::token::record_variables(&voyager).into_iter().collect();
  let place = home.path().join("apart-voyager-run-1");
  let program_env = agency_server::door::place_run::resident_environment(&run_env, &place);

  assert!(
    !program_env.contains_key(agency_server::lines::token::RUN_ROLE),
    "the program's own environment holds no turn's variable"
  );

  agency_server::lines::token::issue_for_program(
    &ground,
    "search",
    "search",
    &place,
    std::path::Path::new("/tmp"),
    &program_env,
    &run_env,
    None,
    u64::MAX,
  )
  .await
  .unwrap()
  .expect("a token");

  let record = issued.lock().unwrap().pop().expect("a record was issued");
  let capabilities: Vec<String> = serde_json::from_value(record["capabilities"].clone()).unwrap();

  assert_eq!(record["labels"]["role"], json!("voyager"), "{record}");

  for bundle in ["web.read", "web.sites", "web.branches", "web.rulebooks"] {
    assert!(capabilities.contains(&bundle.to_owned()), "the voyager's {bundle}: {capabilities:?}");
  }
}

// A road whose program is Claude Code on agency's gateway: DeepSeek's member, its key where the settings' key
// helper reads it.
fn on_deepseek(ground: &Ground) -> agency_server::lines::chat::Door {
  use std::os::unix::fs::PermissionsExt;

  let key = ground.roots.data.join("env/llm-gateway.key");

  std::fs::create_dir_all(key.parent().unwrap()).unwrap();
  std::fs::write(&key, "sk-gateway\n").unwrap();
  std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();

  agency_server::lines::chat::Door::Gateway {
    held: Box::new(agency_server::lines::standing::GatewayModel {
      name: "deepseek-flash".into(),
      label: "DeepSeek Flash".into(),
      home: ground.roots.data.join("claude-roads/deepseek/deepseek-flash"),
      context_tokens: 131_072,
      answer_tokens: 16_384,
      thinking: true,
      registration: None,
      road: Some("deepseek"),
    }),
    placement: None,
  }
}

// A role session's turn on DeepSeek's gateway road, composed as the backend composes it and stood as the road
// stands its program: the command's words, and the servers of the configuration it names.
async fn stood_on_deepseek(ground: &Ground, apart: &Apart) -> (Vec<String>, Vec<String>) {
  let args = composed_on(ground, apart, "deepseek", &on_deepseek(ground)).await;
  let config = after(&args, "--mcp-config").unwrap();
  let servers: Value = serde_json::from_str(&std::fs::read_to_string(config).unwrap()).unwrap();
  let servers = servers["mcpServers"].as_object().unwrap().keys().cloned().collect();

  (args, servers)
}

// A role session's turn on the provider and gateway door given, composed as the backend composes it: the
// command's words.
async fn composed_on(
  ground: &Ground,
  apart: &Apart,
  provider: &str,
  door: &agency_server::lines::chat::Door,
) -> Vec<String> {
  let trees = Trees {
    code: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    ..Trees::of(ground)
  };
  let mapping = Mapping {
    model: apart.model.clone(),
    effort: Some("high".into()),
    tier: None,
    answer_tokens: None,
  };
  let role = apart.role.clone().unwrap();
  let composed = agency_server::lines::role::compose(ground, &trees, apart, &role, &mapping)
    .await
    .unwrap();
  let run = composed.run(agency_server::lines::role::Turned {
    place: "apart-run-1".into(),
    model: apart.model.clone(),
    prompt: "look around".into(),
    resume: None,
    env: BTreeMap::new(),
    cards: Default::default(),
  });
  let step = agency_server::stand_in::Step {
    provider: provider.into(),
    model: apart.model.clone(),
    effort: "high".into(),
    thinking: true,
    answer_tokens: None,
  };

  agency_server::lines::chat::compose_stand_in(ground, &ground.machine(), &run, &step, door, &[], 1)
    .await
    .unwrap()
    .command
    .args
}

// The words a flag stands before.
fn after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
  args.iter().position(|arg| arg == flag).map(|at| args[at + 1].as_str())
}

// A worker's run on DeepSeek holds the worker line's tools as one on Claude Code's own road does: the line's own
// permission mode over every built-in but those it denies, its plugin, co-workers and settings, and its
// configuration with the web and worker namespaces (docs/launch-lines.md, 「What a line's program is allowed, by
// road」).
#[tokio::test]
async fn a_worker_role_s_turn_on_a_gateway_road_holds_the_worker_line_s_tools_and_servers() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = machine(home.path());

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  let ground = ground(&scratch, &machine, None);
  let work = home.path().join("work");

  std::fs::create_dir_all(&work).unwrap();

  let worker = Apart {
    provider: "deepseek".into(),
    model: "deepseek-flash".into(),
    cwd: work.to_string_lossy().into_owned(),
    role: Some("worker".into()),
    ..Apart::default()
  };
  let (args, servers) = stood_on_deepseek(&ground, &worker).await;
  // The line's denied tools and every subagent type its carries do not keep (docs/launch-lines.md, 「What a line
  // carries」).
  let denied = agency_server::lines::claude::line_denied("worker");

  assert_eq!(args.first().map(String::as_str), Some("claude"), "{args:?}");
  assert_eq!(after(&args, "--permission-mode"), Some("bypassPermissions"), "{args:?}");
  assert_eq!(after(&args, "--disallowedTools"), Some(denied.as_str()), "{args:?}");
  assert!(!args.iter().any(|arg| arg == "--tools"), "every built-in is shown: {args:?}");
  assert!(
    args.iter().any(|arg| arg == "--strict-mcp-config"),
    "no server of the login beside the line's own: {args:?}"
  );

  for flag in ["--plugin-dir", "--settings", "--system-prompt-file"] {
    assert!(after(&args, flag).is_some_and(|value| !value.is_empty()), "{flag}: {args:?}");
  }
  assert!(!args.iter().any(|arg| arg == "--agents"), "no subagent definition is handed: {args:?}");

  assert_eq!(after(&args, "--setting-sources"), Some("project,local"), "{args:?}");

  for server in ["web", "worker"] {
    assert!(servers.contains(&server.to_owned()), "{server}: {servers:?}");
  }
}

// A voyager's run on DeepSeek holds the search line's servers and nothing of its program's own, as on
// Claude Code's own road (docs/agent-roles.md, 「The voyager and the seeker roles」).
#[tokio::test]
async fn a_voyager_s_turn_on_a_gateway_road_holds_the_web_bundles_and_the_eye_and_research() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = machine(home.path());

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  let ground = ground(&scratch, &machine, None);
  let voyager = Apart {
    provider: "deepseek".into(),
    model: "deepseek-flash".into(),
    cwd: "/tmp".into(),
    role: Some("voyager".into()),
    line: Some("search".into()),
    system_prompt: Some("FIND IT.".into()),
    ..Apart::default()
  };
  let (args, servers) = stood_on_deepseek(&ground, &voyager).await;
  let allowed: Vec<&str> = after(&args, "--allowedTools").unwrap().split(',').collect();

  assert_eq!(after(&args, "--permission-mode"), Some("dontAsk"), "{args:?}");
  assert_eq!(after(&args, "--tools"), Some(""), "no built-in tool is shown: {args:?}");

  for server in ["bmo", "web", "eye", "research"] {
    assert!(servers.contains(&server.to_owned()), "{server}: {servers:?}");
    assert!(allowed.contains(&format!("mcp__{server}").as_str()), "{server}: {allowed:?}");
  }
}

// The final answers jobs' runs on a registered model stopped on, each a tool call written as text: a
// `<tool_call>` block with a template's control token where the call's name belongs, and a bare `<invoke>`
// naming no real tool.
const CALL_AS_TEXT: &str = "\n\n<tool_call>\n<|im_start|>function>\n</function>\n</tool_call>";
const INVOKE_AS_TEXT: &str = "<invoke name=\"function\">\n</function>";

// What the stop hook a settings file names answers a bot's run whose final answer is `answer`: the hook's
// words read off the settings as the program reads them, the bot's run begun first in the place they name.
async fn stop_hook_answers(settings: &str, answer: &str) -> Vec<String> {
  use agency_server::verbs::agent_status::{Asked, Ground as HookGround, Refuses, agent_status};

  let settings: Value = serde_json::from_str(&std::fs::read_to_string(settings).unwrap()).unwrap();
  let stop = settings["hooks"]["Stop"]
    .as_array()
    .unwrap()
    .iter()
    .flat_map(|entry| entry["hooks"].as_array().cloned().unwrap_or_default())
    .filter_map(|hook| hook["command"].as_str().map(str::to_owned))
    .find(|command| command.contains("'agent-status' 'stop'"))
    .expect("the place's stop hook");
  let words = shlex::split(&stop).unwrap();
  let place = after(&words, "--place").unwrap().to_owned();
  let flag = |name: &str| words.iter().any(|word| word == name);
  let now = chrono::Utc::now().timestamp_millis();
  let config = tempfile::tempdir().unwrap();

  agency_server::door::place_run::begin_run(
    std::path::Path::new(&place),
    "worker-job-1",
    "a job",
    now - 1_000,
    BTreeMap::new(),
  )
  .unwrap();

  let asked = Asked {
    kind: "stop".into(),
    place,
    hold_tasks: flag("--hold-tasks"),
    hold_text_tool_call: flag("--hold-text-tool-call"),
    hold_wording: flag("--hold-wording"),
    compose_typed: flag("--compose-typed"),
    carry_background: flag("--carry-background"),
    refuses: Refuses {
      claude: flag("--refuse-claude"),
      job: flag("--guard-job"),
    },
  };
  let hook_ground = HookGround {
    env: &BTreeMap::new(),
    code_tree: config.path(),
    machine_config_dir: config.path(),
    now_ms: now,
  };

  agent_status(&asked, &json!({ "last_assistant_message": answer }).to_string(), &hook_ground).await
}

// The one answer a stop hook gave, read as the block it must be.
fn block_of(answered: &[String]) -> Value {
  assert_eq!(answered.len(), 1, "{answered:?}");

  serde_json::from_str(&answered[0]).unwrap()
}

// A job's run on a registered model is held where its final answer is a tool call written as text, as a
// worker's run on Claude Code's own road is: its place's stop hook asks for the hold, and that hook answers
// the answer with a block asking for a native call (docs/agent-control.md, 「What a resident turn reads」).
#[tokio::test]
async fn a_job_on_a_registered_model_is_held_where_its_final_answer_is_a_tool_call_written_as_text() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = machine(home.path());

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  let ground = ground(&scratch, &machine, None);
  let work = home.path().join("work");

  std::fs::create_dir_all(&work).unwrap();

  let registration = agency_server::control::registry::Registration {
    name: "coder-flash".into(),
    label: "Coder Flash".into(),
    model: "coder-flash".into(),
    context_tokens: 262_144,
    answer_tokens: 16_384,
    thinking: false,
    walked: false,
    keeps_research: false,
    card_service: None,
    role_files: false,
    languages: vec![],
    owner_account: None,
    owner_unit: None,
    enabled: true,
    addresses: vec![],
  };
  let door = agency_server::lines::chat::Door::Gateway {
    held: Box::new(agency_server::lines::standing::GatewayModel::registration(&machine, &registration)),
    placement: None,
  };
  let job = Apart {
    provider: "registered".into(),
    model: "coder-flash".into(),
    cwd: work.to_string_lossy().into_owned(),
    role: Some("worker".into()),
    ..Apart::default()
  };
  let args = composed_on(&ground, &job, "registered", &door).await;

  for answer in [CALL_AS_TEXT, INVOKE_AS_TEXT] {
    let held = block_of(&stop_hook_answers(after(&args, "--settings").unwrap(), answer).await);

    assert_eq!(held["decision"], "block", "{answer}: {held}");
    assert!(held["reason"].as_str().unwrap().contains("native tool call"), "{held}");
  }
}

// A job's run in a pane of its own stands in no agent's place, so its hooks are given a place of the run's own,
// marked as a thrown-away one, whose stop hook holds a final answer that is a tool call written as text; a run
// of a line that holds nothing is given none.
#[tokio::test]
async fn a_job_in_a_pane_of_its_own_is_given_a_place_whose_stop_hook_holds_a_tool_call_written_as_text() {
  use agency_server::door::apart::{Apart as PaneRun, hooks_place};
  use agency_server::lines::claude::Command;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = machine(home.path());
  let ground = ground(&scratch, &machine, None);
  let line_settings = home.path().join("worker-settings.json");

  std::fs::write(&line_settings, r#"{"env":{"A":"1"}}"#).unwrap();

  let run = |line: &'static str| PaneRun {
    agent: "worker",
    role: "worker",
    line,
    provider: "claude-code",
    title: "a job".into(),
    command: Command {
      args: vec!["claude".into(), "--settings".into(), line_settings.to_string_lossy().into_owned()],
      cwd: home.path().to_string_lossy().into_owned(),
      env: BTreeMap::new(),
    },
    session: "1f0c5a52-4c0e-4f4e-9d5a-2b7e6d0c9a11".into(),
    prompt: "do the work".into(),
    model: "claude-sonnet-5".into(),
    effort: String::new(),
    account: None,
    parent: None,
    root: None,
    run: Some("worker-job-1".into()),
    resume: None,
    resume_else: None,
  };
  let (place, args) = hooks_place(&ground, &run("worker")).unwrap().expect("a place for the hooks");
  let settings = after(&args, "--settings").unwrap();

  assert!(agency_server::apart_places::is_apart_place(&ground.roots.data, &place), "{place}");
  assert_eq!(
    serde_json::from_str::<Value>(&std::fs::read_to_string(settings).unwrap()).unwrap()["env"]["A"],
    "1",
    "the line's own settings stand under the hooks"
  );

  let held = block_of(&stop_hook_answers(settings, CALL_AS_TEXT).await);

  assert_eq!(held["decision"], "block", "{held}");
  assert!(hooks_place(&ground, &run("search")).unwrap().is_none());
}

#[tokio::test]
async fn a_node_s_thrown_away_session_takes_its_second_turn_after_its_workspace_went() {
  use agency_server::apart_places;
  use agency_server::supervisor_turn::run::placed;
  use agency_server::watches::links;

  use crate::integration::common::herdr::HerdrModel;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = machine(home.path());
  let node_herdr = home.path().join("data").join("nodes").join("pc");

  std::fs::create_dir_all(&node_herdr).unwrap();

  let node = HerdrModel::stand(node_herdr.join("herdr.sock"));
  let ground = ground(&scratch, &machine, Some(herdr::Herdr::there(home.path().join("own-herdr.sock"))));
  let session = SessionId("actor@run:1793".into());
  let name = apart_places::place_name(&session);

  apart_places::mark(&ground, &name, &session, None).unwrap();
  links::mark_place_on(&ground.roots.place(&name), Some("pc"), "/work").unwrap();

  let (label, on) = placed(&ground, &name).await.unwrap().unwrap();

  assert_eq!(on.as_deref(), Some("pc"));

  // Each turn stands the place's home in the node's herdr before it puts its words in.
  let home_of_turn = || async { ground.home_on(&name, &label, on.as_deref()).await.unwrap().unwrap() };
  let first = home_of_turn().await.stand().await.unwrap();

  // The head pane's loop ends while the second turn stands the home: after it found the workspace and
  // before it looks for the head in it, the workspace going with its one pane.
  node.ends_after_next_snapshot(&first.head_pane);

  let second = home_of_turn().await.stand().await.expect("the second turn finds its workspace");
  let workspaces = node.workspaces();

  assert_ne!(second.workspace, first.workspace, "the workspace that went is made again");
  assert_eq!(
    workspaces.iter().map(|one| one.id.as_str()).collect::<Vec<_>>(),
    [second.workspace.as_str()]
  );
  assert!(
    node
      .panes()
      .iter()
      .any(|pane| pane.id == second.head_pane && pane.workspace == second.workspace),
    "the second turn's head pane stands in the workspace made again"
  );

  apart_places::end(&ground, &name).await;

  assert!(node.workspaces().is_empty(), "the session's end closes its workspace on the node");
}
