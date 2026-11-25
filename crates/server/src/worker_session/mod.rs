//! One turn on a worker's own session (docs/agent-control.md, 「The keepalive ping goes into the same
//! program as the turns」): run on the line a job runs on, in the resident program a job's turn goes
//! into, through Claude Code's transcript door. A word said to the worker and the keepalive's ping are
//! the same run with something else said, so the login it goes out on, the session it is judged by, what
//! the record says afterwards, the downgrade heal and the fold are settled here once, and the answer is
//! handed back for the caller to use.

pub mod fold;
pub mod held;
pub mod records;
pub mod told;
pub mod warmth;

use std::collections::BTreeMap;
use std::time::Duration;

use policy::keepalive::KEEPALIVE;

use self::held::{SessionRun, prepare, remember};
use self::records::{
  WorkerSession, fallen_model, heal, launch_model, note_carried, note_keepalive, note_usage, record,
};
use crate::control::claude_accounts::{self, Account};
use crate::control::harness::provider_enabled;
use crate::door::execution::Identity;
use crate::door::record::Reply;
use crate::door::surface::interrupt_of;
use crate::door::turn::{Ending, Seat, Timings, Turn, Watch, pasted_lead, put};
use crate::lines::claude::{self, Command, Session};
use crate::lines::place::{Holds, Place, Standby, read_settings, write_place_settings};
use crate::lines::standing::{status_root, worker_prefix_digest};
use crate::lines::worker::{LINE, WorkerRun, configured_pinned, worker_run};
use crate::lines::{Trees, fault};
use crate::places::{AgentRow, Ground};
use crate::{Refusal, now_ms};

/// The provider a worker's own session stands on.
pub const PROVIDER: &str = "claude-code";

/// The words the program says when it is asked to resume a session it no longer holds.
pub const UNKNOWN_SESSION: &str = "No conversation found with session ID";

/// The words the program says when the session it was asked to resume cannot be continued: the record
/// stands, but the state it ends on is one the program will not take up.
pub const STALE_SESSION: &str = "No deferred tool marker found in the resumed session";

/// Whether an error line says the session the turn named is gone or cannot be continued. Either way the
/// record goes and the next turn opens a fresh session.
pub fn unknown_session(said: &str) -> bool {
  said.contains(UNKNOWN_SESSION) || said.contains(STALE_SESSION)
}

/// What kept a turn from the session: a question a person has to answer, the folder-trust question that
/// would not clear, a prompt that never became a turn, a stop, Claude Code switched off, a session the
/// program no longer holds, or a ping with no prefix to warm, which is never put in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Missed {
  Person,
  Untrusted,
  Unsubmitted,
  Stopped,
  Disabled,
  Lost,
  Idle,
  /// No login that may carry the turn answers now: every one is off, at its limit or waiting for a person.
  Resting,
}

/// What one turn on the session left: whether its words reached the session at all, which is not the
/// same as whether anything was answered, and the answer where there was one.
#[derive(Debug)]
pub struct Turned {
  pub reached: bool,
  pub reply: Option<Reply>,
  pub missed: Option<Missed>,
}

impl Turned {
  fn missed(missed: Missed) -> Self {
    Self {
      reached: false,
      reply: None,
      missed: Some(missed),
    }
  }

  fn reached(reply: Option<Reply>) -> Self {
    Self {
      reached: true,
      reply,
      missed: None,
    }
  }
}

/// One turn as its caller names it: the worker, the model and session it goes on, what is said, the
/// pane's run name with what a person watching herdr reads above it, the word the machine's log lines
/// call it by, and how long it goes before its caller is asked about it, none where its caller asks nothing.
pub struct Asked<'a> {
  pub worker: &'a AgentRow,
  pub model: String,
  pub session: Option<String>,
  pub prompt: String,
  pub name: String,
  pub title: String,
  pub label: String,
  pub clock: Option<Duration>,
}

/// What a session turn stands on: the ground, whose bus a downgrade and the turn's usage are published on.
pub struct Side<'a> {
  pub ground: &'a Ground,
}

// The registered account of a login by its id; none for the login this machine already had.
pub async fn account_named(ground: &Ground, id: &str) -> Result<Option<Account>, Refusal> {
  Ok(
    claude_accounts::accounts(&ground.db)
      .await
      .map_err(|refused| fault(refused.error))?
      .into_iter()
      .find(|account| account.id == id),
  )
}

// Whether logins are registered to carry the worker line and none of them answers now, which a turn
// is never put out on the machine's own login in place of.
async fn no_login_answers(ground: &Ground) -> Result<bool, Refusal> {
  let carried = claude_accounts::carried_for_user(&ground.db, None)
    .await
    .map_err(|refused| fault(refused.error))?;

  Ok(!carried.is_empty())
}

/// The login a job's walk offers now on the worker line.
pub async fn walked_account(ground: &Ground) -> Result<Option<Account>, Refusal> {
  Ok(
    claude_accounts::walk_for(claude_accounts::Judging::of(ground))
      .await
      .map_err(|refused| fault(refused.error))?
      .into_iter()
      .next(),
  )
}

/// The turn put into the worker's resident program: its line as the program's command with the place's
/// settings, on the session it continues or the one it opens — the one the standing program opened
/// where nothing has been said in it yet — and the pane's environment by name.
pub async fn compose(
  ground: &Ground,
  row: &AgentRow,
  line: &WorkerRun,
  account: Option<&Account>,
  resume: Option<&str>,
  turn: Composed,
) -> Result<Turn, Refusal> {
  let home = ground
    .home_of_row(row)
    .await?
    .ok_or_else(|| fault("this machine names no herdr for the worker's place"))?;

  compose_at(ground, &row.name, home, line, account, resume, turn).await
}

/// The turn on the worker's line put into the resident program of the agent named, standing in the home
/// given: the worker's own, or a standing branch checkout's, which is an agent of its own.
pub async fn compose_at(
  ground: &Ground,
  agent: &str,
  home: crate::places::home::Home,
  line: &WorkerRun,
  account: Option<&Account>,
  resume: Option<&str>,
  turn: Composed,
) -> Result<Turn, Refusal> {
  let trees = Trees::of(ground);
  let place = Place::of(&trees.data, agent);
  let settings = write_place_settings(
    &place.path,
    read_settings(&line.settings),
    &status_root(ground),
    Holds::of_line(LINE),
  )?;
  let recorded =
    account.map_or_else(|| claude_accounts::SEED_ACCOUNT.to_owned(), |account| account.id.clone());
  let session = match resume {
    Some(id) => Session::Resume(id.to_owned()),
    None => Session::Open(
      Standby::opening(&place.path, &line.model, &recorded)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
    ),
  };

  turn_on(ground, Seat::Standing(home), line, &settings, account, &session, turn)
}

/// The turn on the worker's line in a pane of its own, on a session it opens and throws away: a run that is
/// nobody's standing session, such as a co-work branch outside every standing checkout, which goes out with
/// the line's own settings.
pub fn compose_own(
  ground: &Ground,
  herdr: herdr::Herdr,
  line: &WorkerRun,
  account: Option<&Account>,
  turn: Composed,
) -> Result<Turn, Refusal> {
  let session = Session::Open(uuid::Uuid::new_v4().to_string());

  turn_on(ground, Seat::Own(herdr), line, &line.settings, account, &session, turn)
}

// The turn on the worker's line, put where the seat says, with the settings given and on the session named.
fn turn_on(
  ground: &Ground,
  seat: Seat,
  line: &WorkerRun,
  settings: &std::path::Path,
  account: Option<&Account>,
  session: &Session,
  turn: Composed,
) -> Result<Turn, Refusal> {
  // A pane of the run's own stands here; a standing program goes out from wherever its place stands.
  let runs = match &seat {
    Seat::Standing(home) => {
      claude::Runs::of_place(&ground.roots.data, &Place::of(&ground.roots.data, &home.agent).path)
    }
    Seat::Own(_) => claude::Runs::here(ground),
  };
  let command = Command {
    args: claude::worker_args(line, settings, Some(session))?,
    cwd: line.cwd.clone(),
    env: claude::command_env(ground.env(), account, &runs, line.thinking)?,
  };
  let env: BTreeMap<String, String> = claude::line_environment(&command, &turn.name, account);
  let resident = harness_providers::provider(PROVIDER)
    .and_then(|declared| declared.declared().resident.as_ref())
    .ok_or_else(|| fault("Claude Code stands no resident program"))?;

  Ok(Turn {
    name: turn.name,
    title: turn.title,
    seat,
    cwd: command.cwd.clone(),
    command: command.args,
    env,
    prompt: turn.prompt,
    session: session.id().to_owned(),
    identity: Identity {
      agent: None,
      root: None,
      parent: None,
      role: Some(crate::agent_roles::job_role().to_owned()),
      model: Some(line.model.clone()),
      effort: (!line.effort.is_empty()).then(|| line.effort.clone()),
    },
    line: LINE.to_owned(),
    provider: PROVIDER.to_owned(),
    lead: pasted_lead(),
    // A worker's place is the directory the owner named for it or a branch place the bot made, and a
    // line told to ask no permission there trusts the folder by the same decision.
    trusts_folder: true,
    clock: turn.clock,
    interrupt: interrupt_of(resident),
    timings: Timings::default(),
  })
}

// Whether a ping on this run warms anything: Claude Code on and the road the worker's next job would take
// — the road resolution's answer for it (docs/cache-fallback.md, 「One road resolution」) — Claude Code's,
// and the session held, where one is, standing on this run's provider, model and effort.
async fn idle_turn_stands(
  ground: &Ground,
  worker: &str,
  held: Option<&held::Held>,
  line: &WorkerRun,
) -> Result<bool, Refusal> {
  let db = &ground.db;

  if !provider_enabled(db, PROVIDER).await.map_err(|refused| fault(refused.error))? {
    return Ok(false);
  }

  let resolution =
    crate::road::for_job(ground, None, Some(worker), None, &[], None).await.map_err(fault)?;

  if resolution.road().is_none_or(|taken| taken.provider != PROVIDER) {
    return Ok(false);
  }

  Ok(
    held.is_none_or(|held| held.mode == PROVIDER && held.model == line.model && held.effort == line.effort),
  )
}

/// What a composed turn is beside its line: its run name, its title, its words and its clock.
pub struct Composed {
  pub name: String,
  pub title: String,
  pub prompt: String,
  pub clock: Option<Duration>,
}

/// What an ending that is no answer says: the program's own error, why its program was lost, or the screen
/// it stood at, which a judgement reads and nobody is told.
pub fn evidence(ending: &Ending) -> &str {
  match ending {
    Ending::Errored { error, .. } => error,
    Ending::ProgramLost { why, .. } | Ending::SilentPeer { why, .. } => why,
    Ending::Blocked { screen }
    | Ending::Untrusted { screen, .. }
    | Ending::Vanished { screen }
    | Ending::Overran { screen }
    | Ending::Silent { screen }
    | Ending::Unsubmitted { screen, .. }
    | Ending::Stopped { screen } => screen,
    Ending::Answered { .. } => "",
  }
}

fn failure_line(ending: &Ending) -> String {
  if unknown_session(evidence(ending)) {
    return "the CLI cannot continue the session".to_owned();
  }

  match ending {
    Ending::Overran { .. } => "the run was cut on its caller's verdict".to_owned(),
    Ending::Silent { .. } => "the run wrote no answer to its record".to_owned(),
    Ending::Errored { error, .. } => format!("the CLI answered with an error: {error}"),
    _ => "the pane went away before it answered".to_owned(),
  }
}

/// One turn on the worker's own session, on the line a job runs on and in the program a job's turn goes
/// into. A turn holds open the session it names, so it goes out on the login that session was opened
/// under; one bringing a session up goes out on the login a job's walk offers now, named outright, which
/// is the login the job resuming it goes out on. A session held under a login that cannot answer now is
/// not resumed, since no other login can reach it: the turn brings a fresh one up on the login the walk
/// offers, and where the walk offers none, nothing is put.
pub async fn turn<W: Watch>(side: &Side<'_>, asked: Asked<'_>, watch: &mut W) -> Result<Turned, Refusal> {
  let ground = side.ground;
  let worker = &asked.worker.name;
  let label = &asked.label;
  let model = &asked.model;
  let cwd = ground.checkout_of(asked.worker);
  let trees = Trees::of(ground);
  let held: Option<WorkerSession> = match &asked.session {
    None => None,
    Some(id) => records::sessions(&ground.db, worker)
      .await?
      .into_iter()
      .find(|one| &one.session_id == id),
  };
  let closed = match &held {
    Some(held) => {
      !claude_accounts::account_open(claude_accounts::Judging::of(ground), &held.account, now_ms())
        .await
        .map_err(|refused| fault(refused.error))?
    }
    None => false,
  };
  let (held, resume) = if closed { (None, None) } else { (held, asked.session.clone()) };
  let account = match &held {
    None => walked_account(ground).await?,
    Some(held) => account_named(ground, &held.account).await?,
  };

  if account.is_none() && no_login_answers(ground).await? {
    eprintln!("{label} {worker} {model}: no login answers now, so nothing was put");

    return Ok(Turned::missed(Missed::Resting));
  }

  let machine = ground.machine();
  // The model the worker's pin names on Claude Code is the one its turn goes out on (docs/model-tiers.md,
  // 「An agent's own model」), the job key's mapping there otherwise.
  let line = configured_pinned(
    ground,
    &machine,
    worker_run(ground, &trees, Some(worker), account.as_ref(), model, &cwd).await?,
    worker,
    PROVIDER,
  )
  .await?;
  let configured_model = line.model.clone();
  let mut launched = line;

  // A kept session the safeguard refused is turned on the model that answered it; a session the
  // preparation replaced is a fresh one and goes out on its own model (docs/cache-fallback.md).
  let held_road = held::held(ground, &held::scope_of(worker)).await?;

  launched.model = launch_model(
    &ground.db,
    worker,
    held.as_ref().and_then(|held| held.answered_model.as_deref()),
    launched.model,
  )
  .await?;

  // A ping is worth a turn only where it reads a prefix again: not on a road this worker's key is set
  // away from, and not where the session held stands on another road than this run.
  if asked.prompt == KEEPALIVE.prompt
    && !idle_turn_stands(ground, worker, held_road.as_ref(), &launched).await?
  {
    return Ok(Turned::missed(Missed::Idle));
  }

  let mut run = prepare(
    ground,
    worker,
    SessionRun {
      mode: held::MODE,
      line: launched,
      prompt: asked.prompt,
      resume: resume.clone(),
      account,
      placement: None,
    },
  )
  .await?;

  if run.resume.is_none() {
    run.line.model.clone_from(&configured_model);
  }

  if !provider_enabled(&ground.db, PROVIDER).await.map_err(|refused| fault(refused.error))? {
    return Ok(Turned::missed(Missed::Disabled));
  }

  let turn = compose(
    ground,
    asked.worker,
    &run.line,
    run.account.as_ref(),
    run.resume.as_deref(),
    Composed {
      name: asked.name,
      title: asked.title,
      prompt: run.prompt.clone(),
      clock: asked.clock,
    },
  )
  .await?;
  let ending = put(ground, &turn, watch).await.map_err(fault)?;
  let account = run.recorded_account();

  note_keepalive(&ground.db, worker).await?;

  let reply = match ending {
    Ending::Answered { reply, .. } => *reply,
    Ending::Blocked { .. } => {
      eprintln!("{label} {worker} {model}: the run is waiting on a person, so nothing was warmed");

      return Ok(Turned::missed(Missed::Person));
    }
    Ending::Untrusted { folder, config, .. } => {
      eprintln!(
        "{label} {worker} {model}: the program would not get past the folder-trust question in {folder} (trusted in {config}), so nothing was warmed"
      );

      return Ok(Turned::missed(Missed::Untrusted));
    }
    Ending::Unsubmitted { .. } => {
      eprintln!("{label} {worker} {model}: the prompt never became a turn, so nothing was warmed");

      return Ok(Turned::missed(Missed::Unsubmitted));
    }
    Ending::Stopped { .. } => {
      eprintln!("{label} {worker} {model}: the run was stopped while it ran, so nothing was warmed");

      return Ok(Turned::missed(Missed::Stopped));
    }
    other => {
      // A session the turn could not reach is not one to resume: the record goes, and the next job on
      // this model starts a fresh session.
      if resume.is_some() {
        records::forget(&ground.db, worker, model, Some(&account)).await?;
      }

      eprintln!("{label} {worker} {model}: {}", failure_line(&other));

      // A session the program no longer holds or cannot continue is one the words never reached; anything
      // else came apart after they went in, and saying them again would repeat the work.
      return Ok(if unknown_session(evidence(&other)) {
        Turned::missed(Missed::Lost)
      } else {
        Turned::reached(None)
      });
    }
  };

  let Some(session_id) = reply.session_id.clone() else {
    eprintln!("{label} {worker} {model}: the run named no session, so there is none to hold open");

    return Ok(Turned::reached(Some(reply)));
  };

  remember(ground, worker, &run, &reply).await?;
  note_usage(&ground.db, &ground.windows, &ground.events, worker, &reply).await?;

  let fell = heal(ground, worker, model, &run.line.model, &account, &reply.spend, None).await?;

  if fell.dropped {
    println!("{label} {worker} {model}: dropped {session_id}, the next job opens a fresh one");

    return Ok(Turned::reached(Some(reply)));
  }

  let digest = worker_prefix_digest(ground, &trees, &cwd, &run.line.model, Some(&run.line.effort)).await?;
  let fallen = fallen_model(&configured_model, &run.line.model, &fell.models);

  record(&ground.db, worker, model, &account, &session_id, Some(&digest), fallen.as_deref()).await?;
  note_carried(&ground.db, worker, model, &account, run.resume.is_none(), &reply).await?;
  println!(
    "{label} {worker} {model}: {} {session_id}",
    if run.resume.is_none() { "brought up" } else { "resumed" }
  );

  let fold_ratio = fold::worker_fold_ratio(ground, &reply.mode, None, Some(&cwd)).await;

  if fold::worker_session_full(reply.context_ratio, fold_ratio) {
    println!("{label} {worker} {model}: {session_id} is full, folding it");
    fold::fold(ground, asked.worker, model, &account, &session_id).await;
  }

  Ok(Turned::reached(Some(reply)))
}
