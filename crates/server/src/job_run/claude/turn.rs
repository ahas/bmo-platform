//! A job's turn on its worker's own session (docs/api.md, 「Sessions」, a work turn's parts): carried by the
//! logins its work key is offered for its user, each tried in turn where one fails on the login rather
//! than on the work; on the model, the mapping and the session its runner named, behind the agent lock the
//! worker's jobs share, with the owner's waiting reset carried out, the standby claimed and the held session
//! judged before it goes; in its folder, under its run name and title, its variables beside the line's
//! own; and read back as the whole of its end — the reply, or the failure by its kind — with what it went
//! out on, where its time went and whether its session was compacted.
//!
//! Which model, which session, which login and which folder a job's turn needs is its runner's: this
//! carries that choice through the worker's records, the held session's judgement and the door, and
//! judges none of it. Where the session the turn settles on is not the one its runner composed its prompt
//! for, nothing goes in and the turn goes back to its runner with the session it would continue.
//!
//! A thrown-away session's turn — a co-work branch's or a runner line's — is the same turn on a session no
//! worker keeps: on the provider, model and folder its scope names, with nothing of a worker's records read,
//! judged, recorded or remembered for it, in the standing branch checkout its folder is or in a pane of its
//! own; a branch's opens a session of its own, and a runner line's the one its agent's role gives it. One whose scope hands in a runner line's instructions goes out on that line
//! instead of the worker line, whole, in a pane of its own, its variables the run's record variables
//! (docs/api.md, 「sessions」).

use std::time::Duration;

use agent_session::{
  self as contract, Apart, EngineError, Fault, FaultKind, JobMark, Outcome, Resume, SessionId, TurnEnd,
  WentOut, WorkParts,
};
use sea_orm::DatabaseConnection;
use serde_json::Value;

use super::failure::{AccountFailure, Failure, RunEnding, of_unanswered};
use super::renewal::Renewing;
use super::reset::carry_out_worker_reset;
use super::walk::{Attempt, PassedOver, Walking, on_accounts};
use crate::Refusal;
use crate::control::claude_accounts::Account;
use crate::control::harness::provider_enabled;
use crate::control::work::{add_road, continued};
use crate::door::apart::{self, new_session};
use crate::door::confirm::{Asking, Confirmation};
use crate::door::execution::{cancelled, execution_root};
use crate::door::record::Reply;
use crate::door::stall::StallChange;
use crate::door::tool_clock::{CallVerdict, ClockNote, SilentCall, ToolClock};
use crate::door::turn::{Ending, Overrun, Overtime, Turn, Watch, put};
use crate::job_stall::JobStall;
use crate::keepalive::logins;
use crate::lines::Trees;
use crate::lines::claude::{self, Command, Runs, Session, command_env};
use crate::lines::place::Place;
use crate::lines::runner::{RunnerRun, given_run};
use crate::lines::standing::{claim_standby, session_ttl_ms, worker_prefix_digest};
use crate::lines::worker::{LINE, WorkerRun, effort_of, role_run, worker_run};
use crate::places::branch::{self, BranchRun};
use crate::places::home::Home;
use crate::places::{AgentRow, Ground, locks};
use crate::queue::orderer::JobService;
use crate::queue::stops::job_stop_reason;
use crate::road::Road;
use crate::run_clock::CallAsks;
use crate::run_log::lines::Line;
use crate::run_log::spans::JobSpan;
use crate::run_log::{RunLog, Target};
use crate::session_questions::{Questions, TurnQuestion};
use crate::worker_session::fold::{fold, worker_fold_ratio, worker_session_full};
use crate::worker_session::held::{self, SessionRun};
use crate::worker_session::records::{self, fallen_model, heal, launch_model, note_carried, record};
use crate::worker_session::{
  Composed, PROVIDER, compose, compose_at, compose_own, evidence, unknown_session,
};

/// The variable the agent a turn's program is a turn of goes by, so what the verbs it runs file stands
/// under that agent.
pub const TURN_AGENT: &str = "BMO_TURN_AGENT";

/// The variable that tells the verbs a turn's program runs that they work for the turn's job.
pub const TASK_LOCKED: &str = "BMO_TASK_LOCKED";

/// A job's turn as its runner asks it: the agent it runs as, the session it goes on, its work parts, the
/// job it is of, and its words.
pub struct WorkAsked<'a> {
  pub ground: &'a Ground,
  pub agent: &'a str,
  pub seat: WorkSeat<'a>,
  pub work: &'a WorkParts,
  pub job: Option<JobMark>,
  pub prompt: &'a str,
  /// The session the turn goes on, which a question the turn stops on is put to the caller under.
  pub session: &'a SessionId,
  pub questions: &'a Questions,
}

/// The session a job's turn goes on: the worker's own, by its row, or one thrown away, by its scope.
#[derive(Clone, Copy)]
pub enum WorkSeat<'a> {
  Worker(&'a AgentRow),
  Apart(&'a Apart),
}

/// Where a thrown-away session's turn goes: the standing branch checkout its folder is, under the co-work
/// task's label, or a pane of its own on this machine's herdr.
#[derive(Clone, Debug)]
pub enum ApartSeat {
  Standing(Home),
  Own(herdr::Herdr),
}

/// Where a thrown-away session's turn goes, by the folder its scope names, a session naming its prompt
/// document always in a pane of its own; none where it names no standing checkout and this machine names
/// no herdr to open a pane on.
pub fn apart_seat(ground: &Ground, apart: &Apart) -> Option<ApartSeat> {
  if apart.on_runner_line() {
    return ground.herdr.clone().map(ApartSeat::Own);
  }

  let run = match (apart.todo, &apart.role) {
    (Some(todo), Some(role)) => Some(BranchRun {
      todo,
      role: role.clone(),
    }),
    _ => None,
  };

  match branch::home(ground, &apart.cwd, run.as_ref()) {
    Some(home) => Some(ApartSeat::Standing(home)),
    None if branch::place_of(&ground.roots.data, &apart.cwd).is_some() => None,
    None => ground.herdr.clone().map(ApartSeat::Own),
  }
}

/// The runner line a thrown-away session on a runner line goes out on, as one login carries it on the
/// session given: the line it names with the instructions its caller handed in, on the scope's model and
/// the effort and tier the turn's mapping names, in the scope's folder or else the line's own, with the
/// turn's variables and the record the scope names in its environment, where the record its tool session is
/// issued reads them.
pub async fn runner_command(
  ground: &Ground,
  apart: &Apart,
  work: &WorkParts,
  account: Option<&Account>,
  session: &str,
) -> Result<(RunnerRun, Command), Refusal> {
  let trees = Trees::of(ground);
  let effort = work.mapping.effort.as_deref().unwrap_or_default();
  let tier = work.mapping.tier.as_deref().unwrap_or_default();
  let (Some(line), Some(instructions)) = (&apart.line, &apart.system_prompt) else {
    return Err(crate::lines::fault("a session on the worker line names no runner line"));
  };
  let run = given_run(ground, &trees, line, instructions, (apart.model.as_str(), effort, tier)).await?;
  let mut env = command_env(ground.env(), account, &Runs::here(ground), run.thinking)?;

  for (name, value) in
    work.variables.clone().into_iter().chain(crate::lines::token::record_variables(apart))
  {
    env.insert(name, Some(value));
  }

  let cwd = match apart.cwd.trim() {
    "" => run.cwd.to_string_lossy().into_owned(),
    cwd => cwd.to_owned(),
  };
  let command = Command {
    args: run.args(session)?,
    cwd,
    env,
  };

  Ok((run, command))
}

/// The worker line's command a role's run goes out on at its scope's folder, as one login carries it on the
/// session given (docs/agents.md, 「A run in a pane of its own」): the platform's harness opening and the
/// role's own prompt, the line's standing rules and, where the folder is a repository's checkout, the
/// repository's own; on the scope's model at the effort the turn's mapping names; with the turn's
/// variables, the role and the record its scope names, the call clock's MCP idle window, and the job's mark
/// where it is a job's, in its environment.
#[allow(clippy::too_many_arguments)]
pub async fn role_command(
  ground: &Ground,
  agent: &str,
  apart: &Apart,
  role: &str,
  work: &WorkParts,
  account: Option<&Account>,
  session: &str,
  job: bool,
) -> Result<(WorkerRun, Command), Refusal> {
  let trees = Trees::of(ground);
  let machine = ground.machine();
  let checkout = crate::queue::workers::checkout_at(&ground.db, &apart.cwd).await?;
  let mut line =
    role_run(ground, &trees, role, checkout.as_deref(), account, &apart.model, &apart.cwd, PROVIDER)
      .await?;

  line.effort = effort_of(
    ground,
    &machine,
    PROVIDER,
    &apart.model,
    work.mapping.effort.as_deref(),
    line.reasoning.as_ref(),
  )
  .await?
  .unwrap_or_default();
  line.model.clone_from(&apart.model);
  line.tier.clone_from(&work.mapping.tier);

  let mut env = command_env(ground.env(), account, &Runs::here(ground), line.thinking)?;

  for (name, value) in
    work.variables.clone().into_iter().chain(crate::lines::token::record_variables(apart))
  {
    env.insert(name, Some(value));
  }

  env.insert(TURN_AGENT.to_owned(), Some(agent.to_owned()));

  if job {
    env.insert(TASK_LOCKED.to_owned(), Some("1".to_owned()));
  }

  let (idle, window) = mcp_idle_window();

  env.entry(idle).or_insert(Some(window));

  let command = Command {
    args: claude::worker_args(&line, &line.settings, Some(&Session::Open(session.to_owned())))?,
    cwd: line.cwd.clone(),
    env,
  };

  Ok((line, command))
}

/// Which session an attempt goes on, or that the turn goes back to its runner with the session it would
/// continue, none for a fresh one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
  Go(Option<String>),
  Back(Option<String>),
}

/// The session a turn's `resume` goes on, given the one the worker keeps for the turn's model and the login
/// carrying it: under `auto` the kept one, under `fresh` none, and under `session` the one named where it
/// is the kept one; a session named that the login carrying the turn does not keep is not the one the
/// turn's prompt was composed for.
pub fn choose(resume: &Resume, kept: Option<String>) -> Choice {
  match resume {
    Resume::Auto => Choice::Go(kept),
    Resume::Fresh => Choice::Go(None),
    Resume::Session { id } if kept.as_deref() == Some(id.as_str()) => Choice::Go(kept),
    Resume::Session { .. } => Choice::Back(kept),
  }
}

/// What a turn that did not answer on a resumed session comes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unanswered {
  /// The program no longer held the session: under `auto`, asked again once on a fresh one.
  Again,
  /// The program no longer held the session the runner named: back to the runner for a fresh one.
  Back,
  /// Read as a failure.
  Failed,
}

/// What a turn that left no answer comes to, by the session it went on and what it left.
pub fn after_unanswered(resume: &Resume, went_on: Option<&str>, evidence: &str) -> Unanswered {
  if went_on.is_none() || !unknown_session(evidence) {
    return Unanswered::Failed;
  }

  match resume {
    Resume::Session { .. } => Unanswered::Back,
    Resume::Auto | Resume::Fresh => Unanswered::Again,
  }
}

/// The composed turn handed what its runner named beyond the line: its variables beside the line's own,
/// the agent it is a turn of, the job marker where it has a job and none otherwise, the MCP idle window of
/// the call clock, its work key, and the unit its execution is a child of and counted under, so a stop of
/// the unit reaches it.
pub fn with_work(mut turn: Turn, agent: &str, work: &WorkParts, job: Option<JobMark>) -> Turn {
  for (name, value) in &work.variables {
    turn.env.entry(name.clone()).or_insert_with(|| value.clone());
  }

  turn.env.insert(TURN_AGENT.to_owned(), agent.to_owned());

  if job.is_some() {
    turn.env.insert(TASK_LOCKED.to_owned(), "1".to_owned());
  } else {
    turn.env.remove(TASK_LOCKED);
  }

  let (name, window) = mcp_idle_window();

  turn.env.entry(name).or_insert(window);

  if let Some(unit) = &work.unit {
    turn.identity.parent = Some(unit.clone());
    turn.identity.root = Some(execution_root(unit));
  }

  turn
}

/// The variable the program reads its MCP idle window from, and the window a job's turn and a role's run
/// give it. A call that waits inside the program on a socket sends no response; the window is the call
/// clock's in-process bound, and at it the program aborts the call and hands the model a failed result
/// (docs/agent-control.md, 「A run's tool calls are held to a clock」). A value the caller set stands.
pub fn mcp_idle_window() -> (String, String) {
  use policy::providers::claude_code::CLAUDE_CODE;
  use policy::tool_clock::TOOL_CLOCK;

  let seconds = TOOL_CLOCK.call.in_process_seconds.max(TOOL_CLOCK.call.min_seconds);

  (CLAUDE_CODE.launch.mcp_idle_timeout.clone(), seconds.saturating_mul(1_000).to_string())
}

/// Whether any of the record's lines is the program compacting its session.
pub fn compacts(lines: &[String]) -> bool {
  lines.iter().any(|line| {
    serde_json::from_str::<Value>(line).is_ok_and(|event| {
      event.get("type").and_then(Value::as_str) == Some("system")
        && event.get("subtype").and_then(Value::as_str) == Some("compact_boundary")
    })
  })
}

/// What a line went out on, as a turn's end names it.
pub fn went_out_of(line: &WorkerRun) -> WentOut {
  WentOut {
    provider: PROVIDER.to_owned(),
    model: line.model.clone(),
    effort: (!line.effort.is_empty()).then(|| line.effort.clone()),
    tier: line.tier.clone(),
  }
}

/// A reply as the session contract carries it. The Claude Code line goes to no upstream of its own.
pub fn reply_of(reply: &Reply) -> contract::Reply {
  contract::Reply {
    mode: reply.mode.clone(),
    model: reply.model.clone(),
    effort: reply.effort.clone(),
    text: reply.text.clone(),
    session_id: reply.session_id.clone(),
    account: reply.account.clone(),
    context_ratio: reply.context_ratio,
    context_tokens: reply.context_tokens,
    session_usage: reply.session_usage.as_ref().and_then(|usage| serde_json::to_value(usage).ok()),
    spend: reply
      .spend
      .iter()
      .map(|spent| contract::Spend {
        mode: (!spent.mode.is_empty()).then(|| spent.mode.clone()),
        model: spent.model.clone(),
        tokens: spent.tokens,
        input_tokens: spent.input_tokens,
        output_tokens: spent.output_tokens,
        cache_read_tokens: spent.cache_read_tokens,
        cache_write_tokens: spent.cache_write_tokens,
        cost: Some(spent.cost),
      })
      .collect(),
    live_tasks: reply
      .live_tasks
      .iter()
      .map(|task| contract::LiveTask {
        id: task.id.clone(),
        description: task.description.clone(),
      })
      .collect(),
    workflows: reply
      .workflows
      .iter()
      .map(|workflow| contract::Workflow {
        call: workflow.call.clone(),
        name: workflow.name.clone(),
        status: workflow.status.clone(),
      })
      .collect(),
    upstream: None,
    request_usages: reply.request_usages.as_ref().map(|usages| {
      usages
        .iter()
        .map(|usage| contract::RequestUsage {
          input_tokens: usage.input_tokens,
          output_tokens: usage.output_tokens,
          cache_read_tokens: usage.cache_read_tokens,
          cache_write_tokens: usage.cache_write_tokens,
        })
        .collect()
    }),
  }
}

/// A failed turn as the session contract carries it. A walk's own endings, which a single turn never
/// reaches, are read as errors by their words.
pub fn failure_of(failure: &Failure) -> contract::Failure {
  match failure {
    Failure::Limit {
      resets_at,
      account,
      silenced,
      model,
      ..
    } => contract::Failure::Limit {
      resets_at: *resets_at,
      account: account.clone(),
      silenced: *silenced,
      model: model.clone(),
    },
    Failure::Account { failure, detail } => contract::Failure::Account {
      failure: match failure {
        AccountFailure::Limit => contract::AccountFailure::Limit,
        AccountFailure::Login => contract::AccountFailure::Login,
        AccountFailure::Outage => contract::AccountFailure::Outage,
        AccountFailure::NodeToken => contract::AccountFailure::NodeToken,
      },
      detail: detail.clone(),
    },
    Failure::Died {
      ending,
      said,
      message,
    } => contract::Failure::Died {
      ending: match ending {
        RunEnding::Exit { code } => contract::RunEnding::Exit { code: *code },
      },
      said: said.clone(),
      message: message.clone(),
    },
    Failure::Person => contract::Failure::Person,
    Failure::Untrusted { folder, config } => contract::Failure::Untrusted {
      folder: folder.clone(),
      config: config.clone(),
    },
    Failure::Stopped => contract::Failure::Stopped,
    Failure::Transport { message } => contract::Failure::Transport {
      message: message.clone(),
    },
    Failure::Refused { message } => contract::Failure::Refused {
      message: message.clone(),
    },
    Failure::Unavailable {
      scope,
      mode,
      reason,
    } => contract::Failure::Unavailable {
      scope: scope.clone(),
      mode: mode.clone(),
      reason: reason.clone(),
    },
    Failure::Error { name, message } => contract::Failure::Error {
      name: name.clone(),
      message: message.clone(),
    },
    Failure::Attempt | Failure::Exhausted { .. } => contract::Failure::Error {
      name: "Error".to_owned(),
      message: failure.message(),
    },
  }
}

/// Where one part of a turn's time went, as the session contract carries it.
pub fn span_of(span: &JobSpan) -> contract::Span {
  contract::Span {
    kind: span.kind.clone(),
    name: span.name.clone(),
    seconds: span.seconds,
    times: span.times,
    tokens: span.tokens,
    input_tokens: span.input_tokens,
    output_tokens: span.output_tokens,
    cache_read_tokens: span.cache_read_tokens,
    cache_write_tokens: span.cache_write_tokens,
  }
}

/// What a job's turn shows of itself while it goes, to whoever reads its session: what each attempt went
/// out on, and the words its program writes of its answer, an attempt's going out beginning them again.
pub trait Shows: Sync {
  fn went_out(&self, went_out: &WentOut);

  fn wrote(&self, texts: &[String]);
}

/// Every text block the program's answers wrote in the record's lines, in order: what a turn stopped from
/// outside had written.
pub fn assistant_texts(lines: &[String]) -> Vec<String> {
  lines
    .iter()
    .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    .filter(|event| event.get("type").and_then(Value::as_str) == Some("assistant"))
    .flat_map(|event| {
      event.pointer("/message/content").and_then(Value::as_array).cloned().unwrap_or_default()
    })
    .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
    .filter_map(|block| block.get("text").and_then(Value::as_str).map(str::to_owned))
    .collect()
}

/// What a carried turn settled on: its answer, or its going back to its runner with the session it would
/// continue.
#[derive(Debug)]
pub enum Settled {
  Answered(Box<Reply>),
  Back(Option<String>),
}

/// What a turn came to beside its answer or its failure: what it went out on, where its time went, whether
/// its session was compacted, and the error its engine ended it with where it ended with one.
#[derive(Debug, Default)]
pub struct Beside {
  pub went_out: Option<WentOut>,
  pub spans: Vec<contract::Span>,
  pub compacted: bool,
  pub engine_error: Option<EngineError>,
  /// The key of the command the turn went out under, where its line reads one.
  pub prefix: Option<String>,
}

/// A carried turn's end under its run: an answer, a turn back to its runner, a person waited on, a stop,
/// or a failure with its words and its kind.
pub fn ended(run: &str, walked: Result<Settled, Failure>, beside: Beside) -> TurnEnd {
  let end = match walked {
    Ok(Settled::Answered(reply)) => TurnEnd {
      reply: Some(Box::new(reply_of(&reply))),
      ..TurnEnd::answered(run, reply.text.clone())
    },
    Ok(Settled::Back(continues)) => TurnEnd {
      continues,
      ..TurnEnd::of(run, Outcome::Reprepared)
    },
    Err(Failure::Person) => TurnEnd {
      failure: Some(contract::Failure::Person),
      ..TurnEnd::of(run, Outcome::Blocked)
    },
    Err(Failure::Stopped) => TurnEnd {
      failure: Some(contract::Failure::Stopped),
      ..TurnEnd::of(run, Outcome::Stopped)
    },
    Err(failure) => TurnEnd {
      failure: Some(failure_of(&failure)),
      ..TurnEnd::failed(run, failure.message(), beside.engine_error.clone())
    },
  };

  TurnEnd {
    went_out: beside.went_out,
    spans: beside.spans,
    compacted: beside.compacted,
    prefix: beside.prefix,
    ..end
  }
}

/// One job's turn carried on its session to its end. A turn on a provider other than Claude Code, or naming
/// a user that is no user's id, is refused; every other way the turn ends is its end.
pub async fn carry(asked: &WorkAsked<'_>, shown: &dyn Shows) -> Result<TurnEnd, Fault> {
  let work = asked.work;
  let (provider, model) = match asked.seat {
    WorkSeat::Worker(_) => (work.provider.as_str(), work.model.as_str()),
    WorkSeat::Apart(apart) => (apart.provider.as_str(), apart.model.as_str()),
  };

  if provider != PROVIDER {
    return Err(Fault::new(
      FaultKind::Refused,
      format!("{}: a job's turn goes out on {PROVIDER}, not {provider}", asked.agent),
    ));
  }

  // A turn naming its user walks that user's own logins; any other walks the logins its supply takes: a
  // person's own, the lendable ones for a person's platform source, the platform's for the deployment's own
  // (docs/supply.md, 「Choosing a source as a run starts」).
  let user = match work.user.as_deref().map(str::trim).filter(|user| !user.is_empty()) {
    None => {
      let data = &asked.ground.roots.data;
      let chosen = asked
        .job
        .and_then(|job| crate::supply::chosen_for(data, &crate::supply::job_key(job.job)))
        .or_else(|| crate::supply::chosen_for(data, asked.agent));

      crate::supply::carrying(chosen.as_ref())
    }
    Some(user) => crate::control::claude_accounts::Carrying::Own(
      user
        .parse::<i64>()
        .map_err(|_| Fault::new(FaultKind::Refused, format!("{user} is not a user's id")))?,
    ),
  };
  let mut log = RunLog::new(Target {
    agent: asked.agent.to_owned(),
    run: work.unit.clone().unwrap_or_else(|| work.name.clone()),
    chat_id: Some(work.chat),
    job_id: asked.job.map(|job| job.job),
  });
  let mut attempting = Attempting {
    asked,
    log: &mut log,
    shown,
    beside: Beside::default(),
    continued: None,
  };
  let walked = on_accounts(
    &Walking {
      ground: asked.ground,
      model,
    },
    &work.work_key,
    user,
    &mut attempting,
  )
  .await;
  let mut beside = attempting.beside;

  beside.spans = log.spans().iter().map(span_of).collect();

  Ok(ended(&work.name, walked, beside))
}

/// What an answered job turn says about usage, recorded under its agent and the run it is logged under: the
/// agent's last answer written, the windows its reply read kept, and what it spent published.
pub async fn noted(ground: &Ground, agent: &str, run: &str, reply: &Reply) -> Result<(), Refusal> {
  records::note_job_usage(&ground.db, &ground.windows, &ground.events, agent, reply, Some(run)).await
}

fn refused(why: Refusal) -> Failure {
  Failure::error(why.detail)
}

// What one primary run left: its answer, a turn back to its runner, or the text a turn that did not
// answer left behind.
enum Primary {
  Answered(Box<Reply>),
  Back(Option<String>),
  Unanswered(String),
}

// The turn on the logins the walk offers, and what it learned beside its end: the prompt it goes out with on
// the next login, where the walk passed one over, opened with the line saying it continues the work.
struct Attempting<'r, 'a> {
  asked: &'r WorkAsked<'a>,
  log: &'r mut RunLog,
  shown: &'r dyn Shows,
  beside: Beside,
  continued: Option<String>,
}

/// A job's log as what the renewal step before its program says its wait into: each line a note of the
/// job's own log, so a job waiting on its login's renewal never stands running with an empty log
/// (docs/run-records.md, 「Agent log」).
pub struct JobWaits<'l> {
  db: &'l DatabaseConnection,
  log: &'l mut RunLog,
}

impl<'l> JobWaits<'l> {
  /// The job's log its waits are written into, on the store it is kept in.
  pub fn of(db: &'l DatabaseConnection, log: &'l mut RunLog) -> Self {
    Self { db, log }
  }
}

impl Renewing for JobWaits<'_> {
  async fn say(&mut self, words: &str) {
    self.log.write(self.db, Line::note(words)).await;
  }
}

/// What a job's turn makes of a login the walk passed over for its own failure (docs/cache-fallback.md,
/// 「The Claude Code road is a list of accounts」): a note in the job's log naming that login, its own words
/// and the login the turn goes out on next, or that none is left; that next login kept among the job's roads,
/// the login passed over and its words its reason; and the prompt the turn goes out with on it, opened with
/// the line saying it continues the work, since the session it was on belongs to the login it left.
pub struct JobMoves<'l> {
  ground: &'l Ground,
  log: &'l mut RunLog,
  job: Option<i64>,
  model: &'l str,
}

impl<'l> JobMoves<'l> {
  /// The moves of the job named, on the model its turn goes out on, said into its log.
  pub fn of(ground: &'l Ground, log: &'l mut RunLog, job: Option<i64>, model: &'l str) -> Self {
    Self {
      ground,
      log,
      job,
      model,
    }
  }

  /// One login passed over, told and kept: the prompt the turn goes out with on the next login, none where
  /// none is left.
  pub async fn passed_over(&mut self, passed: &PassedOver<'_>, prompt: &str) -> Option<String> {
    let left = passed.account.words();
    let why = match passed.failure {
      AccountFailure::Limit => format!("Claude account {left} is at its ceiling: {}", passed.said),
      AccountFailure::NodeToken => {
        format!("Claude account {left} has no token for the work nodes: {}", passed.said)
      }
      _ => format!("Claude account {left} refused the turn: \"{}\"", passed.said),
    };
    let note = match passed.next {
      Some(next) => format!("{why}. The job goes out again on Claude account {}.", next.words()),
      None => format!("{why}. No Claude account of the road is left."),
    };

    self.log.write(&self.ground.db, Line::note(note)).await;

    let next = passed.next?;
    let road = |account: &Account| Road {
      provider: PROVIDER.to_owned(),
      model: self.model.to_owned(),
      account: Some(account.id.clone()),
    };

    let Some(job) = self.job else {
      return Some(continued(&road(passed.account), &why, prompt));
    };

    add_road(self.ground, job, &road(next), &why).await;

    // The login left cannot be asked for the session's handoff: the job's run log stands for it, opening the
    // turn on the next login (docs/sessions.md, 「The handoff a job's session leaves」).
    let handoff = crate::job_handoff::road_stopped(self.ground, job).await;

    Some(crate::job_handoff::ahead(
      &handoff.text,
      &continued(&road(passed.account), &why, prompt),
    ))
  }
}

// A job's turn says its wait for the login's renewal into the job's log.
impl Renewing for Attempting<'_, '_> {
  async fn say(&mut self, words: &str) {
    JobWaits::of(&self.asked.ground.db, self.log).say(words).await;
  }
}

impl Attempt<Settled> for Attempting<'_, '_> {
  // On a work node the login is the account's token for the nodes (docs/work-nodes.md, 「The logins on a
  // node」): one marked gone is passed over without a run, and a run there that fails on its login has
  // found that token gone rather than the account's login here.
  async fn attempt(&mut self, account: Option<Account>) -> Result<Settled, Failure> {
    let ground = self.asked.ground;
    let data = &ground.roots.data;
    let on_node = self.on_node();

    if on_node
      && let Some(held) = &account
      && logins::node_token_gone(&ground.db, held, data).await.map_err(refused)?
    {
      return Err(Failure::Account {
        failure: AccountFailure::NodeToken,
        detail: format!("claude account {}: its token for the work nodes is gone", held.id),
      });
    }

    match self.on_login(account).await {
      Err(failed) if on_node && failed.account_failure() == Some(AccountFailure::Login) => {
        Err(Failure::Account {
          failure: AccountFailure::NodeToken,
          detail: failed.message(),
        })
      }
      tried => tried,
    }
  }

  // A turn into a program on a work node never goes out on an endpoint account, whose key stays on this
  // machine (docs/work-nodes.md).
  fn takes(&self, account: &Account) -> bool {
    account.credential.runs_on_nodes() || !self.on_node()
  }

  async fn passed_over(&mut self, passed: &PassedOver<'_>) {
    let ground = self.asked.ground;
    let job = self.asked.job.map(|job| job.job);
    let model = self.model().to_owned();
    let prompt = self.asked.prompt;

    if let Some(continued) = JobMoves::of(ground, self.log, job, &model).passed_over(passed, prompt).await {
      self.continued = Some(continued);
    }
  }
}

impl Attempting<'_, '_> {
  // The words the turn goes out with: the job's own, opened with the line saying it continues the work where
  // the walk carried it off a login it was on.
  fn prompt(&self) -> String {
    self.continued.clone().unwrap_or_else(|| self.asked.prompt.to_owned())
  }

  // The model the turn goes out on, as the job keeps its roads: its mapping's, or its thrown-away session's.
  fn model(&self) -> &str {
    match self.asked.seat {
      WorkSeat::Worker(_) => &self.asked.work.mapping.model,
      WorkSeat::Apart(apart) => &apart.model,
    }
  }

  // Whether the turn goes into a standing program on a work node.
  fn on_node(&self) -> bool {
    let data = &self.asked.ground.roots.data;

    self.standing_agent().is_some_and(|agent| Place::of(data, &agent).node().is_some())
  }

  // The agent whose standing program the turn goes into: the worker, or the standing branch checkout a
  // thrown-away session's folder is; none for a pane of the turn's own.
  fn standing_agent(&self) -> Option<String> {
    match self.asked.seat {
      WorkSeat::Worker(row) => Some(row.name.clone()),
      WorkSeat::Apart(apart) => match apart_seat(self.asked.ground, apart) {
        Some(ApartSeat::Standing(home)) => Some(home.agent),
        _ => None,
      },
    }
  }

  // One login's attempt: the owner's reset and the standby first, the session the turn's resume settles on,
  // and the turn asked again on a fresh session where the program has lost the one it resumed. A
  // thrown-away session's turn reads none of a worker's sessions.
  async fn on_login(&mut self, account: Option<Account>) -> Result<Settled, Failure> {
    let ground = self.asked.ground;
    let work = self.asked.work;
    let worker = self.asked.agent;

    if let WorkSeat::Apart(apart) = self.asked.seat {
      if apart.on_runner_line() {
        return self.on_runner(apart, account).await;
      }

      // A role's run is a pane of its own on the session its role gives it; a co-work branch in a standing
      // branch checkout goes into that checkout's program.
      if !matches!(apart_seat(ground, apart), Some(ApartSeat::Standing(_))) {
        return self.on_role(apart, account).await;
      }

      return match self.primary(None, account).await? {
        Primary::Answered(reply) => Ok(Settled::Answered(reply)),
        Primary::Back(continues) => Ok(Settled::Back(continues)),
        Primary::Unanswered(left) => Err(of_unanswered(&left, &work.work_key)),
      };
    }
    let recorded = account.as_ref().map_or_else(
      || crate::control::claude_accounts::SEED_ACCOUNT.to_owned(),
      |account| account.id.clone(),
    );

    if carry_out_worker_reset(ground, worker).await.map_err(refused)? {
      println!("{worker}: its sessions are cleared as the owner asked, so this job opens a new one");
    }

    claim_standby(ground, worker, &Place::of(&ground.roots.data, worker))
      .await
      .map_err(refused)?;

    let ttl = session_ttl_ms(ground, &work.model).await;
    let kept = records::resumable(&ground.db, worker, &work.model, &recorded, ttl)
      .await
      .map_err(refused)?;
    // A job whose agent keeps no sessions opens a new one for a turn that leaves the choice to agency
    // (docs/agent-roles.md, 「A role's sessions」); a job whose agent cannot be told resumes as it would.
    let kept = match crate::role_agents::seated_at(&ground.db, None, Some(&work.cwd)).await {
      Ok(Some(agent)) if !agent.sessions.keeps() && matches!(work.resume, Resume::Auto) => None,
      _ => kept,
    };
    let resume = match choose(&work.resume, kept) {
      Choice::Go(resume) => resume,
      Choice::Back(continues) => return Ok(Settled::Back(continues)),
    };
    let left = match self.primary(resume.clone(), account.clone()).await? {
      Primary::Answered(reply) => return Ok(Settled::Answered(reply)),
      Primary::Back(continues) => return Ok(Settled::Back(continues)),
      Primary::Unanswered(left) => left,
    };

    match after_unanswered(&work.resume, resume.as_deref(), &left) {
      Unanswered::Failed => Err(of_unanswered(&left, &work.work_key)),
      Unanswered::Back => {
        records::forget(&ground.db, worker, &work.model, Some(&recorded))
          .await
          .map_err(refused)?;

        Ok(Settled::Back(None))
      }
      Unanswered::Again => {
        records::forget(&ground.db, worker, &work.model, Some(&recorded))
          .await
          .map_err(refused)?;

        match self.primary(None, account).await? {
          Primary::Answered(reply) => Ok(Settled::Answered(reply)),
          Primary::Back(continues) => Ok(Settled::Back(continues)),
          Primary::Unanswered(left) => Err(of_unanswered(&left, &work.work_key)),
        }
      }
    }
  }

  // A thrown-away session's turn on its runner line with the instructions its caller handed in, a run apart
  // in a pane of its own on the session its agent's role gives it: the run's reply its answer. Nothing is
  // held for it, so turns of several such sessions under one agent go out side by side.
  async fn on_runner(&mut self, apart: &Apart, account: Option<Account>) -> Result<Settled, Failure> {
    let ground = self.asked.ground;
    let work = self.asked.work;
    let worker = self.asked.agent;
    let session = new_session();
    let (run, command) =
      runner_command(ground, apart, work, account.as_ref(), &session).await.map_err(refused)?;

    if !provider_enabled(&ground.db, PROVIDER).await.map_err(|why| Failure::error(why.error))? {
      return Err(Failure::Unavailable {
        scope: held::scope_of(worker),
        mode: PROVIDER.to_owned(),
        reason: "Claude Code is switched off".to_owned(),
      });
    }

    let went_out = WentOut {
      provider: PROVIDER.to_owned(),
      model: run.model.clone(),
      effort: (!run.effort.is_empty()).then(|| run.effort.clone()),
      tier: None,
    };

    self.shown.went_out(&went_out);
    self.beside.went_out = Some(went_out);
    self
      .log
      .write(
        &ground.db,
        Line {
          source: Some(PROVIDER.to_owned()),
          state: Some("running"),
          ..Line::note(format!("{PROVIDER} · {} · {}", run.model, run.line))
        },
      )
      .await;

    let prompt = self.prompt();
    let mut watch = WorkWatch {
      ground,
      log: self.log,
      compacted: &mut self.beside.compacted,
      session: self.asked.session,
      run: &work.name,
      questions: self.asked.questions,
      shown: self.shown,
      job: self.asked.job,
      asks: self
        .asked
        .job
        .map(|job| CallAsks::new(&work.name, worker, job.job, JobService::of(ground.vars()))),
      stall: self.asked.job.map(|job| JobStall::new(job.job)),
    };
    let ending = apart::run(
      ground,
      apart::Apart {
        agent: worker,
        role: apart.role.as_deref().unwrap_or_else(|| crate::agent_roles::job_role()),
        line: run.line,
        provider: PROVIDER,
        title: work.title.clone(),
        command,
        session,
        prompt,
        model: run.model.clone(),
        effort: run.effort.clone(),
        account: account.as_ref().map(|account| account.id.clone()),
        parent: work.unit.clone(),
        root: work.unit.as_deref().map(execution_root),
        run: Some(work.name.clone()),
        resume: apart.resume.clone(),
        resume_else: apart.resume_else.clone(),
      },
      &mut watch,
    )
    .await
    .map_err(|message| Failure::Transport { message })?;

    if let Ending::Errored { kind, .. } = &ending {
      self.beside.engine_error = Some(EngineError { kind: kind.clone() });
    }

    let reply = apart::answered(ending, &work.work_key)?;

    noted(ground, worker, work.unit.as_deref().unwrap_or(&work.name), &reply)
      .await
      .map_err(refused)?;

    Ok(Settled::Answered(Box::new(reply)))
  }

  // A role's run on the worker line in a pane of its own, on the session its role gives it at the folder,
  // provider and model: the run's reply its answer. A job's run that answered on a lesser model than it went
  // out on has its session dropped where its repository heals, and the fall is told.
  async fn on_role(&mut self, apart: &Apart, account: Option<Account>) -> Result<Settled, Failure> {
    let ground = self.asked.ground;
    let work = self.asked.work;
    let agent = self.asked.agent;
    let role = apart.role.as_deref().unwrap_or_else(|| crate::agent_roles::job_role());
    let session = new_session();
    let job = self.asked.job;
    let (line, command) =
      role_command(ground, agent, apart, role, work, account.as_ref(), &session, job.is_some())
        .await
        .map_err(refused)?;
    let prefix = crate::apart_sessions::key(LINE, &command, &apart::records_home(ground, &command));

    // The session its caller names, opened under another command than this turn's, is put back to the caller
    // before anything goes in: its prefix moved, and the caller asks it for its handoff first (docs/sessions.md,
    // 「The handoff a job's session leaves」).
    if apart.resume.is_some() && apart.resume_prefix.as_ref().is_some_and(|held| *held != prefix) {
      return Ok(Settled::Back(None));
    }

    self.beside.prefix = Some(prefix);

    if !provider_enabled(&ground.db, PROVIDER).await.map_err(|why| Failure::error(why.error))? {
      return Err(Failure::Unavailable {
        scope: held::scope_of(agent),
        mode: PROVIDER.to_owned(),
        reason: "Claude Code is switched off".to_owned(),
      });
    }

    let went_out = went_out_of(&line);

    self.shown.went_out(&went_out);
    self.beside.went_out = Some(went_out);
    self
      .log
      .write(
        &ground.db,
        Line {
          source: Some(PROVIDER.to_owned()),
          state: Some("running"),
          ..Line::note(format!("{PROVIDER} · {} · {role}", line.model))
        },
      )
      .await;

    let prompt = self.prompt();
    let mut watch = WorkWatch {
      ground,
      log: self.log,
      compacted: &mut self.beside.compacted,
      session: self.asked.session,
      run: &work.name,
      questions: self.asked.questions,
      shown: self.shown,
      job: self.asked.job,
      asks: self
        .asked
        .job
        .map(|job| CallAsks::new(&work.name, agent, job.job, JobService::of(ground.vars()))),
      stall: self.asked.job.map(|job| JobStall::new(job.job)),
    };
    let ending = apart::run(
      ground,
      apart::Apart {
        agent,
        role,
        line: LINE,
        provider: PROVIDER,
        title: work.title.clone(),
        command,
        session,
        prompt,
        model: line.model.clone(),
        effort: line.effort.clone(),
        account: account.as_ref().map(|account| account.id.clone()),
        parent: work.unit.clone(),
        root: work.unit.as_deref().map(execution_root),
        run: Some(work.name.clone()),
        resume: apart.resume.clone(),
        resume_else: apart.resume_else.clone(),
      },
      &mut watch,
    )
    .await
    .map_err(|message| Failure::Transport { message })?;

    if let Ending::Errored { kind, .. } = &ending {
      self.beside.engine_error = Some(EngineError { kind: kind.clone() });
    }

    let reply = match apart::answered(ending, &work.work_key) {
      Err(_) if self.stopped().await => return Err(Failure::Stopped),
      answered => answered?,
    };

    noted(ground, agent, work.unit.as_deref().unwrap_or(&work.name), &reply)
      .await
      .map_err(refused)?;

    if job.is_some() {
      records::heal_role_run(ground, agent, role, &line.model, &reply, job.map(|job| job.job))
        .await
        .map_err(refused)?;
    }

    Ok(Settled::Answered(Box::new(reply)))
  }

  // The run on the worker line with the turn's mapping laid on it, delivered under the agent's lock: the
  // held session judged, the turn put in, and the session that answered kept.
  async fn primary(
    &mut self,
    resume: Option<String>,
    account: Option<Account>,
  ) -> Result<Primary, Failure> {
    let ground = self.asked.ground;
    let db = &ground.db;
    let work = self.asked.work;
    let seat = self.asked.seat;
    let worker = self.asked.agent;
    let trees = Trees::of(ground);
    let machine = ground.machine();
    // A thrown-away session goes out on the model and in the folder its scope names.
    let (kept_on, model, cwd) = match seat {
      WorkSeat::Worker(_) => (&work.model, &work.mapping.model, &work.cwd),
      WorkSeat::Apart(apart) => (&apart.model, &apart.model, &apart.cwd),
    };
    let mut line = worker_run(ground, &trees, Some(worker), account.as_ref(), kept_on, cwd)
      .await
      .map_err(refused)?;

    line.effort = effort_of(
      ground,
      &machine,
      PROVIDER,
      model,
      work.mapping.effort.as_deref(),
      line.reasoning.as_ref(),
    )
    .await
    .map_err(refused)?
    .unwrap_or_default();
    line.model.clone_from(model);
    line.tier.clone_from(&work.mapping.tier);

    // A kept session the safeguard refused goes out on the model that answered it, whatever the mapping
    // names; a fresh one goes out on its own model.
    let own = line.model.clone();

    if let Some(held) = &resume {
      let mark = records::sessions(db, worker)
        .await
        .map_err(refused)?
        .into_iter()
        .find(|one| &one.session_id == held)
        .and_then(|one| one.answered_model);

      line.model = launch_model(db, worker, mark.as_deref(), line.model).await.map_err(refused)?;
    }

    let lock = locks::agent_lock_path(&ground.roots.data, &held::scope_of(worker));
    // Another turn in the same program is waited out, the wait said in the job's log as it starts and ends.
    let held_lock = match locks::try_hold(&lock) {
      Some(held_lock) => held_lock,
      None => {
        self
          .log
          .write(db, Line::note(format!("Waiting for another turn in {worker}'s program to end")))
          .await;

        let held_lock =
          locks::hold(lock).await.map_err(|why| Failure::error(format!("agent lock: {why}")))?;

        self.log.write(db, Line::note(format!("{worker}'s program is free, going out"))).await;
        held_lock
      }
    };
    let session = SessionRun {
      mode: held::MODE,
      line,
      prompt: self.prompt(),
      resume,
      account,
      placement: None,
    };
    // A session thrown away is no session the worker holds, and is judged by nothing.
    let prepared = match seat {
      WorkSeat::Worker(_) => held::prepare(ground, worker, session).await.map_err(refused)?,
      WorkSeat::Apart(_) => session,
    };

    if !provider_enabled(db, PROVIDER).await.map_err(|why| Failure::error(why.error))? {
      return Err(Failure::Unavailable {
        scope: held::scope_of(worker),
        mode: PROVIDER.to_owned(),
        reason: "Claude Code is switched off".to_owned(),
      });
    }

    // The session the runner named, replaced by the judging, is not the one its prompt was composed for.
    if let (WorkSeat::Worker(_), Resume::Session { id }) = (seat, &work.resume)
      && prepared.resume.as_deref() != Some(id.as_str())
    {
      return Ok(Primary::Back(prepared.resume.clone()));
    }

    let mut sent = prepared.clone();

    // A session the preparation replaced is a fresh one, and a fresh session carries no mark.
    if sent.resume.is_none() && sent.line.model != own {
      sent.line.model.clone_from(&own);
    }

    let went_out = went_out_of(&sent.line);

    self.shown.went_out(&went_out);
    self.beside.went_out = Some(went_out);
    self
      .log
      .write(
        db,
        Line {
          source: Some(PROVIDER.to_owned()),
          state: Some("running"),
          ..Line::note(format!("{PROVIDER} · {}", sent.line.model))
        },
      )
      .await;

    let composing = Composed {
      name: work.name.clone(),
      title: work.title.clone(),
      prompt: sent.prompt.clone(),
      clock: None,
    };
    let account = sent.account.as_ref();
    let composed = match seat {
      WorkSeat::Worker(row) => {
        compose(ground, row, &sent.line, account, sent.resume.as_deref(), composing).await
      }
      WorkSeat::Apart(apart) => match apart_seat(ground, apart) {
        Some(ApartSeat::Standing(home)) => {
          let agent = home.agent.clone();

          compose_at(ground, &agent, home, &sent.line, account, None, composing).await
        }
        Some(ApartSeat::Own(herdr)) => compose_own(ground, herdr, &sent.line, account, composing),
        None => {
          return Err(Failure::Transport {
            message: format!("{worker}: this machine names no herdr for the place {cwd} stands in"),
          });
        }
      },
    }
    .map_err(refused)?;
    let turn = with_work(composed, worker, work, self.asked.job);
    let mut watch = WorkWatch {
      ground,
      log: self.log,
      compacted: &mut self.beside.compacted,
      session: self.asked.session,
      run: &work.name,
      questions: self.asked.questions,
      shown: self.shown,
      job: self.asked.job,
      asks: self
        .asked
        .job
        .map(|job| CallAsks::new(&work.name, worker, job.job, JobService::of(ground.vars()))),
      stall: self.asked.job.map(|job| JobStall::new(job.job)),
    };
    let ending = put(ground, &turn, &mut watch).await.map_err(Failure::error)?;
    let primary = match ending {
      Ending::Blocked { .. } => Err(Failure::Person),
      Ending::Untrusted { folder, config, .. } => Err(Failure::Untrusted { folder, config }),
      // A job's turn cut at its bound was marked stopped before the interrupt, so its item is released.
      Ending::Overran { .. } => Err(Failure::Stopped),
      Ending::Answered { reply, .. } => {
        noted(ground, worker, work.unit.as_deref().unwrap_or(&work.name), &reply)
          .await
          .map_err(refused)?;

        // A session thrown away keeps no record of what it was sent or of the model it fell to.
        if let WorkSeat::Worker(row) = seat {
          held::remember(ground, worker, &prepared, &reply).await.map_err(refused)?;
          self.kept(row, &sent, &own, &reply).await.map_err(refused)?;
        }

        Ok(Primary::Answered(reply))
      }
      other => {
        // Stopped from outside while the turn ran: the ending is the stop, however the pane looked.
        if self.stopped().await {
          Err(Failure::Stopped)
        } else {
          if let Ending::Errored { kind, .. } = &other {
            self.beside.engine_error = Some(EngineError { kind: kind.clone() });
          }

          Ok(Primary::Unanswered(evidence(&other).to_owned()))
        }
      }
    };

    drop(held_lock);
    primary
  }

  // The session that answered recorded for the turn's model and login — unless it answered on a lesser
  // model, when its record is dropped — with the context it carried, and folded once it is full.
  async fn kept(&self, row: &AgentRow, sent: &SessionRun, own: &str, reply: &Reply) -> Result<(), Refusal> {
    let ground = self.asked.ground;
    let work = self.asked.work;
    let worker = row.name.as_str();
    let Some(session_id) = reply.session_id.clone() else {
      println!("{worker}: the job's turn named no session, so there is none to hold open");

      return Ok(());
    };
    let account = sent.recorded_account();
    let job = self.asked.job.map(|job| job.job);
    let fell = heal(ground, worker, &work.model, &sent.line.model, &account, &reply.spend, job).await?;

    if fell.dropped {
      println!("{worker} {}: dropped {session_id}, the next job opens a fresh one", work.model);

      return Ok(());
    }

    let digest = worker_prefix_digest(
      ground,
      &Trees::of(ground),
      &work.cwd,
      &sent.line.model,
      Some(&sent.line.effort),
    )
    .await?;
    let fallen = fallen_model(own, &sent.line.model, &fell.models);

    record(&ground.db, worker, &work.model, &account, &session_id, Some(&digest), fallen.as_deref())
      .await?;
    note_carried(&ground.db, worker, &work.model, &account, sent.resume.is_none(), reply).await?;

    let fold_ratio = worker_fold_ratio(ground, &reply.mode, None, Some(&work.cwd)).await;

    if worker_session_full(reply.context_ratio, fold_ratio) {
      println!("{worker} {}: {session_id} is full, folding it", work.model);
      fold(ground, row, &work.model, &account, &session_id).await;
    }

    // The session is remembered as its agent's warm one where the agent the job was let in on keeps its
    // cache warm (docs/keepalive-schedule.md, 「An agent's kept session」): the job still holds its slot at
    // its checkout, which names the agent.
    match crate::role_agents::seated_at(&ground.db, None, Some(&work.cwd)).await {
      Ok(Some(agent)) if agent.keeps_cache => {
        crate::keepalive::agents::remember(crate::keepalive::agents::Warm {
          session: session_id,
          role: agent.role,
          agent: agent.name,
          provider: reply.mode.clone(),
          model: work.model.clone(),
          how: crate::keepalive::agents::How::Worker { row: row.clone() },
        })
      }
      Ok(Some(agent)) => {
        crate::keepalive::agents::forget(&agent.name);
      }
      Ok(None) => {}
      Err(why) => eprintln!("{worker}: the agent its job was let in on could not be read: {why}"),
    }

    Ok(())
  }

  // Whether the turn's job was stopped: the owner's stop of its job, or a cancellation over its run or the
  // unit it runs under.
  async fn stopped(&self) -> bool {
    let db = &self.asked.ground.db;
    let work = self.asked.work;

    if let Some(job) = self.asked.job
      && job_stop_reason(db, job.job).await.ok().flatten().is_some()
    {
      return true;
    }

    let names = [work.name.as_str(), work.unit.as_deref().unwrap_or("")];

    cancelled(db, &names).await.unwrap_or(false)
  }
}

/// The clocks a turn the door carries for a job or a role's session is held to.
#[derive(Clone, Debug)]
pub struct WorkClocks {
  /// The call clock, which holds a job's turn and a role's run alike: a call that makes no progress is cut
  /// and the turn goes on (docs/agent-control.md, 「A run's tool calls are held to a clock」).
  pub call: ToolClock,
  /// The hard bound, a job's turn's alone: it holds an item and the queue behind it.
  pub bound: Option<Duration>,
}

/// The clocks of a turn of the job named, or of a role session's turn where none is.
pub fn work_clocks(job: Option<JobMark>) -> WorkClocks {
  use policy::tool_clock::TOOL_CLOCK;

  WorkClocks {
    call: ToolClock::from(&TOOL_CLOCK.call),
    bound: job.map(|_| Duration::from_secs(TOOL_CLOCK.turn.bound_minutes.saturating_mul(60))),
  }
}

// What a job's turn or a role session's turn is watched with: the run log it feeds, and whether the program
// compacted its session. A question the program stops on is put to the turn's caller over the session it
// goes on.
struct WorkWatch<'w> {
  ground: &'w Ground,
  log: &'w mut RunLog,
  compacted: &'w mut bool,
  session: &'w SessionId,
  run: &'w str,
  questions: &'w Questions,
  shown: &'w dyn Shows,
  /// The job the turn is of; none for a role session's turn, which keeps no ask and no bound.
  job: Option<JobMark>,
  /// The asks opened on the job's tool calls gone silent and idle.
  asks: Option<CallAsks>,
  /// The notice of the job's turn standing still.
  stall: Option<JobStall>,
}

impl Watch for WorkWatch<'_> {
  async fn note(&mut self, text: &str) {
    self.log.write(&self.ground.db, Line::note(text)).await;
  }

  async fn clock(&mut self, _sentence: &str, note: &ClockNote) {
    self.log.write(&self.ground.db, Line::clock(note)).await;
  }

  fn tool_clock(&self) -> Option<ToolClock> {
    Some(work_clocks(self.job).call)
  }

  fn turn_clock(&self) -> Option<Duration> {
    work_clocks(self.job).bound
  }

  // A job's call gone silent and idle is asked about, its verdict read on each read while the ask stands.
  async fn silent_call(&mut self, call: &SilentCall) -> CallVerdict {
    match self.asks.as_mut() {
      Some(asks) => asks.hear(&self.ground.db, &self.ground.events, call).await,
      None => CallVerdict::NotAsked,
    }
  }

  async fn waits_on_question(&mut self, call: &str) -> bool {
    match self.asks.as_mut() {
      Some(asks) => asks.waits_on_question(&self.ground.db, call).await,
      None => false,
    }
  }

  async fn call_ended(&mut self, call: Option<&str>) {
    if let Some(asks) = self.asks.as_mut() {
      asks.ended(&self.ground.db, &self.ground.events, call).await;
    }

    if call.is_none()
      && let Some(stall) = self.stall.as_mut()
    {
      stall.ended(&self.ground.db, self.ground.vars()).await;
    }
  }

  // A job's turn standing still is told to its item's orderer, and let go once it moves again.
  async fn stalled(&mut self, change: &StallChange) {
    if let Some(stall) = self.stall.as_mut() {
      stall.heard(&self.ground.db, self.ground.vars(), change).await;
    }
  }

  // A call with no process of its own that did not take its cut within the grace: the job is marked
  // stopped with the reason, so its item is released for a rerun, and the door cuts the turn.
  async fn held_by_processless_call(&mut self, tool: &str, seconds: u64) -> Option<String> {
    let job = self.job?;
    use policy::tool_clock::TOOL_CLOCK;

    let why = policy::fill_marks(
      &TOOL_CLOCK.call.in_process_grace_cut,
      &[("tool", tool), ("seconds", &seconds.to_string())],
    );

    if let Err(refused) = crate::queue::stops::mark_job_stopped(&self.ground.db, job.job, &why).await {
      eprintln!(
        "{}: job {} was not marked stopped at a cut call's grace: {}",
        self.run, job.job, refused.detail
      );
    }

    println!(
      "{}: the call {tool} did not take its cut, so job {} is marked stopped and the turn is ended",
      self.run, job.job
    );

    Some(why)
  }

  // A job's turn past its bound is cut, not asked about: it holds an item and the queue behind it, so a
  // turn that will not end is marked stopped at once and the door interrupts it. A job that ends on a
  // stop mark gives its item's lease back, so the item is open for a rerun.
  async fn overtime(&mut self, asked: &Overrun) -> Overtime {
    let Some(job) = self.job else { return Overtime::NotAsked };
    use policy::tool_clock::TOOL_CLOCK;

    let minutes = (asked.bound_ms / 60_000).max(1);
    let why = policy::fill_marks(&TOOL_CLOCK.turn.cut, &[("minutes", &minutes.to_string())]);

    if let Err(refused) = crate::queue::stops::mark_job_stopped(&self.ground.db, job.job, &why).await {
      eprintln!(
        "{}: job {} was not marked stopped at its turn's bound: {}",
        self.run, job.job, refused.detail
      );
    }

    println!(
      "{}: the turn passed its {minutes}-minute bound, so job {} is marked stopped and the turn is cut",
      self.run, job.job
    );

    Overtime::Cut
  }

  async fn record(&mut self, lines: &[String]) {
    let texts = assistant_texts(lines);

    if !texts.is_empty() {
      self.shown.wrote(&texts);
    }

    *self.compacted |= compacts(lines);
    self.log.feed(&self.ground.db, lines, PROVIDER).await;
  }

  fn puts_questions(&self) -> bool {
    true
  }

  async fn confirm(&mut self, asked: &Confirmation) -> Option<Asking> {
    let (question, asking) = self.questions.put(self.session, asked);

    self.ground.events.publish(TurnQuestion {
      session: self.session.0.clone(),
      run: self.run.to_owned(),
      question,
    });

    Some(asking)
  }
}

/// The harness session a worker's next turn on a model would continue now, as its record and the first
/// login its walk offers hold it, none for a fresh one.
pub async fn continues(ground: &Ground, worker: &str, model: &str) -> Result<Option<String>, Refusal> {
  let account = crate::worker_session::walked_account(ground)
    .await?
    .map_or_else(|| crate::control::claude_accounts::SEED_ACCOUNT.to_owned(), |account| account.id);
  let ttl = session_ttl_ms(ground, model).await;

  records::resumable(&ground.db, worker, model, &account, ttl).await
}
