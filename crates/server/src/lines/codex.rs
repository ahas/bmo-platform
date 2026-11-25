//! The worker's run on Codex, as a job's walk sends it there (docs/cache-fallback.md, 「The Codex
//! substitute is the Codex CLI」): how Codex takes the run, and the command Codex's interactive program
//! stands on in the worker's pane — the same model, sandbox and configuration a run of `exec` is handed,
//! resumed by its thread where the worker has one, and no prompt, which goes in once it is up. A run of
//! `exec` itself — the keepalive's ping on a thread no program stands on — is composed here too, and its
//! answer read out of the events it prints and the requests the thread's rollout counted.

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use agent_host::place::ResidentCommand;
use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::{Value, json};

use super::fault;
use super::place::{PLACE_VARIABLE, is_turn_variable};
use super::stand_in::{StandInRun, Taking};
use crate::Refusal;
use crate::door::record::{ModelSpend, Reply, RequestUsage};
use crate::host_run::Command;

/// The provider the run goes out on.
pub const PROVIDER: &str = "codex";

// The program it is, looked up on the search path.
const PROGRAM: &str = "codex";

/// How Codex takes a worker's run: the prompt on its input, and the thread it opens kept.
pub const TAKING: Taking = Taking {
  prompt_delivery: "input",
  keep_session: true,
};

// One row of Codex's configuration, its value written as JSON.
fn config(key: &str, value: Value) -> [String; 2] {
  ["--config".to_owned(), format!("{key}={value}")]
}

#[derive(Deserialize)]
struct McpFile {
  #[serde(default, rename = "mcpServers")]
  servers: IndexMap<String, McpServer>,
}

#[derive(Deserialize)]
struct McpServer {
  #[serde(default, rename = "type")]
  kind: Option<String>,
  #[serde(default)]
  url: Option<String>,
  #[serde(default)]
  headers: Option<IndexMap<String, Value>>,
  #[serde(default)]
  command: Option<Value>,
  #[serde(default)]
  args: Option<Value>,
  #[serde(default)]
  env: Option<IndexMap<String, Value>>,
}

/// The variable a bearer header reads its token from, in the `${NAME}` form the deployment's files write it.
pub fn bearer_variable(headers: &IndexMap<String, Value>) -> Option<String> {
  let said = headers.get("Authorization")?.as_str()?;
  let name = said.strip_prefix("Bearer ${")?.strip_suffix('}')?;
  let mut letters = name.chars();

  (letters.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
    && letters.all(|rest| rest.is_ascii_alphanumeric() || rest == '_'))
  .then(|| name.to_owned())
}

/// The run's own `BMO_` variables, by name, which Codex hands a server it starts; and the place a
/// resident program reads a run's variables from with them.
pub fn forwarded_names(env: &BTreeMap<String, String>) -> Vec<String> {
  let mut names: Vec<String> = std::iter::once(PLACE_VARIABLE.to_owned())
    .chain(env.keys().filter(|name| name.starts_with("BMO_")).cloned())
    .collect();

  names.sort();
  names.dedup();
  names
}

/// What a run is allowed and how hard it thinks, in the terms Codex's programs are told it: a worker's run,
/// or a run of any other line by what it shows and allows.
pub struct Shape {
  pub worker: bool,
  pub has_tools: bool,
  pub may_write: bool,
  /// Whether every gate of Codex's own is put aside: a worker's, and a chat's allowed all it shows.
  pub bypass: bool,
  pub reasoning: String,
  pub model: String,
  pub cwd: String,
  /// The whole system file in Codex's place, where the run names one.
  pub system: Option<PathBuf>,
  /// The worker's rules, where it names no system file of its own.
  pub rules: Option<PathBuf>,
  pub mcp: Option<PathBuf>,
}

impl Shape {
  /// A worker's run: everything allowed, every gate aside.
  pub fn of_worker(run: &StandInRun) -> Self {
    Self {
      worker: true,
      has_tools: true,
      may_write: true,
      bypass: true,
      reasoning: if run.thinking { run.effort.clone() } else { "low".to_owned() },
      model: run.model.clone(),
      cwd: run.cwd.clone(),
      system: run.system_prompt_path.clone(),
      rules: Some(run.extensions.rules_path.clone()),
      mcp: Some(run.mcp_config_path.clone()),
    }
  }

  /// A run of a line other than the worker's: tools where it shows any, writing where it allows what it
  /// shows, and every gate aside only for a chat allowed all it shows.
  #[allow(clippy::too_many_arguments)]
  pub fn of_line(
    line: &str,
    model: &str,
    effort: &str,
    thinking: bool,
    cwd: &str,
    system: Option<PathBuf>,
    mcp: Option<PathBuf>,
    shown: &str,
    allowed: &str,
  ) -> Self {
    let has_tools = !shown.is_empty();
    let listed = |tools: &str, keep: &dyn Fn(&str) -> bool| {
      tools
        .split(',')
        .map(str::trim)
        .filter(|tool| !tool.is_empty() && keep(tool))
        .collect::<Vec<_>>()
        .join(",")
    };
    let may_write =
      has_tools && listed(allowed, &|tool| !tool.starts_with("mcp__")) == listed(shown, &|_| true);

    Self {
      worker: false,
      has_tools,
      may_write,
      bypass: line == "chat" && may_write,
      reasoning: if thinking { effort.to_owned() } else { "low".to_owned() },
      model: model.to_owned(),
      cwd: cwd.to_owned(),
      system,
      rules: None,
      mcp,
    }
  }

  /// The sandbox Codex runs the line in: none where the chat may write and bypasses, the workspace where the
  /// line may write, and reading only otherwise.
  pub fn sandbox(&self) -> &'static str {
    if self.bypass {
      "danger-full-access"
    } else if self.may_write {
      "workspace-write"
    } else {
      "read-only"
    }
  }
}

// the deployment's MCP servers in Codex's own spelling, one server at a time: a server on the tool host as
// its URL and the variable its token is read from, any other as its command, arguments and variables.
fn mcp_rows(shape: &Shape, env: &BTreeMap<String, String>) -> Result<Vec<String>, Refusal> {
  let Some(path) = &shape.mcp else { return Ok(vec![]) };
  let text = std::fs::read_to_string(path).map_err(|why| fault(format!("{}: {why}", path.display())))?;
  let file: McpFile =
    serde_json::from_str(&text).map_err(|why| fault(format!("{}: {why}", path.display())))?;
  let mut rows = vec![];

  for (name, server) in file.servers {
    if server.kind.as_deref() == Some("http")
      && let Some(url) = &server.url
    {
      rows.extend(config(&format!("mcp_servers.{name}.url"), json!(url)));

      if let Some(token) = server.headers.as_ref().and_then(bearer_variable) {
        rows.extend(config(&format!("mcp_servers.{name}.bearer_token_env_var"), json!(token)));
      }

      continue;
    }

    let Some(Value::String(command)) = &server.command else { continue };

    rows.extend(config(&format!("mcp_servers.{name}.command"), json!(command)));

    if let Some(Value::Array(args)) = &server.args
      && args.iter().all(Value::is_string)
    {
      rows.extend(config(&format!("mcp_servers.{name}.args"), Value::Array(args.clone())));
    }

    for (key, value) in server.env.iter().flatten() {
      if let Value::String(value) = value {
        rows.extend(config(&format!("mcp_servers.{name}.env.{key}"), json!(value)));
      }
    }

    rows.extend(config(&format!("mcp_servers.{name}.env_vars"), json!(forwarded_names(env))));
  }

  Ok(rows)
}

// The configuration the program is handed row by row: its approvals and sandbox; a worker's own prompt
// beside Codex's or its base prompt in Codex's place with the checkout's documents read either way, and any
// other line's whole system file in Codex's place with no document of where it stands; how hard it thinks,
// what is switched off, and the run's MCP servers.
fn config_rows(shape: &Shape, env: &BTreeMap<String, String>) -> Result<Vec<String>, Refusal> {
  let mut rows = vec![];

  rows.extend(config("approval_policy", json!("never")));
  rows.extend(config("sandbox_mode", json!(shape.sandbox())));

  match (&shape.system, &shape.rules) {
    (None, rules) => {
      rows.extend(config("project_doc_fallback_filenames", json!(["CLAUDE.md"])));

      if let (true, Some(rules)) = (shape.worker, rules) {
        let rules =
          std::fs::read_to_string(rules).map_err(|why| fault(format!("{}: {why}", rules.display())))?;

        rows.extend(config("developer_instructions", json!(rules)));
      }
    }
    (Some(system), _) if shape.worker => {
      rows.extend(config("model_instructions_file", json!(system)));
      rows.extend(config("project_doc_fallback_filenames", json!(["CLAUDE.md"])));
    }
    (Some(system), _) => {
      rows.extend(config("model_instructions_file", json!(system)));
      rows.extend(config("project_doc_max_bytes", json!(0)));
    }
  }

  rows.extend(config("model_reasoning_effort", json!(shape.reasoning)));
  rows.extend(config("web_search", json!("disabled")));
  rows.extend(config("features.apps", json!(false)));
  rows.extend(config("features.multi_agent", json!(shape.worker)));
  rows.extend(config("features.memories", json!(false)));
  rows.extend(config("features.goals", json!(false)));
  rows.extend(config("features.shell_tool", json!(shape.has_tools)));

  if !shape.worker {
    for switch in [
      "include_permissions_instructions",
      "include_apps_instructions",
      "include_collaboration_mode_instructions",
      "include_environment_context",
    ] {
      rows.extend(config(switch, json!(false)));
    }
  }

  rows.extend(mcp_rows(shape, env)?);

  Ok(rows)
}

/// The command the worker's Codex program stands on: on the thread it resumes where it has one, with the
/// run's configuration and with the update offer and the hooks review switched off, under the program's
/// own variables, none of one turn's.
pub fn resident_command(
  run: &StandInRun,
  thread: Option<&str>,
  env: &BTreeMap<String, String>,
) -> Result<ResidentCommand, Refusal> {
  resident_command_of(&Shape::of_worker(run), thread, env)
}

/// The command a run's Codex program stands on, by the run's shape: on the thread it resumes where it has
/// one, its sandbox put aside or named, with the run's configuration and with the update offer and the
/// hooks review switched off, under the program's own variables, none of one turn's.
pub fn resident_command_of(
  shape: &Shape,
  thread: Option<&str>,
  env: &BTreeMap<String, String>,
) -> Result<ResidentCommand, Refusal> {
  let env: BTreeMap<String, String> = env
    .iter()
    .filter(|(name, _)| !is_turn_variable(name))
    .map(|(name, value)| (name.clone(), value.clone()))
    .collect();
  let mut args = vec![PROGRAM.to_owned()];

  if let Some(thread) = thread {
    args.extend(["resume".to_owned(), thread.to_owned()]);
  }

  args.extend(["--model".to_owned(), shape.model.clone()]);

  if shape.bypass {
    args.push("--dangerously-bypass-approvals-and-sandbox".to_owned());
  } else {
    args.extend([
      "--sandbox".to_owned(),
      if shape.may_write { "workspace-write" } else { "read-only" }.to_owned(),
    ]);
  }

  args.extend(["--cd".to_owned(), shape.cwd.clone()]);
  args.extend(config_rows(shape, &env)?);
  args.extend(config("check_for_update_on_startup", json!(false)));
  args.extend(config("features.hooks", json!(false)));

  Ok(ResidentCommand {
    args,
    cwd: PathBuf::from(&shape.cwd),
    env,
    login_file: None,
  })
}

/// The command a run of `exec` is: the worker's run with every gate of Codex's own put aside, on the thread
/// it resumes where it names one and opening one otherwise, kept where the run keeps its session, with the
/// prompt on its input and the run's whole environment.
pub fn exec_command(
  run: &StandInRun,
  thread: Option<&str>,
  prompt: &str,
  env: &BTreeMap<String, String>,
) -> Result<Command, Refusal> {
  exec_command_of(&Shape::of_worker(run), run.keep_session, thread, prompt, env)
}

/// The command a run of `exec` is, by the run's shape: every gate of Codex's own put aside where the shape
/// puts it aside, and otherwise the sandbox it allows named on a thread it opens; on the thread it resumes
/// where it names one and opening one otherwise, kept where the run keeps its session, with the prompt on
/// its input and the run's whole environment.
pub fn exec_command_of(
  shape: &Shape,
  keep_session: bool,
  thread: Option<&str>,
  prompt: &str,
  env: &BTreeMap<String, String>,
) -> Result<Command, Refusal> {
  let mut args = vec![PROGRAM.to_owned(), "exec".to_owned()];

  if thread.is_some() {
    args.push("resume".to_owned());
  }

  args.extend(
    [
      "--json",
      "--ignore-user-config",
      "--ignore-rules",
      "--strict-config",
      "--skip-git-repo-check",
      "--model",
    ]
    .map(str::to_owned),
  );
  args.push(shape.model.clone());

  if !keep_session {
    args.push("--ephemeral".to_owned());
  }

  // A worker has to commit, push and reach outside its checkout, and a chat allowed every tool it shows
  // calls the deployment's loopback services from its scripts, so for those two Codex's sandbox and
  // approvals are off; every other run keeps the sandbox it allows.
  if shape.bypass {
    args.push("--dangerously-bypass-approvals-and-sandbox".to_owned());
  } else if thread.is_none() {
    args.extend([
      "--sandbox".to_owned(),
      if shape.may_write { "workspace-write" } else { "read-only" }.to_owned(),
    ]);
  }

  if thread.is_none() {
    args.extend(["--cd".to_owned(), shape.cwd.clone(), "--color".to_owned(), "never".to_owned()]);
  }

  args.extend(config_rows(shape, env)?);
  args.extend(thread.map(str::to_owned));
  args.push("-".to_owned());

  Ok(Command {
    args,
    cwd: shape.cwd.clone(),
    env: env.clone(),
    stdin: Some(prompt.to_owned()),
  })
}

// The objects a run of `exec` printed, one to a line; a line that is no object is no event.
fn events(stdout: &str) -> Vec<Value> {
  stdout
    .lines()
    .filter(|line| !line.trim().is_empty())
    .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    .filter(Value::is_object)
    .collect()
}

fn kind(event: &Value) -> Option<&str> {
  event.get("type").and_then(Value::as_str)
}

/// A count Codex reported, where it is a positive number; nought otherwise.
pub fn count(value: Option<&Value>) -> i64 {
  value
    .and_then(Value::as_f64)
    .filter(|number| number.is_finite() && *number > 0.0)
    .map_or(0, |number| number.floor() as i64)
}

/// What a run of `exec` answered: the last message it completed, where it exited cleanly; the thread it
/// opened, or the one it resumed, where the run keeps its session; and what its turn spent, in the
/// deployment's four disjoint kinds, with each request the thread's rollout counted under the sessions
/// directory given.
pub fn exec_answer(
  stdout: &str,
  exit_code: i32,
  run: &StandInRun,
  resumed: Option<&str>,
  sessions: &Path,
) -> Reply {
  exec_reply(stdout, exit_code, &run.model, &run.effort, run.keep_session, resumed, sessions)
}

/// What a run of `exec` on a model and effort answered, as `exec_answer` reads a worker's; the thread named
/// only where the run keeps its session.
pub fn exec_reply(
  stdout: &str,
  exit_code: i32,
  model: &str,
  effort: &str,
  keep_session: bool,
  resumed: Option<&str>,
  sessions: &Path,
) -> Reply {
  let events = events(stdout);
  let text = events
    .iter()
    .filter(|event| kind(event) == Some("item.completed"))
    .filter(|event| event.pointer("/item/type").and_then(Value::as_str) == Some("agent_message"))
    .filter_map(|event| event.pointer("/item/text").and_then(Value::as_str))
    .next_back()
    .unwrap_or_default();
  let usage = events
    .iter()
    .rfind(|event| kind(event) == Some("turn.completed"))
    .and_then(|event| event.get("usage"));
  let thread = events
    .iter()
    .filter(|event| kind(event) == Some("thread.started"))
    .filter_map(|event| event.get("thread_id").and_then(Value::as_str))
    .rfind(|thread| !thread.is_empty())
    .map(str::to_owned)
    .or_else(|| resumed.map(str::to_owned));
  let field = |name: &str| count(usage.and_then(|usage| usage.get(name)));
  let read = field("cached_input_tokens");
  let write = field("cache_write_input_tokens");
  let output = field("output_tokens");
  // Codex counts cached and cache-written tokens inside its input, so the ordinary input is the rest.
  let input = (field("input_tokens") - read - write).max(0);
  let tokens = input + output + read + write;
  let requests = thread.as_deref().and_then(|thread| turn_requests(sessions, thread));

  Reply {
    mode: PROVIDER.to_owned(),
    model: Some(model.to_owned()),
    effort: Some(effort.to_owned()),
    text: if exit_code == 0 { text.trim().to_owned() } else { String::new() },
    session_id: thread.filter(|_| keep_session),
    context_ratio: None,
    context_tokens: None,
    session_usage: None,
    windows: vec![],
    spend: if tokens == 0 {
      vec![]
    } else {
      vec![ModelSpend {
        mode: PROVIDER.to_owned(),
        model: model.to_owned(),
        tokens,
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: read,
        cache_write_tokens: write,
        cost: 0.0,
      }]
    },
    live_tasks: vec![],
    workflows: vec![],
    account: None,
    last_request_usage: requests.as_ref().and_then(|requests| requests.last().cloned()),
    request_usages: requests,
    answers: Vec::new(),
  }
}

/// A gate's run of `exec` (docs/cache-fallback.md, 「The Codex substitute is the Codex CLI」): the gate's
/// instructions as the whole of Codex's prompt with no project document beside them, a read-only sandbox,
/// no shell, nothing of Codex's own switched on, on the thread it resumes where it names one and opening
/// one otherwise, kept only where the gate keeps its session, with the prompt on its input and the
/// process's own environment.
#[allow(clippy::too_many_arguments)]
pub fn gate_exec_command(
  model: &str,
  effort: &str,
  thinking: bool,
  system_path: &str,
  cwd: &str,
  thread: Option<&str>,
  keep_session: bool,
  prompt: &str,
  env: &BTreeMap<String, String>,
) -> Command {
  let mut args = vec![PROGRAM.to_owned(), "exec".to_owned()];

  if thread.is_some() {
    args.push("resume".to_owned());
  }

  args.extend(
    [
      "--json",
      "--ignore-user-config",
      "--ignore-rules",
      "--strict-config",
      "--skip-git-repo-check",
      "--model",
    ]
    .map(str::to_owned),
  );
  args.push(model.to_owned());

  if !keep_session {
    args.push("--ephemeral".to_owned());
  }

  if thread.is_none() {
    args.extend(["--sandbox", "read-only", "--cd", cwd, "--color", "never"].map(str::to_owned));
  }

  args.extend(config("approval_policy", json!("never")));
  args.extend(config("sandbox_mode", json!("read-only")));
  args.extend(config("model_instructions_file", json!(system_path)));
  args.extend(config("project_doc_max_bytes", json!(0)));
  args.extend(config("model_reasoning_effort", json!(if thinking { effort } else { "low" })));
  args.extend(config("web_search", json!("disabled")));

  for feature in ["apps", "multi_agent", "memories", "goals", "shell_tool"] {
    args.extend(config(&format!("features.{feature}"), json!(false)));
  }

  for instructions in ["permissions", "apps", "collaboration_mode"] {
    args.extend(config(&format!("include_{instructions}_instructions"), json!(false)));
  }

  args.extend(config("include_environment_context", json!(false)));
  args.extend(thread.map(str::to_owned));
  args.push("-".to_owned());

  Command {
    args,
    cwd: cwd.to_owned(),
    env: env.clone(),
    stdin: Some(prompt.to_owned()),
  }
}

/// What a run of `exec` said about failing: the last error its events carried, or the last line of its
/// error stream, or its exit.
pub fn exec_failure(stdout: &str, stderr: &str, exit_code: i32) -> String {
  let said = events(stdout).iter().rev().find_map(|event| {
    let message = match kind(event) {
      Some("error") => event.get("message").and_then(Value::as_str),
      Some("turn.failed") => event.pointer("/error/message").and_then(Value::as_str),
      _ => None,
    };

    message.filter(|message| !message.is_empty()).map(str::to_owned)
  });

  said
    .or_else(|| stderr.lines().map(str::trim).rfind(|line| !line.is_empty()).map(str::to_owned))
    .unwrap_or_else(|| format!("exit {exit_code}"))
}

// How much of a rollout is read at a time from its end, and how far back a turn's start is looked for: a
// turn longer than that is told by its last request alone rather than by a part that would read as a turn
// of its own.
const CHUNK_BYTES: u64 = 1 << 20;
const TURN_SCAN_BYTES: u64 = 64 << 20;

// One request a rollout's count recorded, where it is a count; a count of no input is the program folding
// its own thread, which is no request.
fn request_of(event: &Value) -> Result<Option<RequestUsage>, ()> {
  let usage = event.pointer("/payload/info/last_token_usage").ok_or(())?;
  let number = |name: &str| {
    usage
      .get(name)
      .and_then(Value::as_f64)
      .filter(|value| value.is_finite() && *value >= 0.0)
  };
  let (Some(input), Some(read), Some(output)) =
    (number("input_tokens"), number("cached_input_tokens"), number("output_tokens"))
  else {
    return Err(());
  };

  if input == 0.0 {
    return Ok(None);
  }

  let write = usage.get("cache_write_input_tokens").and_then(Value::as_f64).unwrap_or(0.0);

  Ok(Some(RequestUsage {
    input_tokens: (input - read - write).max(0.0) as i64,
    cache_read_tokens: read as i64,
    cache_write_tokens: write as i64,
    output_tokens: output as i64,
  }))
}

// The rollout a thread is recorded in, where exactly one under the directory names it.
fn rollout_of(sessions: &Path, thread: &str) -> Option<PathBuf> {
  let suffix = format!("{thread}.jsonl");
  let mut found = vec![];
  let mut dirs = vec![sessions.to_path_buf()];

  while let Some(dir) = dirs.pop() {
    for entry in std::fs::read_dir(&dir).ok()?.flatten() {
      let path = entry.path();

      if path.is_dir() {
        dirs.push(path);
      } else if path.file_name().is_some_and(|name| name.to_string_lossy().ends_with(&suffix)) {
        found.push(path);
      }
    }
  }

  (found.len() == 1).then(|| found.remove(0))
}

/// Every request of the thread's latest turn, oldest first, as its rollout counted them since the turn
/// began; none where the rollout cannot be read or carries a count that is not one.
pub fn turn_requests(sessions: &Path, thread: &str) -> Option<Vec<RequestUsage>> {
  if thread.is_empty() || !thread.chars().all(|letter| letter.is_ascii_alphanumeric() || letter == '-') {
    return None;
  }

  let mut file = std::fs::File::open(rollout_of(sessions, thread)?).ok()?;
  let end = file.metadata().ok()?.len();
  let mut position = end;
  let mut carry: Vec<u8> = vec![];
  let mut newest_first: Vec<RequestUsage> = vec![];

  while position > 0 {
    let length = CHUNK_BYTES.min(position);

    position -= length;
    file.seek(SeekFrom::Start(position)).ok()?;

    let mut data = vec![0; length as usize];

    file.read_exact(&mut data).ok()?;
    data.extend_from_slice(&carry);
    carry.clear();

    // A line is read only once it is whole; a newline byte never sits inside a character.
    if position > 0 {
      match data.iter().position(|byte| *byte == b'\n') {
        None => carry = std::mem::take(&mut data),
        Some(cut) => {
          carry = data[..cut].to_vec();
          data.drain(..=cut);
        }
      }
    }

    for line in String::from_utf8_lossy(&data).split('\n').rev() {
      let Ok(event) = serde_json::from_str::<Value>(line) else { continue };

      if kind(&event) != Some("event_msg") {
        continue;
      }

      match event.pointer("/payload/type").and_then(Value::as_str) {
        Some("task_started") => {
          newest_first.reverse();
          return Some(newest_first);
        }
        Some("token_count") => match request_of(&event) {
          Ok(Some(request)) => newest_first.push(request),
          Ok(None) => {}
          Err(()) => return None,
        },
        _ => {}
      }
    }

    if end - position >= TURN_SCAN_BYTES {
      newest_first.truncate(1);
      return Some(newest_first);
    }
  }

  newest_first.reverse();
  Some(newest_first)
}

/// The thread a standing Codex program's words resume, where they resume one.
pub fn session_of(args: &[String]) -> Option<&str> {
  (args.get(1).map(String::as_str) == Some("resume"))
    .then(|| args.get(2).map(String::as_str))
    .flatten()
}

/// Whether a failure says the program no longer holds the thread it was told to resume: the one resume
/// failure that makes a kept thread worthless, where a login, a limit or a model failing leaves it whole.
pub fn lost_thread(said: &str) -> bool {
  let lower = said.to_lowercase();

  lower.contains("no rollout found for thread id") || lower.contains("no saved session found")
}
