use std::collections::HashMap;
use std::path::Path;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::lines::Trees;
use agency_server::lines::chat::*;
use agency_server::lines::claude::Session;
use agency_server::places::Ground;
use agency_server::supervisor_limits::Limits;
use harness_providers::Machine;
use policy::chat_line::CHAT_LINE;

use super::token::{PARAGRAPH, tool_host};
use crate::support::rows::supervisor;
use crate::support::scratch::Scratch;

// A supervisor whose session is kept across every room or in each room as the scope says.
async fn chat_agent(scratch: &Scratch, scope: &str) {
  supervisor(scratch.db(), "chat-agent", &[("session_scope", scope.into())]).await;
}

// The line's words for a program whose files stand where it is handed them.
fn line<'a>(dir: &'a Path, system: &'a Path) -> Words<'a> {
  Words {
    line: "chat",
    model: "local-model@node",
    effort: "low",
    advisor: None,
    system,
    mcp: dir,
    plugin: None,
    hands: Hands::AllBut {
      denied: "WebSearch,mcp__job__job_item_create",
      kept: None,
    },
    host_commands: false,
  }
}

#[test]
fn every_conversational_agent_stands_on_the_chat_line_with_its_own_prompt_and_session() {
  let dir = tempfile::tempdir().unwrap();
  let system = dir.path().join("supervisor-a.md");
  let settings = dir.path().join("settings.json");

  std::fs::write(&system, "a supervisor").unwrap();
  std::fs::write(&settings, "{}").unwrap();

  let words = line(dir.path(), &system);
  let opened = standing_words(&words, &settings, &Session::Open("s-1".into())).unwrap();
  let resumed = standing_words(&words, &settings, &Session::Resume("s-1".into())).unwrap();
  let at =
    |args: &[String], flag: &str| args.iter().position(|word| word == flag).map(|at| args[at + 1].clone());

  assert_eq!(opened[0], "claude");
  assert!(!opened.iter().any(|word| word == "-p" || word == "--print"));
  assert!(!opened.iter().any(|word| word == "--disable-slash-commands"));
  assert_eq!(at(&opened, "--system-prompt-file"), Some(system.to_string_lossy().into_owned()));
  assert_eq!(at(&opened, "--permission-mode").as_deref(), Some("bypassPermissions"));
  assert_eq!(at(&opened, "--disallowedTools").as_deref(), Some("WebSearch,mcp__job__job_item_create"));
  assert!(!opened.iter().any(|word| word == "--allowedTools" || word == "--tools"), "{opened:?}");
  assert_eq!(at(&opened, "--settings"), Some(settings.to_string_lossy().into_owned()));
  assert_eq!(opened[opened.len() - 2..], ["--session-id".to_owned(), "s-1".to_owned()]);
  assert_eq!(resumed[resumed.len() - 2..], ["--resume".to_owned(), "s-1".to_owned()]);
  assert_eq!(opened[..opened.len() - 2], resumed[..resumed.len() - 2]);

  // The composed prompt stands whole in the program's own place; nothing is appended to the program's.
  let words = agency_server::lines::chat::words(&line(dir.path(), &system)).unwrap();

  assert_eq!(at(&words, "--system-prompt-file"), Some(system.to_string_lossy().into_owned()));
  assert!(!words.iter().chain(&opened).any(|word| word == "--append-system-prompt-file"));

  // A run that holds only what it is shown and allowed — a gate's judgement, shown nothing — runs in don't-ask
  // mode over its lists, and is denied nothing besides.
  let gate = agency_server::lines::chat::words(&Words {
    hands: Hands::Listed {
      shown: "",
      allowed: "",
    },
    ..line(dir.path(), &system)
  })
  .unwrap();

  assert_eq!(at(&gate, "--permission-mode").as_deref(), Some("dontAsk"));
  assert_eq!(at(&gate, "--tools").as_deref(), Some(""));
  assert_eq!(at(&gate, "--allowedTools").as_deref(), Some(""));
  assert!(!gate.iter().any(|word| word == "--disallowedTools"), "{gate:?}");
}

// The tree this checkout is, which the chat's documents and rulebooks are read out of.
fn tree() -> std::path::PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).canonicalize().unwrap()
}

#[test]
fn the_tool_hosts_paragraph_stands_right_after_the_brains_discipline_and_before_the_persona() {
  let home = tempfile::tempdir().unwrap();
  let trees = Trees {
    code: tree(),
    agents: home.path().join("agents"),
    source: home.path().to_path_buf(),
    data: home.path().join("data"),
    home: home.path().to_path_buf(),
  };
  let header = &CHAT_LINE.persona.header;
  let carried = chat_system(
    &trees,
    &ChatDocuments::default(),
    Some("the persona"),
    Some("the brain's discipline"),
    Some(PARAGRAPH),
    None,
  )
  .unwrap();
  let without = chat_system(
    &trees,
    &ChatDocuments::default(),
    Some("the persona"),
    Some("the brain's discipline"),
    None,
    None,
  )
  .unwrap();

  assert!(
    carried.ends_with(&format!("the brain's discipline\n\n{PARAGRAPH}\n\n{header}\nthe persona")),
    "{carried}"
  );
  assert!(without.ends_with(&format!("the brain's discipline\n\n{header}\nthe persona")));
  assert!(!without.contains(PARAGRAPH));
}

// A chat launch's system prompt as the chat line composes it on a machine whose tool host is the socket
// named.
async fn launched_system(scratch: &Scratch, home: &Path, socket: &Path) -> String {
  let run = launched(scratch, home, socket).await;

  std::fs::read_to_string(&run.system_path).unwrap().trim_end().to_owned()
}

// A chat launch's run as the chat line composes it on a machine whose tool host is the socket named.
async fn launched(scratch: &Scratch, home: &Path, socket: &Path) -> ChatRun {
  let at = |path: &Path| path.to_string_lossy().into_owned();
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), at(home)),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
      ("BMO_DEPLOY_DIR".to_owned(), at(&tree())),
      ("BMO_AGENTS_DIR".to_owned(), at(&home.join("agents"))),
      ("BMO_MCP_HOST_SOCKET".to_owned(), at(socket)),
      ("BMO_MCP_HOST_ISSUER_TOKEN".to_owned(), "issuer".to_owned()),
    ],
    home.join("data"),
  );
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let ground = Ground::new(&service, &machine, None);
  let launch = ChatLaunch {
    agent: "chat-agent",
    chat_id: -5,
    room: RoomContext {
      chat_id: -5,
      text: "the room's block".into(),
      mark: "[a]".into(),
    },
    via: None,
    resume: None,
    privileged: false,
    ping: false,
    base_persona: Some("the persona"),
    model: Some("claude-model"),
    cwd: Some(&at(home)),
  };
  chat_run(&ground, &machine, &launch, "hello", false).await.unwrap()
}

// The chat's whole system prompt opens on the platform's harness opening read out of the tree the line is
// composed from, then the role, and goes to Claude Code in the program's own place (docs/agent-documents.md,
// 「A supervisor of the window shape」).
#[tokio::test]
async fn a_chat_launch_opens_on_the_platform_s_harness_and_is_handed_whole() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let harness = std::fs::read_to_string(tree().join("prompts/harness.md")).unwrap();

  chat_agent(&scratch, "room").await;

  let run = launched(&scratch, home.path(), &home.path().join("nobody.sock")).await;
  let system = std::fs::read_to_string(&run.system_path).unwrap();
  let args = agency_server::lines::chat::args(&run).unwrap();
  let flag = args
    .iter()
    .position(|word| word == "--system-prompt-file")
    .expect("the whole-prompt flag");

  assert!(system.starts_with(&format!("{}\n\n", harness.trim())), "{system}");
  assert!(system.find("ABSOLUTE RULES").is_some_and(|rules| rules > harness.trim().len()));
  assert_eq!(args[flag + 1], run.system_path.to_string_lossy());
  assert!(!args.iter().any(|word| word == "--append-system-prompt-file"));

  // A program that is a harness of its own is handed the role and the persona with no opening of ours.
  let trees = Trees {
    code: tree(),
    agents: home.path().join("agents"),
    source: home.path().to_path_buf(),
    data: home.path().join("data"),
    home: home.path().to_path_buf(),
  };
  let on_dsh = run.system_parts.as_ref().unwrap().compose(&trees, &[]).unwrap();

  assert!(!on_dsh.contains(harness.trim()));
}

#[tokio::test]
async fn a_chat_launch_carries_the_tool_hosts_paragraph_where_the_host_answers() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("mcp-host.sock");
  let header = &CHAT_LINE.persona.header;

  tool_host(&socket, "issuer");
  chat_agent(&scratch, "room").await;

  let answered = launched_system(&scratch, home.path(), &socket).await;
  let unanswered = launched_system(&scratch, home.path(), &home.path().join("nobody.sock")).await;

  assert!(answered.ends_with(&format!("{PARAGRAPH}\n\n{header}\nthe persona")), "{answered}");
  assert!(!unanswered.contains(PARAGRAPH));
  assert!(unanswered.ends_with(&format!("\n\n{header}\nthe persona")));
}

// The chat's system prompt as this checkout composes it, with no persona and no paragraph of the brain's or
// the tool host's.
fn composed() -> String {
  let home = tempfile::tempdir().unwrap();
  let trees = Trees {
    code: tree(),
    agents: home.path().join("agents"),
    source: home.path().to_path_buf(),
    data: home.path().join("data"),
    home: home.path().to_path_buf(),
  };

  chat_system(&trees, &ChatDocuments::default(), None, None, None, None).unwrap()
}

// The section of the composed prompt under a heading, up to the next heading of the manual's shape.
fn section(system: &str, heading: &str) -> String {
  let at = system
    .find(&format!("\n{heading}:\n"))
    .unwrap_or_else(|| panic!("no {heading} section"));
  let rest = &system[at + heading.len() + 3..];
  let end = regex::Regex::new(r"\n\n[A-Z][A-Z ,()'-]+:")
    .unwrap()
    .find(rest)
    .map_or(rest.len(), |end| end.start());

  rest[..end].to_owned()
}

#[test]
fn no_document_of_the_chat_fixes_the_language_it_answers_in() {
  let system = composed();
  let todo = std::fs::read_to_string(tree().join("prompts/agents/chat/procedures/todo.md")).unwrap();
  let fixed = regex::Regex::new(
    r"(?i)\b(reply|answer|respond|refuse[ds]?|tell[^.\n]{0,40}) in (one short )?(your own )?Korean\b|short Korean (sentence|line)|your own Korean",
  )
  .unwrap();
  let language = section(&system, "THE LANGUAGE YOU ANSWER IN");

  for text in [&system, &todo] {
    assert!(!fixed.is_match(text), "{:?}", fixed.find(text).map(|found| found.as_str()));
  }
  assert!(language.contains("is your own judgement"), "{language}");
  assert!(language.contains("never make its writer an English speaker"), "{language}");
  assert!(!language.contains("ANSWER LANGUAGE"), "{language}");
  assert!(system.find("THE LANGUAGE YOU ANSWER IN:") < system.find("RESPONSE PROTOCOL:"));
}

#[test]
fn the_room_hears_what_the_agent_will_do_and_never_which_tool_does_it() {
  let system = composed();
  let tools = section(&system, "SPEAKING OF YOUR TOOLS");

  assert!(tools.contains("No message names a tool"), "{tools}");
  assert!(tools.contains("look it up, look into it or translate it"), "{tools}");
  assert!(tools.contains("in your own words"), "{tools}");
  // Nor where it knows a thing from: what it knows is its own knowledge, never something written somewhere.
  assert!(tools.contains("you say as your own knowledge"), "{tools}");
  assert!(
    tools.contains("Never say that something is written, recorded, noted, set or kept somewhere"),
    "{tools}"
  );
  assert!(!tools.contains('"'), "a rule, with no sentence for the agent to repeat: {tools}");
  assert!(system.find("SPEAKING OF YOUR TOOLS:") < system.find("RESPONSE PROTOCOL:"));
}

#[test]
fn the_manual_tells_what_an_answer_to_each_round_of_the_draft_process_is() {
  let system = composed();
  let drafting = section(&system, "DRAFTING A MESSAGE");

  for said in [
    "in rounds",
    "notes",
    "wrote into the brain",
    "still unknown",
    "tools are open in every round",
    "The first round has the most freedom",
    "search for it quietly",
    "Never tell the room that something may be out of date",
    "opens with <<SEND>> is the message",
    "<<SKIP>> anywhere in an answer ends it with nothing sent",
    "In the last round you write the message",
    // Before speaking to or naming a person, the agent reads what they are called: the turn's reference first,
    // then the brain's one exact read of their forms of address.
    "take what they are called from the turn's Reference for this message block",
    "`brain_list` with the subject `{\"type\": \"person\", \"key\": \"<their id>\"}`",
    "and the type `name`, with no words",
    "never by a form they asked not to be called",
  ] {
    assert!(drafting.contains(said), "{said}: {drafting}");
  }
  assert_eq!(drafting.trim().lines().count(), 1, "one paragraph: {drafting}");

  // The duplicate rule is the answer-content rulebook's and the voice of every round the persona header's: each
  // is said once in the prompt, where it belongs.
  for elsewhere in ["counts as said", "drafted and not sent", "in the voice the persona sets"] {
    assert!(!drafting.contains(elsewhere), "{elsewhere}: {drafting}");
  }
}

#[test]
fn a_draft_that_was_never_sent_is_no_message_the_duplicate_rule_holds() {
  let rulebook = std::fs::read_to_string(tree().join("prompts/rules/answer-content.md")).unwrap();

  assert!(rulebook.contains("Only a message actually sent to the room counts as said"));
  assert!(rulebook.contains("Where an\n    answer was drafted and not sent, the next turn says it"));
  assert!(rulebook.contains("Do not do again what was already done"));
}

#[test]
fn a_plain_answer_is_the_default_and_a_reply_targets_the_message_being_answered() {
  let capabilities = std::fs::read_to_string(tree().join("prompts/agents/chat/capabilities.md")).unwrap();

  assert!(capabilities.contains("Your answer goes out as a plain message: that is the default."));
  assert!(
    capabilities.contains("a reply only where the room has moved past the message you are answering")
  );
  assert!(capabilities.contains("Never send a reply in the owner's private chat."));
  assert!(capabilities.contains("<<REPLY:<number>>>, the number the transcript shows"));
  assert!(capabilities.contains("never a message it does not address; where no message fits, send plain"));
  assert!(capabilities.contains("One reply per answer at most."));
  assert!(capabilities.contains("whether to reply is your own judgement, never a count of lines"));
  assert!(!capabilities.contains("9166"), "the rule names no example number");
}

#[test]
fn an_unfolded_answer_is_the_default_and_a_fold_is_the_agents_own_mark() {
  let capabilities = std::fs::read_to_string(tree().join("prompts/agents/chat/capabilities.md")).unwrap();
  let section = capabilities.split("CAPABILITY foldable: ").nth(1).expect("the fold's section");

  assert!(
    section.contains("Fold an answer whose body runs long behind a lead the room reads first"),
    "{section}"
  );
  assert!(section.contains("past about 600 characters or eight lines"), "{section}");
  assert!(section.contains("folds however warmly it is written"), "{section}");
  assert!(section.contains("Leave unfolded a short answer"), "{section}");
  assert!(section.contains("end the answer with <<FOLD>>"), "{section}");
  assert!(section.contains("nothing folds an answer you leave unmarked"), "{section}");
  assert!(
    capabilities.contains("or for folding where its dialect\nfolds"),
    "the section stands only there"
  );
}

#[test]
fn no_member_holds_the_owners_authority_in_a_room_and_no_block_carries_room_rules() {
  let rules = std::fs::read_to_string(tree().join("prompts/agents/chat/rules.md")).unwrap();
  let absolute = std::fs::read_to_string(tree().join("prompts/rules/absolute-rules.md")).unwrap();

  // A member's request beyond what everyone may use is filed for the owner's approval, never carried out.
  assert!(rules.contains("waits for the owner's approval"), "{rules}");
  assert!(absolute.contains("No other member acts on the owner's authority"), "{absolute}");

  // No record of a room's permissions is named anywhere.
  for text in [&rules, &absolute] {
    assert!(!text.contains("ROOM RULES"), "{text}");
    assert!(!text.contains("GRANTS IN FORCE"), "{text}");
    assert!(!text.contains("room_grant"), "{text}");
  }

  // The system's secrets open to the owner alone.
  let secrecy = absolute.lines().find(|line| line.starts_with("5. SYSTEM SECRECY.")).unwrap();

  assert!(
    secrecy.contains("The owner's own question is the exception and is answered."),
    "{secrecy}"
  );
  assert!(secrecy.contains("Everyone else is refused wherever they ask"), "{secrecy}");
}

#[test]
fn the_deploy_waits_on_plain_quiet_and_a_worker_never_deploys() {
  let trees = tree();
  let text = std::fs::read_to_string(trees.join("testbed.toml")).unwrap();
  let declaration: toml::Value = toml::from_str(&text).unwrap();
  let quiet = declaration["deploy"]["quiet"].as_str().unwrap();

  assert!(!quiet.contains("--with-jobs"), "the deploy counts no job: {quiet}");
  let script = std::fs::read_to_string(trees.join("deploy/quiet")).unwrap();
  assert!(script.contains("bmo-agency quiet"), "agency's own quiet is asked: {script}");
  assert!(!script.contains("bmo-receiver"), "the receiver's quiet is its own: {script}");
  // A job's close ends at the merge, and the supervisor puts a repository's batch live through the hosted deploy
  // tool; a batch that fails comes back to the item whose commit broke it.
  let rules = std::fs::read_to_string(trees.join("prompts/agents/worker/standing-rules.md")).unwrap();

  assert!(rules.contains("A job never deploys, and ends at the merge."));
  assert!(rules.contains("that item comes back to be mended"));
}

#[test]
fn a_repository_is_added_to_the_live_machine_by_the_supervisor_alone_on_the_owner_s_approval() {
  let read = |path: &str| std::fs::read_to_string(tree().join(path)).unwrap();
  // Adding a repository is the platform's own work, which the platform administrator's supervisors read
  // (docs/prompt-layers.md, 「The platform's rules」).
  let manual = read("prompts/platform/operations.md");
  let rules = read("prompts/agents/worker/standing-rules.md");

  for said in [
    "`bmo install --add <repository>`, is yours alone and only on the owner's approval for that one repository.",
    "no item's approval or standing word covers it",
    "a later repository is asked again",
    "run it only once he has approved that repository in his own words",
    "A job or a worker never runs it",
  ] {
    assert!(manual.contains(said), "{said}: {manual}");
  }
  assert!(rules.contains("**A job never installs.**"), "{rules}");
  assert!(rules.contains("`--status`\n  and a `--dry-run` change nothing"), "{rules}");
  assert!(rules.contains("`bmo job ask` and waits on the answer"), "{rules}");
}

#[test]
fn a_saved_secret_is_used_by_its_name_before_anybody_is_asked() {
  let read = |path: &str| std::fs::read_to_string(tree().join(path)).unwrap();
  let rules = read("prompts/agents/worker/standing-rules.md");
  let trailer = read("prompts/agents/worker/trailer.md");
  let manual = read("prompts/agents/chat/manual.md");
  let procedure = read("prompts/agents/chat/procedures/secrets.md");

  // A job looks among the owner's saved secrets first, hands one to its consumer by its path, never shows it,
  // and asks its supervisor only where none fits.
  for said in [
    "is first looked for among the secrets the owner has\n  saved",
    "`ls \"$BMO_DATA_DIR\"/env`",
    "`cut -s -d= -f1`",
    "Hand the consumer the file's path",
    "never write it into the repository, a commit, a note, a report",
    "`bmo/node_exec`'s own argument for saved secrets",
    "**Nobody types or signs in by hand for what a saved secret covers.**",
    "**Only where no saved secret fits** does the job ask its supervisor with `bmo job ask`",
  ] {
    assert!(rules.contains(said), "{said}");
  }
  assert!(
    trailer.contains("a password or an account of his that no saved secret holds"),
    "{trailer}"
  );

  // The supervisor looks for a saved one before it sends a request link, and never puts a sign-in a saved
  // secret covers on the owner.
  assert!(manual.contains("look for it first among the ones already saved"), "{manual}");
  assert!(
    manual.contains("never wait on him to sign in by hand, for what a saved secret covers"),
    "{manual}"
  );
  assert!(manual.contains("Where none is saved, never ask for its value in a room."), "{manual}");
  let first = procedure
    .find("## First, look for it among the saved ones")
    .expect("the procedure's first step");
  let request = procedure.find("## What the request names").expect("the request's section");

  assert!(first < request, "the saved ones are looked at before a request is written");
}

#[test]
fn a_research_ask_continuing_a_report_names_the_report_it_follows_from() {
  let manual = std::fs::read_to_string(tree().join("prompts/agents/chat/manual.md")).unwrap();
  let line = manual
    .lines()
    .find(|line| line.contains("research/ask"))
    .expect("the research unit's line");

  for said in [
    "the last one you reported in this room",
    "follows the item that report was told for",
    "document that report's document",
    "because one line",
  ] {
    assert!(line.contains(said), "{said}: {line}");
  }
  assert!(!line.contains('"'), "a rule, with no sentence for the agent to repeat: {line}");
}

#[test]
fn the_agent_approves_every_sound_draft_itself_and_names_each_item_s_repository() {
  let manual = std::fs::read_to_string(tree().join("prompts/agents/chat/manual.md")).unwrap();
  let todo = std::fs::read_to_string(tree().join("prompts/agents/chat/procedures/todo.md")).unwrap();
  let queue = manual
    .split("THE QUEUE (")
    .nth(1)
    .and_then(|rest| rest.split("\n\n").next())
    .expect("the manual's THE QUEUE");

  // An approver's approval opens a draft, and the agent is one for routine work it can judge.
  for said in [
    "the owner's on the console, or your own",
    "Approve a draft yourself once you have read it whole and found it sound",
    "a worker's follow-up raised from a run",
    "a dependency's security fix or version bump that the repository's tests cover",
    "a repair of something broken",
    "housekeeping a repository's own document prescribes",
    "is first given the repository its brief's work is in",
    "with its number and why",
    "Leave to the owner, as a draft waiting on his approval, only work that needs his own authority",
    "a credential, an account or a login he has not already given (a secret he saved is that authority \
     given, and work that uses it by its name waits on nobody), money, his own files, people's data, or a \
     judgement of taste",
    "never a member's ask",
    "An item you approved yourself is yours to launch",
    // An item names its repository; one change reaching several is one item naming them all.
    "file_item's repository",
    "Host work names none",
    "One change that reaches two or more repositories is one item",
    "its repositories label naming every repository it reaches",
  ] {
    assert!(queue.contains(said), "{said}: {queue}");
  }
  assert!(!queue.contains("is your recommendation"), "{queue}");

  for said in [
    "**Your approval opens a draft.**",
    "**on your own judgement, for every draft you read whole and find sound:**",
    "A draft is never left",
    "**The item names its repository:** `repository`",
    "`series=<name>` label",
    "**An item you approved yourself is yours to launch,** as its repository's worker frees",
    "relay each run's end",
  ] {
    assert!(todo.contains(said), "{said}");
  }
  assert!(!todo.contains("is your recommendation"));
}

#[test]
fn a_result_for_a_person_is_in_the_asker_s_language_and_what_agents_read_of_each_other_is_english() {
  let language = section(&composed(), "THE LANGUAGE YOU ANSWER IN");
  let prompt = |path: &str| std::fs::read_to_string(tree().join("prompts").join(path)).unwrap();
  let rule = "A result written for a person is written in the language of the person who asked";

  assert!(
    language.contains("You, answering people in rooms, write in a speaker's language"),
    "{language}"
  );
  // A result written for a person is in the asker's language, which the item's brief names for a run
  // (docs/agent-documents.md, docs/worker-language.md).
  assert!(language.contains(rule), "{language}");
  assert!(
    language.contains("every brief names the language of the person who asked"),
    "{language}"
  );
  assert!(prompt("agents/chat/procedures/todo.md").contains("(`Results in: <language>`)"));
  assert!(prompt("agents/worker/standing-rules.md").contains(rule));
  assert!(prompt("agents/worker/standing-rules.md").contains("Everything else a job writes is English"));
  assert!(
    prompt("agents/worker/plugin/skills/bmo-closing-a-job/SKILL.md")
      .contains("written in the language of the\nperson who asked, as the item's brief names it")
  );
  assert!(prompt("agents/pieces/handoff.md").contains("Write it in English"));
}

// A room's session handed over consolidates its brain in the handoff turn before it writes its note, bounded as
// at a fold and leaving the fold's report to the receiver (docs/supervisors.md, 「Its limit」).
#[test]
fn a_room_session_handed_over_consolidates_its_brain_before_its_note() {
  let handoff = std::fs::read_to_string(tree().join("prompts/agents/pieces/handoff.md")).unwrap();
  let at = |words: &str| handoff.find(words).unwrap_or_else(|| panic!("{words:?} is missing: {handoff}"));

  assert!(at("brain_unconsolidated") < at("brain_learn"), "{handoff}");
  assert!(at("brain_learn") < at("brain_consolidated"), "{handoff}");
  assert!(at("brain_consolidated") < at("Then write that note"), "{handoff}");
  at("never all of them");
  at("as its grounds");
  at("Do not call brain_fold");
  assert!(!handoff.contains("Use no tool for it."), "{handoff}");
}

// A chat with no folder of its own works in the deploy tree's link, whose path stays the same across
// releases, so the resident program's record of its session is taken up again after a restart rather
// than a fresh session opened under a new release path (docs/sessions.md, 「A restart hands the session
// over」).
#[tokio::test]
async fn a_chat_with_no_folder_of_its_own_works_in_the_deploy_link() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let deploy = tree();
  let at = |path: &Path| path.to_string_lossy().into_owned();
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), at(home.path())),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
      ("BMO_DEPLOY_DIR".to_owned(), at(&deploy)),
      ("BMO_AGENTS_DIR".to_owned(), at(&home.path().join("agents"))),
      ("BMO_SOURCE_DIR".to_owned(), at(&home.path().join("a-release"))),
    ],
    home.path().join("data"),
  );
  let variables: HashMap<String, String> = [("AGENCY_DEPLOY_DIR".to_owned(), at(&deploy))].into();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let ground = Ground::new(&service, &machine, None);

  chat_agent(&scratch, "agent").await;

  let launch = ChatLaunch {
    agent: "chat-agent",
    chat_id: -5,
    room: RoomContext {
      chat_id: -5,
      text: "the room's block".into(),
      mark: "[a]".into(),
    },
    via: None,
    resume: None,
    privileged: false,
    ping: false,
    base_persona: None,
    model: Some("claude-model"),
    cwd: None,
  };
  let run = chat_run(&ground, &machine, &launch, "hello", false).await.unwrap();

  assert_eq!(run.cwd, at(&deploy));
  assert_ne!(run.cwd, at(&home.path().join("a-release")));
}

#[test]
fn the_tools_paragraph_names_the_programs_own_tools_the_servers_and_the_entries_shown() {
  let paragraph = tools_paragraph(
    &["bash", "read"],
    &["bmo".to_owned(), "job".to_owned()],
    &["bmo/say".to_owned(), "web/web_fetch".to_owned()],
  );

  assert!(paragraph.contains("bash, read"), "{paragraph}");
  assert!(paragraph.contains("bmo, job"), "{paragraph}");
  assert!(paragraph.contains("mcp__bmo__say, mcp__web__web_fetch"), "{paragraph}");

  // The read path is named exactly, by the names this program calls the tools.
  for named in [
    "mcp__bmo__brain_list with its subject and type",
    "mcp__bmo__brain_similar with the question's own words",
    "mcp__gate__reference_search, then mcp__gate__reference_piece",
    "A tool shown to you by name is called as that tool, never through your shell or `bmo use`",
    "only a hosted tool not shown to you is reached with `bmo use`, as the opening says",
  ] {
    assert!(paragraph.contains(named), "{named}: {paragraph}");
  }
  // How `bmo use` finds, reads and calls a tool, the session's token and the units' stores are the harness
  // opening's to say, once.
  for opening in ["bmo use --tools", "--describe", "token", "store, port, database or files"] {
    assert!(!paragraph.contains(opening), "{opening}: {paragraph}");
  }
}

// The role follows the harness opening as one prompt: what the opening says (how words reach the session, the
// deploy tree against a checkout, how a refusal is read, how a hosted tool is found) stands there alone, and no
// statement the current documents have left behind stays in the role.
#[test]
fn the_role_says_nothing_the_opening_says_and_nothing_no_longer_true() {
  let system = composed();
  let procedures = ["todo.md", "fasttrack.md", "overtime.md", "secrets.md"]
    .map(|name| std::fs::read_to_string(tree().join("prompts/agents/chat/procedures").join(name)).unwrap());
  let capabilities = std::fs::read_to_string(tree().join("prompts/agents/chat/capabilities.md")).unwrap();

  for gone in [
    "headless",
    "never from the checkout your shell stands in",
    "never take a tool's name from memory",
    "A tool call that fails says why in its own words",
    "Your program's own plan list",
    "Run privilege line",
    "run-header lines",
    "BMO QUEUE",
    "formatting gate",
    "WORDING.md",
    "a separate check",
    "an item event does, and nothing else does",
    "is not folded",
    "toolbook",
    "the ABSOLUTE RULES at the top",
    "Put it to the owner in his private chat,",
    "`bmo.db`",
  ] {
    assert!(!system.contains(gone), "{gone}");
  }
  for text in procedures.iter().chain([&capabilities]) {
    for gone in ["no room outbox", "no tool that sends", "docs/agent-control.md", "(the harness)"] {
      assert!(!text.contains(gone), "{gone}");
    }
  }
  assert!(!CHAT_LINE.persona.header.contains("at the top of this prompt"));

  // The capability document keeps its sections as headings of their own, and names no sending list: the tool
  // shows the sending entries in every room, and the road judges each send itself.
  for kept in ["CAPABILITY replyTo:", "CAPABILITY foldable:"] {
    assert!(capabilities.contains(kept), "{kept}: {capabilities}");
  }
  assert!(!capabilities.contains("SENDING TOOLS"), "{capabilities}");

  // Each of the three kinds of turn says what it asks and what ends it.
  let turn = section(&system, "THE TURN AND THE ROOM");
  for said in [
    "a room's round carries a room's words",
    "it ends with the process's answer",
    "<<SEND>> from any round that may send",
    "\"Quiet instruction from the system\" block, closed by END OF QUIET INSTRUCTION",
    "It ends once carried out",
    "A line the owner types into your pane",
    "Your answer to it ends it",
  ] {
    assert!(turn.contains(said), "{said}: {turn}");
  }
  assert!(section(&system, "RESPONSE PROTOCOL").contains("under YOUR QUEUE"));
}

#[test]
fn the_chats_prompt_names_the_tools_of_the_program_it_is_composed_for_and_the_manual_names_none() {
  let home = tempfile::tempdir().unwrap();
  let trees = Trees {
    code: tree(),
    agents: home.path().join("agents"),
    source: home.path().to_path_buf(),
    data: home.path().join("data"),
    home: home.path().to_path_buf(),
  };
  let parts = SystemParts {
    servers: vec!["bmo".to_owned(), "web".to_owned()],
    ..SystemParts::default()
  };
  let on_claude = parts.compose(&trees, &[CHAT_LINE.tools.listed.as_str()]).unwrap();
  let on_dsh = parts.compose(&trees, &agency_server::lines::dsh::SESSION_TOOLS).unwrap();
  let manual = std::fs::read_to_string(tree().join("prompts/agents/chat/manual.md")).unwrap();

  assert!(on_claude.contains(&format!("own tools are {}.", CHAT_LINE.tools.listed)), "{on_claude}");
  assert!(on_dsh.contains("own tools are bash, read, write, edit"), "{on_dsh}");
  assert!(!on_dsh.contains("ListMcpResourcesTool"));
  assert!(!manual.contains("ListMcpResourcesTool") && !manual.contains("Your tools are Bash"));
}

#[test]
fn every_program_s_chat_prompt_is_the_role_then_the_persona_whose_voice_holds_in_every_round() {
  let home = tempfile::tempdir().unwrap();
  let trees = Trees {
    code: tree(),
    agents: home.path().join("agents"),
    source: home.path().to_path_buf(),
    data: home.path().join("data"),
    home: home.path().to_path_buf(),
  };
  let parts = SystemParts {
    persona: Some("To the owner, the persona's own voice.".to_owned()),
    servers: vec!["bmo".to_owned()],
    ..SystemParts::default()
  };
  let programs: [(&str, &[&str]); 2] = [
    ("Claude Code", &["Bash", "ListMcpResourcesTool"]),
    ("dsh", &agency_server::lines::dsh::SESSION_TOOLS),
  ];

  for (program, tools) in programs {
    let system = parts.compose(&trees, tools).unwrap();
    let persona = system
      .find(&CHAT_LINE.persona.header)
      .unwrap_or_else(|| panic!("{program}: no persona"));

    // The role opens with the chat's own opening and its absolute rules, the persona closes it, and its header
    // says the voice it sets holds for every message, a report among them.
    assert!(system.find("ABSOLUTE RULES").is_some_and(|rules| rules < persona), "{program}");
    assert!(system.trim_end().ends_with("To the owner, the persona's own voice."), "{program}");
    assert!(CHAT_LINE.persona.header.contains("in every round"), "{program}");
    assert!(CHAT_LINE.persona.header.contains("the owner included"), "{program}");
    assert!(CHAT_LINE.persona.header.contains("a report"), "{program}");

    // No past case, no translation hint and no gate's rulebook stands in the role.
    for gone in ["real case", "translate it through gate/translate", "formatting gate reads"] {
      assert!(!system.contains(gone), "{program}: {gone}");
    }

    // What the room hears of the tools: never that a thing is written or recorded.
    assert!(system.contains("Never say that something is written, recorded"), "{program}");
  }
}

#[tokio::test]
async fn the_chat_s_servers_file_items_for_its_agent_and_the_job_plugin_serves_no_item_creation() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let at = |path: &Path| path.to_string_lossy().into_owned();
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), at(home.path())),
      ("BMO_AGENTS_DIR".to_owned(), at(&home.path().join("agents"))),
      ("BMO_JOB_PLUGIN".to_owned(), "/opt/bmo-job".to_owned()),
      ("BMO_JOB_URL".to_owned(), "http://[::1]:1".to_owned()),
      ("BMO_AGENCY_URL".to_owned(), "http://[::1]:2".to_owned()),
      ("BMO_EXPLORER_PLUGIN".to_owned(), "/opt/bmo-explorer".to_owned()),
      ("BMO_DATA_DIR".to_owned(), at(&home.path().join("data"))),
      ("AGENCY_CONTROL_TOKEN".to_owned(), "control-token-value".to_owned()),
      ("BMO_JOB_TOKEN".to_owned(), "job-token-value".to_owned()),
    ],
    home.path().join("data"),
  );
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let ground = Ground::new(&service, &machine, None);
  let path = mcp_config(&ground, &Trees::of(&ground), "chat-agent", &Limits::open()).unwrap();
  let text = std::fs::read_to_string(&path).unwrap();

  // No token stands in the configuration by value: each server is handed the file it stands in, and the job
  // plugin, which reads its token from its environment alone, is started behind a shell that reads the file.
  assert!(!text.contains("control-token-value") && !text.contains("job-token-value"), "{text}");
  let config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
  let args = |server: &str| -> Vec<String> {
    config["mcpServers"][server]["args"]
      .as_array()
      .unwrap_or_else(|| panic!("{server}: {config}"))
      .iter()
      .map(|arg| arg.as_str().unwrap().to_owned())
      .collect()
  };
  let job = args("job");
  let agency = args("agency");
  let explorer = args("explorer");

  assert!(path.ends_with("chat.chat-agent.mcp.json"), "{}", path.display());
  let root = at(&home.path().join("data"));
  assert_eq!(explorer, vec!["mcp".to_owned(), "--data-root".to_owned(), root]);
  assert_eq!(config["mcpServers"]["explorer"]["command"].as_str(), Some("/opt/bmo-explorer"));
  assert!(job.windows(2).any(|pair| pair == ["--without", "job_item_create"]), "{job:?}");
  assert!(agency.windows(2).any(|pair| pair == ["--agent", "chat-agent"]), "{agency:?}");
  assert_eq!(config["mcpServers"]["job"]["command"].as_str(), Some("/bin/sh"));
  assert_eq!(job[0], "-c");
  assert_eq!(job[2], "/opt/bmo-job");
  assert_eq!(job[3], "mcp");

  let job_file = config["mcpServers"]["job"]["env"]["BMO_JOB_TOKEN_FILE"].as_str().unwrap();
  let control_file = config["mcpServers"]["agency"]["env"]["AGENCY_CONTROL_TOKEN_FILE"].as_str().unwrap();

  assert_eq!(std::fs::read_to_string(job_file).unwrap(), "job-token-value");
  assert_eq!(std::fs::read_to_string(control_file).unwrap(), "control-token-value");

  // Another supervisor's configuration carries the same servers, agency's own plugin among them, under its
  // own name.
  let supervisor = mcp_config(&ground, &Trees::of(&ground), "supervisor-agent", &Limits::open()).unwrap();
  let config: serde_json::Value =
    serde_json::from_str(&std::fs::read_to_string(&supervisor).unwrap()).unwrap();
  let agency = config["mcpServers"]["agency"]["args"].as_array().unwrap();

  assert!(agency.windows(2).any(|pair| pair == ["--agent", "supervisor-agent"]), "{config}");
  assert!(config["mcpServers"]["job"].is_object(), "{config}");
  assert!(config["mcpServers"]["explorer"].is_object(), "{config}");

  // A supervisor whose tool limit leaves filing jobs out holds neither the job service's server nor agency's own,
  // and keeps every server the limit does not close.
  let limits = Limits::new(true, true, Some(vec!["web.pages".to_owned()]));
  let limited = mcp_config(&ground, &Trees::of(&ground), "limited-agent", &limits).unwrap();
  let config: serde_json::Value =
    serde_json::from_str(&std::fs::read_to_string(&limited).unwrap()).unwrap();

  assert!(
    config["mcpServers"]["job"].is_null() && config["mcpServers"]["agency"].is_null(),
    "{config}"
  );
  assert!(config["mcpServers"]["explorer"].is_object(), "{config}");
}

// The job plugin's `mcp` verb as the job service's command-line document states it (its docs/cli.md,
// 「`bmo-job mcp`」): the service's address required, a holder and queues each needing the other. The token is
// read from the environment, which the stand-in records apart.
fn job_mcp_verb() -> clap::Command {
  use clap::{Arg, ArgAction, Command};

  Command::new("bmo-job").subcommand_required(true).subcommand(
    Command::new("mcp")
      .arg(Arg::new("service").long("service").required(true))
      .arg(Arg::new("token").long("token"))
      .arg(Arg::new("holder").long("holder").requires("queue"))
      .arg(Arg::new("queue").long("queue").action(ArgAction::Append).requires("holder"))
      .arg(Arg::new("channel").long("channel"))
      .arg(Arg::new("without").long("without").action(ArgAction::Append)),
  )
}

// The started server entry, killed and reaped when it is dropped; the shell in front of the plugin replaces
// itself with it, so it is the one process.
struct Started(std::process::Child);

impl Drop for Started {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

#[tokio::test]
async fn the_job_server_a_supervisor_s_config_names_starts_and_answers_on_the_plugin_s_grammar() {
  use std::io::{BufRead, Write};
  use std::os::unix::fs::PermissionsExt;
  use std::time::{Duration, Instant};

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let at = |path: &Path| path.to_string_lossy().into_owned();

  // A stand-in for the job plugin: it records the words it was started on and the token it was handed, answers
  // the harness's first request and ends once its input closes, as the plugin does.
  let plugin = home.path().join("bmo-job");
  let recorded = home.path().join("args");
  let token = home.path().join("token");
  std::fs::write(
    &plugin,
    format!(
      "#!/bin/sh\nprintf '%s\\0' \"$@\" > '{}'\nprintf %s \"$BMO_JOB_TOKEN\" > '{}'\nIFS= read -r request\n\
       printf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"serverInfo\":{{\"name\":\"job\"}}}}}}'\n\
       while IFS= read -r rest; do :; done\n",
      at(&recorded),
      at(&token),
    ),
  )
  .unwrap();
  std::fs::set_permissions(&plugin, std::fs::Permissions::from_mode(0o755)).unwrap();

  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), at(home.path())),
      ("BMO_AGENTS_DIR".to_owned(), at(&home.path().join("agents"))),
      ("BMO_JOB_PLUGIN".to_owned(), at(&plugin)),
      ("BMO_JOB_URL".to_owned(), "http://[::1]:1".to_owned()),
      ("BMO_AGENCY_URL".to_owned(), "http://[::1]:2".to_owned()),
      ("BMO_DATA_DIR".to_owned(), at(&home.path().join("data"))),
      ("BMO_JOB_TOKEN".to_owned(), "job-token-value".to_owned()),
    ],
    home.path().join("data"),
  );
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let ground = Ground::new(&service, &machine, None);
  let path = mcp_config(&ground, &Trees::of(&ground), "chat-agent", &Limits::open()).unwrap();
  let config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
  let job = &config["mcpServers"]["job"];
  let words = |value: &serde_json::Value| -> Vec<String> {
    value.as_array().unwrap().iter().map(|word| word.as_str().unwrap().to_owned()).collect()
  };

  // Started as a harness starts a stdio server: its command on its words, on an environment holding only the
  // entries the configuration writes.
  let mut command = std::process::Command::new(job["command"].as_str().unwrap());
  command.args(words(&job["args"])).env_clear();
  for (name, value) in job["env"].as_object().into_iter().flatten() {
    command.env(name, value.as_str().unwrap());
  }
  let mut started = Started(
    command
      .stdin(std::process::Stdio::piped())
      .stdout(std::process::Stdio::piped())
      .stderr(std::process::Stdio::null())
      .spawn()
      .unwrap(),
  );

  let mut input = started.0.stdin.take().unwrap();
  writeln!(input, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{}}}}"#).unwrap();
  let output = started.0.stdout.take().unwrap();
  let (answered, answer) = std::sync::mpsc::channel();
  std::thread::spawn(move || {
    let mut line = String::new();
    let _ = std::io::BufReader::new(output).read_line(&mut line);
    let _ = answered.send(line);
  });
  let line = answer.recv_timeout(Duration::from_secs(10)).expect("the job server never answered");
  let reply: serde_json::Value = serde_json::from_str(&line).unwrap_or_else(|_| panic!("{line:?}"));

  assert_eq!(reply["result"]["serverInfo"]["name"], "job", "{reply}");

  // The server ends once the harness closes its input.
  drop(input);
  let bound = Instant::now() + Duration::from_secs(10);
  let ended = loop {
    if let Some(status) = started.0.try_wait().unwrap() {
      break status;
    }
    assert!(Instant::now() < bound, "the job server did not end when its input closed");
    std::thread::sleep(Duration::from_millis(20));
  };
  assert!(ended.success(), "{ended:?}");

  // The plugin was handed the token the configuration's file holds, and started on words its `mcp` verb takes.
  assert_eq!(std::fs::read_to_string(&token).unwrap(), "job-token-value");
  let handed: Vec<String> = std::fs::read_to_string(&recorded)
    .unwrap()
    .split_terminator('\0')
    .map(str::to_owned)
    .collect();
  let mut grammar = job_mcp_verb();
  let parsed = grammar
    .try_get_matches_from_mut(std::iter::once("bmo-job".to_owned()).chain(handed.iter().cloned()))
    .unwrap_or_else(|error| panic!("{handed:?}: {error}"));

  assert_eq!(parsed.subcommand_name(), Some("mcp"), "{handed:?}");
}

// "asked to speak, the supervisor says its own answer aloud with speak, makes any other sound with voice/generate as
// a file that moves no face, leaves the spoken words to the voice unit, and takes no tool call in a briefing turn"
#[test]
fn the_manual_speaks_with_speak_generates_other_sounds_and_keeps_a_briefing_turn_tool_free() {
  let system = composed();
  let voice = section(&system, "A LINE FOR THE VOICE");

  for said in [
    "`bmo use bmo/speak` with no argument, so it is said in the neutral voice",
    "`--arg emotion=<a feeling>` only where the person asks for one",
    "goes out as a voice note in your voice",
    "your face speaks it",
    "The voice unit derives how each word is said",
    "speak is refused with the road's reason",
    "bmo use voice/generate --arg voiceName=",
    "your face does not move for it",
    "your own voice speaks only through speak",
    "That turn takes no tool call",
  ] {
    assert!(voice.contains(said), "{said}: {voice}");
  }
  for gone in ["You send no voice note", "--spoken", "bmo talk"] {
    assert!(!voice.contains(gone), "{gone}: {voice}");
  }
  assert!(!voice.contains('"'), "a rule, with no sentence for the agent to repeat: {voice}");
}

#[test]
fn words_handed_to_the_supervisor_are_said_as_its_rounds_message_with_their_facts_and_intent_kept() {
  let system = composed();
  let handed = section(&system, "WORDS HANDED TO YOU");

  for said in [
    "Every message a room receives comes from that room's supervisor",
    "hands it to you as an instruction, and nothing of it has reached the room",
    "names who handed it and what was asked of the room",
    "Say it as your round's message, in your own words and your own voice",
    "or answer <<SKIP>> where it is not worth saying",
    "Keep its facts exact: every number",
    "Keep the intent of whoever handed it",
    "A voice note or a spoken line handed is said as words in your message",
    "with no caption, and the caption's words go into your message",
  ] {
    assert!(handed.contains(said), "{said}: {handed}");
  }
  assert!(!handed.contains("fixed line"), "no fixed line is handed: {handed}");
  assert!(!handed.contains('"'), "a rule, with no sentence for the agent to repeat: {handed}");
}

// The words a flag stands before.
fn after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
  args.iter().position(|arg| arg == flag).map(|at| args[at + 1].as_str())
}

// The machine a supervisor's program is composed on: the deploy tree, the variables its chat servers are started
// from, and DeepSeek's member on agency's gateway, its key where the settings' key helper reads it.
fn launch_machine(home: &Path) -> Machine {
  use std::os::unix::fs::PermissionsExt;

  let at = |path: &Path| path.to_string_lossy().into_owned();
  let data = home.join("data");
  let artifact_token = home.join("artifact.token");
  let key = data.join("env/llm-gateway.key");
  let members = data.join("llm-gateway/members.json");

  std::fs::write(&artifact_token, "artifact-token-value").unwrap();
  std::fs::create_dir_all(key.parent().unwrap()).unwrap();
  std::fs::write(&key, "sk-gateway\n").unwrap();
  std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
  std::fs::create_dir_all(members.parent().unwrap()).unwrap();
  std::fs::write(
    &members,
    serde_json::json!([{
      "name": "deepseek-flash",
      "road": "deepseek",
      "label": "DeepSeek Flash",
      "provider": "deepseek",
      "model": "deepseek-flash",
      "api_base": "http://[::1]:4/v1",
      "key_file": "deepseek.key",
      "context_tokens": 131_072,
      "answer_tokens": 16_384,
      "thinking": true
    }])
    .to_string(),
  )
  .unwrap();

  Machine::with_env(
    [
      ("HOME".to_owned(), at(home)),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
      ("BMO_DEPLOY_DIR".to_owned(), at(&tree())),
      ("BMO_AGENTS_DIR".to_owned(), at(&home.join("agents"))),
      ("BMO_JOB_PLUGIN".to_owned(), "/opt/bmo-job".to_owned()),
      ("BMO_JOB_URL".to_owned(), "http://[::1]:1".to_owned()),
      ("BMO_AGENCY_URL".to_owned(), "http://[::1]:2".to_owned()),
      ("BMO_EXPLORER_PLUGIN".to_owned(), "/opt/bmo-explorer".to_owned()),
      ("BMO_DATA_DIR".to_owned(), at(&data)),
      ("BMO_ARTIFACT_URL".to_owned(), "http://127.0.0.1:3".to_owned()),
      ("BMO_ARTIFACT_TOKEN_FILE".to_owned(), at(&artifact_token)),
      ("BMO_ARTIFACT_PLUGIN".to_owned(), "/opt/bmo-artifact".to_owned()),
    ],
    data,
  )
}

/// What `chat-agent`'s program is started with: the words of a window-shaped turn on Claude Code's own road and
/// on DeepSeek's gateway road and of a pieces-shaped turn on that gateway road, each under the road it names,
/// and the refusal its turn meets on a program that is a harness of its own, where it meets one.
struct Launched {
  words: Vec<(&'static str, Vec<String>)>,
  on_own_harness: Option<String>,
}

async fn launched_supervisor(scratch: &Scratch, home: &Path) -> Launched {
  let machine = launch_machine(home);
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let ground = Ground::new(&service, &machine, None);
  let cwd = home.to_string_lossy().into_owned();
  let launch = ChatLaunch {
    agent: "chat-agent",
    chat_id: -5,
    room: RoomContext {
      chat_id: -5,
      text: "the room's block".into(),
      mark: "[a]".into(),
    },
    via: None,
    resume: None,
    privileged: true,
    ping: false,
    base_persona: Some("the persona"),
    model: Some("claude-model"),
    cwd: Some(&cwd),
  };
  let run = chat_run(&ground, &machine, &launch, "hello", false).await.unwrap();
  let on_claude = compose_prepared(&ground, &machine, &run, "hello", "run-1", &[], 1)
    .await
    .unwrap()
    .command
    .args;
  let door = Door::Gateway {
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
  };
  let step = agency_server::stand_in::Step {
    provider: "deepseek".into(),
    model: "deepseek-flash".into(),
    effort: "high".into(),
    thinking: true,
    answer_tokens: None,
  };
  let on_deepseek = compose_stand_in(&ground, &machine, &run, &step, &door, &[], 1)
    .await
    .unwrap()
    .command
    .args;
  let on_own_harness =
    compose_stand_in(&ground, &machine, &run, &step, &Door::Codex { thread: None }, &[], 1)
      .await
      .err()
      .map(|refused| refused.detail);
  let generation = indexmap::IndexMap::new();
  let pieces = agency_server::lines::pieces::compose(
    &ground,
    &machine,
    &agency_server::lines::pieces::SupervisorTurn {
      agent: "chat-agent",
      chat_id: -5,
      provider: "deepseek",
      model: "deepseek-flash",
      system: "the supervisor's system prompt",
      prompt: "hello",
      privileged: true,
      resume: None,
      card: None,
      run: "run-1",
      answers: &[],
      generation: &generation,
      began: 1,
    },
  )
  .await
  .unwrap()
  .command
  .args;

  Launched {
    words: vec![("claude-code", on_claude), ("deepseek", on_deepseek), ("pieces on deepseek", pieces)],
    on_own_harness,
  }
}

/// The items of the list a flag names, none where the words name no such flag.
fn listed<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
  after(args, flag).map(|list| list.split(',').collect()).unwrap_or_default()
}

/// The servers of the MCP configuration the words hand the program.
fn configured(args: &[String]) -> serde_json::Value {
  let path = after(args, "--mcp-config").unwrap();
  let config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();

  config["mcpServers"].clone()
}

async fn limited_to(scratch: &Scratch, capabilities: &[String]) {
  agency_server::control::supervisor_settings::saved(
    scratch.db(),
    &serde_json::json!({ "agent": "chat-agent", "toolsAllowed": capabilities }),
    "tester",
  )
  .await
  .unwrap();
}

// A supervisor's program runs in bypass-permissions mode with the chat line's denied list as its only
// restriction, writes no allow list and narrows nothing it is shown, and holds every server of its chat
// configuration, on Claude Code's own road and on DeepSeek's gateway road alike (docs/launch-lines.md, 「What a
// line's program is allowed, by road」).
#[tokio::test]
async fn a_supervisor_s_program_bypasses_permissions_and_holds_every_chat_server_on_every_road() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  chat_agent(&scratch, "agent").await;

  let launched = launched_supervisor(&scratch, home.path()).await;
  let denied = denied();
  let mut servers: Vec<String> = policy::run_lines::RUN_LINES.namespaces(LINE).to_vec();

  servers.extend(["job", "agency", "artifact", "explorer"].map(str::to_owned));

  for (road, args) in &launched.words {
    assert_eq!(args.first().map(String::as_str), Some("claude"), "{road}: {args:?}");
    assert_eq!(after(args, "--permission-mode"), Some("bypassPermissions"), "{road}: {args:?}");
    assert_eq!(after(args, "--disallowedTools"), Some(denied.as_str()), "{road}: {args:?}");
    assert!(
      !args.iter().any(|arg| arg == "--allowedTools" || arg == "--tools" || arg == "dontAsk"),
      "{road}: no allow list and nothing narrowed: {args:?}"
    );

    let config = configured(args);

    for server in &servers {
      assert!(config[server].is_object(), "{road}: {server}: {config}");
    }
  }

  for server in ["eye", "ear", "testbed", "voice", "research"] {
    assert!(servers.contains(&server.to_owned()), "the chat line names {server}: {servers:?}");
  }
}

// A tool limit naming every capability a supervisor could hold but one: on every road, each built-in tool under
// that capability is neither among the only built-in tools the program is started with nor missing from its
// denied list, and each server under it is not in its configuration, while those of every other capability stay
// (docs/supervisors.md, 「What each capability stands for」).
async fn a_limit_leaving_out(capability: &str) {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  chat_agent(&scratch, "agent").await;

  let offered = agency_server::supervisor_limits::offered();

  assert!(offered.iter().any(|one| one == capability), "{capability}: {offered:?}");

  let named: Vec<String> = offered.into_iter().filter(|one| one != capability).collect();

  limited_to(&scratch, &named).await;

  let launched = launched_supervisor(&scratch, home.path()).await;
  let held = policy::run_lines::RUN_LINES.held(LINE);

  for (road, args) in &launched.words {
    let kept = listed(args, "--tools");
    let denied = listed(args, "--disallowedTools");
    let config = configured(args);

    assert!(kept.contains(&"ToolSearch"), "{road}: {args:?}");

    for entry in held {
      let stays = entry.capability != capability;

      for tool in &entry.tools {
        assert_eq!(kept.contains(&tool.as_str()), stays, "{road}: {tool} kept: {args:?}");
        assert_eq!(denied.contains(&tool.as_str()), !stays, "{road}: {tool} denied: {args:?}");
      }

      for server in &entry.servers {
        assert_eq!(config[server].is_object(), stays, "{road}: {server}: {config}");
      }
    }
  }
}

#[tokio::test]
async fn a_limit_without_program_files_starts_the_program_with_no_file_tool() {
  a_limit_leaving_out("program.files").await;
}

#[tokio::test]
async fn a_limit_without_program_shell_starts_the_program_with_no_shell() {
  a_limit_leaving_out("program.shell").await;
}

#[tokio::test]
async fn a_limit_without_jobs_file_starts_the_program_with_no_job_or_agency_server() {
  a_limit_leaving_out("jobs.file").await;
}

#[tokio::test]
async fn a_limit_without_web_pages_starts_the_program_with_no_explorer() {
  a_limit_leaving_out("web.pages").await;
}

#[tokio::test]
async fn a_limit_without_documents_file_starts_the_program_with_no_artifact_server() {
  a_limit_leaving_out("documents.file").await;
}

// A supervisor limited to `room.chat` is started on every road with the tool search alone of the program's
// own tools, so no subagent tool, every file, shell and web tool denied, no server the chat line holds
// outside the tool host, and its turn refused on a program that is a harness of its own
// (docs/supervisors.md, 「What each capability stands for」).
#[tokio::test]
async fn a_supervisor_limited_to_room_chat_is_started_with_no_file_shell_or_web_tool() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  chat_agent(&scratch, "agent").await;
  limited_to(&scratch, &["room.chat".to_owned()]).await;

  let launched = launched_supervisor(&scratch, home.path()).await;

  for (road, args) in &launched.words {
    let denied = listed(args, "--disallowedTools");
    let config = configured(args);

    assert_eq!(after(args, "--permission-mode"), Some("bypassPermissions"), "{road}: {args:?}");
    assert_eq!(after(args, "--tools"), Some("ToolSearch"), "{road}: {args:?}");

    for tool in [
      "Read",
      "Grep",
      "Glob",
      "LS",
      "Write",
      "Edit",
      "MultiEdit",
      "NotebookEdit",
      "NotebookRead",
      "Bash",
      "BashOutput",
      "KillShell",
      "WebFetch",
      "WebSearch",
    ] {
      assert!(denied.contains(&tool), "{road}: {tool}: {args:?}");
    }

    for server in ["job", "agency", "explorer", "artifact"] {
      assert!(config[server].is_null(), "{road}: {server}: {config}");
    }
  }

  let refused = launched
    .on_own_harness
    .expect("a harness of its own refuses a limited supervisor's turn");

  assert!(refused.contains("holds a tool limit"), "{refused}");
}

#[test]
fn a_held_item_whose_wait_ended_has_its_hold_lifted_and_is_launched() {
  let manual = std::fs::read_to_string(tree().join("prompts/agents/chat/manual.md")).unwrap();
  let todo = std::fs::read_to_string(tree().join("prompts/agents/chat/procedures/todo.md")).unwrap();
  let notices = manual
    .lines()
    .find(|line| line.starts_with("- What a queue notice asks of you is the kind its event names"))
    .expect("the manual's queue notices line");

  assert!(
    notices.contains("a held item whose wait has ended (item-wait-ended)")
      && notices.contains("has its hold lifted with job_item_update and is launched"),
    "{notices}"
  );

  for said in [
    "**`item-wait-ended`:**",
    "Agency starts nothing itself, so the hold",
    "`held_kind` and `held_reason` dropped",
    "leave it held and say in one line why not",
  ] {
    assert!(todo.contains(said), "{said}");
  }
}

// A need that comes back is made a hosted tool: the manual's fast track sends it to the platform's operations, which
// say a worker writes it to the tool host's authoring guide and never puts it live, and the supervisor puts it
// within what the owner approved and tells him; the procedure it reads says how.
#[test]
fn the_fast_track_has_a_repeated_need_made_a_hosted_tool_the_supervisor_puts_within_the_owners_approval() {
  let system = composed();
  let fast_track =
    section(&system, "FAST TRACK: a tool you do not have (the commands are in fasttrack.md)");
  let operations = std::fs::read_to_string(tree().join("prompts/platform/operations.md")).unwrap();
  let hosted = section(&format!("\n{operations}"), "A HOSTED TOOL MADE");

  assert!(
    fast_track.contains("have it made a hosted tool as A HOSTED TOOL MADE says"),
    "{fast_track}"
  );

  for said in [
    "A need that comes back is made a hosted tool, not done by hand each time.",
    "bmo-pocket's docs/authoring.md, with a test",
    "A worker never puts a tool live",
    "you put it with `bmo pocket put`, allowing it no capability or grant beyond what the owner approved",
    "tell the owner what was added",
  ] {
    assert!(hosted.contains(said), "{said}: {hosted}");
  }
  assert!(!fast_track.contains("tool of the repository's own"), "{fast_track}");

  let procedure =
    std::fs::read_to_string(tree().join("prompts/agents/chat/procedures/fasttrack.md")).unwrap();
  let keeping = procedure
    .split("## Keeping a tool")
    .nth(1)
    .expect("the procedure says how a tool is kept");

  for said in [
    "`docs/authoring.md`",
    "the `bmo` namespace asking the capability `room.tools`",
    "A worker never puts a tool live.",
    "a service's own tool with that service's deployment",
    "`bmo pocket put <the repository's git address> --allow",
    "you never widen it yourself",
    "Tell the owner in his own chat what was added",
    "`bmo pocket take <name>`",
  ] {
    assert!(keeping.contains(said), "{said}: {keeping}");
  }
}

// The agency plugin a chat's configuration names is the deploy link's binary, never the release the server
// runs from, so a session started on the configuration after a deployment stands in no older release
// (docs/agent-control.md, 「The chat line's main tools」).
#[tokio::test]
async fn the_chat_s_agency_plugin_is_named_through_the_deploy_link_and_never_a_release() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let at = |path: &Path| path.to_string_lossy().into_owned();
  let release = home.path().join("deploy-releases").join("r1");
  let link = home.path().join("deploy");

  for binary in ["pane-head", "bmo-agency"] {
    std::fs::create_dir_all(release.join("bin")).unwrap();
    std::fs::write(release.join("bin").join(binary), "").unwrap();
  }

  std::os::unix::fs::symlink(&release, &link).unwrap();

  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), at(home.path())),
      ("BMO_AGENTS_DIR".to_owned(), at(&home.path().join("agents"))),
      ("BMO_AGENCY_URL".to_owned(), "http://[::1]:2".to_owned()),
      ("BMO_DATA_DIR".to_owned(), at(&home.path().join("data"))),
    ],
    home.path().join("data"),
  );
  let variables: HashMap<String, String> = [
    ("AGENCY_DEPLOY_DIR".to_owned(), at(&link)),
    ("AGENCY_CODE_DIR".to_owned(), at(&release)),
  ]
  .into();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let ground = Ground::new(&service, &machine, None);
  let path = mcp_config(&ground, &Trees::of(&ground), "chat-agent", &Limits::open()).unwrap();
  let config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
  let command = config["mcpServers"]["agency"]["command"].as_str().unwrap_or_else(|| panic!("{config}"));

  assert_eq!(command, at(&link.join("bin").join("bmo-agency")));
  assert!(!command.contains("deploy-releases"), "{command}");
}
