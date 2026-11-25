use crate::support::wording::assert_no_instruction;
use policy::chat_line::*;
use policy::run_lines::RUN_LINES;

#[test]
fn reads_its_lines() {
  assert!(CHAT_LINE.tools.shown.split(',').any(|tool| tool == "Bash"));
  assert!(!CHAT_LINE.tools.listed.is_empty());
  assert!(CHAT_LINE.room.mark.contains("{title}"));
  assert_eq!(CHAT_LINE.persona.base_layer, "persona");
}

#[test]
fn an_absent_server_is_a_fact_without_an_instruction() {
  assert!(CHAT_LINE.tools.absent.contains("{server}") && CHAT_LINE.tools.absent.contains("{why}"));
  assert_no_instruction(&CHAT_LINE.tools.absent);
}

// A supervisor's one restriction is the chat line's denied list: the CLI's web tools, the tools that bring a
// session back after its turn, and the job plugin's own item creation; its subagent tool stands, for reading
// and searching inside its turn (docs/launch-lines.md, docs/supervisors.md).
#[test]
fn the_chat_line_denies_the_cli_s_web_its_wake_ups_and_the_job_plugin_s_item_creation_but_not_subagents() {
  let denied = &RUN_LINES.line("chat").unwrap().tools_denied;

  for tool in ["WebSearch", "WebFetch", "Monitor", "ScheduleWakeup", "mcp__job__job_item_create"] {
    assert!(denied.contains(&tool.to_owned()), "{tool}: {denied:?}");
  }

  for tool in ["Agent", "Task"] {
    assert!(!denied.contains(&tool.to_owned()), "{tool}: {denied:?}");
  }
}
