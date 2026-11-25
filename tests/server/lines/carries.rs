//! What a role's run carries (docs/launch-lines.md, 「What a line carries」; docs/agent-roles.md, 「What a role row
//! holds」): for every role the policy seeds, the launch its line composes holds the pieces of its system prompt in
//! the order its line's carries name them and no other, reads the setting sources they name, keeps or drops the
//! program's own commands as they say, takes its skills from where they say, and denies every subagent type of
//! the program's they do not keep.

use std::collections::HashMap;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::job_run::claude::turn::runner_command;
use agency_server::lines::Trees;
use agency_server::lines::claude::{line_denied, worker_args};
use agency_server::lines::worker::role_run;
use agency_server::places::Ground;
use agency_server::seeds::deployment;
use agency_server::translate::{Purpose, composed};
use agent_session::{Apart, Mapping, Resume, WorkParts};
use harness_providers::Machine;
use policy::providers::claude_code::CLAUDE_CODE;
use policy::roles::ROLES;
use policy::run_lines::{Carries, McpSource, RUN_LINES, SkillSource};

use crate::support::scratch::Scratch;

const MODEL: &str = "claude-opus-5-5";
const INSTRUCTIONS: &str = "THE CALLER'S INSTRUCTIONS";

fn machine(home: &std::path::Path) -> Machine {
  let at = |path: &std::path::Path| path.to_string_lossy().into_owned();

  Machine::with_env(
    [
      ("HOME".to_owned(), at(home)),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
      ("BMO_DEPLOY_DIR".to_owned(), env!("CARGO_MANIFEST_DIR").to_owned()),
    ],
    home.join("data"),
  )
}

fn ground(scratch: &Scratch, machine: &Machine) -> Ground {
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());

  Ground::new(&service, machine, None)
}

fn work() -> WorkParts {
  WorkParts {
    provider: "claude-code".into(),
    model: MODEL.into(),
    mapping: Mapping {
      model: MODEL.into(),
      effort: Some("medium".into()),
      tier: None,
      answer_tokens: None,
    },
    resume: Resume::Fresh,
    cwd: String::new(),
    name: "run-1".into(),
    title: "a run".into(),
    chat: 0,
    unit: None,
    user: None,
    work_key: "carries".into(),
    hears_words: false,
    variables: Default::default(),
  }
}

// The word a flag stands before, where it stands.
fn after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
  args
    .iter()
    .position(|arg| arg == flag)
    .and_then(|at| args.get(at + 1))
    .map(String::as_str)
}

fn harness(trees: &Trees) -> String {
  std::fs::read_to_string(trees.harness_path()).unwrap().trim().to_owned()
}

// The subagent types a line's denied list names, of those the program brings, and those it leaves.
fn subagents_denied(denied: &str, carries: &Carries) {
  let denied: Vec<&str> = denied.split(',').collect();

  for kind in &CLAUDE_CODE.launch.agent_types {
    let named = denied.contains(&format!("Agent({kind})").as_str());

    assert_eq!(named, !carries.subagents.contains(kind), "{kind}: {denied:?}");
  }
}

// A runner line's run: its whole system prompt is its carries' pieces in order, its setting sources and its own
// commands are theirs, and it is handed no plugin and no settings.
async fn runner(ground: &Ground, trees: &Trees, role: &str, line: &str, carries: &Carries) {
  let apart = Apart {
    provider: "claude-code".into(),
    model: MODEL.into(),
    line: Some(line.into()),
    system_prompt: Some(INSTRUCTIONS.into()),
    role: Some(role.into()),
    ..Apart::default()
  };
  let (run, command) = runner_command(ground, &apart, &work(), None, "a-session").await.unwrap();
  let expected: Vec<String> = carries
    .prompt
    .iter()
    .map(|piece| match piece.as_str() {
      "harness" => harness(trees),
      "instructions" => INSTRUCTIONS.to_owned(),
      other => panic!("{role}: a runner line's piece {other}"),
    })
    .collect();
  let args = &command.args;

  assert_eq!(
    std::fs::read_to_string(&run.system_prompt).unwrap().trim_end(),
    expected.join("\n\n"),
    "{role}: its system prompt is its line's pieces and nothing else"
  );
  assert_eq!(
    after(args, "--setting-sources"),
    Some(carries.setting_sources_word().as_str()),
    "{role}"
  );
  assert_eq!(
    args.iter().any(|arg| arg == "--disable-slash-commands"),
    !carries.slash_commands,
    "{role}"
  );
  assert_eq!(after(args, "--tools"), Some(""), "{role}: no built-in tool, so no subagent type");
  assert!(carries.skills.is_empty(), "{role}");
  assert_eq!(carries.mcp, [McpSource::Line], "{role}");
  assert!(!args.iter().any(|arg| arg == "--plugin-dir" || arg == "--settings"), "{role}: {args:?}");
}

// A worker line's run: the platform's opening, then the role's prompt where it carries one, then the line's
// rules; the setting sources its carries name, its role's plugin, and every subagent type they do not keep
// denied.
async fn worker(ground: &Ground, trees: &Trees, role: &str, carries: &Carries) {
  let run = role_run(ground, trees, role, None, None, MODEL, "/work", "claude-code").await.unwrap();
  let args = worker_args(&run, &run.settings, None).unwrap();
  let whole = std::fs::read_to_string(&run.harnessed).unwrap();
  let rules = std::fs::read_to_string(&run.rules).unwrap();
  let own = agency_server::lines::role_prompt(&ground.db, role, Some("/work")).await;
  let provider = agency_server::agent_roles::provider_prompt(&ground.db, role, "claude-code")
    .await
    .unwrap();
  // The role layer: its prompt, then the text its role gives runs on this provider.
  let prompt = match (own, provider) {
    (Some(own), Some(added)) => Some(format!("{}\n\n{}", own.trim_end(), added.trim())),
    (own, added) => own.or(added),
  };
  let opening = harness(trees);

  assert_eq!(carries.prompt.first().map(String::as_str), Some("opening"));
  assert_eq!(carries.prompt.last().map(String::as_str), Some("rules"));
  let (whole, rules) = (whole.trim_end(), rules.trim());

  assert!(whole.starts_with(&opening), "{role}: it opens on the platform's opening");
  assert!(whole.ends_with(rules), "{role}: it ends on the line's rules");

  let between = whole[opening.len()..whole.len() - rules.len()].trim();

  // No organisation or personal layer stands for an agent of no account: what stands between the opening and the
  // rules is the role layer — its prompt and its provider prompt — and nothing where it carries neither.
  assert_eq!(between, prompt.as_deref().map(str::trim).unwrap_or_default(), "{role}");
  assert_eq!(
    after(&args, "--setting-sources"),
    Some(carries.setting_sources_word().as_str()),
    "{role}"
  );
  assert_eq!(carries.skills, [SkillSource::Role, SkillSource::Attached], "{role}");
  assert_eq!(carries.mcp, [McpSource::Line, McpSource::Attached], "{role}");
  assert!(after(&args, "--plugin-dir").is_some(), "{role}: its role's plugin");
  assert_eq!(after(&args, "--disallowedTools"), Some(line_denied("worker").as_str()), "{role}");
  subagents_denied(&line_denied("worker"), carries);
}

// A translate line's run: its role's prompt, then the purpose's instructions where the work has one, as the
// line's carries order them.
async fn translate(ground: &Ground, trees: &Trees, role: &str, carries: &Carries) {
  let prompt = agency_server::lines::role_prompt(&ground.db, role, None).await;
  let purpose = (role == "translator").then_some(Purpose::Document);
  let system = composed(trees, prompt.as_deref(), purpose).unwrap();

  assert_eq!(carries.prompt, ["role", "purpose"]);
  assert!(
    prompt.as_deref().is_some_and(|prompt| system.starts_with(prompt.trim())),
    "{role}: {system}"
  );
  assert_eq!(purpose.is_some(), system.len() > prompt.unwrap_or_default().trim().len(), "{role}");
  assert!(!system.contains(&harness(trees)), "{role}: no platform opening");
  assert!(
    carries.setting_sources.is_empty()
      && !carries.slash_commands
      && carries.skills.is_empty()
      && carries.mcp.is_empty()
  );
}

#[tokio::test]
async fn every_seeded_role_s_launch_is_what_its_line_carries_and_its_row_gives() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = machine(home.path());

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  let ground = ground(&scratch, &machine);
  let trees = Trees::of(&ground);

  for role in &ROLES.roles {
    let carries = RUN_LINES.carries(&role.line);

    assert!(
      !carries.prompt.is_empty(),
      "{}: its line {} declares what it carries",
      role.name,
      role.line
    );

    match role.line.as_str() {
      "search" | "browse" | "plain" => runner(&ground, &trees, &role.name, &role.line, carries).await,
      "worker" => worker(&ground, &trees, &role.name, carries).await,
      "translate" => translate(&ground, &trees, &role.name, carries).await,
      other => panic!("{}: no launch of the line {other} is checked", role.name),
    }
  }
}

// The chat line, every supervisor's: no setting source, the program's own commands kept, and every subagent type
// its carries do not keep denied beside its own denied tools.
#[test]
fn the_chat_line_denies_every_subagent_type_it_does_not_carry() {
  let carries = RUN_LINES.carries("chat");
  let denied = agency_server::lines::chat::denied();

  assert!(carries.setting_sources.is_empty() && carries.slash_commands);
  assert_eq!(carries.subagents, ["Explore", "general-purpose"]);
  assert_eq!(carries.skills, [SkillSource::Attached]);
  assert_eq!(carries.mcp, [McpSource::Line, McpSource::Attached]);
  subagents_denied(&denied, carries);

  for tool in &RUN_LINES.line("chat").unwrap().tools_denied {
    assert!(denied.split(',').any(|named| named == tool), "{tool}");
  }
}
