//! The status-line command and the four hooks of an agent's resident program (docs/agent-control.md, 「What
//! a resident turn reads」). Claude Code runs it with what it has to say on standard input: the status
//! line's running totals, or a hook's word that a prompt was submitted, a turn ended or failed, or a
//! session started. Each is written into the agent's place with the moment it came, and the status line is
//! answered with the one line the program draws at the foot of its screen. Started with the hold flag, a
//! turn's end is answered with a block while the turn's background work runs; started with the text-call
//! flag, while the turn's final answer is a tool call written as text; started with the wording flag,
//! while the turn's final answer uses a word `WORDING.md` bans. Started with the typed flag, a
//! person's submitted line is answered with the composition the service hands it. Started with the carry
//! flag, a stop writes the background tasks it names running into the place's background record and a
//! notice's submit is kept as the program's own turn (docs/agent-control.md, 「Background work on a line that
//! carries it apart」). Started as the shell hook, with no place needed, it hands a shell call back with its
//! command behind the line that closes the shell's standard input (docs/live-claude-guard.md, 「Agency's shell
//! hook closes every command's standard input」); with its refusal flag it refuses one that starts Claude
//! Code (docs/agents.md, 「No print mode」), and with its job flag whatever the job guard refuses
//! (docs/live-claude-guard.md).
//!
//! It never fails the program: a place it cannot write to costs the record one line, and the status line
//! still says what it can. A hook that ended otherwise would stand in the program's way.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

use super::{CONTROL_PORT, CONTROL_TOKEN, control_port, control_token};
use crate::command_reading::job_guard::{FILE_WRITERS, JobGround, file_refusal, job_refusal};
use crate::command_reading::records::claude_launch;
use crate::command_reading::{hook_input, hook_refusal};
use crate::door::place_run::{
  OpenHold, STOP_HOLDS, ends_turn, notice_submitted, open_hold, person_submits, read_hooks, read_run,
  record_call, record_notice, status_line, stop_hold, write_launches,
};
use crate::door::record::{Notices, read_turn, turns_owed};
use crate::door::turn::program_login;
use crate::door::written_call::text_tool_call;
use crate::lines::place::{FAILURE_KIND, SHELL_KIND};
use crate::wording::{banned_terms_in, banned_terms_line, read_glossary};

const KINDS: [&str; 6] = ["status", "stop", FAILURE_KIND, "submit", "start", SHELL_KIND];

/// What one call of the hook is asked: the kind of call, the place, and what its line asks of it.
#[derive(Clone, Debug, Default)]
pub struct Asked {
  pub kind: String,
  pub place: String,
  pub hold_tasks: bool,
  pub hold_text_tool_call: bool,
  pub hold_wording: bool,
  pub compose_typed: bool,
  /// Whether the program's background work is carried apart: a stop writes the tasks it names running into
  /// the place's background record, a notice's submit is kept as the program's own turn, and a round reads
  /// its notices apart.
  pub carry_background: bool,
  /// What the shell hook refuses beside closing a shell command's standard input.
  pub refuses: Refuses,
}

/// What the hook stands on beside the place: its own environment, the code tree whose `WORDING.md` it holds
/// an answer to, the machine's Claude configuration home, and the clock.
pub struct Ground<'a> {
  pub env: &'a BTreeMap<String, String>,
  pub code_tree: &'a Path,
  pub machine_config_dir: &'a Path,
  pub now_ms: i64,
}

/// The usage line, said on the error stream where the hook was started without a kind it knows or a place.
pub const USAGE: &str = concat!(
  "usage: bmo-agency agent-status <status|stop|failure|submit|start> --place <dir>, ",
  "or shell [--refuse-claude] [--guard-job]"
);

// The session record's lines written since a moment, or none where it cannot be read.
fn lines_since(payload: &Value, started_at: i64) -> Option<Vec<Value>> {
  let transcript = payload.get("transcript_path").and_then(Value::as_str)?;

  crate::door::place_run::transcript_since(Path::new(transcript), started_at)
}

// A turn's end, a stop or a failure, closes agency's run unless the round still owes a turn: a word still
// queued, or one taken off the queue that opens a turn after this one. The ends are counted by the hooks,
// since the program writes its own end line after this hook may already have run. Read with notices apart,
// a notice owes the round nothing, so only a word handed over and still queued keeps the run open.
fn round_owes_more(place: &Path, kind: &str, payload: &Value, notices: Notices) -> bool {
  let Some(run) = read_run(place).filter(|run| ends_turn(kind) && !run.person && run.ended_at.is_none())
  else {
    return false;
  };
  let Some(lines) = lines_since(payload, run.started_at) else { return false };
  let owed = turns_owed(&lines, notices);

  if notices == Notices::Apart {
    return owed.queued > 0;
  }

  let ends = read_hooks(place, run.started_at).iter().filter(|hook| hook.ends_turn()).count() as u64 + 1;

  owed.queued > 0 || ends < owed.owed
}

// A turn's answer so far: the program's own last message where it hands one over, and the turn's text
// read off the session record otherwise.
fn answer_so_far(payload: &Value, started_at: i64) -> String {
  let handed = payload.get("last_assistant_message").and_then(Value::as_str).unwrap_or_default();

  if handed.is_empty() {
    lines_since(payload, started_at)
      .map(|lines| read_turn(&lines, 0).text)
      .unwrap_or_default()
  } else {
    handed.to_owned()
  }
}

// A job's final answer is the report the owner reads, and a tool call written as text is no report: the
// call never ran and the work never happened. A stop whose answer so far is such text is held with what
// to do instead, within the holds a stop is given, so the program makes the call and writes the report in
// the same turn.
fn text_tool_call_hold(place: &Path, payload: &Value) -> Option<String> {
  let OpenHold { started_at, .. } = open_hold(place, STOP_HOLDS)?;

  if !text_tool_call(&answer_so_far(payload, started_at)) {
    return None;
  }

  Some(
    "Not ended: this turn's final answer is a tool call written as text rather than a call made — the call \
     never ran, and the answer would stand as the job's report. Make the call as a native tool call, do the \
     work, and then write the report as ordinary text."
      .to_owned(),
  )
}

// A job's final answer is the report the owner reads, and it is held to the words the owner banned as the
// deployment's own answers are: a stop whose answer so far uses one is held with the terms named, within
// the holds a stop is given.
fn wording_hold(place: &Path, payload: &Value, code_tree: &Path) -> Option<String> {
  let OpenHold {
    started_at, last, ..
  } = open_hold(place, STOP_HOLDS)?;
  let answer = answer_so_far(payload, started_at);
  let terms = banned_terms_in(&answer, &read_glossary(code_tree));

  if terms.is_empty() {
    return None;
  }

  let mut said = vec![
    format!(
      "Not ended: this turn's final answer is the job's report, and it uses words the owner has banned as purified coinages (WORDING.md): {}.",
      banned_terms_line(&terms)
    ),
    "Write the report again with each of them replaced, choosing among the words given by what the sentence means and fixing the particle after it; a wrong form named on purpose goes inside quotation marks.".to_owned(),
  ];

  if last {
    said.push("This is the last hold; the next stop is let through as it stands.".to_owned());
  }

  Some(said.join(" "))
}

/// The line a submit hook hands over, or none for one left as it is: an empty line, a slash command, which
/// is the program's own, and a submit naming no session.
pub fn typed_submission(payload: &Value) -> Option<(String, String)> {
  let prompt = payload.get("prompt").and_then(Value::as_str)?;
  let session = payload
    .get("session_id")
    .and_then(Value::as_str)
    .filter(|session| !session.is_empty())?;

  if prompt.trim().is_empty() || prompt.trim_start().starts_with('/') {
    return None;
  }

  Some((prompt.to_owned(), session.to_owned()))
}

fn missing(why: &str) -> String {
  format!(
    "The owner typed this turn's line straight into this program. The round of the owner's private chat it would go with could not be composed ({why}), so it goes on as typed."
  )
}

/// What the submit hook prints for a typed line: the composition the service answers with, as the program's
/// added context; or, where the service cannot be asked, refuses the connection or answers an error, a
/// line saying the composition is missing and why. Either way the line goes on. The service's answer is
/// waited for however long the composing takes.
pub async fn typed_hook_output(text: &str, session: &str, port: u16, token: Option<&str>) -> String {
  let context = match token {
    None => missing(&format!("the program has no {CONTROL_TOKEN} to ask agency with")),
    Some(token) => {
      let asked = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/turn-contexts"))
        .bearer_auth(token)
        .json(&json!({ "text": text, "sessionId": session }))
        .send()
        .await;

      match asked {
        Err(why) => missing(&why.to_string()),
        Ok(answer) => {
          let status = answer.status();
          let body: Value = answer.json().await.unwrap_or(Value::Null);

          match body.get("context").and_then(Value::as_str) {
            Some(context) if status.is_success() => context.to_owned(),
            _ => missing(&format!(
              "agency answered {}{}",
              status.as_u16(),
              body
                .get("error")
                .and_then(Value::as_str)
                .map(|error| format!(": {error}"))
                .unwrap_or_default()
            )),
          }
        }
      }
    }
  };

  json!({ "hookSpecificOutput": { "hookEventName": "UserPromptSubmit", "additionalContext": context } })
    .to_string()
}

/// The line put ahead of every shell command, so the shell's own standard input is empty for everything the
/// command runs (docs/live-claude-guard.md, 「Agency's shell hook closes every command's standard input」).
pub const CLOSED_INPUT: &str = "exec </dev/null";

/// What the shell hook refuses beside closing every shell command's standard input: a supervisor's program
/// asks it, every other program does not.
#[derive(Clone, Copy, Debug, Default)]
pub struct Refuses {
  /// A shell command that starts Claude Code (docs/agents.md, 「No print mode」).
  pub claude: bool,
  /// Whatever the job guard refuses of a job's shell command (docs/live-claude-guard.md).
  pub job: bool,
}

/// The shell hook's answer to a PreToolUse call: where `refuses` asks it, a refusal of a command that starts
/// Claude Code in any form the job guard reads one; otherwise a shell call as it came, its command behind the
/// line that closes the shell's standard input, and nothing for any other call. Input it cannot read lets the
/// call through as it is: one hook failing must not stop every call the agent makes.
pub fn shell_guard(input: &str, refuses: Refuses) -> Option<String> {
  let input = hook_input(input).ok()?;
  let command = input.command.filter(|_| input.tool == "Bash")?;

  if refuses.claude
    && let Some(refused) = shell_refusal(&command)
  {
    return Some(hook_refusal(&refused));
  }

  Some(closed_input(input.given, &command))
}

/// The shell hook's answer to a PreToolUse call, the job guard first where `refuses` asks it: its refusal of a
/// job's shell command or of a file tool's write, read with what `job` stands on and the paths spelled under
/// `home`; otherwise the shell guard's answer, which is nothing for a file tool's call. Input it cannot read
/// lets the call through as it is.
pub async fn shell_hook(input: &str, refuses: Refuses, home: &str, job: &impl JobGround) -> Option<String> {
  if refuses.job
    && let Ok(read) = hook_input(input)
  {
    let refused = match (read.tool.as_str(), read.command, read.path) {
      ("Bash", Some(command), _) => job_refusal(&command, read.call, home, read.cwd.as_deref(), job).await,
      (tool, _, Some(path)) if FILE_WRITERS.contains(&tool) => file_refusal(&path, job).await,
      _ => None,
    };

    if let Some(refused) = refused {
      return Some(hook_refusal(&refused));
    }
  }

  shell_guard(input, refuses)
}

// The call's whole input again, its command behind the line that closes the shell's standard input, as the
// program takes a rewritten call: the whole input replaces the one it had. A command that already opens on
// the line is left as it is.
fn closed_input(mut given: serde_json::Map<String, Value>, command: &str) -> String {
  if command.lines().next().map(str::trim) != Some(CLOSED_INPUT) {
    given.insert("command".to_owned(), Value::String(format!("{CLOSED_INPUT}\n{command}")));
  }

  json!({ "hookSpecificOutput": { "hookEventName": "PreToolUse", "updatedInput": given } }).to_string()
}

/// Why a conversational agent's shell may not run this command, or nothing: it starts Claude Code
/// (docs/agents.md, 「No print mode」). Every other command is the agent's to run.
pub fn shell_refusal(command: &str) -> Option<String> {
  if let Some(launch) = claude_launch(command) {
    return Some(format!(
      "refused: {launch}. No agent of this system starts Claude Code itself: file it as an item with file_item and launch it with assign_worker and run_job, and a search or a browse with their own tools (docs/agents.md, 「No print mode」)."
    ));
  }

  None
}

/// One call of the hook: what it writes into the place, and the lines it prints — the status line, a block,
/// or a typed line's composition. What goes wrong is said on the error stream and never ends the call with
/// a fault.
pub async fn agent_status(asked: &Asked, input: &str, ground: &Ground<'_>) -> Vec<String> {
  // The shell hook reads only the call it is handed, so it needs no place; the job guard is `shell_hook`'s,
  // which stands on more than this hook does.
  if asked.kind == SHELL_KIND {
    return shell_guard(input, asked.refuses).into_iter().collect();
  }

  if !KINDS.contains(&asked.kind.as_str()) || asked.place.is_empty() {
    eprintln!("{USAGE}");
    return vec![];
  }

  let place = Path::new(&asked.place);
  let kind = asked.kind.as_str();
  // What was handed over is kept as it came where it is not the JSON the program promises.
  let payload = serde_json::from_str::<Value>(input).unwrap_or_else(|_| Value::String(input.to_owned()));
  // A person's line, on a line that composes typed lines: asked before the submit is recorded, since
  // recording it opens the person's turn.
  let notices = if asked.carry_background { Notices::Apart } else { Notices::Joined };
  // A background task's notice the program hands itself, on a line that carries its background apart: the
  // program's own turn, written down as such and nothing more.
  if kind == "submit" && asked.carry_background && notice_submitted(&payload) {
    if let Err(why) = record_notice(place, &payload, ground.now_ms) {
      eprintln!("agent-status: {why}");
    }

    return vec![];
  }

  if kind == "stop"
    && asked.carry_background
    && let Err(why) = write_launches(place, &payload, ground.now_ms)
  {
    eprintln!("agent-status: {why}");
  }

  let typed = (kind == "submit" && asked.compose_typed && person_submits(place))
    .then(|| typed_submission(&payload))
    .flatten();
  let mut printed = vec![];
  // A stop the line asks to hold while the turn's background work runs is answered with a block and written
  // as a hold: the program goes on in the same turn, and the run stays open.
  let tasks_held = (kind == "stop" && asked.hold_tasks)
    .then(|| stop_hold(place, &payload, STOP_HOLDS))
    .flatten();
  let held = tasks_held
    .or_else(|| {
      (kind == "stop" && asked.hold_text_tool_call)
        .then(|| text_tool_call_hold(place, &payload))
        .flatten()
    })
    .or_else(|| {
      (kind == "stop" && asked.hold_wording)
        .then(|| wording_hold(place, &payload, ground.code_tree))
        .flatten()
    });
  let written = match &held {
    Some(reason) => {
      printed.push(json!({ "decision": "block", "reason": reason }).to_string());
      record_call(place, "held", &payload, ground.now_ms, false, None)
    }
    None => {
      // A status reading carries the login of the program that made it, which is this process's own
      // environment: the program runs its status line with its own.
      let login = (kind == "status").then(|| program_login(ground.machine_config_dir, ground.env));

      record_call(
        place,
        kind,
        &payload,
        ground.now_ms,
        round_owes_more(place, kind, &payload, notices),
        login.as_ref(),
      )
    }
  };

  if let Err(why) = written {
    eprintln!("agent-status: {why}");
  }

  if kind == "status" {
    printed.push(status_line(read_run(place).as_ref()));
  }

  if let Some((text, session)) = typed {
    let port = ground
      .env
      .get(CONTROL_PORT)
      .and_then(|port| port.trim().parse().ok())
      .unwrap_or_else(control_port);
    let token = crate::places::token_or_file(
      ground.env.get(CONTROL_TOKEN).cloned(),
      ground.env.get(&crate::places::file_variable(CONTROL_TOKEN)).cloned(),
    )
    .or_else(control_token);

    printed.push(typed_hook_output(&text, &session, port, token.as_deref()).await);
  }

  printed
}
