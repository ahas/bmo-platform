//! An agent's place as the launcher writes it (docs/agent-control.md, 「One standing pane per agent」,
//! 「What a resident turn reads」): the settings its resident program is started with, the program the
//! head stands, and the session a worker's standing program opened while nothing was said in it. The
//! head reads the program back as the agent host's place module reads it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use agent_host::identity::ProcessIdentity;
use agent_host::layout::name_component;
use agent_host::place::{ResidentCommand, RunningProgram};
use harness_providers::Machine;
use harness_providers::claude_code::ClaudeCode;
use policy::run_lines::RUN_LINES;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::fault;
use crate::Refusal;
use crate::command_reading::job_guard::FILE_WRITERS;
use crate::watches::links::place_node;

/// The variable a resident program names its place in.
pub const PLACE_VARIABLE: &str = "BMO_AGENT_PLACE";

/// The verbs' binary a place's status line and hooks call, from a tree's root, and the verb they call it
/// with.
pub const STATUS_BINARY: &str = "bin/bmo-agency";
pub const STATUS_VERB: &str = "agent-status";
/// The kind of call the verb is told it answers where it guards a shell command.
pub const SHELL_KIND: &str = "shell";
/// The kind of call the verb is told it answers where a failed request ended the turn.
pub const FAILURE_KIND: &str = "failure";

// The words the stop hook is started with where its line holds a turn's end while its background work
// runs, and while its final answer uses a banned word; the word the submit hook is started with where
// its line composes a person's typed line, and how long the program gives that hook.
const HOLD_TASKS: &str = "--hold-tasks";
const HOLD_TEXT_TOOL_CALL: &str = "--hold-text-tool-call";
const HOLD_WORDING: &str = "--hold-wording";
const COMPOSE_TYPED: &str = "--compose-typed";
/// The word the stop and submit hooks are started with where the line carries its program's background work
/// apart (docs/agent-control.md, 「Background work on a line that carries it apart」).
pub const CARRY_BACKGROUND: &str = "--carry-background";
// The word the shell hook is started with where its line guards its shell: it refuses a command that
// starts Claude Code.
const REFUSE_CLAUDE: &str = "--refuse-claude";
// The word the shell hook is started with where its line's programs are jobs': it refuses what the job guard
// refuses (docs/live-claude-guard.md).
const GUARD_JOB: &str = "--guard-job";
// The calls the shell hook reads: the shell's, and on a job's line the file tools that write as well, which the
// job guard judges for the file they name.
const SHELL_CALLS: &str = "Bash";
// The program kills a hook that outlasts its limit, so the submit hook is given the longest the program
// takes, a day, and the composing it waits on is never cut short by the program.
const TYPED_HOOK_SECONDS: u64 = 86_400;

// How long a link's word on a node's processes stands.
const NODE_HEARD_WITHIN_MS: i64 = 20_000;

// How long a killed program is waited on to be gone: rounds of a wait each.
const RELEASE_WAIT_ROUNDS: usize = 40;
const RELEASE_WAIT_MS: u64 = 50;

/// The variables that belong to one run rather than to the program, and the prefix every other one of
/// a run's own stands under.
const TURN_VARIABLES: [&str; 3] = ["BMO_RUN_NAME", "BMO_TASK_LOCKED", "BMO_WORK_ORDER_STEP"];
const TURN_PREFIX: &str = "BMO_TURN_";

/// The variable a resident program's command opts into the shell's carried variables with
/// (docs/agent-control.md, 「dsh's providers: OpenRouter and DeepSeek」): a program whose own harness starts
/// its shell on a scrubbed environment carries a copy of each `BMO_` variable beside it under a name of its
/// own, and its shell's start-up file puts each back.
pub const CARRY_VARIABLE: &str = "BMO_SHELL_CARRY";
/// The name a carried copy stands under: this prefix and the variable's name in upper-case hexadecimal,
/// whose letters spell none of the words a scrub drops a variable by.
pub const CARRIER_PREFIX: &str = "BMO_CARRIED_";
/// The variable bash reads the start-up file of every non-interactive shell from.
pub const SHELL_START: &str = "BASH_ENV";
// The prefix of the variables a program carries.
const CARRIED_PREFIX: &str = "BMO_";

// The shell start-up file that puts each carried variable back under its own name.
const CARRY_SCRIPT: &str = r#"# Written by agency (docs/agent-control.md): each variable a harness drops from its shell by its name,
# put back from the copy the program carries under a name it keeps.
for bmo_carried in "${!BMO_CARRIED_@}"; do
  bmo_hex=${bmo_carried#BMO_CARRIED_}
  bmo_name=
  for ((bmo_at = 0; bmo_at < ${#bmo_hex}; bmo_at += 2)); do
    printf -v bmo_char "\x${bmo_hex:bmo_at:2}"
    bmo_name+=$bmo_char
  done
  if [[ $bmo_name =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] && [[ -z ${!bmo_name+set} ]]; then
    export "$bmo_name=${!bmo_carried}"
  fi
done
unset bmo_carried bmo_hex bmo_name bmo_at bmo_char
# A conversational agent's shell guard (docs/agents.md, 「No print mode」): the command
# about to run handed to agency's guard, and refused before it runs.
if [[ -n ${BMO_SHELL_GUARD-} && -n ${BASH_EXECUTION_STRING-} && -x ${BMO_SHELL_GUARD} ]]; then
  if ! bmo_refused=$(printf '%s' "$BASH_EXECUTION_STRING" | "$BMO_SHELL_GUARD" guard-shell 2>&1); then
    printf '%s\n' "$bmo_refused" >&2
    exit 126
  fi
  unset bmo_refused
fi
"#;

/// The carrier name a variable's copy stands under.
pub fn carrier_of(name: &str) -> String {
  let hex: String = name.bytes().map(|byte| format!("{byte:02X}")).collect();

  format!("{CARRIER_PREFIX}{hex}")
}

/// A command's environment with its carried copies made afresh, where it opts in: every `BMO_` variable
/// beside it under its carrier name, and none of the copies an earlier composition left.
pub fn carry(env: &mut BTreeMap<String, String>) {
  if env.get(CARRY_VARIABLE).map(String::as_str) != Some("1") {
    return;
  }

  env.retain(|name, _| !name.starts_with(CARRIER_PREFIX));

  let copies: Vec<(String, String)> = env
    .iter()
    .filter(|(name, _)| name.starts_with(CARRIED_PREFIX) && name.as_str() != CARRY_VARIABLE)
    .map(|(name, value)| (carrier_of(name), value.clone()))
    .collect();

  env.extend(copies);
}

/// The variable naming the guard a conversational agent's dsh shell asks before each command: the deploy tree's
/// `bmo-agency`, set on a conversational line's program alone.
pub const SHELL_GUARD_VARIABLE: &str = "BMO_SHELL_GUARD";

/// The shell start-up file that puts carried variables back, written under the data root where it is not
/// there as it stands.
pub fn carry_script(data_root: &Path) -> Result<PathBuf, Refusal> {
  let dir = data_root.join("shell");
  let path = dir.join("carry.sh");

  if std::fs::read_to_string(&path).ok().as_deref() != Some(CARRY_SCRIPT) {
    std::fs::create_dir_all(&dir).map_err(|why| fault(format!("{}: {why}", dir.display())))?;
    agent_host::disk::publish_bytes(&path, CARRY_SCRIPT.as_bytes())
      .map_err(|why| fault(format!("{}: {why}", path.display())))?;
  }

  Ok(path)
}

/// Where a place keeps its program's error stream, for a program whose command names it.
pub fn stderr_file(place: &Path) -> PathBuf {
  place.join("stderr.log")
}

/// Where every agent's place stands under a data root.
pub fn places_dir(data_root: &Path) -> PathBuf {
  data_root.join("agent-places")
}

/// An agent's place under a data root.
pub fn place_of(data_root: &Path, agent: &str) -> PathBuf {
  places_dir(data_root).join(name_component(agent))
}

/// What a line asks of its place's hooks.
#[derive(Clone, Copy, Debug, Default)]
pub struct Holds {
  pub tasks: bool,
  pub text_tool_call: bool,
  pub wording: bool,
  pub compose_typed: bool,
  /// Whether a shell command that starts Claude Code is refused before it runs (docs/agents.md, 「No print
  /// mode」). Every shell command is read before it runs either way, to close its standard input.
  pub shell_guard: bool,
  /// Whether the shell hook refuses what the job guard refuses of a job's shell command, the line's programs
  /// being jobs' (docs/live-claude-guard.md).
  pub job_guard: bool,
  /// Whether the stop and submit hooks write the program's background work into the place's background
  /// record, the line carrying that work apart.
  pub carry_background: bool,
}

impl Holds {
  /// What a run of the line asks of its place's hooks, as its declaration names it: the stop holds, a
  /// person's typed line composed and the job guard; no shell guard, which a caller adds where its program
  /// carries one.
  pub fn of_line(line: &str) -> Self {
    let stops = RUN_LINES.holds_stops(line);

    Self {
      tasks: stops,
      text_tool_call: stops,
      wording: stops,
      compose_typed: RUN_LINES.composes_typed_lines(line),
      shell_guard: false,
      job_guard: RUN_LINES.guards_job_shell(line),
      carry_background: RUN_LINES.background_apart(line),
    }
  }
}

// A word as one shell word, quoted.
fn quoted(word: &str) -> String {
  format!("'{}'", word.replace('\'', r"'\''"))
}

/// The words every status-line and hook call of a place's program begins with: this tree's verbs binary; or,
/// for a place standing on a node, the node's link handing the call to the home bridge to run with the CLI's
/// `bmo` on this machine, so what the call writes lands in the place here, where the door reads it
/// (docs/person-nodes.md, 「Which node is a person's」). The place named stays this machine's, which the bridge
/// leaves as it is.
fn hook_verbs(place: &Path, tree: &Path) -> Vec<String> {
  if crate::watches::links::place_node(place).is_some() {
    return [NODE_LINK, NODE_LINK_HOME, BRIDGE_VERBS, STATUS_VERB].map(str::to_owned).to_vec();
  }

  vec![tree.join(STATUS_BINARY).to_string_lossy().into_owned(), STATUS_VERB.to_owned()]
}

/// The node's link binary, found on the search path every program on a node runs with, and its verb that asks
/// the home bridge; and the CLI's verbs binary the bridge runs, under its deploy tree.
const NODE_LINK: &str = "node-link";
const NODE_LINK_HOME: &str = "home";
const BRIDGE_VERBS: &str = "bin/bmo";

/// The settings a resident program is started with: the status-line command and the four hooks, each the
/// tree's `bmo agent status` told which place to write into, and the shell hook, which closes every shell
/// command's standard input and refuses one that starts Claude Code where the line asks for it, merged into
/// the settings the line already hands its program so a hook the line had keeps standing beside these.
/// Written into the place and named back.
pub fn write_place_settings(
  place: &Path,
  mut settings: Map<String, Value>,
  tree: &Path,
  holds: Holds,
) -> Result<PathBuf, Refusal> {
  let verbs = hook_verbs(place, tree);
  let call = |kind: &str, flags: &[&str]| {
    let mut words = verbs.clone();

    words.extend([kind.to_owned(), "--place".to_owned(), place.to_string_lossy().into_owned()]);

    words.extend(flags.iter().map(|flag| (*flag).to_owned()));
    words.iter().map(|word| quoted(word)).collect::<Vec<_>>().join(" ")
  };
  let mut stop_flags = vec![];

  if holds.tasks {
    stop_flags.push(HOLD_TASKS);
  }

  if holds.text_tool_call {
    stop_flags.push(HOLD_TEXT_TOOL_CALL);
  }

  if holds.wording {
    stop_flags.push(HOLD_WORDING);
  }

  let mut submit_flags = vec![];

  if holds.carry_background {
    stop_flags.push(CARRY_BACKGROUND);
    submit_flags.push(CARRY_BACKGROUND);
  }

  let submit = if holds.compose_typed {
    submit_flags.push(COMPOSE_TYPED);
    json!({ "type": "command", "command": call("submit", &submit_flags), "timeout": TYPED_HOOK_SECONDS })
  } else {
    json!({ "type": "command", "command": call("submit", &submit_flags) })
  };
  let mut own = vec![
    (
      "Stop",
      json!({ "hooks": [{ "type": "command", "command": call("stop", &stop_flags) }] }),
    ),
    // A turn a failed request ended fires this in place of the stop, and the program reads nothing it
    // answers.
    (
      "StopFailure",
      json!({ "hooks": [{ "type": "command", "command": call(FAILURE_KIND, &[]) }] }),
    ),
    ("UserPromptSubmit", json!({ "hooks": [submit] })),
    (
      "SessionStart",
      json!({ "hooks": [{ "type": "command", "command": call("start", &[]) }] }),
    ),
  ];

  let shell_flags: Vec<&str> = [(holds.shell_guard, REFUSE_CLAUDE), (holds.job_guard, GUARD_JOB)]
    .into_iter()
    .filter_map(|(asked, flag)| asked.then_some(flag))
    .collect();

  let shell_calls = match holds.job_guard {
    true => std::iter::once(SHELL_CALLS).chain(FILE_WRITERS).collect::<Vec<_>>().join("|"),
    false => SHELL_CALLS.to_owned(),
  };

  own.push(shell_hook(&shell_calls, call(SHELL_KIND, &shell_flags)));
  settings.insert("statusLine".to_owned(), json!({ "type": "command", "command": call("status", &[]) }));
  merge_hooks(&mut settings, own);
  // The program runs with permissions bypassed, so its warning dialog would hold a fresh program at its
  // start until a person answered it; the setting tells it the answer.
  settings.insert("skipDangerousModePermissionPrompt".to_owned(), Value::Bool(true));
  // The plugins the program builds in reach no run (docs/launch-lines.md, 「The CLI's own skills and plugins are
  // switched off on every line」); a plugin the given settings name stands as they name it.
  let mut plugins = settings.get("enabledPlugins").and_then(Value::as_object).cloned().unwrap_or_default();

  for (plugin, off) in builtin_plugins_off() {
    plugins.entry(plugin).or_insert(off);
  }

  settings.insert("enabledPlugins".to_owned(), Value::Object(plugins));

  let path = place.join("settings.json");
  let text = serde_json::to_string_pretty(&settings).map_err(|why| fault(why.to_string()))?;

  std::fs::create_dir_all(place).map_err(|why| fault(format!("{}: {why}", place.display())))?;
  std::fs::write(&path, format!("{text}\n")).map_err(|why| fault(format!("{}: {why}", path.display())))?;

  Ok(path)
}

/// The settings entry naming each plugin Claude Code builds in off, by the name the program gives it.
pub fn builtin_plugins_off() -> Map<String, Value> {
  policy::providers::claude_code::CLAUDE_CODE
    .launch
    .builtin_plugins
    .iter()
    .map(|plugin| (format!("{plugin}@builtin"), Value::Bool(false)))
    .collect()
}

// The shell hook's entry, read before every call of the tools `matcher` names that the program makes.
fn shell_hook(matcher: &str, command: String) -> (&'static str, Value) {
  (
    "PreToolUse",
    json!({ "matcher": matcher, "hooks": [{ "type": "command", "command": command }] }),
  )
}

// The hooks named laid after the ones the settings already hold for the same events.
fn merge_hooks(settings: &mut Map<String, Value>, own: Vec<(&'static str, Value)>) {
  let mut hooks = settings.get("hooks").and_then(Value::as_object).cloned().unwrap_or_default();

  for (event, entry) in own {
    let mut entries = hooks.get(event).and_then(Value::as_array).cloned().unwrap_or_default();

    entries.push(entry);
    hooks.insert(event.to_owned(), Value::Array(entries));
  }

  settings.insert("hooks".to_owned(), Value::Object(hooks));
}

/// Whether the program standing in a place carries its background work apart: the stop hook its settings name
/// is started with the word that says so. A place whose settings cannot be read does not.
pub fn carries_background(place: &Path) -> bool {
  let Some(settings) = agent_host::disk::read_json::<Value>(&place.join("settings.json")) else {
    return false;
  };

  settings
    .pointer("/hooks/Stop")
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter_map(|entry| entry.get("hooks").and_then(Value::as_array))
    .flatten()
    .filter_map(|hook| hook.get("command").and_then(Value::as_str))
    .any(|command| command.contains(CARRY_BACKGROUND))
}

/// Whether a variable belongs to one run rather than to the program.
pub fn is_turn_variable(name: &str) -> bool {
  TURN_VARIABLES.contains(&name) || name.starts_with(TURN_PREFIX)
}

/// The identity of a command a program was started on: its words, where it stands, what it is handed,
/// and the status command the settings file it is handed names now, with every guard the place's own verb
/// stands before a tool call, in a fixed order. Two commands with one digest start the same program; a
/// program keeps the status line and hooks it started with, so one whose settings came to call another
/// command, or to guard its tools otherwise, is another program.
pub fn command_digest(command: &ResidentCommand) -> String {
  #[derive(Serialize)]
  struct Hashed<'a> {
    args: &'a [String],
    cwd: &'a Path,
    env: Vec<(&'a String, &'a String)>,
    settings: Vec<String>,
  }

  let settings = command
    .args
    .windows(2)
    .filter(|pair| pair[0] == "--settings")
    .map(|pair| {
      std::fs::read_to_string(command.cwd.join(&pair[1]))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|settings| {
          let status = settings.pointer("/statusLine/command").and_then(Value::as_str)?.to_owned();
          let guards = settings
            .pointer("/hooks/PreToolUse")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|entry| entry["hooks"].as_array().cloned().unwrap_or_default())
            .filter_map(|hook| hook["command"].as_str().map(str::to_owned))
            .filter(|command| command.contains(STATUS_VERB));

          Some(std::iter::once(status).chain(guards).collect::<Vec<_>>().join("\n"))
        })
        .unwrap_or_default()
    })
    .collect();
  let hashed = Hashed {
    args: &command.args,
    cwd: &command.cwd,
    env: command.env.iter().collect(),
    settings,
  };
  let text = serde_json::to_string(&hashed).unwrap_or_default();

  format!("{:x}", Sha256::digest(text.as_bytes()))[..32].to_owned()
}

/// The program a launcher writes into a place, as the head reads it back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Program {
  pub identity: String,
  pub command: ResidentCommand,
  pub restart: Option<ResidentCommand>,
  /// The record whose existence says the session the first command opens is open, none where it
  /// opens none; absent from a program whose restart can always be taken.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub opened: Option<Option<PathBuf>>,
}

impl Program {
  /// Whether the program stands on the session named: its restart, the form that resumes the session it
  /// stands on, names it.
  pub fn resumes(&self, session: &str) -> bool {
    self.restart.as_ref().is_some_and(|restart| {
      restart.args.windows(2).any(|pair| pair[0] == "--resume" && pair[1] == session)
    })
  }
}

/// A command that opens a session under a name it chose, as the command that resumes that session.
pub fn resumed_command(args: &[String], session: &str) -> Vec<String> {
  let mut resumed = args.to_vec();

  if let Some(at) = args.iter().position(|arg| arg == "--session-id") {
    resumed.splice(at..(at + 2).min(args.len()), ["--resume".to_owned(), session.to_owned()]);
  }

  resumed
}

/// The program a standing agent's command makes: its environment without the run's own variables and
/// with the place it answers to, compared by the form that resumes its session, and the record whose
/// existence says the session it opens is open.
pub fn resident_of(
  machine: &Machine,
  place: &Path,
  cwd: &str,
  args: &[String],
  env: &BTreeMap<String, String>,
  session: &str,
) -> Program {
  let mut env: BTreeMap<String, String> = env
    .iter()
    .filter(|(name, _)| !is_turn_variable(name))
    .map(|(name, value)| (name.clone(), value.clone()))
    .collect();

  env.insert(PLACE_VARIABLE.to_owned(), place.to_string_lossy().into_owned());

  let restart = resumed_command(args, session);
  let opened = args.iter().any(|arg| arg == "--session-id").then(|| {
    let config_dir = env
      .get("CLAUDE_CONFIG_DIR")
      .filter(|dir| !dir.is_empty())
      .map_or_else(|| ClaudeCode::machine_config_dir(machine), PathBuf::from);

    ClaudeCode::records_under(&config_dir, Path::new(cwd), session).swap_remove(0)
  });
  let command = |args: Vec<String>| ResidentCommand {
    args,
    cwd: PathBuf::from(cwd),
    env: env.clone(),
    login_file: None,
  };
  let restart = command(restart);

  Program {
    identity: command_digest(&restart),
    command: command(args.to_vec()),
    restart: Some(restart),
    opened: Some(opened),
  }
}

/// The files a program's commands name that it cannot start without: every word of either command that is the
/// absolute path of a file standing as the program is written. The agent host starts the program only while each
/// is there (docs/agent-control.md, 「One standing pane per supervisor」).
pub fn needed_files(program: &Program) -> Vec<PathBuf> {
  let mut needed: Vec<PathBuf> = std::iter::once(&program.command)
    .chain(program.restart.as_ref())
    .flat_map(|command| command.args.iter())
    .map(PathBuf::from)
    .filter(|path| path.is_absolute() && path.is_file())
    .collect();

  needed.sort();
  needed.dedup();
  needed
}

/// A place: where it is, and the machine whose processes it names.
pub struct Place {
  pub path: PathBuf,
  data: PathBuf,
}

impl Place {
  pub fn of(data_root: &Path, agent: &str) -> Self {
    Self {
      path: place_of(data_root, agent),
      data: data_root.to_path_buf(),
    }
  }

  fn program_file(&self) -> PathBuf {
    self.path.join("program.json")
  }

  /// The program the place holds, or none.
  pub fn program(&self) -> Option<Program> {
    serde_json::from_slice(&std::fs::read(self.program_file()).ok()?).ok()
  }

  /// The program standing, as its head recorded it.
  pub fn running(&self) -> Option<RunningProgram> {
    agent_host::disk::read_json(&self.path.join("running.json"))
  }

  /// The node the place stands on, or none for a place here.
  pub fn node(&self) -> Option<String> {
    place_node(&self.path)
  }

  /// Whether a process the place names is alive, asked of the machine it runs on: this one's kernel
  /// for a place here, what the link last told for a place on a node. A link that has told nothing
  /// lately says nothing either way, and the process is taken as standing rather than stood up again
  /// over a program that may be working.
  pub fn alive(&self, pid: i32, birth: Option<&str>) -> bool {
    if self.node().is_none() {
      return birth.is_some_and(|birth| {
        ProcessIdentity {
          pid,
          birth: birth.to_owned(),
        }
        .alive()
      });
    }

    let told: Option<Value> = agent_host::disk::read_json(&self.path.join("node-alive.json"));
    let Some(at) = told.as_ref().and_then(|told| told.get("at")).and_then(Value::as_i64) else {
      return true;
    };

    if crate::now_ms() - at > NODE_HEARD_WITHIN_MS {
      return true;
    }

    told
      .as_ref()
      .and_then(|told| told.get("alive"))
      .and_then(Value::as_array)
      .is_some_and(|alive| {
        alive.iter().any(|one| {
          one.get("pid").and_then(Value::as_i64) == Some(i64::from(pid))
            && birth.is_none_or(|birth| one.get("birth").and_then(Value::as_str) == Some(birth))
        })
      })
  }

  /// The program standing in the place, where its process is alive and still the one recorded.
  pub fn program_alive(&self) -> Option<RunningProgram> {
    self.running().filter(|running| self.alive(running.pid, running.birth.as_deref()))
  }

  /// A process of the place ended on the machine it runs on: signalled here, or through the node's
  /// link.
  pub async fn end(&self, pid: i32) {
    self.signal(pid, libc::SIGTERM).await;
  }

  // A process of the place sent a signal on the machine it runs on.
  async fn signal(&self, pid: i32, signal: i32) {
    let Some(node) = self.node() else {
      // SAFETY: a signal sent to a pid the place recorded as alive a moment ago.
      unsafe { libc::kill(pid, signal) };

      return;
    };
    let running = self.running();
    let birth = running.as_ref().and_then(|running| {
      if running.pid == pid {
        running.birth.clone()
      } else if running.head.pid == pid {
        Some(running.head.birth.clone())
      } else {
        None
      }
    });
    let socket = self.data.join("nodes").join(&node).join("control.sock");
    let asked = json!({ "signal": { "pid": pid, "birth": birth, "signal": signal } });
    let send = async {
      let mut stream = tokio::net::UnixStream::connect(&socket).await.ok()?;

      stream.write_all(format!("{asked}\n").as_bytes()).await.ok()?;

      let mut answer = String::new();

      BufReader::new(stream).read_line(&mut answer).await.ok()
    };

    // The link's answer is waited for until it comes or its socket closes.
    let _ = send.await;
  }

  // Written whole and private: the command carries the login a program goes out on.
  fn write_program(&self, program: &Program) -> Result<(), Refusal> {
    agent_host::disk::private_dir(&self.path)
      .map_err(|why| fault(format!("{}: {why}", self.path.display())))?;

    // The carried copies are made from the command as it is written, the session token among it.
    let mut program = program.clone();

    for command in std::iter::once(&mut program.command).chain(program.restart.as_mut()) {
      carry(&mut command.env);
    }

    let mut written = serde_json::to_value(&program).map_err(|why| fault(why.to_string()))?;

    written["needs"] =
      serde_json::to_value(needed_files(&program)).map_err(|why| fault(why.to_string()))?;

    let mut bytes = serde_json::to_vec(&written).map_err(|why| fault(why.to_string()))?;

    bytes.push(b'\n');
    agent_host::disk::publish_bytes(&self.program_file(), &bytes)
      .map_err(|why| fault(format!("{}: {why}", self.program_file().display())))
  }

  /// A program written into a place that holds none, for the head to start with nothing put in. A
  /// place that already holds one keeps it. Answers whether it was written.
  pub fn offer(&self, program: &Program) -> Result<bool, Refusal> {
    if self.program_file().exists() {
      return Ok(false);
    }

    self.write_program(program)?;

    Ok(true)
  }

  /// A program written into a place in place of whatever it holds, and the one standing ended where it
  /// is another, which the loop answers by starting the new one in the same pane. The new one is
  /// written first, so the loop finds it rather than nothing. Answers the program the place held
  /// before, and whether a standing program was ended.
  pub async fn replace(&self, program: &Program) -> Result<(Option<Program>, bool), Refusal> {
    let before = self.program();
    let tokened = token_of(program).is_some() && before.as_ref().and_then(token_of).is_none();

    if before.as_ref().is_some_and(|before| before.identity == program.identity) && !tokened {
      return Ok((before, false));
    }

    self.write_program(program)?;

    let Some(running) = self.program_alive() else {
      return Ok((before, false));
    };

    if running.identity == program.identity && !tokened {
      return Ok((before, false));
    }

    self.end(running.pid).await;

    Ok((before, true))
  }

  /// A session the standing program opened for itself, made its own: the place holds the program on that
  /// session, carrying the session token the one it held was started with, and the program standing is
  /// relabelled rather than started again, where it is still the one `from` names.
  pub fn adopt(&self, from: &str, program: &Program) -> Result<(), Refusal> {
    let mut carried = program.clone();

    if let Some(token) = self.program().as_ref().and_then(token_of).map(str::to_owned) {
      for command in std::iter::once(&mut carried.command).chain(carried.restart.as_mut()) {
        command.env.insert(super::token::TOKEN_VARIABLE.to_owned(), token.clone());
      }
    }

    self.write_program(&carried)?;

    if let Some(running) = self.running().filter(|running| running.identity == from) {
      agent_host::place::Place(self.path.clone())
        .record_running(&RunningProgram {
          identity: program.identity.clone(),
          ..running
        })
        .map_err(|why| fault(format!("{}: {why}", self.path.display())))?;
    }

    Ok(())
  }

  /// The place handed back to the host: the program standing ended, and the loop, finding none, serves
  /// the host. Answers the program the place held, where it held one.
  pub async fn release(&self) -> Option<Program> {
    self.release_by(libc::SIGTERM).await.0
  }

  /// The place left holding no program, as `release` leaves it, where the one standing must write nothing
  /// more: killed rather than asked to end, which a program answers by writing its session's record as it
  /// goes, and waited on until it is gone, so a record of its session removed after this is never written
  /// again.
  pub async fn release_now(&self) -> Option<Program> {
    let (before, ended) = self.release_by(libc::SIGKILL).await;

    if let Some(ended) = ended {
      for _ in 0..RELEASE_WAIT_ROUNDS {
        if !self.alive(ended.pid, ended.birth.as_deref()) {
          break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(RELEASE_WAIT_MS)).await;
      }
    }

    before
  }

  // The place's program let go and the one standing sent the signal: the program it held, and the one it
  // signalled.
  async fn release_by(&self, signal: i32) -> (Option<Program>, Option<RunningProgram>) {
    let before = self.program();

    if !self.program_file().exists() {
      return (None, None);
    }

    let _ = std::fs::remove_file(self.program_file());

    let running = self.program_alive();

    if let Some(running) = &running {
      self.signal(running.pid, signal).await;
    }

    (before, running)
  }
}

/// The session token a written program carries.
pub fn token_of(program: &Program) -> Option<&str> {
  program.command.env.get(super::token::TOKEN_VARIABLE).map(String::as_str)
}

/// The session a worker's standing program opened while nothing has been said in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Standby {
  /// The program this session was opened by, as the place names it.
  pub identity: String,
  pub session_id: String,
  pub model: String,
  /// The login, as the worker's session records name it.
  pub account: String,
  /// The session's own record, which exists once anything has been said in it.
  pub record: PathBuf,
  /// What the prefix the session opens on reduces to.
  pub prefix_digest: Option<String>,
}

impl Standby {
  fn file(place: &Path) -> PathBuf {
    place.join("standby.json")
  }

  pub fn read(place: &Path) -> Option<Self> {
    serde_json::from_slice(&std::fs::read(Self::file(place)).ok()?).ok()
  }

  pub fn write(&self, place: &Path) -> Result<(), Refusal> {
    agent_host::disk::private_dir(place).map_err(|why| fault(format!("{}: {why}", place.display())))?;

    let mut bytes = serde_json::to_vec(self).map_err(|why| fault(why.to_string()))?;

    bytes.push(b'\n');
    agent_host::disk::publish_bytes(&Self::file(place), &bytes)
      .map_err(|why| fault(format!("{}: {why}", Self::file(place).display())))
  }

  pub fn drop_from(place: &Path) {
    let _ = std::fs::remove_file(Self::file(place));
  }

  /// The session a turn about to open a fresh one on this model and login opens instead: the one the
  /// standing program already opened, while nothing has been said in it. The standby is spent by it.
  pub fn opening(place: &Path, model: &str, account: &str) -> Option<String> {
    let standby = Self::read(place)?;

    if standby.model != model || standby.account != account || standby.record.exists() {
      return None;
    }

    Self::drop_from(place);

    Some(standby.session_id)
  }
}

/// A line's own settings as an object to lay the place's over; none where the file is not there or
/// is not one.
pub fn read_settings(path: &Path) -> Map<String, Value> {
  std::fs::read_to_string(path)
    .ok()
    .and_then(|text| serde_json::from_str::<Map<String, Value>>(&text).ok())
    .unwrap_or_default()
}
