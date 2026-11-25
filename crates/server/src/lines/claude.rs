//! Claude Code as a resident program of the worker line (docs/launch-lines.md; docs/agent-control.md,
//! 「The interactive surface a provider declares」): its words, its environment, and the environment
//! the pane it stands in is handed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use harness_providers::claude_code::ClaudeCode;
use policy::providers::claude_code::CLAUDE_CODE;
use policy::run_lines::RUN_LINES;

use super::fault;
use super::worker::{LINE, WorkerRun};
use crate::Refusal;
use crate::control::claude_accounts::{Account, Credential};
use crate::watches::links::place_node;

// The variable a run is told its own name in, and the job runner's marker.
const RUN_NAME: &str = "BMO_RUN_NAME";
const JOB_RUNNER_MARKER: &str = "BMO_TASK_LOCKED";

/// Which session a program opens or continues.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Session {
  Resume(String),
  Open(String),
}

impl Session {
  pub fn id(&self) -> &str {
    match self {
      Self::Resume(id) | Self::Open(id) => id,
    }
  }
}

/// A command as a line builds it, before the pane's environment is made of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
  pub args: Vec<String>,
  pub cwd: String,
  /// Every variable the program is started with, an unset one among them as none.
  pub env: BTreeMap<String, Option<String>>,
}

/// The flag the program is handed its advisor by. The advisor stands outside the prefix a session's cache keeps,
/// so it is no word of a session's key or prefix (docs/provider-order.md, 「The advisor」).
pub const ADVISOR_FLAG: &str = "--advisor";

/// A command's words without its advisor: the flag and the model after it left out.
pub fn without_advisor(args: &[String]) -> Vec<&String> {
  let mut kept = Vec::with_capacity(args.len());
  let mut words = args.iter();

  while let Some(word) = words.next() {
    if word == ADVISOR_FLAG {
      words.next();
      continue;
    }

    kept.push(word);
  }

  kept
}

/// The words that start the program with its advisor (docs/provider-order.md, 「The advisor」): none where the run
/// names none, and none where the program would not attach it to the model the run goes out on — a model a
/// gateway serves under a name of its own among them —, which the server's log says, since the program refuses
/// to start on an advisor it cannot pair.
pub fn advisor_words(model: &str, advisor: Option<&str>) -> Vec<String> {
  let Some(advisor) = advisor else {
    return Vec::new();
  };

  if !CLAUDE_CODE.advisor.attaches(model, advisor) {
    eprintln!(
      "agency: {model} goes out without its advisor {advisor}, which Claude Code would not attach to it"
    );

    return Vec::new();
  }

  vec![ADVISOR_FLAG.to_owned(), advisor.to_owned()]
}

/// A path the worker line hands the program, and never one that is not there.
pub fn handed_path(path: &Path) -> Result<String, Refusal> {
  handed(LINE, path)
}

/// A path a line hands the program, and never one that is not there: a program handed a path that is
/// not on the disk exits before it has read a word of the prompt.
pub fn handed(line: &str, path: &Path) -> Result<String, Refusal> {
  if path.exists() {
    return Ok(path.to_string_lossy().into_owned());
  }

  Err(fault(format!(
    "The {line} line hands the CLI {}, which is missing. Without it the CLI exits with code 1 before it \
     reads a word of the prompt, and the run's record says only that no answer came. This happens when the \
     deploy tree lags the checkout: deploy, then run again.",
    path.display()
  )))
}

/// What a line's program is never shown (docs/launch-lines.md, 「What a line carries」): the tools of its own its
/// line denies, and each of the program's own subagent types its carries do not keep, named as the program
/// names a subagent type's call.
pub fn line_denied(line: &str) -> String {
  let kept = &RUN_LINES.carries(line).subagents;
  let denied = RUN_LINES.line(line).map(|line| line.tools_denied.clone()).unwrap_or_default();

  denied
    .into_iter()
    .chain(
      CLAUDE_CODE
        .launch
        .agent_types
        .iter()
        .filter(|kind| !kept.contains(kind))
        .map(|kind| format!("Agent({kind})")),
    )
    .collect::<Vec<_>>()
    .join(",")
}

/// The worker line's words as a person starts the program: no print mode, the flags of the line in
/// their fixed order, and the session it opens or continues last. `settings` is the settings the
/// program is handed, its place's for a resident program.
pub fn worker_args(
  run: &WorkerRun,
  settings: &Path,
  session: Option<&Session>,
) -> Result<Vec<String>, Refusal> {
  let mut args = vec![
    ClaudeCode::PROGRAM.to_owned(),
    "--model".to_owned(),
    run.model.clone(),
    "--effort".to_owned(),
    run.effort.clone(),
  ];

  args.extend(advisor_words(&run.model, run.advisor.as_deref()));
  args.extend([
    "--permission-mode".to_owned(),
    "bypassPermissions".to_owned(),
    "--disallowedTools".to_owned(),
    run.tools_denied.clone(),
  ]);

  // One whole system prompt in the program's own place: a base prompt's where one is chosen, the platform's
  // harness opening's otherwise, the worker's rules after either.
  let prompt = run.system_prompt.as_ref().unwrap_or(&run.harnessed);

  args.extend(["--system-prompt-file".to_owned(), handed_path(prompt)?]);

  args.extend([
    "--setting-sources".to_owned(),
    RUN_LINES.carries(LINE).setting_sources_word(),
    "--settings".to_owned(),
    handed_path(settings)?,
    // Strict, so no MCP server the account's login brings — a claude.ai connector — rides on a run beside
    // the line's own (docs/launch-lines.md, 「What a line's program is allowed, by road」).
    "--strict-mcp-config".to_owned(),
    "--mcp-config".to_owned(),
    handed_path(&run.mcp_config)?,
    "--plugin-dir".to_owned(),
    handed_path(&run.plugin_dir)?,
  ]);

  match session {
    Some(Session::Resume(id)) => args.extend(["--resume".to_owned(), id.clone()]),
    Some(Session::Open(id)) => args.extend(["--session-id".to_owned(), id.clone()]),
    None => {}
  }

  Ok(args)
}

/// A run of a line Claude Code runs apart from the bot — a runner line's — as its words
/// are made of it (docs/agents.md, 「Runs apart」): its model and effort, the instructions standing in
/// place of the CLI's own, the one MCP configuration attached, the tools it is shown and allowed, and
/// whether the CLI's own commands are reachable.
pub struct ApartRun<'a> {
  pub line: &'a str,
  pub model: &'a str,
  pub effort: &'a str,
  pub system_prompt: Option<&'a Path>,
  pub mcp_config: Option<&'a Path>,
  pub tools_shown: &'a str,
  pub tools_allowed: &'a str,
  pub host_commands: bool,
  /// The model the program may consult, where the run's road names one.
  pub advisor: Option<&'a str>,
}

/// The words of a run apart, the interactive program opening the session named, which no run resumes: no
/// settings sources, the strict MCP flag beside the one configuration, no Chrome, and don't-ask mode over
/// what it is shown.
pub fn apart_args(run: &ApartRun, session: &str) -> Result<Vec<String>, Refusal> {
  let mut args = vec![ClaudeCode::PROGRAM.to_owned()];

  args.extend(["--model", run.model, "--effort", run.effort].map(str::to_owned));
  args.extend(advisor_words(run.model, run.advisor));
  args.extend(["--setting-sources".to_owned(), RUN_LINES.carries(run.line).setting_sources_word()]);

  if let Some(prompt) = run.system_prompt {
    args.extend(["--system-prompt-file".to_owned(), handed(run.line, prompt)?]);
  }

  args.push("--strict-mcp-config".to_owned());

  if let Some(config) = run.mcp_config {
    args.extend(["--mcp-config".to_owned(), handed(run.line, config)?]);
  }

  if !run.host_commands {
    args.push("--disable-slash-commands".to_owned());
  }

  args.extend(
    [
      "--no-chrome",
      "--permission-mode",
      "dontAsk",
      "--tools",
      run.tools_shown,
      "--allowedTools",
      run.tools_allowed,
      "--session-id",
      session,
    ]
    .map(str::to_owned),
  );

  Ok(args)
}

/// Where a program goes out from, as its login is handed to it, each account's token for the nodes kept
/// under the data root named (docs/work-nodes.md, 「The logins on a node」).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Runs {
  /// This machine, on the account's login, or on its token for the nodes beside it while it carries on by
  /// that token (docs/cache-fallback.md, 「A directory account carries on by its token」).
  Here { data_root: PathBuf },
  /// This machine, on the account's own login alone: the run that renews that login, and an endpoint
  /// account's launch, whose credential is its own.
  OwnLogin,
  /// A work node, which is handed each account's token for the nodes.
  OnNode { data_root: PathBuf },
}

impl Runs {
  /// This machine, on agency's ground.
  pub fn here(ground: &crate::places::Ground) -> Self {
    Self::Here {
      data_root: ground.roots.data.clone(),
    }
  }

  /// Where a program standing in a place goes out from: the node the place is marked with, or here.
  pub fn of_place(data_root: &Path, place: &Path) -> Self {
    let data_root = data_root.to_path_buf();

    match place_node(place) {
      Some(_) => Self::OnNode { data_root },
      None => Self::Here { data_root },
    }
  }
}

/// The login as the program is handed it: a directory account names its configuration home and clears
/// any token, or hands its token for the nodes beside it on a node where this machine holds one and here
/// while it carries on by that token; a token
/// account hands the token read from its file; an endpoint account names its configuration home, its
/// base URL, the key read from its file as the run's authorisation token and the model id its map names
/// for each family, clearing every other credential the launching process carried, and never goes out
/// from a work node, since its key stays on this machine. An `api-key` account names its configuration
/// home and hands the key read from its secret as `ANTHROPIC_API_KEY`, clearing every other credential; the
/// platform's never goes out from a work node, and a person's goes out on theirs with the run's variables. A
/// person's directory account signed in on their node names the directory there and is handed no token.
pub fn account_env(
  account: Option<&Account>,
  runs: &Runs,
) -> Result<BTreeMap<String, Option<String>>, Refusal> {
  let Some(account) = account else { return Ok(BTreeMap::new()) };

  match &account.credential {
    Credential::Token { file } => {
      let token = std::fs::read_to_string(file)
        .map_err(|_| fault(format!("The token file of account {} cannot be read.", account.id)))?;

      Ok(BTreeMap::from([("CLAUDE_CODE_OAUTH_TOKEN".to_owned(), Some(token.trim().to_owned()))]))
    }
    Credential::Directory { home } => {
      let token = match runs {
        _ if account.on_own_node() => None,
        Runs::OwnLogin => None,
        Runs::Here { data_root } => account.carrying_token(data_root, crate::now_ms()),
        Runs::OnNode { data_root } => account.node_token(data_root),
      };

      Ok(BTreeMap::from([
        ("CLAUDE_CONFIG_DIR".to_owned(), Some(home.to_string_lossy().into_owned())),
        ("CLAUDE_CODE_OAUTH_TOKEN".to_owned(), token),
      ]))
    }
    Credential::Endpoint {
      home,
      url,
      key_file,
      models,
    } => {
      if matches!(runs, Runs::OnNode { .. }) {
        return Err(fault(format!(
          "Account {} is an endpoint account, which does not run on a work node.",
          account.id
        )));
      }

      let key = std::fs::read_to_string(key_file)
        .ok()
        .map(|key| key.trim().to_owned())
        .filter(|key| !key.is_empty())
        .ok_or_else(|| fault(format!("The key file of account {} cannot be read.", account.id)))?;
      let mut env = BTreeMap::from([
        ("CLAUDE_CONFIG_DIR".to_owned(), Some(home.to_string_lossy().into_owned())),
        ("ANTHROPIC_BASE_URL".to_owned(), Some(url.clone())),
        ("ANTHROPIC_AUTH_TOKEN".to_owned(), Some(key)),
        ("ANTHROPIC_API_KEY".to_owned(), None),
        ("CLAUDE_CODE_OAUTH_TOKEN".to_owned(), None),
      ]);

      for (family, model) in models.families() {
        env.insert(format!("ANTHROPIC_DEFAULT_{}_MODEL", family.to_uppercase()), model.map(str::to_owned));
      }

      Ok(env)
    }
    Credential::ApiKey { home, key_file } => {
      if matches!(runs, Runs::OnNode { .. }) && account.user_id.is_none() {
        return Err(fault(format!(
          "Account {} is the platform's api-key account, whose key does not go out to a work node.",
          account.id
        )));
      }

      let key = std::fs::read_to_string(key_file)
        .ok()
        .map(|key| key.trim().to_owned())
        .filter(|key| !key.is_empty())
        .ok_or_else(|| fault(format!("The key of account {} cannot be read.", account.id)))?;

      Ok(BTreeMap::from([
        ("CLAUDE_CONFIG_DIR".to_owned(), Some(home.to_string_lossy().into_owned())),
        ("ANTHROPIC_API_KEY".to_owned(), Some(key)),
        ("ANTHROPIC_AUTH_TOKEN".to_owned(), None),
        ("ANTHROPIC_BASE_URL".to_owned(), None),
        ("CLAUDE_CODE_OAUTH_TOKEN".to_owned(), None),
      ]))
    }
  }
}

/// The program's environment: the launching process's with the line's own variables, the login's over
/// it as handed where the program runs, and thinking turned off where the run thinks not.
/// The launch line for an Endpoint account and an agent's configured model: Claude Code on the
/// account's endpoint, the model id mapped to the family word the account's model map names, and the
/// environment that carries the endpoint, its key and its family defaults. An id the map does not name
/// is refused rather than sent as a model the endpoint never heard of.
pub fn endpoint_launch(account: &Account, model: &str, effort: &str) -> Result<Command, Refusal> {
  let (url, key_file, models) = account.credential.endpoint_parts();
  let (Some(_url), Some(_key_file), Some(models)) = (url, key_file, models) else {
    return Err(fault(format!("Account {} is not an endpoint account.", account.id)));
  };
  let family = models
    .families()
    .into_iter()
    .find_map(|(family, id)| (id == Some(model)).then_some(family))
    .ok_or_else(|| {
      fault(format!(
        "Account {} maps no family to the model {model}; its map is {}.",
        account.id,
        models
          .families()
          .into_iter()
          .filter_map(|(family, id)| id.map(|id| format!("{family}={id}")))
          .collect::<Vec<_>>()
          .join(",")
      ))
    })?;
  let mut env = account_env(Some(account), &Runs::OwnLogin)?;
  // An Endpoint account's environment names no `PATH` of its own, and a program stood with an
  // empty `PATH` finds none of the binaries the deployment keeps under the home; the launching
  // process's own is carried over so the CLI is found where this machine keeps it.
  if let Some(path) = std::env::var_os("PATH").filter(|path| !path.is_empty()) {
    env.insert("PATH".to_owned(), Some(path.to_string_lossy().into_owned()));
  }

  // The model id goes over the environment rather than `--model`: the CLI checks a `--model`
  // name against the names it ships with, and an endpoint's own id is not one of them, so the
  // pane died before it could stand. The family word is what the map is read by, and the id is
  // what the endpoint answers to.
  let _ = family;
  env.insert("ANTHROPIC_MODEL".to_owned(), Some(model.to_owned()));

  Ok(Command {
    args: vec![
      ClaudeCode::PROGRAM.to_owned(),
      "--effort".to_owned(),
      effort.to_owned(),
      // A pane carries no person to answer a permission prompt, and a turn that stops on one
      // never opens: the run's own tools are the ones the launch line grants.
      "--dangerously-skip-permissions".to_owned(),
    ],
    cwd: account.credential.path().to_string_lossy().into_owned(),
    env,
  })
}

pub fn command_env(
  process: &BTreeMap<String, String>,
  account: Option<&Account>,
  runs: &Runs,
  thinking: bool,
) -> Result<BTreeMap<String, Option<String>>, Refusal> {
  let mut env: BTreeMap<String, Option<String>> =
    process.iter().map(|(name, value)| (name.clone(), Some(value.clone()))).collect();

  for (name, value) in &CLAUDE_CODE.launch.variables {
    env.insert(name.clone(), Some(value.clone()));
  }

  env.extend(account_env(account, runs)?);

  if !thinking {
    env.insert("MAX_THINKING_TOKENS".to_owned(), Some("0".to_owned()));
  }

  Ok(env)
}

/// The share of its window, in percent, at which Claude Code compacts a session inside a turn.
pub const COMPACT_SHARE: &str = "CLAUDE_AUTOCOMPACT_PCT_OVERRIDE";

/// What a pane running the line's own command is handed, by name: the search path, every `BMO_`
/// variable but the job runner's marker, the marker where the launcher holds it, the line's own
/// variables and the login's, and the run's name last. The login is the account's own where the run
/// stands on one, and the machine's where it stands on none.
pub fn line_environment(
  command: &Command,
  name: &str,
  account: Option<&Account>,
) -> BTreeMap<String, String> {
  let set = |variable: &str| command.env.get(variable).cloned().flatten().filter(|value| !value.is_empty());
  let mut env =
    BTreeMap::from([("PATH".to_owned(), command.env.get("PATH").cloned().flatten().unwrap_or_default())]);

  for (variable, value) in &command.env {
    if let Some(value) = value
      && variable.starts_with("BMO_")
      && variable != JOB_RUNNER_MARKER
    {
      env.insert(variable.clone(), value.clone());
    }
  }

  if let Some(marker) = set(JOB_RUNNER_MARKER) {
    env.insert(JOB_RUNNER_MARKER.to_owned(), marker);
  }

  env.extend(CLAUDE_CODE.launch.variables.clone());

  let login: Vec<&str> = match account {
    Some(account) => account.credential.variables().to_vec(),
    None => CLAUDE_CODE.launch.login_variables.iter().map(String::as_str).collect(),
  };

  for variable in login {
    if let Some(value) = set(variable) {
      env.insert(variable.to_owned(), value);
    }
  }

  env.insert(RUN_NAME.to_owned(), name.to_owned());
  env
}
