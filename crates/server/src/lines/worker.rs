//! The job worker's line (docs/launch-lines.md; docs/agent-control.md, 「A worker standing with nothing
//! to do」): its model and effort as the job key's tier maps them, the worker's own thinking and level,
//! its whole system prompt — the base prompt the account it runs on names or it chooses, else the
//! platform's harness opening, with the standing rules after it — the MCP configuration, the settings, the
//! plugin and the co-worker definitions it is handed.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use harness_providers::Machine;
use policy::base_prompts::BASE_PROMPTS;
use regex::Regex;
use sha2::{Digest, Sha256};

use super::agent_plugin::Whose;
use super::{Trees, fault, publish_text, read_required};
use crate::Refusal;
use crate::control::claude_accounts::Account;
use crate::control::harness::{levels, named};
use crate::control::registry;
use crate::places::Ground;
use crate::text::listed;
use store::one;

/// The line's name.
pub const LINE: &str = "worker";

// The rulebook every worker's rules carry after the standing rules.
pub const SELF_PRESERVATION: &str = "self-preservation";

/// The platform's rulebooks every run of the worker line carries after the rule against destroying the system,
/// since the line's standing rules are the platform's own work: the platform's environment, and the platform's own
/// parts that rule protects (docs/prompt-layers.md, 「The platform's rules」).
pub const PLATFORM_RULEBOOKS: [&str; 2] = ["environment", "preservation"];

// A value a base prompt names, as `{{name}}`: the base prompts' own syntax.
static VALUE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"\{\{([A-Za-z0-9_]+)\}\}").expect("the value mark"));

/// What an agent's row names of its own thinking and effort, each none where the tier and the line
/// decide.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reasoning {
  pub thinking: Option<bool>,
  pub level: Option<String>,
}

/// One worker's run on its line, composed before any road is chosen.
#[derive(Clone, Debug)]
pub struct WorkerRun {
  pub model: String,
  pub effort: String,
  pub tier: Option<String>,
  pub thinking: bool,
  /// The worker's own choice, which a mapping laid on the run reads; none for a run that is no
  /// worker's.
  pub reasoning: Option<Reasoning>,
  /// The whole system prompt a base prompt opens, the account's or the worker's own choice, with the rules
  /// after it; none where no base is chosen.
  pub system_prompt: Option<PathBuf>,
  /// The whole system prompt Claude Code is handed where no base is chosen: the platform's harness opening
  /// with the rules after it.
  pub harnessed: PathBuf,
  /// The rules alone, which a program that is a harness of its own carries beside its own prompt.
  pub rules: PathBuf,
  pub mcp_config: PathBuf,
  pub settings: PathBuf,
  pub plugin_dir: PathBuf,
  pub tools_denied: String,
  pub cwd: String,
  /// The model the program may consult, its seat's road's where that names one (docs/provider-order.md, 「The
  /// advisor」).
  pub advisor: Option<String>,
}

/// The repository a checkout belongs to, by name: its own name for a base, its repository's for a support
/// worktree or a node checkout. The base prompt it chooses and its sampling are kept under this name.
pub async fn repository_name_of(ground: &Ground, worker: &str) -> Result<String, Refusal> {
  Ok(
    crate::queue::workers::repository_of(&ground.db, worker)
      .await?
      .map_or_else(|| worker.to_owned(), |repository| repository.name),
  )
}

/// The prompt folder under `prompts/agents/` a checkout's repository names for its rules, its row's
/// `prompts`, or none where it names none (docs/repositories.md, 「What a repository row holds」).
pub async fn rules_of(ground: &Ground, worker: &str) -> Result<Option<String>, Refusal> {
  Ok(
    crate::queue::workers::repository_of(&ground.db, worker)
      .await?
      .and_then(|repository| repository.rules)
      .filter(|folder| !folder.trim().is_empty()),
  )
}

/// The reasoning a run's row chooses beside its order: none, since the effort and the thinking a run goes out
/// with are its order's entry's (docs/provider-order.md, 「Effort and thinking」).
pub async fn reasoning_of(_ground: &Ground, _agent: &str) -> Result<Reasoning, Refusal> {
  Ok(Reasoning::default())
}

/// A worker's rules — every worker's rules, the worker's own, its trailer and its prefix documents —
/// written as one file: one shared by every worker with nothing of its own and one for each worker with
/// something.
pub struct StandingRules {
  pub text: String,
  pub file: PathBuf,
}

/// The report trailer every job is held to, the worker plugin's skills and the plain-writing rulebook filled in
/// as the deployment shows them (docs/launch-lines.md): `shown` is the code root by the deployment's link, the
/// documents being read out of `trees`.
pub fn job_trailer(trees: &Trees, shown: &Trees) -> Result<String, Refusal> {
  let documents = trees.agent_dir(LINE);
  let skills = shown.agent_dir(LINE).join("plugin").join("skills").to_string_lossy().into_owned();
  let plain = shown.rulebook_path("plain-writing").to_string_lossy().into_owned();

  Ok(super::render(
    &read_required(&documents.join("trailer.md"))?,
    &[("skills", &skills), ("plainWriting", &plain)],
  ))
}

/// Every worker's rules, the result-language piece after them where the run's model names its languages,
/// the repository's own after those — `own-rules.md` of the prompt folder its row names — the job's report
/// trailer after those where the rules are a worker's, and the documents its row names for its prefix last,
/// as one file: the session's cached prefix, paid for once.
pub fn standing_rules(
  trees: &Trees,
  folder: Option<&str>,
  worker: Option<&str>,
  trailer: Option<&str>,
  documents: Option<&str>,
  languages: Option<&str>,
) -> Result<StandingRules, Refusal> {
  let own_path = folder.map(|folder| trees.agent_dir(folder).join("own-rules.md"));
  let own = match &own_path {
    Some(path) if path.exists() => Some(read_required(path)?),
    _ => None,
  };
  let mut parts = vec![read_required(&trees.agent_dir(LINE).join("standing-rules.md"))?];

  parts.extend(languages.map(str::to_owned));
  parts.push(rulebook(trees, SELF_PRESERVATION)?);
  for name in PLATFORM_RULEBOOKS {
    parts.push(platform_rulebook(trees, name)?);
  }
  parts.extend(own.clone());
  parts.extend(trailer.map(str::to_owned));
  parts.extend(documents.map(str::to_owned));

  let text = parts.join("\n\n");
  let name = match (documents, worker, own, folder) {
    (Some(_), Some(worker), _, _) => format!("worker-rules-{worker}"),
    (_, _, Some(_), Some(folder)) => format!("worker-rules-{folder}"),
    _ => "worker-rules".to_owned(),
  };
  // Rules carrying a result-language piece are a file of their own for each piece, so a run on a model writing
  // every language never shares one with them.
  let name = match languages {
    Some(piece) => format!("{name}-languages-{}", &format!("{:x}", Sha256::digest(piece.as_bytes()))[..12]),
    None => name,
  };
  let file = system_prompt_file(trees, &name, &text)?;

  Ok(StandingRules { text, file })
}

/// The result-language piece a run carries after the standing rules where its model writes a result for a
/// person in some languages alone, filled with them; none where it names none (docs/own-llm.md, 「A run on the
/// worker line is composed as on Claude」).
pub fn result_languages_piece(trees: &Trees, languages: &[String]) -> Result<Option<String>, Refusal> {
  if languages.is_empty() {
    return Ok(None);
  }

  let piece = read_required(&trees.agent_dir(LINE).join("result-languages.md"))?;

  Ok(Some(super::render(&piece, &[("languages", &listed(languages))]).trim().to_owned()))
}

/// The result-language piece of a run on `model` at `provider`: the registration's languages where the run
/// goes out on the registry's provider and its registration names some, none on every other road.
pub async fn result_languages(
  ground: &Ground,
  trees: &Trees,
  provider: &str,
  model: &str,
) -> Result<Option<String>, Refusal> {
  if provider != registry::PROVIDER {
    return Ok(None);
  }

  let languages = registry::registration(&ground.db, model)
    .await
    .map_err(|refused| fault(refused.error))?
    .map(|held| held.languages)
    .unwrap_or_default();

  result_languages_piece(trees, &languages)
}

/// The documents a worker's row names for its session's prefix (docs/worker-bench.md, 「Prefix
/// documents」), read fresh in the list's order, each headed by its path as written: a relative path from
/// the worker's checkout, an absolute one where it stands. A support worker with none of its own takes its
/// base's. A document that cannot be read is left out and said in the log; none where the list is empty.
pub async fn prefix_documents(ground: &Ground, worker: &str, cwd: &str) -> Result<Option<String>, Refusal> {
  let listed: Vec<String> = crate::queue::workers::repository_of(&ground.db, worker)
    .await?
    .map(|repository| repository.prefix_documents)
    .unwrap_or_default();
  let mut read = Vec::new();

  for path in &listed {
    let at = Path::new(cwd).join(path);

    match std::fs::read_to_string(&at) {
      Ok(text) => read.push(format!("DOCUMENT {path}:\n{}", text.trim_end())),
      Err(why) => eprintln!("worker {worker}: its prefix document {} was left out: {why}", at.display()),
    }
  }

  Ok((!read.is_empty()).then(|| read.join("\n\n")))
}

/// A rulebook as an agent is given it, named so the agent can say which rule it applied.
pub fn rulebook(trees: &Trees, name: &str) -> Result<String, Refusal> {
  Ok(format!(
    "RULEBOOK prompts/rules/{name}.md:\n{}",
    read_required(&trees.rulebook_path(name))?
  ))
}

/// One of the platform's rulebooks as an agent is given it: named, as every rulebook is (docs/prompt-layers.md).
pub fn platform_rulebook(trees: &Trees, name: &str) -> Result<String, Refusal> {
  Ok(format!(
    "RULEBOOK prompts/{}/{name}.md:\n{}",
    policy::prompt_layers::PROMPT_LAYERS.platform_folder,
    read_required(&trees.platform_rulebook_path(name))?
  ))
}

/// A composed prompt written where a line hands it over, under the agents root.
pub fn system_prompt_file(trees: &Trees, name: &str, text: &str) -> Result<PathBuf, Refusal> {
  let path = trees.agents.join("prompts").join(format!("{name}.md"));

  publish_text(&path, text)?;

  Ok(path)
}

/// How this side reads each value a base prompt may name, for the run it is composed for.
fn base_prompt_value(ground: &Ground, name: &str, cwd: &str, model: &str) -> Option<String> {
  Some(match name {
    "cwd" => cwd.to_owned(),
    "date" => chrono::Local::now().format("%Y-%m-%d").to_string(),
    "platform" => std::env::consts::OS.to_owned(),
    "os_version" => os_version(),
    "shell" => Path::new(ground.vars().get("SHELL").map_or("sh", String::as_str))
      .file_name()
      .map_or_else(|| "sh".to_owned(), |name| name.to_string_lossy().into_owned()),
    "model" => model.to_owned(),
    _ => return None,
  })
}

// The operating system's name and release, as the kernel answers them.
fn os_version() -> String {
  // SAFETY: `uname` fills the zeroed structure it is handed and nothing else.
  let mut name: libc::utsname = unsafe { std::mem::zeroed() };

  // SAFETY: as above; the fields it fills are NUL-terminated.
  if unsafe { libc::uname(&mut name) } != 0 {
    return String::new();
  }

  let field = |chars: &[libc::c_char]| {
    // SAFETY: `uname` NUL-terminates each field within its array.
    unsafe { std::ffi::CStr::from_ptr(chars.as_ptr()) }.to_string_lossy().into_owned()
  };

  format!("{} {}", field(&name.sysname), field(&name.release))
}

/// A base prompt by name, filled in for the run, or none where the store keeps no prompt of that name
/// (docs/base-prompts.md).
pub async fn base_prompt_text(
  ground: &Ground,
  prompt: &str,
  cwd: &str,
  model: &str,
) -> Result<Option<String>, Refusal> {
  let sql = r#"SELECT "text" FROM "base_prompts" WHERE "name" = $1"#;
  let Some(row) = one(&ground.db, sql, vec![prompt.into()]).await? else {
    return Ok(None);
  };
  let text: String = row.try_get("", "text")?;
  let filled = VALUE.replace_all(&text, |found: &regex::Captures| {
    let name = &found[1];

    BASE_PROMPTS
      .values
      .iter()
      .any(|one| one == name)
      .then(|| base_prompt_value(ground, name, cwd, model))
      .flatten()
      .unwrap_or_else(|| found[0].to_owned())
  });

  Ok(Some(filled.trim().to_owned()))
}

/// The prompt the agent chooses, filled in for the run, or none where it chooses none
/// (docs/base-prompts.md).
pub async fn chosen_base_prompt(
  ground: &Ground,
  agent: &str,
  cwd: &str,
  model: &str,
) -> Result<Option<String>, Refusal> {
  let sql = r#"SELECT "prompt" FROM "agent_base_prompts" WHERE "agent" = $1"#;
  let Some(row) = one(&ground.db, sql, vec![agent.into()]).await? else {
    return Ok(None);
  };
  let prompt: String = row.try_get("", "prompt")?;

  base_prompt_text(ground, &prompt, cwd, model).await
}

/// The worker's whole system prompt where one stands where the program's own would: the base prompt the
/// account names, which wins over the agent's own choice, or the prompt the agent chooses, filled in for
/// the run, then the worker's rules, as one file named by what it holds (docs/base-prompts.md).
#[allow(clippy::too_many_arguments)]
pub async fn worker_system_prompt(
  ground: &Ground,
  trees: &Trees,
  named: Option<&str>,
  account: Option<&Account>,
  rules: &str,
  above: Option<&str>,
  cwd: &str,
  model: &str,
) -> Result<Option<PathBuf>, Refusal> {
  let (base, holder) =
    match account.and_then(|account| account.base_prompt.as_deref().map(|prompt| (account, prompt))) {
      Some((account, prompt)) => {
        let model = account.base_model(model);
        let Some(base) = base_prompt_text(ground, prompt, cwd, &model).await? else {
          return Ok(None);
        };

        (base, format!("account-{}", account.id))
      }
      None => {
        let Some(named) = named else { return Ok(None) };
        let Some(base) = chosen_base_prompt(ground, named, cwd, model).await? else {
          return Ok(None);
        };

        (base, named.to_owned())
      }
    };
  let text = match above {
    Some(above) => format!("{above}\n\n{base}\n\n{rules}"),
    None => format!("{base}\n\n{rules}"),
  };
  let digest = format!("{:x}", Sha256::digest(text.as_bytes()));

  Ok(Some(system_prompt_file(
    trees,
    &format!("worker-system-{holder}-{}", &digest[..12]),
    &text,
  )?))
}

/// The worker's whole system prompt on Claude Code where no base prompt is chosen: the platform's harness
/// opening, the role's prompt where the worker's role names one, then the worker's rules, as one file named
/// by what it holds (docs/base-prompts.md).
pub fn harnessed_system_prompt(
  trees: &Trees,
  rules: &str,
  above: Option<&str>,
) -> Result<PathBuf, Refusal> {
  let text = super::harnessed(
    trees,
    &match above {
      Some(above) => format!("{above}\n\n{rules}"),
      None => rules.to_owned(),
    },
  )?;
  let digest = format!("{:x}", Sha256::digest(text.as_bytes()));

  system_prompt_file(trees, &format!("worker-system-harness-{}", &digest[..12]), &text)
}

/// The worker line's MCP configuration, on the tool host — the web namespace and the worker's own — written
/// where the line reads it when nothing has written it, and left as it stands otherwise: two trees writing
/// one file would move the worker's prefix back and forth between them. A hosted tool those namespaces do
/// not show is reached with `bmo use`.
pub fn mcp_config(ground: &Ground, trees: &Trees) -> Result<PathBuf, Refusal> {
  super::runner::handed_config(trees.agents.join("worker.mcp.json"), || {
    Ok(super::runner::line_servers(ground, &["web", "worker"]))
  })
}

/// The worker's run on its line in Claude Code's own program: `worker` names the worker whose own
/// documents, thinking, level and base prompt it carries, none for a run that is no worker's in
/// particular, and `account` the login it goes out on, whose base prompt stands where the line's own
/// choice would.
pub async fn worker_run(
  ground: &Ground,
  trees: &Trees,
  worker: Option<&str>,
  account: Option<&Account>,
  model: &str,
  cwd: &str,
) -> Result<WorkerRun, Refusal> {
  worker_run_on(ground, trees, worker, account, model, cwd, crate::worker_session::PROVIDER).await
}

/// The worker's run on its line in a program standing on `provider`, whose co-worker definitions are
/// resolved where that program's native calls take their seats (docs/model-tiers.md, 「Common co-worker
/// capacity」).
pub async fn worker_run_on(
  ground: &Ground,
  trees: &Trees,
  worker: Option<&str>,
  account: Option<&Account>,
  model: &str,
  cwd: &str,
  provider: &str,
) -> Result<WorkerRun, Refusal> {
  let role = match worker {
    Some(worker) => crate::conversational::role_of(&ground.db, worker).await.ok().flatten(),
    None => None,
  };

  run_on_line(ground, trees, worker, role.as_deref(), account, model, cwd, provider).await
}

/// A role's run on the worker line in a program standing on `provider` (docs/agents.md, 「A run」): the
/// platform's harness opening and the role's prompt first, then the worker line's standing rules, and where it
/// works in a repository's checkout `checkout`, the repository's own — its rules, the job's trailer and its
/// prefix documents (docs/repositories.md, 「What a run in a repository is handed」).
#[allow(clippy::too_many_arguments)]
pub async fn role_run(
  ground: &Ground,
  trees: &Trees,
  role: &str,
  checkout: Option<&str>,
  account: Option<&Account>,
  model: &str,
  cwd: &str,
  provider: &str,
) -> Result<WorkerRun, Refusal> {
  run_on_line(ground, trees, checkout, Some(role), account, model, cwd, provider).await
}

// The run on the worker line for a checkout, or none, under the role named.
// The role's prompt with the text its run is given where its program stands on `provider` after it
// (docs/agent-roles.md, 「A role's provider prompts」); a store that cannot be read leaves the prompt as it is.
async fn with_provider_prompt(
  ground: &Ground,
  role: &str,
  provider: &str,
  prompt: Option<String>,
) -> Option<String> {
  let added =
    crate::agent_roles::provider_prompt(&ground.db, role, provider)
      .await
      .unwrap_or_else(|fault| {
        eprintln!("agency: the {provider} prompt of the role {role} could not be read: {}", fault.detail);

        None
      });

  match (prompt, added) {
    (Some(prompt), Some(added)) => Some(format!("{}\n\n{}", prompt.trim_end(), added.trim())),
    (prompt, added) => prompt.or(added),
  }
}

/// The role's prompt of a run seated on a person's agent, between that person's organisation's instructions and
/// their own instructions for the agent (docs/prompt-layers.md, 「A role's agents of an account」); a run on an
/// agent of no account keeps its role's prompt alone. Where the account service does not answer, the instructions
/// this process last read for the account stand, and with none read yet the run is refused.
async fn around_role(
  ground: &Ground,
  seat: Option<&crate::role_agents::RoleAgent>,
  role: Option<String>,
) -> Result<Option<String>, Refusal> {
  let Some((seat, account)) = seat.and_then(|seat| Some((seat, seat.owner_account?))) else {
    return Ok(role);
  };
  let layers = crate::prompt_layers::of(&ground.db, &crate::apps::service(ground), Some(account))
    .await
    .map_err(fault)?;
  let parts: Vec<String> = [layers.organisation, role, layers.personal.of(&seat.name)]
    .into_iter()
    .flatten()
    .collect();

  Ok((!parts.is_empty()).then(|| parts.join("\n\n")))
}

#[allow(clippy::too_many_arguments)]
async fn run_on_line(
  ground: &Ground,
  trees: &Trees,
  worker: Option<&str>,
  role: Option<&str>,
  account: Option<&Account>,
  model: &str,
  cwd: &str,
  provider: &str,
) -> Result<WorkerRun, Refusal> {
  let named = match worker {
    Some(worker) => Some(repository_name_of(ground, worker).await?),
    None => None,
  };
  let folder = match worker {
    Some(worker) => rules_of(ground, worker).await?,
    None => None,
  };
  let documents = match worker {
    Some(worker) => prefix_documents(ground, worker, cwd).await?,
    None => None,
  };
  let shown = Trees {
    code: ground.shown_code_root(),
    ..trees.clone()
  };
  let trailer = match worker {
    Some(_) => Some(job_trailer(trees, &shown)?),
    None => None,
  };
  let languages = result_languages(ground, trees, provider, model).await?;
  let StandingRules {
    text: rules_text,
    file: rules,
  } = standing_rules(
    trees,
    folder.as_deref(),
    worker,
    trailer.as_deref(),
    documents.as_deref(),
    languages.as_deref(),
  )?;
  let seat = crate::role_agents::seated_at(&ground.db, role, Some(cwd)).await?;
  let above = match role {
    Some(role) => {
      with_provider_prompt(ground, role, provider, super::role_prompt(&ground.db, role, Some(cwd)).await)
        .await
    }
    None => None,
  };
  let above = around_role(ground, seat.as_ref(), above).await?;
  let system_prompt = worker_system_prompt(
    ground,
    trees,
    named.as_deref(),
    account,
    &rules_text,
    above.as_deref(),
    cwd,
    model,
  )
  .await?;
  let harnessed = harnessed_system_prompt(trees, &rules_text, above.as_deref())?;
  let worker_dir = trees.agent_dir(LINE);
  // A role that places a plugin of its own carries its own bundle; every other run carries the
  // general worker's. A run whose agent names skills carries those alone, else the ones its role names, each
  // in a plugin of the list's own (docs/role-agents.md, 「What a run reads off its agent」).
  let role_plugin = role
    .map(|role| trees.agent_dir(role).join("plugin"))
    .filter(|dir| dir.exists())
    .unwrap_or_else(|| worker_dir.join("plugin"));
  let role_skills = match role {
    Some(role) => crate::agent_roles::named(&ground.db, role)
      .await
      .map_err(|why| fault(why.to_string()))?
      .and_then(|held| held.skills.map(|skills| (held.name, skills))),
    None => None,
  };
  // The skills of the account's attached to the seat join the role's, in the seat's own plugin
  // (docs/person-nodes.md, 「Attaching skills and MCP servers」).
  let attached = match &seat {
    Some(seat) => {
      crate::attachments::attached_skills(ground, LINE, &crate::attachments::Holder::agent(&seat.name))
        .await?
    }
    None => Vec::new(),
  };
  let own = seat.as_ref().map(|seat| seat.name.as_str());
  let plugin_dir = match (
    seat.as_ref().and_then(|agent| agent.skills.as_ref().map(|skills| (agent, skills))),
    role_skills.as_ref(),
    own.filter(|_| !attached.is_empty()),
  ) {
    (Some((agent, skills)), _, _) => {
      super::agent_plugin::placed(trees, &role_plugin, Whose::Agent(&agent.name), skills, &attached)?
    }
    (None, Some((_, skills)), Some(own)) => {
      super::agent_plugin::placed(trees, &role_plugin, Whose::Agent(own), skills, &attached)?
    }
    (None, Some((role, skills)), None) => {
      super::agent_plugin::placed(trees, &role_plugin, Whose::Role(role), skills, &[])?
    }
    (None, None, Some(own)) => super::agent_plugin::placed(
      trees,
      &role_plugin,
      Whose::Agent(own),
      &super::agent_plugin::held_in(&role_plugin),
      &attached,
    )?,
    (None, None, None) => role_plugin,
  };
  let reasoning = match worker {
    Some(worker) => Some(reasoning_of(ground, worker).await?),
    None => None,
  };
  let advisor = match &seat {
    Some(seat) => crate::role_agents::advisor_of(&ground.db, seat).await?,
    None => None,
  };

  Ok(WorkerRun {
    model: model.to_owned(),
    effort: "high".to_owned(),
    tier: None,
    thinking: reasoning.as_ref().and_then(|reasoning| reasoning.thinking).unwrap_or(true),
    reasoning,
    system_prompt,
    harnessed,
    rules,
    mcp_config: mcp_config(ground, trees)?,
    settings: worker_dir.join("settings.json"),
    plugin_dir,
    tools_denied: super::claude::line_denied(LINE),
    cwd: cwd.to_owned(),
    advisor,
  })
}

/// The run with the job role's entry on a provider laid on it: the entry's model, and the effort word the
/// provider makes of the entry's word.
pub async fn configured(
  ground: &Ground,
  machine: &Machine,
  run: WorkerRun,
  provider: &str,
) -> Result<WorkerRun, Refusal> {
  configured_for(ground, machine, run, crate::agent_roles::job_role(), provider).await
}

/// The run with the job role's entry on a provider laid on it, on the model the repository's pin names there
/// where it names one (docs/repositories.md, 「A pinned model」).
pub async fn configured_pinned(
  ground: &Ground,
  machine: &Machine,
  run: WorkerRun,
  worker: &str,
  provider: &str,
) -> Result<WorkerRun, Refusal> {
  let pinned = pinned_model_on(&ground.db, worker, provider).await?;

  lay(ground, machine, run, crate::agent_roles::job_role(), provider, pinned).await
}

/// The model a worker's pin names on a provider: where the worker — or its base, for a support worker — is
/// pinned to that provider, the owner has it on and the pin names one of its models.
pub async fn pinned_model_on(
  db: &sea_orm::DatabaseConnection,
  worker: &str,
  provider: &str,
) -> Result<Option<String>, Refusal> {
  let pin = crate::queue::routes::worker_pin(db, worker)
    .await
    .map_err(|unrouted| fault(unrouted.0))?;

  Ok(pin.filter(|(pinned, _)| pinned == provider).and_then(|(_, model)| model))
}

/// The run with a role's entry on a provider laid on it, as a job's run takes the job role's.
pub async fn configured_for(
  ground: &Ground,
  machine: &Machine,
  run: WorkerRun,
  role: &str,
  provider: &str,
) -> Result<WorkerRun, Refusal> {
  lay(ground, machine, run, role, provider, None).await
}

// The role's entry for the provider laid on the run, on the model named where one is, the entry's own
// otherwise.
async fn lay(
  ground: &Ground,
  machine: &Machine,
  mut run: WorkerRun,
  role: &str,
  provider: &str,
  model: Option<String>,
) -> Result<WorkerRun, Refusal> {
  let entry = crate::order::role_entry(&ground.db, role, provider).await.map_err(fault)?;
  let model = model.unwrap_or(entry.model);

  run.effort =
    effort_of(ground, machine, provider, &model, entry.effort.as_deref(), run.reasoning.as_ref())
      .await?
      .unwrap_or_default();
  run.model = model;
  run.tier = None;

  Ok(run)
}

/// The effort word a run of a mapping goes out with where an agent's choice reaches it: the provider's
/// own answer, and the mapping's word where no choice does.
pub async fn effort_of(
  ground: &Ground,
  machine: &Machine,
  provider: &str,
  model: &str,
  mapped: Option<&str>,
  reasoning: Option<&Reasoning>,
) -> Result<Option<String>, Refusal> {
  let Some(reasoning) = reasoning else { return Ok(mapped.map(str::to_owned)) };
  let provider = named(provider).map_err(|refused| fault(refused.error))?;
  let levels = levels(crate::control::claude_accounts::Judging::of(ground), machine, provider, model).await;

  Ok(provider.run_effort(mapped, reasoning.thinking, reasoning.level.as_deref(), &levels))
}
