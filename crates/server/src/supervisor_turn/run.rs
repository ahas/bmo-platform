//! One run of a supervisor's turn, put into the resident program standing in the supervisor's own pane
//! (docs/agents.md, 「Conversational agents」; docs/supervisors.md, 「What a turn is given」, 「A turn that goes
//! nowhere」): the program stood on the command its turn composed — started again in the same pane where the
//! turn goes on in another room's session — the turn's words put in, the record the program writes fed into
//! the turn's log as it comes, the turn guard watching each call and its tally kept on the room's session as
//! it goes, and the answer read back out of that record. One turn runs at a time in an agent's one program. A
//! run its guard reads as going nowhere, or one past its time, is stopped and ends as a cut. A program left
//! standing on a session that ended is taken down.

use std::time::{Duration, Instant};

use harness_providers::claude_code::ClaudeCode;
use policy::fill_marks;
use policy::handoff::HANDOFF;
use policy::providers::Turns;
use policy::supervisor_runner::SUPERVISOR_RUNNER;
use serde_json::{Value, json};
use store::all;

use super::guard::{Guard, Spent};
use super::sessions::mark_spent;
use crate::Refusal;
use crate::door::execution::{Identity, execution_root, own_unit};
use crate::door::record::{ModelSpend, Reply as Recorded};
use crate::door::surface::{Run, SurfaceEnding, SurfaceTurn, interrupt_of};
use crate::door::turn::{Ending, Seat, Timings, Turn, Watch, pasted_lead, put};
use crate::lines::pieces::ComposedTurn;
use crate::lines::place::{Place, Program, token_of};
use crate::lines::token;
use crate::person_nodes::{NodeWait, Placement, Waited};
use crate::places::locks::{agent_lock_path, hold, try_hold};
use crate::places::{AgentRow, Ground};
use crate::queue::running::EXECUTION_CANCEL_PREFIX;
use crate::queue::settings;
use crate::run_log::RunLog;
use crate::run_log::lines::Line;
use crate::session_output::OutputWatch;

/// What a run answered.
#[derive(Clone, Debug)]
pub struct Reply {
  pub text: String,
  pub session_id: String,
  /// The provider every row of it is filed under, the model and effort it went out on.
  pub mode: String,
  pub model: String,
  pub effort: Option<String>,
  pub spend: Vec<ModelSpend>,
  /// How full the session's context stands after it, against its window, where that can be read.
  pub context_ratio: Option<f64>,
  /// The tokens that context carries, where they can be read.
  pub context_tokens: Option<i64>,
  /// The Claude login it went out on, where it went out on one of agency's.
  pub account: Option<String>,
  /// The answer of each turn of the program the run spanned, which `text` was built from.
  pub answers: Vec<agent_session::TurnAnswer>,
}

/// Why a run gave no answer: its guard cut it, with why, it failed, with the harness's own words, its engine
/// ended it with an error, with the program's words and the error's kind where it names one, the pane left the
/// turn's words unsubmitted in a dialog somebody opened, with why, or the supervisor's reset overtook it, and
/// nothing of it is kept (docs/supervisors.md, 「What the reset cuts off」).
#[derive(Debug)]
pub enum NoAnswer {
  Cut(String),
  /// A stop standing over the run ended it before its program was put to work.
  Stopped(String),
  Failed(String),
  Errored {
    error: String,
    kind: Option<String>,
  },
  Unsubmitted(String),
  Overtaken,
}

/// Where a run's tally goes as it spends: the room's session of the agent, and what the runs before it spent.
pub struct Tally<'a> {
  pub agent: &'a str,
  pub chat_id: i64,
  pub carried: Spent,
}

/// The reader of the program's record: the turn's log, its lines labelled by the provider the turn went out on,
/// its guard and its tally; and the run it stops by the run's own cancellation once its guard cuts it.
pub struct Watched<'a> {
  pub ground: &'a Ground,
  pub log: &'a mut RunLog,
  pub source: &'a str,
  pub run: String,
  pub guard: Option<Guard<'static>>,
  pub tally: Option<&'a Tally<'a>>,
  pub started: Instant,
  pub last_tally: Instant,
  /// Why its guard cut the run, once it has.
  pub cut: Option<String>,
  /// Where the answer's text goes while the turn runs, for a reader of the session.
  pub shown: Option<OutputWatch<'a>>,
}

impl<'a> Watched<'a> {
  fn spent(&self) -> Option<(Spent, &'a str, i64)> {
    let (guard, tally) = (self.guard.as_ref()?, self.tally?);
    let ms = tally.carried.ms + self.started.elapsed().as_millis() as i64;

    Some((
      Spent {
        calls: guard.made(),
        ms,
      },
      tally.agent,
      tally.chat_id,
    ))
  }

  async fn record_tally(&mut self) {
    self.last_tally = Instant::now();

    if let Some((spent, agent, chat_id)) = self.spent()
      && let Err(why) = mark_spent(&self.ground.db, agent, chat_id, spent).await
    {
      eprintln!("supervisor {agent}: the turn's tally was not written: {}", why.detail);
    }
  }

  // The run cut where its guard says so: its own cancellation stood over its name, which the door reads on
  // its next look and stops the turn by.
  async fn cut(&mut self, reason: String) {
    if self.cut.is_some() {
      return;
    }

    self.cut = Some(reason);

    let key = format!("{EXECUTION_CANCEL_PREFIX}{}", self.run);

    if let Err(why) = settings::set(&self.ground.db, &key, &crate::now_ms().to_string()).await {
      eprintln!("supervisor run {}: the cut was not stood over it: {}", self.run, why.detail);
    }
  }

  /// The lines the program wrote, into the turn's log under the provider that wrote them.
  pub async fn feed(&mut self, lines: &[String]) -> Vec<Line> {
    let db = self.ground.db.clone();

    self.log.feed(&db, lines, self.source).await
  }
}

impl Watch for Watched<'_> {
  // A supervisor's turn holds every room it answers while it works, so each of its tool calls is held to
  // the call clock as a job's are; it asks nothing about a silent call and has no hard bound
  // (docs/supervisors.md, 「A supervisor's long work」).
  fn tool_clock(&self) -> Option<crate::door::tool_clock::ToolClock> {
    Some(crate::door::tool_clock::ToolClock::from(&policy::tool_clock::TOOL_CLOCK.call))
  }

  // Kept as the turn's waiting for its read and published as `turn.changed`, as a card's wait is.
  async fn waiting(&mut self, waiting: Option<agent_session::Waiting>) {
    let Some(shown) = &self.shown else { return };

    shown.waiting(waiting.clone());
    self.ground.events.publish(crate::herdr_backend::TurnChanged {
      session: shown.session.0.clone(),
      turn_id: shown.turn,
      run: self.run.clone(),
      waiting,
    });
  }

  async fn note(&mut self, text: &str) {
    let db = self.ground.db.clone();

    self.log.write(&db, Line::note(text)).await;
  }

  async fn record(&mut self, lines: &[String]) {
    let written = self.feed(lines).await;

    if let Some(shown) = &self.shown {
      let texts: Vec<String> = written
        .iter()
        .filter(|line| line.kind == "text")
        .map(|line| line.text.clone())
        .collect();

      if !texts.is_empty() {
        shown.wrote(&texts);
      }
    }

    let mut counted = false;
    let mut reason = None;

    for line in &written {
      if let Some(guard) = self.guard.as_mut()
        && let Some(why) = guard.see(line)
      {
        reason.get_or_insert(why);
      }

      counted |= matches!(line.kind, "tool" | "tool-result");
    }

    if let Some(reason) = reason {
      self.cut(reason).await;
    }

    if counted || self.last_tally.elapsed() >= Duration::from_millis(SUPERVISOR_RUNNER.tally_every_ms) {
      self.record_tally().await;
    }
  }
}

// The lock an agent's one program is held under while a turn of it runs.
fn agent_lock(ground: &Ground, agent: &str) -> std::path::PathBuf {
  agent_lock_path(&ground.roots.data, agent)
}

/// The agent's resident program taken down where it stands on one of the sessions named, which its rooms no
/// longer go on in (docs/supervisors.md, 「The program on an ended session」): its place left holding none and the
/// session token it carried taken back, so the next turn stands it on that turn's own session. A turn holding the
/// agent's lock has stood the program on its own session and is never cut, so the lock is only tried. Answers
/// whether it was taken down.
pub async fn let_go(ground: &Ground, agent: &str, ended: &[String]) -> bool {
  take_down(ground, agent, Release::Asked, |program| {
    ended.iter().any(|session| program.resumes(session))
  })
  .await
}

/// The agent's resident program taken down whatever session it stands on, as `let_go` takes it: after a reset of
/// its conversation every session it may stand on is one the reset took, a session no room ever recorded among them.
/// It is killed, since it holds what the reset takes and would write its session's record again as it ended.
pub async fn let_go_any(ground: &Ground, agent: &str) -> bool {
  take_down(ground, agent, Release::Killed, |_| true).await
}

/// What an overtaken turn stood goes as it ends (docs/supervisors.md, 「What the reset cuts off」), where no room of
/// the agent's goes on in the session it stood the program on any more, as after a reset of its conversation: the
/// program, where it still stands on that session, killed so it writes nothing more, and then the records its harness
/// keeps of the session and the prompts its history kept of it.
pub async fn let_go_overtaken(
  ground: &Ground,
  agent: &str,
  provider: &dyn harness_providers::Provider,
  model: &str,
  session: &str,
) {
  let held = all(
    &ground.db,
    r#"SELECT 1 FROM "room_sessions" WHERE "agent" = $1 AND "harness_session" = $2"#,
    vec![agent.into(), session.into()],
  )
  .await;

  // A room still going on in it keeps it, as one the archive could not be asked about does.
  if !held.is_ok_and(|rows| rows.is_empty()) {
    return;
  }

  take_down(ground, agent, Release::Killed, |program| program.resumes(session)).await;

  let roots = harness_machine::supervisor_prompt::Roots {
    assets: ground.code_root(),
    data: ground.roots.data.clone(),
  };
  let machine = harness_providers::Machine::of_process(ground.roots.data.clone());
  let cwd = harness_machine::supervisor_prompt::folder(&roots, agent);

  // The session may have gone out on any login, whose home keeps its records; where the logins cannot be
  // read, the machine's own configuration is still looked in.
  let logins =
    crate::control::claude_accounts::login_homes(&ground.db, &machine)
      .await
      .unwrap_or_else(|why| {
        eprintln!(
          "supervisor {agent}: the logins its overtaken turn may have run under were not read: {}",
          why.error
        );
        vec![]
      });

  for path in provider.session_records(&machine, &logins, &cwd, session, model) {
    let removed = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };

    if let Err(why) = removed.or_else(|why| if path.exists() { Err(why) } else { Ok(()) }) {
      eprintln!(
        "supervisor {agent}: the record {} of its overtaken turn was not removed: {why}",
        path.display()
      );
    }
  }

  // Its prompt history holds what the turn was given, as the record did.
  if let Err(why) = provider.forget_prompts(&machine, &logins, &cwd, Some(session)) {
    eprintln!("supervisor {agent}: the prompts of its overtaken turn were not forgotten: {why}");
  }

  // So does its place, which its program went on writing into after the reset emptied it.
  let place = crate::lines::place::place_of(&ground.roots.data, agent);

  if let Err(why) = crate::door::place_run::forget_records(&place, Some(session)) {
    eprintln!("supervisor {agent}: its place's records of its overtaken turn were not taken out: {why}");
  }
}

// How a program is taken down: asked to end, or killed where it must write nothing more.
enum Release {
  Asked,
  Killed,
}

async fn take_down(
  ground: &Ground,
  agent: &str,
  release: Release,
  ended: impl Fn(&Program) -> bool,
) -> bool {
  let Some(_held) = try_hold(&agent_lock(ground, agent)) else {
    return false;
  };
  let place = Place::of(&ground.roots.data, agent);

  if !place.program().is_some_and(|program| ended(&program)) {
    return false;
  }

  let released = match release {
    Release::Asked => place.release().await,
    Release::Killed => place.release_now().await,
  };

  if let Some(old) = released.as_ref().and_then(token_of) {
    token::let_go(ground, old, Duration::from_secs(HANDOFF.calls_look_seconds)).await;
  }

  println!("supervisor {agent}: its program stood on a session that ended, and was taken down");

  true
}

/// What a session's record says its last request held: the input of the last request it kept usage for.
pub(crate) fn last_request_tokens(path: &std::path::Path) -> Option<i64> {
  let mut last = None;

  crate::door::record::each_line(path, |_, entry| {
    let Some(usage) = entry.get("message").and_then(|message| message.get("usage")) else {
      return;
    };
    let count = |name: &str| usage.get(name).and_then(Value::as_f64).unwrap_or(0.0) as i64;
    let tokens =
      count("input_tokens") + count("cache_creation_input_tokens") + count("cache_read_input_tokens");

    if tokens > 0 {
      last = Some(tokens);
    }
  });

  last
}

// The words an ending that is neither an answer nor the program's own error is told by.
fn failed(ending: &Ending) -> String {
  format!(
    "Claude Code's turn ended {} with no answer",
    serde_json::to_value(ending)
      .ok()
      .and_then(|value| value.get("outcome").and_then(|outcome| outcome.as_str().map(str::to_owned)))
      .unwrap_or_default()
  )
}

/// The label an agent's place is shown by and the node it stands on, as the store and its place say now; none
/// where no agent of the name stands. A gate place and a thrown-away session's place stand under no agent's row,
/// by their own names (docs/agent-control.md, 「A gate's places」; docs/api.md, 「sessions」), and a thrown-away
/// session's place of an account is placed by that account's present record.
pub async fn placed(ground: &Ground, agent: &str) -> Result<Option<(String, Option<String>)>, Refusal> {
  if let Some(row) = AgentRow::named(&ground.db, agent).await? {
    return Ok(Some((row.shown().to_owned(), row.node)));
  }

  if crate::places::gate_places::recorded_place(&ground.roots.data, agent) {
    return Ok(Some((agent.to_owned(), None)));
  }

  if crate::apart_places::is_apart_place(&ground.roots.data, agent) {
    return Ok(Some((agent.to_owned(), crate::apart_places::placed_now(ground, agent).await?)));
  }

  Ok(None)
}

// A run's wait for its node as the server keeps it (docs/person-nodes.md, 「Which node is a person's」): the link's
// state the link writes under the data root, the run's placement read by `present`, a stop standing over the run,
// and the wait told on the turn's read, as `turn.changed` and beside the turn's kept record.
struct NodeWaiting<'a, Read> {
  ground: &'a Ground,
  run: &'a str,
  present: Read,
  shown: Option<OutputWatch<'a>>,
}

impl<Read, Reading> NodeWait for NodeWaiting<'_, Read>
where
  Read: FnMut() -> Reading + Send,
  Reading: Future<Output = Result<Placement, Refusal>> + Send,
{
  fn heard(&self, node: &str) -> bool {
    crate::watches::links::link_heard(&self.ground.roots.data, node)
  }

  async fn present(&mut self) -> Result<Placement, Refusal> {
    (self.present)().await
  }

  async fn stopped(&mut self) -> bool {
    crate::door::execution::cancelled(&self.ground.db, &[self.run]).await.unwrap_or(false)
  }

  async fn told(&mut self, waiting: Option<(&str, i64)>) {
    let waiting = waiting.map(|(node, since)| agent_session::Waiting {
      kind: agent_session::WaitKind::NodeLink,
      service: node.to_owned(),
      reason: format!("the link of {node} is not heard"),
      since,
    });

    crate::turn_pickup::mark_node_wait(
      &self.ground.db,
      self.run,
      waiting.as_ref().map(|waiting| waiting.service.as_str()),
    )
    .await;

    let Some(shown) = self.shown else { return };

    shown.waiting(waiting.clone());
    self.ground.events.publish(crate::herdr_backend::TurnChanged {
      session: shown.session.0.clone(),
      turn_id: shown.turn,
      run: self.run.to_owned(),
      waiting,
    });
  }
}

#[allow(clippy::too_many_arguments)]
/// One run of a supervisor's turn in its resident program, under its guard where `guarded`, its tally written
/// where `tally` names the room's session. A run the supervisor's reset overtook, its turn having begun at or
/// before the reset point, answers nothing whatever it came to.
pub async fn run_turn(
  ground: &Ground,
  agent: &str,
  line: &str,
  provider: &str,
  composed: &ComposedTurn,
  resumed: Option<&str>,
  log: &mut RunLog,
  guarded: bool,
  tally: Option<&Tally<'_>>,
  shown: Option<OutputWatch<'_>>,
) -> Result<Reply, NoAnswer> {
  let failure = |why: String| NoAnswer::Failed(why);
  let run = log.run().to_owned();
  let gone = || failure(format!("{agent} no longer stands, so its turn ends with it"));
  let Some((label, node)) = placed(ground, agent).await.map_err(|why| failure(why.detail))? else {
    return Err(failure(format!("no agent is named {agent}")));
  };
  // A supervisor of an account with a node stands there, and so does a role run of such an account, whose place
  // is marked with it; either waits while the node's link is not heard, placed again by the present record on
  // every ask, and a supervisor removed meanwhile ends its turn (docs/person-nodes.md; docs/supervisors.md).
  let present = || async {
    Ok(match placed(ground, agent).await? {
      Some((_, node)) => Placement::On(node),
      None => Placement::Gone,
    })
  };
  let mut waiting = NodeWaiting {
    ground,
    run: &run,
    present,
    shown,
  };
  let nodes = &policy::person_nodes::PERSON_NODES;
  let waited = crate::person_nodes::until_placed(
    &run,
    node,
    &mut waiting,
    Duration::from_secs(nodes.wait_seconds),
    Duration::from_secs(nodes.wait_bound_seconds),
  )
  .await;
  let node = match waited {
    Waited::On(node) => node,
    Waited::Gone => return Err(gone()),
    Waited::Stopped => {
      return Err(NoAnswer::Stopped(format!("a stop ended {agent}'s turn while it waited")));
    }
    Waited::Overdue(why) => return Err(failure(why)),
  };
  let home = ground
    .home_on(agent, &label, node.as_deref())
    .await
    .map_err(|why| failure(why.detail))?
    .ok_or_else(|| failure(format!("this machine names no herdr for {agent}'s place")))?;
  // The agent's one program takes one turn at a time, whichever room it is for; a turn queued behind another
  // whose agent was removed meanwhile ends without standing it again.
  let _held = hold(agent_lock(ground, agent)).await.map_err(|why| failure(why.to_string()))?;

  if placed(ground, agent).await.map_err(|why| failure(why.detail))?.is_none() {
    return Err(gone());
  }

  let limits = &SUPERVISOR_RUNNER.limits;
  let carried = tally.map_or(Spent::default(), |tally| tally.carried);
  let title = fill_marks(&SUPERVISOR_RUNNER.log.title, &[("agent", agent)]);
  let own = own_unit(ground.env());
  let root = Some(execution_root(own.as_deref().unwrap_or_default()));
  let mut watched = Watched {
    ground,
    log,
    source: provider,
    run: run.clone(),
    guard: guarded.then(|| Guard::new(limits, carried.calls)),
    tally: tally.filter(|_| guarded),
    started: Instant::now(),
    last_tally: Instant::now(),
    cut: None,
    shown,
  };
  let resident = harness_providers::provider(provider)
    .and_then(|declared| declared.declared().resident.as_ref())
    .ok_or_else(|| failure(format!("{provider} stands no resident program")))?;
  let ended: Result<Result<Recorded, NoAnswer>, String> = if resident.turns == Turns::Transcript {
    let turn = Turn {
      name: run.clone(),
      title: title.clone(),
      seat: Seat::Standing(home),
      cwd: composed.command.cwd.to_string_lossy().into_owned(),
      command: composed.command.args.clone(),
      env: composed.command.env.clone(),
      prompt: composed.prompt.clone(),
      session: composed.session.clone(),
      identity: Identity {
        agent: Some(agent.to_owned()),
        root: root.clone(),
        parent: own.clone(),
        role: None,
        model: Some(composed.model.clone()),
        effort: composed.effort.clone(),
      },
      line: line.to_owned(),
      provider: provider.to_owned(),
      lead: pasted_lead(),
      trusts_folder: true,
      clock: None,
      interrupt: interrupt_of(resident),
      timings: Timings::default(),
    };

    put(ground, &turn, &mut watched).await.map(|ending| match ending {
      Ending::Answered { reply, .. } => Ok(*reply),
      Ending::Errored { error, kind } => Err(NoAnswer::Errored { error, kind }),
      Ending::Unsubmitted { why, .. } => Err(NoAnswer::Unsubmitted(why)),
      other => Err(failure(failed(&other))),
    })
  } else {
    let turn = SurfaceTurn {
      run: Run {
        line: line.to_owned(),
        model: composed.model.clone(),
        effort: composed.effort.clone().unwrap_or_default(),
        tier: None,
        prompt: composed.prompt.clone(),
        cwd: composed.command.cwd.to_string_lossy().into_owned(),
        env: composed.run_env.clone().unwrap_or_else(|| composed.command.env.clone()),
        resume_session_id: resumed.map(str::to_owned),
      },
      given: composed.given.clone().unwrap_or_else(|| json!({ "agent": agent, "line": line })),
      home,
      name: run.clone(),
      title,
      root,
      parent: own,
      command: composed.command.clone(),
      clocks: Default::default(),
    };

    crate::door::surface::turn(ground, provider, &turn, &mut watched)
      .await
      .map(|ending| match ending {
        SurfaceEnding::Reply(reply) => Ok(*reply),
        SurfaceEnding::Errored { error, kind } => Err(NoAnswer::Errored { error, kind }),
        SurfaceEnding::Unsubmitted(why) => Err(NoAnswer::Unsubmitted(why)),
        SurfaceEnding::Error(why) | SurfaceEnding::Stopped(why) => Err(failure(why)),
      })
  };

  watched.record_tally().await;

  // A cut stood over the run is the run's alone: once it has ended, nothing is left standing over its name.
  if watched.cut.is_some() {
    let _ = settings::forget(&ground.db, &format!("{EXECUTION_CANCEL_PREFIX}{run}")).await;
  }

  if super::overtaken::overtaken(&ground.db, agent, composed.began).await {
    return Err(NoAnswer::Overtaken);
  }

  // A stopped run may end in whatever way its door reads a stop; the cut is what it ended for.
  if let Some(reason) = watched.cut {
    return Err(NoAnswer::Cut(reason));
  }

  let reply = ended.map_err(failure)??;
  let session_id = reply.session_id.clone().unwrap_or_else(|| composed.session.clone());

  // On a registration the run is the registry's: every row filed under it and costing nothing, and its
  // context read off the record the program keeps under the registration's home against the window the
  // registration was sized to.
  let Some(window) = &composed.window else {
    return Ok(Reply {
      text: reply.text,
      session_id,
      mode: provider.to_owned(),
      model: composed.model.clone(),
      effort: composed.effort.clone(),
      spend: reply.spend,
      context_ratio: reply.context_ratio,
      context_tokens: reply.context_tokens,
      account: composed.account.clone(),
      answers: reply.answers,
    });
  };
  let record = ClaudeCode::records_under(&window.home, &composed.command.cwd, &session_id)
    .into_iter()
    .next()
    .and_then(|path| last_request_tokens(&path));

  Ok(Reply {
    text: reply.text,
    session_id,
    mode: provider.to_owned(),
    model: composed.model.clone(),
    effort: composed.effort.clone(),
    spend: reply
      .spend
      .into_iter()
      .map(|row| ModelSpend {
        mode: provider.to_owned(),
        cost: 0.0,
        ..row
      })
      .collect(),
    context_ratio: record
      .map(|tokens| tokens as f64 / window.context_tokens as f64)
      .or(reply.context_ratio),
    context_tokens: record.or(reply.context_tokens),
    account: composed.account.clone(),
    answers: reply.answers,
  })
}
