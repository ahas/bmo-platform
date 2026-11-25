//! The runner lines a thrown-away session goes out on with the instructions its caller hands in whole
//! (docs/launch-lines.md, 「Search runner」, 「Browse runner」, 「Plain runner」; docs/api.md, 「sessions」):
//! those instructions in place of the CLI's own prompt, behind the platform's harness opening where the line
//! opens on it, written where the line hands them over; the line's standing MCP configuration, written where
//! nothing has written it; the tools it allows; the model, effort and tier the turn's mapping names and the
//! line's agent's own thinking; and the directory it is deployed to. Each runner answers for what is its own:
//! the tools it is allowed and its configuration. The pieces of its system prompt and whether its program keeps
//! its own commands are the line's carries (docs/launch-lines.md, 「What a line carries」).

use std::path::PathBuf;

use harness_providers::claude_code::ClaudeCode;
use policy::browser::BROWSER;
use policy::run_lines::RUN_LINES;

use super::ordered::Ordered;
use super::token::TOKEN_VARIABLE;
use super::worker::{reasoning_of, system_prompt_file};
use super::{Trees, fault, publish_text};
use crate::Refusal;
use crate::places::Ground;

/// The provider a runner's line is warmed on.
pub const PROVIDER: &str = "claude-code";

// The web reaches every runner through the deployment's own server, and no built-in tool is shown to any.
// A hosted tool the namespaces do not show is reached with `bmo use`.
const WEB_TOOLS: &str = "mcp__bmo,mcp__web,mcp__eye,mcp__research";

/// One runner line: the line, the work key and the agent it is all at once.
pub trait Runner: Sync {
  fn name(&self) -> &'static str;

  /// What it is allowed of the servers it is handed.
  fn tools_allowed(&self) -> &'static str;

  /// Its standing MCP configuration, written where nothing has written it.
  fn mcp_config(&self, ground: &Ground, trees: &Trees) -> Result<PathBuf, Refusal>;
}

/// The search runner: the web, and nothing else.
pub struct Search;

/// The browse runner: the browser's tools and the web.
pub struct Browse;

/// The plain runner: the instructions handed in as the whole of what its run is told, and nothing to reach.
pub struct Plain;

/// A runner line's run as composed, less its prompt, which is put into its program (docs/agents.md, 「Runs
/// apart」).
pub struct RunnerRun {
  pub line: &'static str,
  pub model: String,
  pub effort: String,
  pub tier: String,
  pub system_prompt: PathBuf,
  pub mcp_config: PathBuf,
  pub tools_allowed: &'static str,
  pub host_commands: bool,
  pub thinking: bool,
  pub cwd: PathBuf,
  /// The model the program may consult, its seat's road's where that names one (docs/provider-order.md, 「The
  /// advisor」).
  pub advisor: Option<String>,
}

// The tool host's namespace as a configuration names it: its address, and the header handing the run's
// token, read from the program's own environment.
fn server_on(port: u16, namespace: &str) -> Ordered {
  Ordered::map([
    ("type", Ordered::text("http")),
    ("url", Ordered::text(format!("http://127.0.0.1:{port}/mcp/{namespace}"))),
    (
      "headers",
      Ordered::map([("Authorization", Ordered::text(format!("Bearer ${{{TOKEN_VARIABLE}}}")))]),
    ),
  ])
}

/// A line's servers on the tool host: the namespaces it uses. A hosted tool those namespaces do not show is
/// reached with `bmo use`, never handed as a server of its own.
pub(crate) fn line_servers(ground: &Ground, namespaces: &[&str]) -> Ordered {
  servers_on(ground.roots.mcp_port, namespaces)
}

/// The same servers on the tool host listening on the port given, each behind the run's token.
pub fn servers_on(port: u16, namespaces: &[&str]) -> Ordered {
  Ordered::map(namespaces.iter().copied().map(|namespace| (namespace, server_on(port, namespace))))
}

// A configuration written where the line reads it when nothing has, and left as it stands otherwise: two
// trees writing one file would move the line's prefix back and forth between them.
pub(crate) fn handed_config(
  path: PathBuf,
  compose: impl FnOnce() -> Result<Ordered, Refusal>,
) -> Result<PathBuf, Refusal> {
  let composed = compose()?;

  if !path.exists() {
    publish_text(&path, &Ordered::map([("mcpServers", composed)]).pretty())?;

    return Ok(path);
  }

  // A server the line has taken up since the file was written is added to it; the toolbook a line is no
  // longer handed is taken out; the servers standing there, and anything else the file holds, are left as
  // they are.
  let mut held: Ordered = serde_json::from_str(
    &std::fs::read_to_string(&path).map_err(|why| fault(format!("{}: {why}", path.display())))?,
  )
  .map_err(|why| fault(format!("{}: {why}", path.display())))?;
  let Some(servers) = held
    .entries_mut()
    .and_then(|top| top.get_mut("mcpServers"))
    .and_then(Ordered::entries_mut)
  else {
    return Ok(path);
  };
  let mut changed = false;

  if !composed
    .entries()
    .is_some_and(|entries| entries.contains_key(harness_providers::toolbook::SERVER))
    && servers.shift_remove(harness_providers::toolbook::SERVER).is_some()
  {
    changed = true;
  }

  for (name, server) in composed.entries().into_iter().flatten() {
    if !servers.contains_key(name) {
      servers.insert(name.clone(), server.clone());
      changed = true;
    }
  }

  if changed {
    publish_text(&path, &held.pretty())?;
  }

  Ok(path)
}

impl Runner for Search {
  fn name(&self) -> &'static str {
    "search"
  }

  fn tools_allowed(&self) -> &'static str {
    WEB_TOOLS
  }

  // Every namespace the search line names, on the tool host.
  fn mcp_config(&self, ground: &Ground, trees: &Trees) -> Result<PathBuf, Refusal> {
    handed_config(trees.agents.join("search.mcp.json"), || {
      Ok(line_servers(ground, &named_namespaces("search")))
    })
  }
}

impl Runner for Browse {
  fn name(&self) -> &'static str {
    "browse"
  }

  fn tools_allowed(&self) -> &'static str {
    "mcp__bmo,mcp__chrome-devtools,mcp__web,mcp__eye,mcp__research"
  }

  // The browse agent's servers as a run on the headless strategy is handed them.
  fn mcp_config(&self, ground: &Ground, trees: &Trees) -> Result<PathBuf, Refusal> {
    handed_config(trees.agents.join("browse").join(".mcp.json"), || browse_servers(ground, trees, None))
  }
}

impl Runner for Plain {
  fn name(&self) -> &'static str {
    "plain"
  }

  fn tools_allowed(&self) -> &'static str {
    ""
  }

  // A configuration naming no server, so the strict flag beside it leaves the program none.
  fn mcp_config(&self, _ground: &Ground, trees: &Trees) -> Result<PathBuf, Refusal> {
    handed_config(trees.agents.join("plain.mcp.json"), || Ok(Ordered::map::<&str>([])))
  }
}

/// A private display a browse run's Chrome is shown on: its name, its authority file and the runtime
/// directory a program on it is handed.
pub struct Shown<'a> {
  pub display: &'a str,
  pub xauthority: &'a std::path::Path,
  pub runtime_dir: &'a std::path::Path,
}

/// The browse agent's servers: its own chrome-devtools server, started apart from a person's session — no
/// keyring, the X11 platform, every variable of the desktop session emptied and its bus switched off —
/// headless unless a private display is given, which it is then shown instead; and the web, on the tool
/// host.
pub fn browse_servers(
  ground: &Ground,
  trees: &Trees,
  shown: Option<&Shown<'_>>,
) -> Result<Ordered, Refusal> {
  let own_path = trees.agent_dir("browse").join(".mcp.json");
  let own: Ordered = serde_json::from_str(
    &std::fs::read_to_string(&own_path).map_err(|why| fault(format!("{}: {why}", own_path.display())))?,
  )
  .map_err(|why| fault(format!("{}: {why}", own_path.display())))?;
  let mut servers = own
    .entries()
    .and_then(|file| file.get("mcpServers"))
    .and_then(Ordered::entries)
    .cloned()
    .unwrap_or_default();

  if let Some(Ordered::Map(chrome)) = servers.get("chrome-devtools").cloned() {
    let headless = Ordered::text("--headless");
    let mut args: Vec<Ordered> = match chrome.get("args") {
      Some(Ordered::List(args)) => {
        args.iter().filter(|arg| shown.is_none() || **arg != headless).cloned().collect()
      }
      _ => vec![],
    };
    let mut env = match chrome.get("env") {
      Some(Ordered::Map(env)) => env.clone(),
      _ => Default::default(),
    };
    let mut started = chrome;

    for arg in &BROWSER.detached_arguments {
      let arg = Ordered::text(format!("--chromeArg={arg}"));

      if !args.contains(&arg) {
        args.push(arg);
      }
    }

    for name in &BROWSER.session_variables {
      env.insert(name.clone(), Ordered::text(""));
    }

    env.insert("DBUS_SESSION_BUS_ADDRESS".to_owned(), Ordered::text("disabled:"));

    if let Some(shown) = shown {
      env.insert("DISPLAY".to_owned(), Ordered::text(shown.display));
      env.insert("XAUTHORITY".to_owned(), Ordered::text(shown.xauthority.to_string_lossy()));
      env.insert("XDG_RUNTIME_DIR".to_owned(), Ordered::text(shown.runtime_dir.to_string_lossy()));
    }

    started.insert("args".to_owned(), Ordered::List(args));
    started.insert("env".to_owned(), Ordered::Map(env));
    servers.insert("chrome-devtools".to_owned(), Ordered::Map(started));
  }

  for (name, server) in line_servers(ground, &named_namespaces("browse")).entries().into_iter().flatten() {
    servers.insert(name.clone(), server.clone());
  }

  Ok(Ordered::Map(servers))
}

// The tool host namespaces a line names, each a server of its configuration.
fn named_namespaces(line: &str) -> Vec<&'static str> {
  policy::run_lines::RUN_LINES.namespaces(line).iter().map(String::as_str).collect()
}

/// The runner lines a thrown-away session may name to go out on with instructions its caller hands in whole
/// (docs/api.md, 「sessions」).
pub const LINES: [&str; 3] = ["search", "browse", "plain"];

/// Whether a name is a runner line's.
pub fn is_runner_line(name: &str) -> bool {
  LINES.contains(&name)
}

/// A runner line's run with the instructions a thrown-away session's caller handed in whole (docs/api.md,
/// 「sessions」): those instructions as one system prompt, behind the platform's harness opening where the line
/// opens on it, written under a name its text gives so two runs handed the same instructions read one prefix;
/// the line's standing MCP configuration, the tools it allows, whether its program keeps its own commands and
/// its agent's own thinking; on the model, effort and tier given.
pub async fn given_run(
  ground: &Ground,
  trees: &Trees,
  line: &str,
  instructions: &str,
  mapped: (&str, &str, &str),
) -> Result<RunnerRun, Refusal> {
  use sha2::Digest as _;

  let (model, effort, tier) = mapped;
  let runner: &dyn Runner = match line {
    "browse" => &Browse,
    "search" => &Search,
    "plain" => &Plain,
    other => {
      return Err(fault(format!("{other} is no runner line; a session names one of {}", LINES.join(", "))));
    }
  };
  let name = runner.name();
  let carries = RUN_LINES.carries(name);
  let mcp_config = runner.mcp_config(ground, trees)?;
  let text = given_prompt(trees, &carries.prompt, instructions)?;
  let digest = sha2::Sha256::digest(text.as_bytes());
  let advisor = match crate::role_agents::seated_at(&ground.db, None, None).await? {
    Some(seat) => crate::role_agents::advisor_of(&ground.db, &seat).await?,
    None => None,
  };
  let named: String = digest.iter().take(8).map(|byte| format!("{byte:02x}")).collect();

  Ok(RunnerRun {
    line: name,
    model: model.to_owned(),
    effort: effort.to_owned(),
    tier: tier.to_owned(),
    system_prompt: system_prompt_file(trees, &format!("{name}-given-{named}"), &text)?,
    mcp_config,
    tools_allowed: runner.tools_allowed(),
    host_commands: carries.slash_commands,
    thinking: reasoning_of(ground, name).await?.thinking.unwrap_or(true),
    cwd: trees.agents.join(name),
    advisor,
  })
}

/// A runner line's system prompt, its carries' pieces in order (docs/launch-lines.md, 「What a line carries」): the
/// platform's harness opening where it names `harness`, and the instructions handed in where it names
/// `instructions`. A piece a runner line cannot fill is refused rather than left out.
pub fn given_prompt(trees: &Trees, pieces: &[String], instructions: &str) -> Result<String, Refusal> {
  let parts = pieces
    .iter()
    .map(|piece| match piece.as_str() {
      "harness" => super::read_required(&trees.harness_path()),
      "instructions" => Ok(instructions.to_owned()),
      other => Err(fault(format!("a runner line's system prompt holds no piece named {other}"))),
    })
    .collect::<Result<Vec<_>, _>>()?;

  Ok(parts.join("\n\n"))
}

impl RunnerRun {
  /// The run as Claude Code's words for it, opening the session named.
  pub fn args(&self, session: &str) -> Result<Vec<String>, Refusal> {
    super::claude::apart_args(
      &super::claude::ApartRun {
        line: self.line,
        model: &self.model,
        effort: &self.effort,
        system_prompt: Some(&self.system_prompt),
        mcp_config: Some(&self.mcp_config),
        tools_shown: "",
        tools_allowed: self.tools_allowed,
        host_commands: self.host_commands,
        advisor: self.advisor.as_deref(),
      },
      session,
    )
  }

  /// The program it runs.
  pub fn program() -> &'static str {
    ClaudeCode::PROGRAM
  }
}
