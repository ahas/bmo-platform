use policy::run_lines::*;

#[test]
fn the_worker_line_keeps_its_session_and_leaves_its_tools_to_its_agents_role() {
  let worker = RUN_LINES.line("worker").expect("the worker line is declared");

  assert!(worker.keeps_session);
  assert!(!RUN_LINES.composes_typed_lines("worker"));
  assert!(RUN_LINES.composes_typed_lines("chat"));
  assert!(RUN_LINES.background_apart("chat") && !RUN_LINES.background_apart("worker"));
  assert!(RUN_LINES.capabilities("worker").is_empty());
  assert_eq!(
    RUN_LINES.capabilities("chat"),
    ["room.tools", "gate.translate", "gate.reference", "research.ask"],
    "no line gives a capability that operates the platform"
  );
  assert_eq!(
    policy::person_nodes::PERSON_NODES.administrator_capabilities,
    ["deploy", "research.owner"]
  );
  assert_eq!(
    RUN_LINES.namespaces("chat"),
    ["bmo", "gate", "eye", "ear", "testbed", "voice", "research"]
  );
  assert!(RUN_LINES.capabilities("gate").is_empty());
}

// A job's pane has nobody to answer the CLI's own question tool, and the session contract cannot carry
// the option it would pick: a job asks the owner through bmo job ask instead (docs/launch-lines.md).
#[test]
fn the_worker_line_keeps_out_the_cli_s_own_question_tool() {
  let worker = RUN_LINES.line("worker").expect("the worker line is declared");

  assert!(worker.tools_denied.iter().any(|tool| tool == "AskUserQuestion"));
}

#[test]
fn only_the_worker_runner_and_chat_lines_show_their_most_called_entries_by_name() {
  assert!(!RUN_LINES.shows("worker", "worker/graph_impact"), "the graph service is retired");
  assert!(RUN_LINES.shows("worker", "web/web_fetch"));
  assert!(!RUN_LINES.shows("worker", "web/research"));
  assert!(RUN_LINES.shows("search", "web/research"));
  assert!(RUN_LINES.shows("browse", "web/web_fetch"));
  assert!(!RUN_LINES.shows("search", "web/reddit"), "the site tools stay in the legacy repository");
  assert!(RUN_LINES.shows("browse", "eye/see"));
  assert!(RUN_LINES.shows("search", "eye/see"));
  assert!(RUN_LINES.shows("worker", "worker/todo_show"));
  assert!(!RUN_LINES.shows("editor", "fragments/fragment_write"), "no editor line stands");
  assert!(!RUN_LINES.shows("chat", "bmo/todo_show"), "the registry holds no such entry");
  assert!(!RUN_LINES.shows("chat", "bmo/todo_list"));
  assert!(RUN_LINES.shows("chat", "bmo/brain_learn"));
  assert!(RUN_LINES.shows("chat", "bmo/brain_similar"));
  assert!(!RUN_LINES.shows("chat", "bmo/memory_add"));
  assert!(RUN_LINES.shows("chat", "bmo/say"));
  assert!(
    RUN_LINES.shows("chat", "bmo/brain_move"),
    "a memory shown wrong is closed where it stands"
  );
  for gone in [
    "bmo/task",
    "bmo/running",
    "bmo/todo_add",
    "bmo/todo_append",
    "bmo/todo_approve",
    "bmo/room_grant",
  ] {
    assert!(!RUN_LINES.shows("chat", gone), "{gone}: the registry holds no such entry");
  }
  assert!(!RUN_LINES.shows("chat", "bmo/jobs"));
  assert!(!RUN_LINES.shows("chat", "bmo/digest_now"));
  assert!(!RUN_LINES.shows("chat", "bmo/sticker"));
  assert!(!RUN_LINES.shows("chat", "bmo/note_add"));
  assert!(!RUN_LINES.shows("chat", "web/web_search"));
  assert!(!RUN_LINES.shows("chat", "web/web_fetch"));
  assert!(!RUN_LINES.shows("chat", "web/web_search"));
  assert!(RUN_LINES.shows("chat", "gate/translate"));
  assert_eq!(RUN_LINES.variables("chat"), ["BMO_GATE_URL", "BMO_GATE_TOKEN_FILE"]);
  assert_eq!(
    RUN_LINES.variables("worker"),
    ["BMO_GATE_URL", "BMO_GATE_TOKEN_FILE"],
    "the queue tools read the titles' kept translations from the gate"
  );
  assert!(!RUN_LINES.shows("supervisor", "worker/todo_show"));
}

#[test]
fn a_runner_line_writes_the_deployment_agent_s_brain_and_shows_the_tools_for_it() {
  for line in ["search", "browse"] {
    assert!(RUN_LINES.capabilities(line).iter().any(|one| one == "room.chat"), "{line}");
    assert!(RUN_LINES.namespaces(line).iter().any(|one| one == "bmo"), "{line}");
    assert!(RUN_LINES.shows(line, "bmo/brain_learn"), "{line}");
    assert!(RUN_LINES.shows(line, "bmo/brain_similar"), "{line}");
    assert!(RUN_LINES.writes_agent_brain(line), "{line}");
  }

  assert!(!RUN_LINES.writes_agent_brain("worker"));
  assert!(!RUN_LINES.writes_agent_brain("chat"));
}

// "the worker line reads the brain of the supervisor its runs work for and writes their lessons there, through
// the namespace its record alone names, its runs staying their own"
#[test]
fn the_worker_line_works_for_a_supervisor_s_brain_and_stays_its_own() {
  assert!(RUN_LINES.works_for_brain("worker"));
  assert!(!RUN_LINES.writes_agent_brain("worker"));
  assert!(RUN_LINES.namespaces("worker").iter().any(|one| one == "bmo"));
  assert!(!RUN_LINES.shows("worker", "bmo/brain_learn"), "reached with bmo use, shown by no name");

  for line in ["search", "browse"] {
    assert!(RUN_LINES.works_for_brain(line), "{line}");
  }

  assert!(!RUN_LINES.works_for_brain("chat"));
}

// Every server the chat line holds under a capability and starts with this machine's addresses and secrets has the
// tool host's entries standing in for it on a node (docs/person-nodes.md, 「A supervisor's tools on a node」).
#[test]
fn each_server_the_chat_line_holds_has_its_hosted_stand_in_on_a_node() {
  let hosted = |capability: &str| {
    RUN_LINES
      .held("chat")
      .iter()
      .find(|held| held.capability == capability)
      .and_then(|held| held.hosted.as_ref())
      .unwrap_or_else(|| panic!("{capability} has no stand-in"))
  };

  assert_eq!(hosted("jobs.file").namespaces, ["agency"]);
  assert!(hosted("jobs.file").shown.contains(&"agency/file_item".to_owned()));
  assert_eq!(hosted("web.pages").namespaces, ["web"]);
  assert_eq!(hosted("web.pages").capabilities, ["web.read"]);
  assert_eq!(hosted("documents.file").namespaces, ["artifact"]);
  assert_eq!(hosted("documents.file").capabilities, ["documents.file"]);
  assert!(
    RUN_LINES
      .held("chat")
      .iter()
      .filter(|held| !held.servers.is_empty())
      .all(|held| held.hosted.is_some())
  );
}

// The plain line hands its run the instructions its caller gives and nothing of the tool host's
// (docs/launch-lines.md, 「Plain runner」).
#[test]
fn the_plain_line_reaches_nothing_of_the_tool_host() {
  let plain = RUN_LINES.line("plain").expect("the plain line is declared");

  assert!(!plain.keeps_session);
  assert!(plain.needs.is_empty());
  assert!(RUN_LINES.capabilities("plain").is_empty());
  assert!(RUN_LINES.namespaces("plain").is_empty());
  assert!(RUN_LINES.shown("plain").is_empty());
}

// "chat: skills [attached], mcp [line, attached]; worker: skills [role, attached], mcp [line, attached]; gate,
// translate: skills [], mcp []; search, browse, plain: skills [], mcp [line]" (docs/launch-lines.md, 「What a line
// carries」).
#[test]
fn each_line_names_the_sources_of_its_skills_and_its_mcp_servers() {
  for (line, skills, mcp) in [
    ("chat", vec![SkillSource::Attached], vec![McpSource::Line, McpSource::Attached]),
    (
      "worker",
      vec![SkillSource::Role, SkillSource::Attached],
      vec![McpSource::Line, McpSource::Attached],
    ),
    ("gate", vec![], vec![]),
    ("translate", vec![], vec![]),
    ("search", vec![], vec![McpSource::Line]),
    ("browse", vec![], vec![McpSource::Line]),
    ("plain", vec![], vec![McpSource::Line]),
  ] {
    let carries = RUN_LINES.carries(line);

    assert_eq!(carries.skills, skills, "{line}");
    assert_eq!(carries.mcp, mcp, "{line}");
    assert_eq!(carries.takes_attached_skills(), line == "chat" || line == "worker", "{line}");
    assert_eq!(carries.takes_attached_mcp(), line == "chat" || line == "worker", "{line}");
  }

  assert!(!RUN_LINES.carries("no-such-line").takes_attached_skills());
}
