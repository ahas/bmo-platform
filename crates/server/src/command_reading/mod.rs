//! The reading of a shell command an agent's program is about to run (docs/live-claude-guard.md;
//! docs/agents.md, 「No print mode」): the words it runs, read the way a POSIX shell splits them by a
//! maintained shell parser, the programs it starts, and whether one of them is the Claude Code CLI or
//! writes into its session records; the job guard's readings of a job's command; and the hook's input it is
//! handed and the refusal it answers with.

pub mod habits;
pub mod job_guard;
pub mod reading;
pub mod records;
pub mod removal;
pub mod shell;
pub mod testbed;
pub mod writes;

use serde_json::{Map, Value, json};

/// What the Bash tool call around the command asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShellCall {
  pub timeout_ms: Option<u64>,
  pub background: bool,
}

/// A PreToolUse hook's input as the program hands it.
#[derive(Clone, Debug, PartialEq)]
pub struct HookInput {
  pub tool: String,
  pub command: Option<String>,
  pub path: Option<String>,
  pub call: ShellCall,
  /// The directory the program stood in when it made the call.
  pub cwd: Option<String>,
  pub given: Map<String, Value>,
}

/// The hook's input read, or why it could not be.
pub fn hook_input(text: &str) -> Result<HookInput, String> {
  let parsed: Value = serde_json::from_str(text).map_err(|why| why.to_string())?;
  let given = parsed.get("tool_input").and_then(Value::as_object).cloned().unwrap_or_default();
  let text_of = |value: Option<&Value>| value.and_then(Value::as_str).map(str::to_owned);

  Ok(HookInput {
    tool: text_of(parsed.get("tool_name")).unwrap_or_else(|| "Bash".into()),
    command: text_of(given.get("command")),
    path: text_of(given.get("file_path")).or_else(|| text_of(given.get("notebook_path"))),
    call: ShellCall {
      timeout_ms: given.get("timeout").and_then(Value::as_u64),
      background: given.get("run_in_background") == Some(&Value::Bool(true)),
    },
    cwd: text_of(parsed.get("cwd")).filter(|cwd| cwd.starts_with('/')),
    given,
  })
}

/// The answer refusing a call, with the reason the program reads.
pub fn hook_refusal(reason: &str) -> String {
  json!({
    "hookSpecificOutput": { "hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": reason },
  })
  .to_string()
}
