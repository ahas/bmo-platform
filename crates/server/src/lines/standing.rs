//! The program a worker stands with nothing put in (docs/agent-control.md, 「A worker standing with
//! nothing to do」): Claude Code on the worker's own line, on its kept session or a fresh one; or, for a
//! worker pinned to a provider whose surface stands with nothing put in, that provider's program — the
//! registry's Claude Code on the gateway, or the terminal client on a dsh session for OpenRouter and DeepSeek.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use harness_providers::claude_code::ClaudeCode;
use harness_providers::registered::Registered;
use harness_providers::{Machine, Provider};
use policy::providers::registered::REGISTERED;
use serde::Serialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::claude::{self, Command, Session};
use super::place::{
  Holds, Place, Program, Standby, command_digest, read_settings, resident_of, write_place_settings,
};
use super::prefix::{PrefixReading, prefix_digest};
use super::worker::{
  self, LINE, WorkerRun, configured, configured_pinned, effort_of, pinned_model_on, worker_run,
  worker_run_on,
};
use super::{Trees, fault};
use crate::control::claude_accounts::{self, Account, SEED_ACCOUNT};
use crate::control::registry;
use crate::places::Ground;
use crate::places::trust::{config_file, mark_onboarded, trust_folder};
use crate::worker_session::warmth;
use crate::{Refusal, now_ms};
use store::{all, one, run};

/// The model a worker's run is first composed on, before the job key's tier lays its own on it: the
/// value a base prompt's `{{model}}` reads in a pinned provider's program.
pub const WORKER_MODEL: &str = "claude-opus-5";

/// The program a worker stands, and the session it opened where it opened one with nothing said in it
/// yet.
#[derive(Serialize)]
pub struct Standing {
  pub program: Program,
  pub standby: Option<Standby>,
  /// What the program stands on, for the line the pass writes.
  pub on: String,
}

// The login a session record names: its account, or the seed account a record naming none stands for.
fn recorded(account: Option<&Account>) -> String {
  account.map_or_else(|| SEED_ACCOUNT.to_owned(), |account| account.id.clone())
}

/// How long a worker's kept session on a model stands unused before its record is retired: the cache
/// lifetime its Claude Code session is judged warm by, or the life the environment gives a session where
/// that is sooner (docs/sessions.md, 「A worker's session per model」), so no session whose cache has gone
/// is resumed.
pub async fn session_ttl_ms(ground: &Ground, model: &str) -> Option<i64> {
  let configured = ground
    .vars()
    .get("BMO_WORKER_SESSION_TTL_MINUTES")
    .and_then(|minutes| minutes.trim().parse::<i64>().ok())
    .filter(|minutes| *minutes > 0)
    .map(|minutes| minutes * 60_000);
  let lifetime = warmth::lifetime_ms("claude-code", model);

  warmth::record_ttl_ms(configured, lifetime)
}

// The worker's kept session on this model and login, where one is resumable: one idle past the life a
// session is given is retired.
async fn resumable_session(
  ground: &Ground,
  worker: &str,
  model: &str,
  account: &str,
) -> Result<Option<(String, String, Option<String>)>, Refusal> {
  let held = crate::worker_session::records::sessions(&ground.db, worker)
    .await?
    .into_iter()
    .find(|held| held.model == model && held.account == account);
  let Some(held) = held else { return Ok(None) };

  if let Some(ttl) = session_ttl_ms(ground, model).await
    && now_ms() - held.last_reply_at * 1000 > ttl
  {
    crate::worker_session::records::forget(&ground.db, worker, model, Some(account)).await?;

    return Ok(None);
  }

  Ok(Some((held.session_id, held.account, held.answered_model)))
}

// The model a kept session goes on: the one it fell to, where the worker does not heal a downgrade.
async fn session_launch_model(
  ground: &Ground,
  worker: &str,
  fallen: Option<String>,
  model: String,
) -> Result<String, Refusal> {
  let Some(fallen) = fallen else { return Ok(model) };
  let heals = one(
    &ground.db,
    r#"SELECT "downgrade_heal" FROM "repositories" WHERE "name" = $1"#,
    vec![worker.into()],
  )
  .await?
  .map(|row| row.try_get::<i64>("", "downgrade_heal"))
  .transpose()?
  .is_none_or(|heal| heal != 0);

  Ok(if heals { model } else { fallen })
}

/// The prefix the worker line opens a session on in `cwd`, on `model` at `effort` — the line's own where
/// none is named — as one short word.
pub async fn worker_prefix_digest(
  ground: &Ground,
  trees: &Trees,
  cwd: &str,
  model: &str,
  effort: Option<&str>,
) -> Result<String, Refusal> {
  let source = trees.source.to_string_lossy().into_owned();
  // The run this stands for goes out on the login the walk offers now, so its base prompt is that
  // login's.
  let account = claude_accounts::walk_for(claude_accounts::Judging::of(ground))
    .await
    .map_err(|refused| fault(refused.error))?
    .into_iter()
    .next();
  let mut line = worker_run(ground, trees, None, account.as_ref(), model, &source).await?;

  if let Some(effort) = effort {
    line.effort = effort.to_owned();
  }

  let args = claude::worker_args(&line, &line.settings, None)?;

  Ok(prefix_digest(&PrefixReading {
    cwd: Path::new(cwd),
    args: &args,
    code_root: &trees.code,
    source_root: &trees.source,
    data_root: &trees.data,
    home: &trees.home,
    plugin_dir: &line.plugin_dir,
    settings: &line.settings,
    rules: &line.rules,
    mcp_config: &line.mcp_config,
  }))
}

/// Claude Code's standing program for a worker: the job's line on the configured job model — its pin's
/// where it names one on Claude Code — and the login the walk offers now, on the worker's kept session
/// where one is resumable and on a fresh one otherwise, its settings the worker's own with the place's
/// status line and hooks.
pub async fn claude_program(
  ground: &Ground,
  machine: &Machine,
  worker: &str,
  cwd: &str,
) -> Result<Standing, Refusal> {
  let trees = Trees::of(ground);
  // The logins its supply takes: a person's job's own or the lendable ones, the platform's otherwise
  // (docs/supply.md, 「Choosing a source as a run starts」).
  let carrying = crate::supply::carrying(crate::supply::chosen_for(&ground.roots.data, worker).as_ref());
  let walked = claude_accounts::walk_for_user(claude_accounts::Judging::of(ground), carrying, None)
    .await
    .map_err(|refused| fault(refused.error))?
    .into_iter()
    .next();

  // A program is stood only on a login that answers now, never on the machine's own in place of one.
  if walked.is_none()
    && !claude_accounts::carried_for_user(&ground.db, carrying)
      .await
      .map_err(|refused| fault(refused.error))?
      .is_empty()
  {
    return Err(fault(format!("{worker}: no login answers now, so no program is stood")));
  }
  // The model the worker's pin names on Claude Code where it names one (docs/model-tiers.md, 「An agent's
  // own model」), the job key's mapping there otherwise.
  let model = match pinned_model_on(&ground.db, worker, "claude-code").await? {
    Some(model) => model,
    None => {
      crate::order::role_entry(&ground.db, crate::agent_roles::job_role(), "claude-code")
        .await
        .map_err(fault)?
        .model
    }
  };
  let held = resumable_session(ground, worker, &model, &recorded(walked.as_ref())).await?;
  let account = match &held {
    None => walked.clone(),
    Some((_, account, _)) => claude_accounts::accounts(&ground.db)
      .await
      .map_err(|refused| fault(refused.error))?
      .into_iter()
      .find(|one| &one.id == account)
      .map(|one| claude_accounts::judged(one, &ground.roots.data, crate::now_ms())),
  };
  let mut line = configured_pinned(
    ground,
    machine,
    worker_run(ground, &trees, Some(worker), account.as_ref(), &model, cwd).await?,
    worker,
    "claude-code",
  )
  .await?;

  if let Some((_, _, fallen)) = &held {
    line.model = session_launch_model(ground, worker, fallen.clone(), line.model).await?;
  }

  let place = Place::of(&trees.data, worker);
  let settings = write_place_settings(
    &place.path,
    read_settings(&line.settings),
    &status_root(ground),
    Holds::of_line(LINE),
  )?;
  let session = match &held {
    Some((id, _, _)) => Session::Resume(id.clone()),
    None => Session::Open(
      Standby::opening(&place.path, &line.model, &recorded(account.as_ref()))
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
    ),
  };
  let command = Command {
    args: claude::worker_args(&line, &settings, Some(&session))?,
    cwd: line.cwd.clone(),
    env: claude::command_env(
      ground.env(),
      account.as_ref(),
      &claude::Runs::of_place(&trees.data, &place.path),
      line.thinking,
    )?,
  };
  let env = claude::line_environment(&command, &format!("bmo-standby-{worker}"), account.as_ref());
  let program = resident_of(machine, &place.path, &command.cwd, &command.args, &env, session.id());
  let standby = match (&session, program.opened.clone().flatten()) {
    (Session::Open(id), Some(record)) => Some(Standby {
      identity: program.identity.clone(),
      session_id: id.clone(),
      model: line.model.clone(),
      account: recorded(account.as_ref()),
      record,
      prefix_digest: Some(worker_prefix_digest(ground, &trees, cwd, &line.model, None).await?),
    }),
    _ => None,
  };
  let on = match &session {
    Session::Resume(id) => format!("session {id}"),
    Session::Open(_) => "a fresh session".to_owned(),
  };

  Ok(Standing {
    program,
    standby,
    on,
  })
}

/// The tree the place's status verb is run out of: the deployed one where it holds the verbs' binary, the
/// running code's otherwise.
pub fn status_root(ground: &Ground) -> PathBuf {
  if ground.roots.deploy.join(super::place::STATUS_BINARY).exists() {
    ground.roots.deploy.clone()
  } else {
    ground.code_root()
  }
}

/// The worker's line on a pinned provider: the job key's mapping there, or, for a pin naming one of the
/// provider's own models, that model on its own defaults unless the key's tier maps to it. The line is composed
/// on the model the run goes out on, the pin's or else the entry's, so what it carries for that model — a
/// registration's result-language piece — stands in it from the first turn.
pub(crate) async fn pinned_run(
  ground: &Ground,
  machine: &Machine,
  trees: &Trees,
  worker: &str,
  cwd: &str,
  provider: &str,
  model: Option<&str>,
) -> Result<(WorkerRun, Option<i64>), Refusal> {
  let entry = crate::order::role_entry(&ground.db, crate::agent_roles::job_role(), provider)
    .await
    .ok();
  let composed_on = model.or(entry.as_ref().map(|entry| entry.model.as_str())).unwrap_or(WORKER_MODEL);
  let base = worker_run_on(ground, trees, Some(worker), None, composed_on, cwd, provider).await?;
  let Some(model) = model else {
    return Ok((configured(ground, machine, base, provider).await?, None));
  };
  let (mapped, answer_tokens) = match entry {
    Some(entry) if entry.model == model => (entry.effort, entry.answer_tokens),
    _ => (None, None),
  };
  let mut run = base;

  run.effort = effort_of(ground, machine, provider, model, mapped.as_deref(), run.reasoning.as_ref())
    .await?
    .unwrap_or_default();
  run.model = model.to_owned();
  run.tier = None;

  Ok((run, answer_tokens))
}

/// The registry's Claude Code on the gateway for a worker pinned to it: the worker's line on the
/// registration, placed on one of its nodes where it has several, with the place's settings and its
/// home's first-run questions answered, on a session it opens itself.
pub async fn registered_program(
  ground: &Ground,
  machine: &Machine,
  worker: &str,
  cwd: &str,
  model: Option<&str>,
) -> Result<Standing, Refusal> {
  let trees = Trees::of(ground);
  let (run, answer_tokens) = pinned_run(ground, machine, &trees, worker, cwd, "registered", model).await?;
  let (command, held) =
    registered_command(ground, machine, worker, &run, answer_tokens, Placing::Composed, None).await?;

  Ok(Standing {
    program: Program {
      identity: command_digest(&command),
      command,
      restart: None,
      opened: None,
    },
    standby: None,
    on: held,
  })
}

/// Claude Code on the gateway for a worker pinned to a member road: the worker's line on the member, with
/// the place's settings and its home's first-run questions answered, on a session it opens itself.
pub async fn member_program(
  ground: &Ground,
  machine: &Machine,
  worker: &str,
  cwd: &str,
  road: &'static str,
  model: Option<&str>,
) -> Result<Standing, Refusal> {
  let trees = Trees::of(ground);
  let (run, answer_tokens) = pinned_run(ground, machine, &trees, worker, cwd, road, model).await?;
  let held = member_model(machine, road, &run.model)?;
  let command =
    member_command(ground, machine, worker, road, &run, &held, answer_tokens, Placing::Composed, None)
      .await?;

  Ok(Standing {
    program: Program {
      identity: command_digest(&command),
      command,
      restart: None,
      opened: None,
    },
    standby: None,
    on: held.name,
  })
}

/// The member a road holds by a name, or the refusal naming it.
pub fn member_model(machine: &Machine, road: &'static str, name: &str) -> Result<GatewayModel, Refusal> {
  let members = harness_providers::member::road_members(machine, road).map_err(fault)?;
  let held = members.into_iter().find(|one| one.member.name == name).ok_or_else(|| {
    let not_held = harness_providers::provider(road)
      .and_then(|provider| provider.not_registered())
      .unwrap_or("Not a model of that provider");

    fault(format!("{not_held}: {name}"))
  })?;

  Ok(GatewayModel::member(machine, road, &held))
}

/// Claude Code on the gateway for a run of the worker's line on a member: the line on the member's name,
/// the member's home, and the window, the answer ceiling, the wait and the thinking the member names.
#[allow(clippy::too_many_arguments)]
pub async fn member_command(
  ground: &Ground,
  machine: &Machine,
  worker: &str,
  road: &'static str,
  run: &WorkerRun,
  held: &GatewayModel,
  answer_tokens: Option<i64>,
  placing: Placing,
  session: Option<&Session>,
) -> Result<agent_host::place::ResidentCommand, Refusal> {
  let sampling = worker_sampling(ground, worker)
    .await?
    .map(|sampling| serde_json::to_string(&sampling))
    .transpose()
    .map_err(|why| fault(why.to_string()))?;
  let claude_effort =
    harness_providers::provider(road).and_then(|provider| provider.claude_effort(&run.effort));
  let on = OnGateway {
    agent: worker,
    model: &run.model,
    effort: &run.effort,
    cwd: &run.cwd,
    settings: read_settings(&run.settings),
    holds: Holds::of_line(LINE),
    sampling,
    role: None,
    card_terms: Default::default(),
    thinking: Some(run.thinking),
    claude_effort,
    backend_effort: Some(&run.effort),
  };

  gateway_command(ground, machine, held, on, answer_tokens, placing, |name, effort, thinking, settings| {
    let mut sent = run.clone();

    sent.model = name.to_owned();
    sent.effort = effort.to_owned();
    sent.thinking = thinking;

    claude::worker_args(&sent, settings, session)
  })
  .await
  .map(|(command, _)| command)
}

/// Claude Code on the gateway for a run of the worker's line on a registration: the line on the
/// registration's name, on its node where it stands on one, with the place's settings and its home's
/// first-run questions answered, on the session named or one the program opens itself; the command, and
/// the registration's name.
pub async fn registered_command(
  ground: &Ground,
  machine: &Machine,
  worker: &str,
  run: &WorkerRun,
  answer_tokens: Option<i64>,
  placing: Placing,
  session: Option<&Session>,
) -> Result<(agent_host::place::ResidentCommand, String), Refusal> {
  let held = registry::registration(&ground.db, &run.model)
    .await
    .map_err(|refused| fault(refused.error))?
    .filter(|held| held.offered())
    .ok_or_else(|| fault(format!("{}: {}", REGISTERED.presence.not_registered, run.model)))?;
  let model = GatewayModel::registration(machine, &held);
  let sampling = worker_sampling(ground, worker)
    .await?
    .map(|sampling| serde_json::to_string(&sampling))
    .transpose()
    .map_err(|why| fault(why.to_string()))?;
  let on = OnGateway {
    agent: worker,
    model: &run.model,
    effort: &run.effort,
    cwd: &run.cwd,
    settings: read_settings(&run.settings),
    holds: Holds::of_line(LINE),
    sampling,
    role: None,
    card_terms: Default::default(),
    thinking: None,
    claude_effort: None,
    backend_effort: None,
  };

  gateway_command(
    ground,
    machine,
    &model,
    on,
    answer_tokens,
    placing,
    |name, effort, thinking, settings| {
      let mut sent = run.clone();

      sent.model = name.to_owned();
      sent.effort = effort.to_owned();
      sent.thinking = thinking;

      claude::worker_args(&sent, settings, session)
    },
  )
  .await
}

/// Where a run's session on the gateway stands as its command is composed.
pub enum Placing {
  /// Placed as the command is composed, after the place's settings are written: a program stood with
  /// nothing put in, whose picker names the model alone.
  Composed,
  /// Placed before, on the node named or on none in particular, which the picker names too.
  At(Option<String>),
}

/// The model a run on the gateway goes out on: a registration, or a member of an external provider's road.
/// The home, the window, the answer ceiling and the thinking default are the model's own.
#[derive(Clone, Debug)]
pub struct GatewayModel {
  /// The name the run is sent under.
  pub name: String,
  /// What the program shows in its picker.
  pub label: String,
  /// The home Claude Code keeps the model's records under.
  pub home: PathBuf,
  pub context_tokens: i64,
  pub answer_tokens: i64,
  /// Whether a run on it thinks where its effort names neither thinking nor no thinking.
  pub thinking: bool,
  /// The registration behind it, where it is one; none for a member.
  pub registration: Option<registry::Registration>,
  /// The road it belongs to, where it is a member; none for a registration.
  pub road: Option<&'static str>,
}

impl GatewayModel {
  /// A registration's model, its home under the registry's homes.
  pub fn registration(machine: &Machine, held: &registry::Registration) -> Self {
    Self {
      name: held.name.clone(),
      label: held.label.clone(),
      home: Registered::home(machine, &held.name),
      context_tokens: held.context_tokens,
      answer_tokens: held.answer_tokens,
      thinking: held.thinking,
      registration: Some(held.clone()),
      road: None,
    }
  }

  /// A member of a road, its home under the road's homes.
  pub fn member(machine: &Machine, road: &'static str, held: &harness_providers::member::Road) -> Self {
    Self {
      name: held.member.name.clone(),
      label: held.member.label.clone().unwrap_or_else(|| held.member.name.clone()),
      home: harness_providers::member::home(machine, road, &held.member.name),
      context_tokens: held.context_tokens,
      answer_tokens: held.answer_tokens,
      thinking: held.thinking,
      registration: None,
      road: Some(road),
    }
  }

  /// The name the run is sent under, on the node it is placed on where it is a registration's.
  pub fn placed(&self, node: &Option<String>) -> String {
    match (&self.registration, node) {
      (Some(_), Some(node)) => format!("{}@{node}", self.name),
      _ => self.name.clone(),
    }
  }
}

/// A line's run on a registered model's road, as its place stands it: the registration it names, the effort
/// its thinking is read from, the folder it works in, the settings its place is given, whether a person's
/// typed line is composed, whether its shell is guarded, and the sampling it asks the gateway for.
pub struct OnGateway<'a> {
  pub agent: &'a str,
  pub model: &'a str,
  pub effort: &'a str,
  pub cwd: &'a str,
  pub settings: serde_json::Map<String, Value>,
  /// What the place's hooks are asked: the run's line's holds, and the shell guard where its program carries one.
  pub holds: Holds,
  pub sampling: Option<String>,
  /// The role whose saved prefix the run's requests start from on an engine that keeps role files.
  pub role: Option<&'a str>,
  /// What the run says of its demands, which a request asked for a card at the gateway's door names.
  pub card_terms: harness_machine::card_master::CardTerms,
  /// Whether the run thinks, where its caller knows it otherwise than by the effort word.
  pub thinking: Option<bool>,
  /// The Claude Code effort word the run goes out with, where the provider's words differ from Claude
  /// Code's.
  pub claude_effort: Option<&'a str>,
  /// The backend effort word the gateway is asked with, where the provider has one.
  pub backend_effort: Option<&'a str>,
}

/// The command Claude Code stands on for a line on a model of the gateway: the model's home settled, the
/// place's settings with the model's row in the program's picker, the words the line makes of the name the
/// run is sent under and its thinking — the place's settings handed through the home's own — and the
/// gateway's variables; with the model's name.
pub async fn gateway_command(
  ground: &Ground,
  machine: &Machine,
  held: &GatewayModel,
  on: OnGateway<'_>,
  answer_tokens: Option<i64>,
  placing: Placing,
  words: impl FnOnce(&str, &str, bool, &Path) -> Result<Vec<String>, Refusal>,
) -> Result<(agent_host::place::ResidentCommand, String), Refusal> {
  let trees = Trees::of(ground);
  let home = held.home.clone();
  let thinking = on.thinking.unwrap_or(match on.effort {
    "thinking" => true,
    "no-thinking" => false,
    _ => held.thinking,
  });

  std::fs::create_dir_all(&home).map_err(|why| fault(format!("{}: {why}", home.display())))?;
  mark_onboarded(&config_file(&home));
  trust_folder(&config_file(&home), on.cwd);

  let place = Place::of(&trees.data, on.agent);
  // The model's row in the program's picker, whose label the program shows at the top of its screen in
  // place of the name the run is sent under (docs/own-llm.md, 「Claude Code in the pane」): the run as it
  // is placed when the settings are written.
  let mut picked = on.settings;
  let shown = match &placing {
    Placing::Composed => held.name.clone(),
    Placing::At(node) => held.placed(node),
  };

  picked.insert(
    "modelPicker".to_owned(),
    json!({ "options": [{ "model": shown, "label": held.label }] }),
  );

  let settings = write_place_settings(&place.path, picked, &status_root(ground), on.holds)?;
  let placement = match placing {
    Placing::Composed => match &held.registration {
      Some(registration) => place_registration(ground, registration).await?,
      None => None,
    },
    Placing::At(node) => node,
  };
  let placed = held.placed(&placement);
  // A person's run goes out under their own call key, and on their own key's member where its supply is
  // personal and its member is not theirs already (docs/own-llm.md, 「The keys」 and 「Personal members」;
  // docs/supply.md, 「Owned supply」).
  // The run is its agent's, a thrown-away session's place read as its session's agent — a job's checkout — and its
  // account the one the run's supply was chosen for — a job's is its holder's, whose checkout names no account —
  // else the one the agent itself is for.
  let run_agent =
    crate::apart_places::session_agent(&ground.roots.data, on.agent).unwrap_or_else(|| on.agent.to_owned());
  let chosen = crate::supply::chosen_for(&ground.roots.data, &run_agent);
  let account = match &chosen {
    Some(chosen) => chosen.account,
    None => crate::supply::account_of(ground, on.agent).await?,
  };
  let person = crate::supply::person(ground, account).await?;
  let name = match (&held.registration, person) {
    (None, Some(_)) => crate::supply::personal_member(chosen.as_ref(), &placed).unwrap_or(placed),
    _ => placed,
  };
  let key_file = match person {
    Some(account) => crate::call_keys::ensure(&ground.db, &ground.roots.data, account).await?,
    None => Registered::key_file(machine),
  };
  let claude_word = on.claude_effort.unwrap_or(if thinking { "medium" } else { "low" });
  let mut args = words(&name, claude_word, thinking, &settings)?;

  if let Some(at) = args.iter().position(|arg| arg == "--settings")
    && let Some(given) = args.get(at + 1).cloned()
  {
    args[at + 1] = keyed_settings_at(&home, Some(&given), &key_file)?.to_string_lossy().into_owned();
  }

  let mut headers = gateway_headers(on.sampling.as_deref(), ClaudeCode.long_tools(), on.role);

  headers.push(format!("x-bmo-run: {run_agent}"));

  headers.extend(on.card_terms.headers());

  if let Some(word) = on.backend_effort {
    headers.push(format!("x-bmo-effort: {word}"));
  }

  let mut env: BTreeMap<String, Option<String>> =
    ground.env().iter().map(|(name, value)| (name.clone(), Some(value.clone()))).collect();

  env.extend(
    policy::providers::claude_code::CLAUDE_CODE
      .launch
      .variables
      .iter()
      .map(|(name, value)| (name.clone(), Some(value.clone()))),
  );

  for (variable, value) in
    gateway_variables(ground, held, &home, &name, answer_tokens.unwrap_or(held.answer_tokens), &headers)
  {
    env.insert(variable.to_owned(), value);
  }

  if !thinking {
    env.insert("MAX_THINKING_TOKENS".to_owned(), Some("0".to_owned()));
  }

  // The program's own error stream goes to the place, where the pane's loop and a person both read it. A
  // program that ends before its prompt leaves its reason there rather than on a pane that is gone.
  env.insert(
    agent_host::place::STDERR_VARIABLE.to_owned(),
    Some(place.path.join("stderr.log").to_string_lossy().into_owned()),
  );

  let env: BTreeMap<String, String> =
    env.into_iter().filter_map(|(name, value)| Some((name, value?))).collect();
  let command = agent_host::place::ResidentCommand {
    args,
    cwd: PathBuf::from(on.cwd),
    env,
    login_file: Some(config_file(&home)),
  };

  Ok((command, held.name.clone()))
}

/// Where a new session on the registration a model names stands, recorded as given now; nowhere in
/// particular on a registration of one address, or a model the registry does not hold.
pub async fn place_registered(ground: &Ground, model: &str) -> Result<Option<String>, Refusal> {
  match registry::registration(&ground.db, model)
    .await
    .map_err(|refused| fault(refused.error))?
  {
    Some(held) => place_registration(ground, &held).await,
    None => Ok(None),
  }
}

// Where a new session on a registration stands: on one of several addresses, the one given a new
// session least recently — an address never given one first, and the registration's own order among
// equals — recorded as given now; on a registration of one address, nowhere in particular.
async fn place_registration(
  ground: &Ground,
  held: &registry::Registration,
) -> Result<Option<String>, Refusal> {
  if held.addresses.len() < 2 {
    return Ok(None);
  }

  let rows = all(
    &ground.db,
    r#"SELECT "node", "placed_at" FROM "registered_model_addresses" WHERE "registration" = $1
       ORDER BY "position", "node""#,
    vec![held.name.as_str().into()],
  )
  .await?;
  let mut placed = Vec::with_capacity(rows.len());

  for row in rows {
    placed.push((
      row.try_get::<String>("", "node")?,
      row.try_get::<Option<i64>>("", "placed_at")?.unwrap_or(-1),
    ));
  }

  let Some((node, _)) = placed.iter().min_by_key(|(_, at)| *at).cloned() else {
    return Ok(None);
  };

  run(
    &ground.db,
    r#"UPDATE "registered_model_addresses" SET "placed_at" = $1 WHERE "registration" = $2 AND "node" = $3"#,
    vec![(now_ms() / 1000).into(), held.name.as_str().into(), node.as_str().into()],
  )
  .await?;

  Ok(Some(node))
}

/// The settings a run on the gateway is handed: those given, where a run is given any, with the helper that
/// prints the key file named — the gateway's own, a person's call key, or a road's own — beside them and the
/// warning a program started in bypass mode stands on passed over, written under the registration's home by
/// what they were made from.
pub(crate) fn keyed_settings_at(
  home: &Path,
  given: Option<&str>,
  key_file: &Path,
) -> Result<PathBuf, Refusal> {
  let mut base = given
    .and_then(|given| std::fs::read_to_string(given).ok())
    .and_then(|text| serde_json::from_str::<Map<String, Value>>(&text).ok())
    .unwrap_or_default();
  let given = given.unwrap_or_default();
  let key = key_file.to_string_lossy().replace('\'', r"'\''");

  base.insert("apiKeyHelper".to_owned(), Value::String(format!("cat '{key}'")));
  base.insert("skipDangerousModePermissionPrompt".to_owned(), Value::Bool(true));

  let dir = home.join("settings");
  let path = dir.join(format!("{}.json", &format!("{:x}", Sha256::digest(given.as_bytes()))[..16]));
  let text = serde_json::to_string_pretty(&base).map_err(|why| fault(why.to_string()))?;
  let fresh = dir.join(format!(
    ".{}.{}",
    path.file_name().map(|name| name.to_string_lossy()).unwrap_or_default(),
    std::process::id()
  ));

  std::fs::create_dir_all(&dir).map_err(|why| fault(format!("{}: {why}", dir.display())))?;
  agent_host::disk::private_file(&fresh)
    .and_then(|mut file| std::io::Write::write_all(&mut file, format!("{text}\n").as_bytes()))
    .and_then(|()| std::fs::rename(&fresh, &path))
    .map_err(|why| fault(format!("{}: {why}", path.display())))?;

  Ok(path)
}

/// The headers a request of a run on the gateway carries: the run's own sampling where it names any, the
/// tools of its program whose calls wait long where it names any, and the role its prompt is where its work
/// key names one (docs/own-llm.md, 「Role files」).
pub fn gateway_headers(sampling: Option<&str>, long_tools: &[String], role: Option<&str>) -> Vec<String> {
  let mut headers = Vec::new();

  if let Some(sampling) = sampling {
    headers.push(format!("x-bmo-sampling: {sampling}"));
  }

  if !long_tools.is_empty() {
    headers.push(format!("x-bmo-long-tools: {}", long_tools.join(",")));
  }

  if let Some(role) = role {
    headers.push(format!("x-bmo-role: {role}"));
  }

  headers
}

/// The share of a registration's window, in percent, a run on it is told to fold its conversation at: what
/// the answer ceiling leaves of the window, rounded down to a ten-thousandth, so a request at the fold
/// still carries the whole ceiling (docs/own-llm.md, 「A turn at the input limit」).
pub fn fold_share(window: i64, answer: i64) -> String {
  let millionths =
    ((window - answer).max(0) as i128 * 1_000_000 / window.max(1) as i128).clamp(1, 1_000_000);

  format!("{}.{:04}", millionths / 10_000, millionths % 10_000)
}

/// The longest wait Claude Code's timers hold, about twenty-four days: the program gives a request up after a
/// wait of its own and its runtime drops a connection silent past one, and neither takes a value meaning
/// none, so both are told this one, which no request reaches (docs/own-llm.md, 「How requests go」).
const NO_WAIT_MS: i64 = 2_147_483_647;

/// What Claude Code is pointed at the gateway with for a run on a registration, under the name the run is
/// sent as (docs/own-llm.md, 「How requests go」): no login of its own, the registration's home, the
/// gateway's address, the name in every model slot, the subagent's among them, so a subagent the program
/// starts goes out on the run's own model, the window and answer it is sized to and the fold that leaves the answer room, the longest wait the program and the runtime under
/// it take, so no request is given up on their clocks, and the headers given.
pub(crate) fn gateway_variables(
  ground: &Ground,
  held: &GatewayModel,
  home: &Path,
  name: &str,
  answer_tokens: i64,
  headers: &[String],
) -> Vec<(&'static str, Option<String>)> {
  vec![
    ("CLAUDE_CODE_OAUTH_TOKEN", None),
    ("ANTHROPIC_API_KEY", None),
    ("ANTHROPIC_AUTH_TOKEN", None),
    ("CLAUDE_CONFIG_DIR", Some(home.to_string_lossy().into_owned())),
    (
      "ANTHROPIC_BASE_URL",
      Some(harness_machine::gateway::url(
        ground.vars().get("BMO_LLM_GATEWAY_URL").map(String::as_str),
      )),
    ),
    ("ANTHROPIC_MODEL", Some(name.to_owned())),
    ("ANTHROPIC_DEFAULT_OPUS_MODEL", Some(name.to_owned())),
    ("ANTHROPIC_DEFAULT_SONNET_MODEL", Some(name.to_owned())),
    ("ANTHROPIC_DEFAULT_HAIKU_MODEL", Some(name.to_owned())),
    ("CLAUDE_CODE_SUBAGENT_MODEL", Some(name.to_owned())),
    ("CLAUDE_CODE_ATTRIBUTION_HEADER", Some("0".to_owned())),
    ("CLAUDE_CODE_MAX_CONTEXT_TOKENS", Some(held.context_tokens.to_string())),
    ("CLAUDE_CODE_MAX_OUTPUT_TOKENS", Some(answer_tokens.to_string())),
    (claude::COMPACT_SHARE, Some(fold_share(held.context_tokens, answer_tokens))),
    ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", Some("1".to_owned())),
    // Behind a base URL the program defers its MCP tools behind a tool search the model must call first; a
    // registered model is shown every tool outright instead.
    ("ENABLE_TOOL_SEARCH", Some("false".to_owned())),
    ("API_TIMEOUT_MS", Some(NO_WAIT_MS.to_string())),
    ("BUN_CONFIG_HTTP_IDLE_TIMEOUT", Some((NO_WAIT_MS / 1_000).to_string())),
    ("ANTHROPIC_CUSTOM_HEADERS", (!headers.is_empty()).then(|| headers.join("\n"))),
  ]
}

// The worker's own sampling as the gateway's header carries it, in the order it was saved: every
// setting but the answer ceiling, a setting the chat template reads under the template's own field;
// none where it names none.
pub(crate) async fn worker_sampling(
  ground: &Ground,
  worker: &str,
) -> Result<Option<indexmap::IndexMap<String, Value>>, Refusal> {
  let named = worker::repository_name_of(ground, worker).await?;
  let Some(text) = crate::queue::settings::get(&ground.db, &format!("agent_sampling:{named}")).await?
  else {
    return Ok(None);
  };
  let Ok(saved) = serde_json::from_str::<indexmap::IndexMap<String, Value>>(&text) else {
    return Ok(None);
  };
  let generation = &policy::supervisors::GENERATION;
  let mut sampling: indexmap::IndexMap<String, Value> = indexmap::IndexMap::new();
  let mut template: indexmap::IndexMap<String, Value> = indexmap::IndexMap::new();

  for (name, value) in saved {
    let usable = value.as_f64().is_some_and(f64::is_finite) || value.is_boolean();

    if !usable || name == "max_tokens" {
      continue;
    }

    match generation.setting(&name) {
      Some(setting) if setting.template => {
        template.insert(name, value);
      }
      _ if value.is_number() => {
        sampling.insert(name, value);
      }
      _ => {}
    }
  }

  if !template.is_empty() {
    sampling.insert(
      "chat_template_kwargs".to_owned(),
      serde_json::to_value(&template).map_err(|why| fault(why.to_string()))?,
    );
  }

  Ok((!sampling.is_empty()).then_some(sampling))
}

/// The pane program for a worker pinned to a provider that stands one with nothing put in: the terminal client on the
/// worker's place and dsh session, on the model its pin names where it names one, its screen kept open
/// across turns and a line typed at it sent into the same session.
#[allow(clippy::too_many_arguments)]
pub async fn inbox_program(
  ground: &Ground,
  machine: &Machine,
  worker: &str,
  cwd: &str,
  provider: &str,
  program: &str,
  dsh: &str,
  model: Option<&str>,
) -> Result<Standing, Refusal> {
  let trees = Trees::of(ground);
  let (run, _) = pinned_run(ground, machine, &trees, worker, cwd, provider, model).await?;
  let place = Place::of(&trees.data, worker);
  let mut env: BTreeMap<String, String> = ground.env().clone();

  env.extend(policy::providers::claude_code::CLAUDE_CODE.launch.variables.clone());

  let laid = super::dsh::Composed {
    line: LINE,
    provider,
    system: run.system_prompt.as_deref(),
    rules: Some(&run.rules),
    shown: None,
    allowed: None,
    cwd: &run.cwd,
    model: &run.model,
    effort: &run.effort,
    chosen: run.reasoning.as_ref().is_some_and(|reasoning| reasoning.level.is_some()),
    thinking: run.thinking,
    mcp: Some(&run.mcp_config),
    compact_at: None,
  };
  let command = super::openrouter::resident_command(
    ground,
    machine,
    provider,
    program,
    dsh,
    &place.path,
    worker,
    &laid,
    &env,
  )?;

  Ok(Standing {
    program: Program {
      identity: command_digest(&command),
      command,
      restart: None,
      opened: None,
    },
    standby: None,
    on: run.model,
  })
}

/// A session a person opened in the standing program, taken as the worker's kept session on that
/// program's model and login. A session nothing has been said in is left standing, unless the program
/// that opened it is no longer the place's: then there is nothing to take and nothing to keep. Answers
/// the session taken.
pub async fn claim_standby(
  ground: &Ground,
  worker: &str,
  place: &Place,
) -> Result<Option<String>, Refusal> {
  let Some(standby) = Standby::read(&place.path) else { return Ok(None) };

  if !standby.record.exists() {
    if place.program().is_none_or(|program| program.identity != standby.identity) {
      Standby::drop_from(&place.path);
    }

    return Ok(None);
  }

  let replied = std::fs::metadata(&standby.record)
    .and_then(|meta| meta.modified())
    .ok()
    .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
    .map_or(0, |since| since.as_secs() as i64);

  let _ = replied;

  crate::worker_session::records::record(
    &ground.db,
    worker,
    &standby.model,
    &standby.account,
    &standby.session_id,
    standby.prefix_digest.as_deref(),
    None,
  )
  .await?;
  Standby::drop_from(&place.path);

  Ok(Some(standby.session_id))
}
