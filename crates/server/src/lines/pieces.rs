//! A supervisor's turn as a turn of its resident program on the chat line (docs/agents.md, 「Conversational
//! agents」; docs/supervisors.md, 「What a turn shows」): the chat line's words with the system prompt its
//! settings compose in place of the program's own, in the supervisor's own folder, on the room's session —
//! the one it resumes, or one it opens — and the room, the supervisor, the run and the messages the turn
//! answers among the run's own variables. On the registry's provider the program is Claude Code on the
//! gateway, sized to the registration and placed on one of its nodes; on Claude Code's own, the program on
//! the login the walk offers the supervisor, as every Claude run is.

use std::collections::BTreeMap;
use std::path::PathBuf;

use agent_host::place::ResidentCommand;
use harness_machine::supervisor_prompt;
use harness_providers::Machine;
use harness_providers::claude_code::ClaudeCode;
use indexmap::IndexMap;
use policy::chat_line::CHAT_LINE;
use policy::fill_marks;
use policy::providers::Turns;
use policy::providers::registered::REGISTERED;
use policy::supervisor_runner::SUPERVISOR_RUNNER;
use serde_json::Value;

use super::chat::{Hands, Words, denied, standing_words};
use super::claude::{Command, Runs, Session, command_env, line_environment};
use super::place::{Holds, Place, write_place_settings};
use super::standing::{GatewayModel, OnGateway, Placing, gateway_command, status_root};
use super::worker::{effort_of, reasoning_of, system_prompt_file};
use super::{Trees, fault};
use crate::Refusal;
use crate::control::claude_accounts::{self, Account};
use crate::control::registry::{self, Registration};
use crate::places::Ground;

/// The line a supervisor's turn goes out on: the chat line, as every conversational agent's does.
pub const LINE: &str = super::chat::LINE;

// The provider whose models are the registry's registrations, carried by Claude Code on the gateway.
const REGISTERED_PROVIDER: &str = "registered";

/// What the program says of a session it is asked to resume and holds no record of.
pub const LOST_SESSION: &str = "No conversation found with session ID";

/// The words the program says when the session it was asked to resume cannot be continued: the record
/// stands, but the state it ends on is one the program will not take up.
pub const STALE_SESSION: &str = "No deferred tool marker found in the resumed session";

/// Whether an error line says the session the turn named is gone or cannot be continued. Either way the
/// session is let go and the turn is asked again once on a fresh one.
pub fn lost_session(said: &str) -> bool {
  said.contains(LOST_SESSION) || said.contains(STALE_SESSION)
}

/// One turn's run as its runner asks for it.
pub struct SupervisorTurn<'a> {
  pub agent: &'a str,
  pub chat_id: i64,
  pub provider: &'a str,
  pub model: &'a str,
  pub system: &'a str,
  pub prompt: &'a str,
  /// Whether every new message the turn answers is the owner's, or no person's message opened it, which
  /// the tool host's grant lock reads as the deployment's turns'.
  pub privileged: bool,
  /// The harness session the turn goes on in, and the place it stands on; none opens a new one.
  pub resume: Option<(&'a str, Option<&'a str>)>,
  /// The node of the card the turn holds, on a registration naming a card service, which the turn is
  /// placed on whatever its session last stood on (docs/own-llm.md, 「An engine the arbiter stands」).
  pub card: Option<&'a str>,
  pub run: &'a str,
  /// The messages the turn answers.
  pub answers: &'a [i64],
  /// Its generation settings, in the order they were saved.
  pub generation: &'a IndexMap<String, Value>,
  /// The moment the turn took its words, in unix milliseconds, which a reset begun after it overtakes
  /// (docs/supervisors.md, 「What the reset cuts off」).
  pub began: i64,
}

/// The turn composed: the command the supervisor's program stands on, the turn's own variables among its
/// environment; the session that command opens or resumes; the model and effort its answer is written down
/// under, the place its session is kept as standing on, and the registration's window where the run is
/// one's.
pub struct ComposedTurn {
  /// The system prompt's file the program reads.
  pub system_path: PathBuf,
  pub command: ResidentCommand,
  pub session: String,
  pub prompt: String,
  pub model: String,
  pub effort: Option<String>,
  pub placement: Option<String>,
  pub window: Option<Window>,
  /// The Claude login the run goes out on, where it goes out on one of the deployment's.
  pub account: Option<String>,
  /// The moment the turn took its words, as its variables carry it.
  pub began: i64,
  /// The variables the turn hands its run where they are not the command's own: a stand-in road's, whose
  /// command keeps the turn's variables and the road's tap out of the program's identity.
  pub run_env: Option<BTreeMap<String, String>>,
  /// The run as a stand-in road describes it, which a loop reads its turn by.
  pub given: Option<Value>,
}

/// What a run on a registration was sized to: its window, and the home its program keeps its records under.
pub struct Window {
  pub context_tokens: i64,
  pub home: PathBuf,
}

/// What the place's settings ask of the program: the shell guard every conversational agent's program
/// carries, and a person's typed line put in as typed, since only the supervisor's typed lines are
/// composed.
pub fn holds() -> Holds {
  Holds {
    shell_guard: true,
    ..Holds::default()
  }
}

/// What the run is handed of the settings: the answer ceiling where they name one, and the rest as its
/// sampling, a setting the chat template reads under the template's own field.
pub fn generation_run(values: &IndexMap<String, Value>) -> (Option<i64>, IndexMap<String, Value>) {
  let generation = &policy::supervisors::GENERATION;
  let mut ceiling = None;
  let mut sampling = IndexMap::new();
  let mut template = IndexMap::new();

  for (name, value) in values {
    if name == "max_tokens" {
      ceiling = value.as_i64().or_else(|| value.as_f64().map(|value| value as i64));
      continue;
    }

    match generation.setting(name) {
      Some(setting) if setting.template => {
        template.insert(name.clone(), value.clone());
      }
      _ if value.is_number() => {
        sampling.insert(name.clone(), value.clone());
      }
      _ => {}
    }
  }

  if !template.is_empty() {
    sampling.insert("chat_template_kwargs".to_owned(), serde_json::to_value(&template).unwrap_or_default());
  }

  (ceiling, sampling)
}

/// The login the walk offers an agent now on Claude Code's own road, none where it carries no login of the
/// deployment's, and the refusal where every login it carries is closed.
pub(crate) async fn walked_login(ground: &Ground, agent: &str) -> Result<Option<Account>, Refusal> {
  walked_login_for(ground, agent, false).await
}

/// The login the walk offers an agent now on Claude Code's own road for a program standing here or, where
/// `on_node`, on a node, which takes only a login that runs there, since an endpoint account's key stays on
/// this machine; none where the road carries no login of the deployment's, and the refusal where every login
/// it carries is closed.
pub(crate) async fn walked_login_for(
  ground: &Ground,
  agent: &str,
  on_node: bool,
) -> Result<Option<Account>, Refusal> {
  // The logins the agent's supply takes: a person's own or the lendable ones, the platform's otherwise
  // (docs/supply.md, 「Choosing a source as a run starts」).
  let run_agent =
    crate::apart_places::session_agent(&ground.roots.data, agent).unwrap_or_else(|| agent.to_owned());
  let carrying =
    crate::supply::carrying(crate::supply::chosen_for(&ground.roots.data, &run_agent).as_ref());
  let login = claude_accounts::walk_for_user(claude_accounts::Judging::of(ground), carrying, None)
    .await
    .map_err(|refused| fault(refused.error))?
    .into_iter()
    .find(|login| !on_node || login.runs_on_nodes());

  if login.is_none()
    && !claude_accounts::carried_for_user(&ground.db, carrying)
      .await
      .map_err(|refused| fault(refused.error))?
      .is_empty()
  {
    return Err(fault(format!("{agent}: no Claude login answers now")));
  }

  Ok(login)
}

/// The registration a run names, with an address, or the refusal naming it.
pub async fn registration_named(ground: &Ground, model: &str) -> Result<Registration, Refusal> {
  registry::registration(&ground.db, model)
    .await
    .map_err(|refused| fault(refused.error))?
    .filter(|held| held.offered())
    .ok_or_else(|| fault(format!("{}: {model}", REGISTERED.presence.not_registered)))
}

/// The provider a supervisor is pinned to, where it carries a supervisor's answers and its model has an
/// address where the registry serves it; otherwise the sentence saying why its turns cannot go out.
pub async fn pinned_provider(
  ground: &Ground,
  provider: Option<&str>,
  model: Option<&str>,
) -> Result<&'static dyn harness_providers::Provider, String> {
  let refused = &SUPERVISOR_RUNNER.refused;
  let (Some(provider), Some(model)) = (provider, model) else {
    return Err(refused.no_pin.clone());
  };
  let Some(found) = harness_providers::provider(provider) else {
    return Err(fill_marks(&refused.no_provider, &[("provider", provider)]));
  };

  if !carries_answers(found) {
    return Err(fill_marks(&refused.no_line, &[("label", &found.declared().label)]));
  }

  if found.not_registered().is_some() && registration_named(ground, model).await.is_err() {
    return Err(fill_marks(&refused.no_model, &[("model", model)]));
  }

  Ok(found)
}

/// Whether the provider carries a supervisor's answers: it declares the supervisor's line.
pub fn carries_answers(provider: &dyn harness_providers::Provider) -> bool {
  provider.declared().lines.iter().any(|line| line == LINE)
}

/// Whether a supervisor's program on the provider takes the platform's harness opening where the supervisor
/// chooses no base prompt (docs/supervisors.md, 「What a turn is given」): every program on Claude Code does,
/// and a program that carries its own turns through its inbox, a harness of its own, keeps its own prompt.
pub fn takes_harness(provider: &str) -> bool {
  !crate::chat_stand_in::inbox_declared(provider)
}

/// The turn of one supervisor, composed.
pub async fn compose(
  ground: &Ground,
  machine: &Machine,
  turn: &SupervisorTurn<'_>,
) -> Result<ComposedTurn, Refusal> {
  let trees = Trees::of(ground);
  let own = crate::supervisors::own_variables(&ground.db, turn.agent).await;
  let reasoning = reasoning_of(ground, turn.agent).await?;
  let thinking = reasoning.thinking.unwrap_or(false);
  let effort = effort_of(ground, machine, turn.provider, turn.model, None, Some(&reasoning))
    .await?
    .unwrap_or_default();
  let system = system_prompt_file(&trees, &format!("supervisor-{}", turn.agent), turn.system)?;
  let roots = supervisor_prompt::Roots {
    assets: trees.code.clone(),
    data: trees.data.clone(),
  };
  let folder = supervisor_prompt::folder(&roots, turn.agent);

  std::fs::create_dir_all(&folder).map_err(|why| fault(format!("{}: {why}", folder.display())))?;

  let cwd = folder.to_string_lossy().into_owned();
  let limits = crate::supervisor_limits::Limits::of(&ground.db, turn.agent).await?;
  let mcp = super::chat::mcp_config(ground, &trees, turn.agent, &limits)?;
  // The turn's own variables, which the place holds for the run while the program keeps none of them.
  let variables = [
    ("BMO_TURN_CHAT", turn.chat_id.to_string()),
    ("BMO_TURN_AGENT", turn.agent.to_owned()),
    (
      "BMO_TURN_MESSAGES",
      turn.answers.iter().map(i64::to_string).collect::<Vec<_>>().join(","),
    ),
    ("BMO_TURN_PRIVILEGED", if turn.privileged { "1" } else { "0" }.to_owned()),
    ("BMO_TURN_BEGAN", turn.began.to_string()),
  ];

  // A provider whose surface carries its own turns stands its pane program rather than a program that reads
  // the turn's transcript: the program and harness it declares, on the overlay the chat line's words compose,
  // with the harness's key in the program's environment alone.
  if harness_providers::provider(turn.provider)
    .and_then(|declared| declared.declared().resident.as_ref())
    .is_some_and(|resident| resident.turns == Turns::Inbox)
  {
    // Such a program takes its tools whole or not at all, so it cannot hold a limited supervisor to the
    // built-in tools its limit keeps (docs/supervisors.md, 「What each capability stands for」).
    if limits.kept().is_some() {
      return Err(fault(format!(
        "{} holds a tool limit, so its turns go out on Claude Code alone and never on a harness of its own.",
        turn.agent
      )));
    }

    let mut env = ground.env().clone();

    env.extend(own);
    env.extend(variables.iter().map(|(name, value)| ((*name).to_owned(), value.clone())));

    let compact_at = super::dsh::compact_share(ground, turn.agent, turn.provider).await;
    let laid = super::dsh::Composed {
      line: LINE,
      provider: turn.provider,
      system: Some(&system),
      rules: None,
      // A program told its tools is allowed every one it is shown.
      shown: Some(&CHAT_LINE.tools.shown),
      allowed: Some(&CHAT_LINE.tools.shown),
      cwd: &cwd,
      model: turn.model,
      effort: &effort,
      chosen: reasoning.level.is_some(),
      thinking,
      mcp: Some(&mcp),
      compact_at,
    };
    let command = super::chat::inbox_command(ground, machine, turn.provider, turn.agent, &laid, &env)?;

    return Ok(ComposedTurn {
      system_path: system,
      command,
      session: uuid::Uuid::new_v4().to_string(),
      prompt: turn.prompt.to_owned(),
      model: turn.model.to_owned(),
      effort: (!effort.is_empty()).then(|| effort.clone()),
      placement: None,
      window: None,
      account: None,
      began: turn.began,
      run_env: Some(env),
      given: Some(serde_json::json!({ "prompt": turn.prompt })),
    });
  }

  let gateway = match turn.provider {
    REGISTERED_PROVIDER => {
      Some(GatewayModel::registration(machine, &registration_named(ground, turn.model).await?))
    }
    road if crate::chat_stand_in::member_declared(road) => {
      let provider = harness_providers::provider(road)
        .ok_or_else(|| fault(format!("{road} is no provider of this build")))?;

      Some(super::standing::member_model(machine, provider.name(), turn.model)?)
    }
    _ => None,
  };
  // On Claude Code's own road the program goes out on the login the walk offers the supervisor now, as every
  // Claude run does, and never on one that cannot answer: where none answers, the turn is not run.
  let login = match gateway {
    Some(_) => None,
    None => walked_login(ground, turn.agent).await?,
  };
  // A program asked to resume a session it keeps no record of ends before it reads a word: such a session
  // is refused in the program's own words, which the turn reads as a session gone and opens a new one
  // for. The record stands under the home the program keeps its records in: the model's, the login's
  // directory, or the machine's own.
  let home = match (&gateway, login.as_ref().and_then(|login| login.credential.own_home())) {
    (Some(gateway), _) => gateway.home.clone(),
    (None, Some(home)) => home.to_path_buf(),
    (None, None) => ClaudeCode::machine_config_dir(machine),
  };

  if let Some((id, _)) = turn.resume
    && !ClaudeCode::records_under(&home, &folder, id).iter().any(|record| record.exists())
  {
    return Err(fault(format!("{LOST_SESSION}: {id}")));
  }

  let (ceiling, sampling) = generation_run(turn.generation);
  let session = match turn.resume {
    Some((id, _)) => Session::Resume(id.to_owned()),
    None => Session::Open(uuid::Uuid::new_v4().to_string()),
  };
  let denied = limits.denied(&denied());
  let kept = limits.kept();
  // The advisor goes out on the supervisor's own road on Claude Code alone; a road the gateway serves takes none
  // (docs/provider-order.md, 「The advisor」).
  let advisor = crate::supervisors::named(&ground.db, turn.agent)
    .await
    .map_err(|why| fault(why.detail))?
    .and_then(|supervisor| supervisor.road())
    .filter(|road| road.provider == turn.provider && road.model == turn.model)
    .and_then(|road| road.advisor_taken());
  let plugin = crate::attachments::supervisor_plugin(ground, turn.agent).await?;
  let line = |model: &str, effort: &str, advisor: Option<&str>, settings: &std::path::Path| {
    standing_words(
      &Words {
        line: super::chat::LINE,
        model,
        effort,
        advisor,
        system: &system,
        mcp: &mcp,
        plugin: plugin.as_deref(),
        hands: Hands::AllBut {
          denied: &denied,
          kept: kept.as_deref(),
        },
        host_commands: true,
      },
      settings,
      &session,
    )
  };
  let Some(gateway) = gateway else {
    let place = Place::of(&trees.data, turn.agent);
    let settings =
      write_place_settings(&place.path, serde_json::Map::new(), &status_root(ground), holds())?;
    let mut env =
      command_env(ground.env(), login.as_ref(), &Runs::of_place(&trees.data, &place.path), thinking)?;

    env.extend(own.into_iter().map(|(name, value)| (name, Some(value))));
    env.extend(variables.map(|(name, value)| (name.to_owned(), Some(value))));

    let command = Command {
      args: line(turn.model, &effort, advisor.as_deref(), &settings)?,
      cwd: cwd.clone(),
      env,
    };
    let env = line_environment(&command, turn.run, login.as_ref());

    return Ok(ComposedTurn {
      system_path: system,
      command: ResidentCommand {
        args: command.args,
        cwd: PathBuf::from(&cwd),
        env,
        login_file: None,
      },
      session: session.id().to_owned(),
      prompt: turn.prompt.to_owned(),
      model: turn.model.to_owned(),
      effort: Some(effort),
      placement: None,
      window: None,
      account: login.map(|login| login.id),
      began: turn.began,
      run_env: None,
      given: None,
    });
  };

  // A session it resumes stands where it stood; a fresh one is placed where the registration places it and
  // kept as standing there. A member road is placed nowhere: its home is the member's alone.
  let registration = gateway.registration.as_ref();
  let (placement, kept) = match (registration, turn.resume) {
    (None, _) => (None, None),
    (Some(_), _) if turn.card.is_some() => (turn.card.map(str::to_owned), None),
    (Some(_), Some((_, Some(node)))) => (Some(node.to_owned()), Some(node.to_owned())),
    (Some(_), Some((_, None))) => (super::standing::place_registered(ground, turn.model).await?, None),
    (Some(_), None) => {
      let node = super::standing::place_registered(ground, turn.model).await?;

      (node.clone(), node)
    }
  };

  if let Some(node) = &placement
    && let Some(registration) = registration
    && !registration.holds_node(node)
  {
    return Err(fault(fill_marks(
      &REGISTERED.presence.node_missing,
      &[("name", &registration.name), ("node", node)],
    )));
  }

  let sampled = (!sampling.is_empty())
    .then(|| serde_json::to_string(&sampling))
    .transpose()
    .map_err(|why| fault(why.to_string()))?;
  let model = gateway;
  let member_thinking = reasoning.thinking.unwrap_or(model.thinking);
  let claude_effort = model.road.and_then(|road| {
    harness_providers::provider(road).and_then(|provider| provider.claude_effort(&effort))
  });
  let on = OnGateway {
    agent: turn.agent,
    model: turn.model,
    effort: &effort,
    cwd: &cwd,
    settings: serde_json::Map::new(),
    holds: holds(),
    sampling: sampled,
    role: None,
    card_terms: harness_machine::card_master::CardTerms {
      rank: crate::cards::supervisor_rank(&ground.db, turn.agent).await,
      pool: None,
    },
    thinking: model.road.map(|_| member_thinking),
    claude_effort,
    backend_effort: model.road.map(|_| effort.as_str()),
  };
  let (mut command, _) = gateway_command(
    ground,
    machine,
    &model,
    on,
    ceiling,
    Placing::At(placement),
    |name, effort, _, settings| line(name, effort, None, settings),
  )
  .await?;

  command.env.extend(own);
  command.env.extend(variables.map(|(name, value)| (name.to_owned(), value)));

  Ok(ComposedTurn {
    system_path: system,
    command,
    session: session.id().to_owned(),
    prompt: turn.prompt.to_owned(),
    model: turn.model.to_owned(),
    effort: (!effort.is_empty()).then_some(effort),
    placement: kept,
    window: Some(Window {
      context_tokens: model.context_tokens,
      home: model.home,
    }),
    account: None,
    began: turn.began,
    run_env: None,
    given: None,
  })
}
