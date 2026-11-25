//! dsh's headless run of the worker's line on OpenRouter (docs/cache-fallback.md, 「The OpenRouter
//! substitute is the dsh road」): the run one task long, at the version the deployment pins dsh at, its
//! composition written as an overlay of the pinned profile's rows — the worker's rules after dsh's own
//! opening line, every tool row on but the web, the sandbox open, the gateway as dsh's one route with the
//! run's thinking, and the deployment's MCP servers — and its task as words read from a file of their own;
//! what it answered, and what it said about failing. The keepalive's ping on a worker's OpenRouter run goes
//! out on it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use policy::providers::Dsh;
use policy::providers::openrouter::OPENROUTER;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::fault;
use super::openrouter::{LOCAL_URL, PROVIDER};
use super::ordered::Ordered;
use super::stand_in::StandInRun;
use super::worker::LINE;
use crate::Refusal;
use crate::door::record::Reply;
use crate::host_run::Command;

// The version the deployment runs dsh at. A harness is its whole composition — the rows its profile mounts,
// the ids they are addressed by — and the overlay below addresses rows by id, so the version moves only
// with a reading of what moved with it.
const VERSION: &str = "0.2.1-rc.2";

// The profile that runs one task and prints the answer.
const PROFILE: &str = "headless";

// The persona prefix the profiles of the pinned version open their system prompt with, written again beside a
// run's persona suffix so the profile's own opening stands whatever the overlay's row replaces; `{{model}}` is
// dsh's own prompt variable, and the working directory is dsh's own runtime context.
const HEADLESS_OPENER: &str = "You are a coding agent powered by the {{model}} model.";

// The one row of the pinned profile's tools a worker goes without, every other one on: the web reaches it
// through the deployment's own server.
const WEB_ROW: &str = "tool-web";

// The rows a standing session goes without beside the web: a goal keeps the session working past the turn
// that was put in, and a chat's turn ends with its answer.
const LOOP_ROWS: [&str; 1] = ["tool-goal"];

// The row that spends a second model call titling a session nothing opens again; a worker keeps the
// instruction-file reader, since the checkout's documents are its work's.
const TITLE_ROW: &str = "session-title-llm";

// What file effects a worker's run may have, in dsh's own word: any, since editing a checkout is what it is
// for.
const PERMISSION_MODE: &str = "danger-full-access";

/// A header dsh evaluates from its own environment as it loads the overlay, written as a placeholder and
/// laid into the overlay's text as YAML's `!!js` tag.
pub const ENVIRONMENT_MARK: &str = "bmo-bearer-from-environment:";

/// A value dsh reads from its own environment as it loads the overlay, written as a placeholder naming the
/// variable and laid into the overlay's text as YAML's `!!js` tag, so a server's secret never stands in the
/// overlay's file.
pub const VALUE_MARK: &str = "bmo-value-from-environment:";

/// The most one argument may hold, well inside the kernel's limit whatever the text is made of, and the
/// UTF-16 units that always fit in it.
pub const WORD_BYTES: usize = 100_000;
pub const WORD_UNITS: usize = WORD_BYTES / 3;

// What npx runs once the pinned package is on its path: bash reads the task's words out of the task file,
// removes it, and becomes dsh with each word an argument of its own, so a long task never travels in the
// one string npx hands its shell.
const LAUNCH: &str = r#"exec bash -c 'mapfile -d "" -t words < "$1"; rm -f -- "$1"; shift; exec dsh "$@" "${words[@]}"' bmo-dsh-task"#;

// How much of an error stream's line is kept beside an exit.
const FAILURE_TAIL_CHARS: usize = 300;

fn text(value: &str) -> Ordered {
  Ordered::text(value)
}

fn map(entries: Vec<(&str, Ordered)>) -> Ordered {
  Ordered::map(entries)
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
  url: Option<Ordered>,
  #[serde(default)]
  headers: Option<IndexMap<String, Ordered>>,
  #[serde(default)]
  command: Option<Ordered>,
  #[serde(default)]
  args: Option<Ordered>,
  #[serde(default)]
  env: Option<Ordered>,
}

// The variable a bearer header reads its token from, in the `${NAME}` form the deployment's files write it.
fn bearer_variable(headers: &IndexMap<String, String>) -> Option<String> {
  let name = headers.get("Authorization")?.strip_prefix("Bearer ${")?.strip_suffix('}')?;
  let mut letters = name.chars();

  (letters.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
    && letters.all(|rest| rest.is_ascii_alphanumeric() || rest == '_'))
  .then(|| name.to_owned())
}

/// The variable a stdio server's own value is carried under in dsh's environment where the program's
/// environment holds another under the same name: `BMO_MCP_<SERVER>_<NAME>`.
pub fn server_variable(server: &str, name: &str) -> String {
  let server: String = server
    .chars()
    .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
    .collect();

  format!("BMO_MCP_{server}_{name}")
}

/// The values the run's stdio servers are given that the program's environment does not hold under their own
/// names, each under its server's own variable, for the program's environment to carry.
pub fn server_values(
  path: Option<&Path>,
  env: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, Refusal> {
  let Some(path) = path.filter(|path| !path.as_os_str().is_empty()) else {
    return Ok(BTreeMap::new());
  };
  let read = std::fs::read_to_string(path).map_err(|why| fault(format!("{}: {why}", path.display())))?;
  let file: McpFile =
    serde_json::from_str(&read).map_err(|why| fault(format!("{}: {why}", path.display())))?;
  let mut values = BTreeMap::new();

  for (name, server) in &file.servers {
    if let Some(Ordered::Map(given)) = &server.env {
      for (key, value) in given {
        if let Some(value) = value.as_text()
          && env.get(key).map(String::as_str) != Some(value)
        {
          values.insert(server_variable(name, key), value.to_owned());
        }
      }
    }
  }

  Ok(values)
}

// The MCP servers the run may reach, each a client of its own under the name its tools are called by: a
// server on the tool host as a Streamable HTTP row with its bearer header read from dsh's environment, any
// other as the command it is spawned on. A loud client that cannot open fails its own activation, which dsh
// writes to its error stream with the cause; a quiet one is let go without a word.
fn mcp_rows(
  path: Option<&Path>,
  cwd: &str,
  loud: bool,
  env: &BTreeMap<String, String>,
) -> Result<Vec<Ordered>, Refusal> {
  let Some(path) = path.filter(|path| !path.as_os_str().is_empty()) else {
    return Ok(vec![]);
  };

  let read = std::fs::read_to_string(path).map_err(|why| fault(format!("{}: {why}", path.display())))?;
  let file: McpFile =
    serde_json::from_str(&read).map_err(|why| fault(format!("{}: {why}", path.display())))?;
  let mut rows = vec![];

  for (name, server) in file.servers {
    let id = format!("mcp-{name}");

    if server.kind.as_deref() == Some("http")
      && let Some(Ordered::Text(url)) = &server.url
    {
      let headers: IndexMap<String, String> = server
        .headers
        .iter()
        .flatten()
        .filter_map(|(key, value)| value.as_text().map(|value| (key.clone(), value.to_owned())))
        .collect();
      let token = bearer_variable(&headers);
      let mut said: Vec<(&str, Ordered)> =
        headers.iter().map(|(key, value)| (key.as_str(), text(value))).collect();

      if let Some(token) = &token {
        said.push(("Authorization", Ordered::Text(format!("{ENVIRONMENT_MARK}{token}"))));
      }

      rows.push(map(vec![
        ("id", Ordered::Text(id)),
        ("name", text("@deepseek-ai/dsh-mcp-client")),
        (
          "config",
          map(vec![
            ("transport", text("streamable-http")),
            ("serverName", Ordered::Text(name.clone())),
            ("url", text(url)),
            ("headers", map(said)),
            ("failOnStartupError", Ordered::Flag(loud)),
          ]),
        ),
      ]));
    } else if let Some(Ordered::Text(command)) = &server.command {
      let args = match &server.args {
        Some(args @ Ordered::List(_)) => args.clone(),
        _ => Ordered::List(vec![]),
      };
      // Each variable the server is handed is read from dsh's own environment, where the program's
      // command carries it, rather than written into the overlay: under its own name where the program
      // holds the same value, under the server's own variable otherwise.
      let given = match &server.env {
        Some(Ordered::Map(given)) => Ordered::map(
          given
            .iter()
            .map(|(key, value)| {
              let own = value.as_text().is_none_or(|value| env.get(key).map(String::as_str) == Some(value));
              let read = if own { key.clone() } else { server_variable(&name, key) };

              (key.clone(), Ordered::Text(format!("{VALUE_MARK}{read}")))
            })
            .collect::<Vec<_>>(),
        ),
        _ => Ordered::map(Vec::<(String, Ordered)>::new()),
      };

      rows.push(map(vec![
        ("id", Ordered::Text(id)),
        ("name", text("@deepseek-ai/dsh-mcp-client")),
        (
          "config",
          map(vec![
            ("transport", text("stdio")),
            ("serverName", Ordered::Text(name.clone())),
            ("command", text(command)),
            ("args", args),
            ("env", given),
            ("cwd", text(cwd)),
            ("failOnStartupError", Ordered::Flag(loud)),
          ]),
        ),
      ]));
    }
  }

  Ok(rows)
}

// A text of agency's as dsh's prompt takes it word for word: dsh reads every `{{name}}` in a persona as a
// variable of its own and refuses the whole prompt on a name it has not registered, so each `{{` is broken
// by a word joiner, which the model reads past.
fn literal(text: &str) -> String {
  text.replace("{{", "{\u{2060}{")
}

// A file the overlay hands over whole.
fn read(path: &Path) -> Result<String, Refusal> {
  std::fs::read_to_string(path).map_err(|why| fault(format!("{}: {why}", path.display())))
}

/// What dsh's overlay for a run of any line is composed out of: the line, its provider, which dsh knows the
/// route by, its whole system file or the worker's rules, what it shows and allows of its tools — none for
/// the worker, which takes the whole working set — where it works, the model, the effort and whether its
/// agent chose one, whether it thinks, and its MCP servers.
pub struct Composed<'a> {
  pub line: &'a str,
  pub provider: &'a str,
  pub system: Option<&'a Path>,
  pub rules: Option<&'a Path>,
  pub shown: Option<&'a str>,
  pub allowed: Option<&'a str>,
  pub cwd: &'a str,
  pub model: &'a str,
  pub effort: &'a str,
  pub chosen: bool,
  pub thinking: bool,
  pub mcp: Option<&'a Path>,
  /// The share of the model's window a standing session compacts itself at, where its session folds itself
  /// (docs/sessions.md, 「Session compaction」); none leaves dsh's own threshold.
  pub compact_at: Option<f64>,
}

impl Composed<'_> {
  fn worker(&self) -> bool {
    self.line == LINE
  }

  // Whether the run allows exactly what it shows, MCP tools aside.
  fn allows_what_it_shows(&self) -> bool {
    let (Some(shown), Some(allowed)) = (self.shown, self.allowed) else {
      return self.shown == self.allowed;
    };
    let listed = |tools: &str, keep: &dyn Fn(&str) -> bool| {
      tools
        .split(',')
        .map(str::trim)
        .filter(|tool| !tool.is_empty() && keep(tool))
        .collect::<Vec<_>>()
        .join(",")
    };

    listed(allowed, &|tool| !tool.starts_with("mcp__")) == listed(shown, &|_| true)
  }

  /// What file effects the run may have, in dsh's own word: none composed for a run shown nothing, any for
  /// the worker, and writes within where it works for a run that allows what it shows, reads alone
  /// otherwise.
  pub fn permission_mode(&self) -> Option<&'static str> {
    if self.shown == Some("") {
      None
    } else if self.worker() {
      Some(PERMISSION_MODE)
    } else if self.allows_what_it_shows() {
      Some("workspace-write")
    } else {
      Some("read-only")
    }
  }

  // The tool rows a run's shown tools stand for: the filesystem, and the shell where it is shown Bash; the
  // worker takes every row but the web, which reaches it through the deployment's own server.
  fn working_rows(&self) -> Vec<&'static str> {
    if self.worker() {
      return TOOL_ROWS.into_iter().filter(|row| *row != WEB_ROW).collect();
    }

    let files = ["tool-fs", "tool-fs-search", "tool-str-replace-editor"];
    let shell = self.shown.unwrap_or_default().split(',').any(|tool| tool.trim() == "Bash");

    if shell { std::iter::once("tool-bash").chain(files).collect() } else { files.to_vec() }
  }
}

// How the overlay treats dsh's own tool rows: a headless run takes the rows the line's declared tools stand
// for, while a standing session keeps what dsh ships and only the web is replaced by the deployment's own
// servers.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tools {
  Line,
  Session,
}

// The overlay for one run: its prompt, the rows it goes without, the sandbox where it is shown tools, the
// gateway as dsh's one route with the model and its thinking, and — where dsh is not handed them through
// the session instead — its MCP servers.
fn overlay_of(run: &Composed, env: &BTreeMap<String, String>, route: &Dsh) -> Result<Ordered, Refusal> {
  overlay_of_with(run, env, true, Tools::Line, route)
}

fn overlay_of_with(
  run: &Composed,
  env: &BTreeMap<String, String>,
  mcp: bool,
  tools: Tools,
  route: &Dsh,
) -> Result<Ordered, Refusal> {
  let mut rows = vec![];
  // dsh's own prompt is the harness and stands whole: its identity line, the profile's opening line and its
  // guidance. A run's role and persona, or a worker's rules, follow the guidance as the persona suffix; a
  // worker's chosen base prompt alone stands in the opening line's place, since it is a harness of its own.
  let (prefix, suffix) = match (run.system, run.rules) {
    (Some(system), _) if run.worker() => (literal(&read(system)?), None),
    (Some(system), _) => (HEADLESS_OPENER.to_owned(), Some(literal(&read(system)?))),
    (None, Some(rules)) if run.worker() => (HEADLESS_OPENER.to_owned(), Some(literal(&read(rules)?))),
    _ => (String::new(), None),
  };

  if !prefix.is_empty() {
    let mut config = vec![("personaPrefix", Ordered::Text(prefix))];

    if let Some(suffix) = suffix {
      config.push(("personaSuffix", Ordered::Text(suffix)));
    }

    rows.push(map(vec![("id", text("system-prompt")), ("config", map(config))]));
  }

  // The worker keeps the instruction-file reader, since the checkout's documents are its work's.
  for id in ["agent-instructions", TITLE_ROW] {
    if !(run.worker() && id == "agent-instructions") {
      rows.push(map(vec![("id", text(id)), ("disabled", Ordered::Flag(true))]));
    }
  }

  match (tools, run.permission_mode()) {
    (_, None) => {
      for id in TOOL_ROWS {
        rows.push(map(vec![("id", text(id)), ("disabled", Ordered::Flag(true))]));
      }
    }
    // A standing session keeps dsh's own tools; the web is the one the deployment replaces, with the
    // servers its MCP configuration hands the session.
    (Tools::Session, Some(mode)) => {
      for id in [WEB_ROW].into_iter().chain(LOOP_ROWS) {
        rows.push(map(vec![("id", text(id)), ("disabled", Ordered::Flag(true))]));
      }

      rows.push(map(vec![
        ("id", text("sandbox-policy")),
        ("config", map(vec![("mode", text(mode)), ("workspaceRoot", text(run.cwd))])),
      ]));
    }
    (Tools::Line, Some(mode)) => {
      let working = run.working_rows();

      for id in TOOL_ROWS.into_iter().filter(|row| !working.contains(row)) {
        rows.push(map(vec![("id", text(id)), ("disabled", Ordered::Flag(true))]));
      }

      rows.push(map(vec![
        ("id", text("sandbox-policy")),
        ("config", map(vec![("mode", text(mode)), ("workspaceRoot", text(run.cwd))])),
      ]));
    }
  }

  // A run that must not think names off, the one level every model answers to; one that thinks at no level
  // its agent chose is left at the model's own; one that does is declared that level and asks for it.
  let level = if !run.thinking {
    Some("off")
  } else if !run.chosen || run.effort.is_empty() {
    None
  } else {
    Some(run.effort)
  };

  rows.extend(route_rows(run.provider, run.model, level, env, route));

  // A standing session that folds itself compacts at its session's fold ratio rather than dsh's own default.
  if tools == Tools::Session
    && let Some(share) = run.compact_at.and_then(serde_json::Number::from_f64)
  {
    rows.push(map(vec![
      ("id", text(COMPACTION_ROW)),
      ("config", map(vec![("thresholdRatio", Ordered::Number(share))])),
    ]));
  }

  // The clients are rows the profile does not hold, so they stand in one insert entry: a row naming an id
  // the profile lacks is a change to nothing, which dsh mounts nothing for. A standing session's clients
  // say why one did not open; a headless run's go quietly, its error stream being its failure's account.
  if mcp {
    let servers = mcp_rows(run.mcp, run.cwd, tools == Tools::Session, env)?;

    if !servers.is_empty() {
      rows.push(map(vec![("insert", Ordered::List(servers))]));
    }
  }

  Ok(Ordered::List(rows))
}

/// The profile's row of dsh's own automatic compaction, whose threshold a session that folds itself sets.
pub const COMPACTION_ROW: &str = "compaction-basic";

/// The share of its model's window a supervisor's standing session compacts itself at: its compaction
/// threshold; none where its row cannot be read.
pub async fn compact_share(ground: &crate::places::Ground, agent: &str, _provider: &str) -> Option<f64> {
  crate::supervisors::named(&ground.db, agent)
    .await
    .ok()?
    .map(|supervisor| supervisor.fold_ratio)
}

/// The tools a standing session keeps of dsh's own, as the pinned version names them to the model: every row
/// a line that grants tools leaves on, the MCP resource readers its clients bring among them.
pub const SESSION_TOOLS: [&str; 23] = [
  "bash",
  "read",
  "write",
  "edit",
  "glob",
  "grep",
  "read_image",
  "working_directory",
  "skill",
  "todo_write",
  "workflow",
  "subagent",
  "subagent_fork",
  "send_message",
  "interrupt_agent",
  "list_agents",
  "job_list",
  "job_output",
  "job_kill",
  "exit_plan_mode",
  "list_mcp_resources",
  "list_mcp_resource_templates",
  "read_mcp_resource",
];

// The package an MCP client row is mounted from, which a failed activation names beside its row.
const MCP_CLIENT: &str = "@deepseek-ai/dsh-mcp-client";

/// The servers whose clients dsh's error stream says did not activate, each with the cause it gave: a
/// failed row is written as its id and its package before the error, and the error's own cause follows
/// below its stack where the first line only says the connection failed.
pub fn inactive_servers(lines: &[String]) -> Vec<(String, String)> {
  let mut found: Vec<(String, String)> = vec![];
  let marker = format!(" ({MCP_CLIENT}): ");
  let mut open: Option<usize> = None;

  for line in lines {
    let trimmed = line.trim();

    if let Some((id, said)) = trimmed.split_once(&marker)
      && let Some(server) = id.strip_prefix("mcp-")
    {
      found.retain(|(named, _)| named != server);
      found.push((server.to_owned(), said.trim().to_owned()));
      open = Some(found.len() - 1);
      continue;
    }

    let Some(at) = open else { continue };

    if trimmed.is_empty() || trimmed.starts_with("at ") {
      continue;
    }

    // The first line under a failure's stack is the cause it was thrown for; anything after it is
    // another entry's or the harness's own.
    found[at].1 = format!("{}: {trimmed}", found[at].1);
    open = None;
  }

  found
}

// The overlay for one worker's run.
fn overlay(run: &StandInRun, env: &BTreeMap<String, String>) -> Result<Ordered, Refusal> {
  overlay_of(&worker_composed(run), env, openrouter_route()?)
}

// A worker's run as its overlay is composed out of.
fn worker_composed(run: &StandInRun) -> Composed<'_> {
  Composed {
    line: LINE,
    provider: PROVIDER,
    system: run.system_prompt_path.as_deref(),
    rules: Some(&run.extensions.rules_path),
    shown: None,
    allowed: None,
    cwd: &run.cwd,
    model: &run.model,
    effort: &run.effort,
    chosen: run.reasoning.as_ref().is_some_and(|reasoning| reasoning.effort.is_some()),
    thinking: run.thinking,
    mcp: Some(&run.mcp_config_path),
    compact_at: None,
  }
}

// The provider as dsh's one route, known by the provider's own name, with the model on it at the level the
// run asks where it asks one, and the model as the run's own.
fn route_rows(
  provider: &str,
  model_id: &str,
  level: Option<&str>,
  env: &BTreeMap<String, String>,
  route: &Dsh,
) -> [Ordered; 2] {
  let anthropic = model_id.starts_with("anthropic/");
  let declared = level.filter(|level| *level != "off");
  let base = env.get(LOCAL_URL).cloned().unwrap_or_else(|| route.api_on(env).to_owned());
  let mut route = vec![
    ("apiKeyEnv", text(&route.key_env)),
    ("baseURL", Ordered::Text(base)),
    ("api", text("openai-completions")),
  ];

  if let Some(level) = level {
    route.push(("reasoning", text(level)));
  }

  if anthropic {
    route.push(("cacheRetention", text("long")));
  }

  let mut model = vec![("id", text(model_id))];

  if let Some(level) = declared {
    model.push(("reasoningEfforts", map(vec![("off", Ordered::Null), (level, text(level))])));
    model.push(("compat", map(vec![("thinkingFormat", text("openrouter"))])));
  }

  if anthropic {
    let mut compat = vec![];

    if declared.is_some() {
      compat.push(("thinkingFormat", text("openrouter")));
    }

    compat.push(("supportsLongCacheRetention", Ordered::Flag(true)));
    model.push(("compat", map(compat)));
  }

  route.push(("models", Ordered::List(vec![map(model)])));

  [
    map(vec![
      ("id", text("llm-pi-ai")),
      ("config", map(vec![("providers", map(vec![(provider, map(route))]))])),
    ]),
    map(vec![
      ("id", text("agent-default-model")),
      ("config", map(vec![("provider", text(provider)), ("model", text(model_id))])),
    ]),
  ]
}

// Every row of the pinned profile that puts a tool in front of the model, the user question beside them and
// the plan mode that puts one more there: a gate is shown nothing, which dsh takes as every one of them off.
const TOOL_ROWS: [&str; 18] = [
  "tool-bash",
  "tool-pwsh",
  "tool-jobs",
  "tool-fs",
  "tool-fs-search",
  "tool-skill",
  "tool-subagent-control",
  "tool-subagent-list-agents",
  "tool-subagent",
  "tool-subagent-fork",
  "tool-workflow",
  "tool-todo",
  "tool-goal",
  "tool-str-replace-editor",
  "tool-working-directory",
  "tool-web",
  "plan-mode",
  "user-questions",
];

// The overlay for a gate's run: its instructions as dsh's persona, the instruction-file reader and the
// title call off, every tool row off and no sandbox to set, and the gateway as its one route.
fn openrouter_route() -> Result<&'static Dsh, Refusal> {
  OPENROUTER.declared.dsh.as_ref().ok_or_else(|| fault("openrouter names no dsh route"))
}

fn gate_overlay(
  model: &str,
  thinking: bool,
  system_path: &Path,
  env: &BTreeMap<String, String>,
) -> Result<Ordered, Refusal> {
  let mut rows = vec![map(vec![
    ("id", text("system-prompt")),
    ("config", map(vec![("persona", Ordered::Text(read(system_path)?))])),
  ])];

  for id in ["agent-instructions", TITLE_ROW].into_iter().chain(TOOL_ROWS) {
    rows.push(map(vec![("id", text(id)), ("disabled", Ordered::Flag(true))]));
  }

  rows.extend(route_rows(PROVIDER, model, (!thinking).then_some("off"), env, openrouter_route()?));

  Ok(Ordered::List(rows))
}

/// The command a gate's run on dsh is: the pinned package by npx, the task's words and the overlay each in
/// a file of the run's own, dsh's home beside the run's environment, and nothing on its input; a gate
/// composes no file effects, so no permission mode is named.
pub fn gate_command(
  model: &str,
  thinking: bool,
  system_path: &Path,
  cwd: &str,
  prompt: &str,
  env: &BTreeMap<String, String>,
  data: &Path,
) -> Result<Command, Refusal> {
  let overlay = overlay_text(&gate_overlay(model, thinking, system_path, env)?)?;
  launched(GATE_LINE, prompt, &overlay, cwd, env, data, None)
}

/// The overlay a standing session reads, written under dsh's home and named by its own text: the same run
/// composes the same path, so the pane's command — and its identity — stands while nothing it reads has
/// changed, and a changed prompt or route is a new path the pane is stood again on. Its MCP servers are
/// mounted here, each one that cannot open left out rather than failing the session — a namespace the tool
/// host places nothing under for the session answers not found — and dsh's own tools stand except the web,
/// which those servers replace.
pub fn session_patch(
  run: &Composed,
  env: &BTreeMap<String, String>,
  data: &Path,
  route: &Dsh,
) -> Result<PathBuf, Refusal> {
  let text = overlay_text(&overlay_of_with(run, env, true, Tools::Session, route)?)?;
  let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
  let dir = dsh_home(data).join("overlays");
  let path = dir.join(format!("{}-{digest}.yml", run.line));

  agent_host::disk::private_dir(&dir).map_err(|why| fault(format!("{}: {why}", dir.display())))?;
  agent_host::disk::publish_bytes(&path, text.as_bytes())
    .map_err(|why| fault(format!("{}: {why}", path.display())))?;

  Ok(path)
}

/// The overlay as dsh reads it: JSON, which YAML reads too, with the tool host's bearer placeholder replaced
/// by the tagged expression it stands for.
pub fn overlay_text(overlay: &Ordered) -> Result<String, Refusal> {
  static MARKED: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(&format!(
      r#""({}|{})([A-Za-z_][A-Za-z0-9_]*)""#,
      regex::escape(ENVIRONMENT_MARK),
      regex::escape(VALUE_MARK)
    ))
    .expect("the marks")
  });
  let text = serde_json::to_string_pretty(overlay).map_err(|why| fault(why.to_string()))?;
  let tagged = MARKED.replace_all(&text, |found: &regex::Captures| {
    let name = &found[2];
    let expression = if &found[1] == ENVIRONMENT_MARK {
      format!("`Bearer ${{process.env.{name}}}`")
    } else {
      format!("process.env.{name}")
    };

    format!("!!js {}", serde_json::to_string(&expression).unwrap_or_default())
  });

  Ok(format!("{tagged}\n"))
}

// The last index at or before `from` that holds a space, as a string's own backward search finds it.
fn last_space(units: &[u16], from: i64) -> i64 {
  let from = from.clamp(0, units.len() as i64 - 1);

  (0..=from).rev().find(|at| units[*at as usize] == u16::from(b' ')).unwrap_or(-1)
}

fn is_hyphen(units: &[u16], at: usize) -> bool {
  units.get(at) == Some(&u16::from(b'-'))
}

/// The task as the words headless is handed, each short enough to be one argument: cut at a space that is
/// dropped, so that joined again with one space it is the task exactly, never before a hyphen, which would
/// read as an option; a stretch with no such space cut where it runs out, never inside a surrogate pair;
/// and no NUL, which cannot stand in an argument.
pub fn task_words(prompt: &str) -> Vec<String> {
  let mut words = vec![];
  let mut rest: Vec<u16> = prompt.replace('\0', "").encode_utf16().collect();

  while String::from_utf16_lossy(&rest).len() > WORD_BYTES {
    let mut cut = last_space(&rest, WORD_UNITS as i64);

    while cut > 0 && is_hyphen(&rest, cut as usize + 1) {
      cut = last_space(&rest, cut - 1);
    }

    if cut > 0 {
      let cut = cut as usize;

      words.push(String::from_utf16_lossy(&rest[..cut]));
      rest.drain(..=cut);
      continue;
    }

    let high = rest.get(WORD_UNITS - 1).is_some_and(|unit| (0xD800..=0xDBFF).contains(unit));
    let mut end = if high { WORD_UNITS - 1 } else { WORD_UNITS };

    while end > 1 && is_hyphen(&rest, end) {
      end -= 1;
    }

    words.push(String::from_utf16_lossy(&rest[..end]));
    rest.drain(..end);
  }

  words.push(String::from_utf16_lossy(&rest));
  words
}

// A file of the run's own under dsh's home, named so no other run in flight holds it.
fn own_file(home: &Path, line: &str, dir: &str, extension: &str, body: &str) -> Result<PathBuf, Refusal> {
  let dir = home.join(dir);
  let path = dir.join(format!("{line}-{}.{extension}", uuid::Uuid::new_v4()));

  std::fs::create_dir_all(&dir).map_err(|why| fault(format!("{}: {why}", dir.display())))?;
  std::fs::write(&path, body).map_err(|why| fault(format!("{}: {why}", path.display())))?;

  Ok(path)
}

fn shell_word(word: &str) -> String {
  format!("'{}'", word.replace('\'', r"'\''"))
}

/// Where dsh keeps everything it writes — its sessions, its overlays and its task files: the data root's
/// `dsh`, which a deployment lays nothing over, rather than anybody's home or a tree a deployment rebuilds.
pub fn dsh_home(data: &Path) -> PathBuf {
  data.join("dsh")
}

/// The command a worker's run on dsh is: the pinned package by npx, the task's words and the overlay each
/// in a file of the run's own, dsh's home and its permission mode beside the run's environment, and nothing
/// on its input.
pub fn command(
  run: &StandInRun,
  prompt: &str,
  env: &BTreeMap<String, String>,
  data: &Path,
) -> Result<Command, Refusal> {
  launched(
    LINE,
    prompt,
    &overlay_text(&overlay(run, env)?)?,
    &run.cwd,
    env,
    data,
    Some(PERMISSION_MODE),
  )
}

/// The command a run of any line on dsh is, as `command` is a worker's: its permission mode named where it
/// composes one.
pub fn line_command(
  run: &Composed,
  prompt: &str,
  env: &BTreeMap<String, String>,
  data: &Path,
) -> Result<Command, Refusal> {
  let mut env = env.clone();

  env.extend(server_values(run.mcp, &env)?);

  let overlay = overlay_text(&overlay_of(run, &env, openrouter_route()?)?)?;

  launched(run.line, prompt, &overlay, run.cwd, &env, data, run.permission_mode())
}

// The line a gate's run goes out on, which its files under dsh's home are named by.
const GATE_LINE: &str = "gate";

// The pinned package by npx on a run's task words and overlay, each written to a file of the line's own,
// with its permission mode where it composes one and none otherwise.
fn launched(
  line: &str,
  prompt: &str,
  overlay: &str,
  cwd: &str,
  env: &BTreeMap<String, String>,
  data: &Path,
  mode: Option<&str>,
) -> Result<Command, Refusal> {
  let home = dsh_home(data);
  let words: String = task_words(prompt).iter().map(|word| format!("{word}\0")).collect();
  let task = own_file(&home, line, "tasks", "words", &words)?;
  let patch = own_file(&home, line, "overlays", "yml", overlay)?;
  let call = std::iter::once(LAUNCH.to_owned())
    .chain(
      [
        task.to_string_lossy().as_ref(),
        "--profile",
        PROFILE,
        "--patch",
        patch.to_string_lossy().as_ref(),
      ]
      .map(shell_word),
    )
    .collect::<Vec<_>>()
    .join(" ");
  let mut env = env.clone();

  env.insert("DSH_HOME".to_owned(), home.to_string_lossy().into_owned());
  match mode {
    Some(mode) => env.insert("DSH_PERMISSION_MODE".to_owned(), mode.to_owned()),
    None => env.remove("DSH_PERMISSION_MODE"),
  };

  Ok(Command {
    args: vec![
      "npx".to_owned(),
      "--yes".to_owned(),
      "--package".to_owned(),
      format!("@deepseek-ai/dsh@{VERSION}"),
      "--call".to_owned(),
      call,
    ],
    cwd: cwd.to_owned(),
    env,
    stdin: None,
  })
}

/// What headless answered: everything it printed, where it exited cleanly; it reports no session and no
/// spend of its own.
pub fn answer(stdout: &str, exit_code: i32, run: &StandInRun) -> Reply {
  reply(stdout, exit_code, &run.model)
}

/// What headless answered on a model, as `answer` reads a worker's.
pub fn reply(stdout: &str, exit_code: i32, model: &str) -> Reply {
  Reply {
    mode: PROVIDER.to_owned(),
    model: Some(model.to_owned()),
    effort: None,
    text: if exit_code == 0 { stdout.trim().to_owned() } else { String::new() },
    session_id: None,
    context_ratio: None,
    context_tokens: None,
    session_usage: None,
    windows: vec![],
    spend: vec![],
    live_tasks: vec![],
    workflows: vec![],
    account: None,
    last_request_usage: None,
    request_usages: None,
    answers: Vec::new(),
  }
}

// The first characters of a line, by the UTF-16 units a string counts.
fn first_units(line: &str, units: usize) -> String {
  let all: Vec<u16> = line.encode_utf16().collect();

  String::from_utf16_lossy(&all[..units.min(all.len())])
}

/// What dsh said about a run that did not answer: its own word about the failure wherever in a line it
/// stands, or the last line that is not the model thinking aloud with the exit, or the exit alone.
pub fn failure(stderr: &str, exit_code: i32) -> String {
  let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
  let aloud = |line: &&&str| !line.starts_with("dsh: reasoning");

  if let Some(said) = lines.iter().filter(aloud).rfind(|line| line.contains("dsh: ")) {
    return first_units(said, FAILURE_TAIL_CHARS);
  }

  match lines.iter().rfind(aloud) {
    Some(tail) => format!("exit {exit_code}: {}", first_units(tail, FAILURE_TAIL_CHARS)),
    None => format!("exit {exit_code}"),
  }
}
