//! A run in a pane of its own (docs/agents.md, 「A run in a pane of its own」): one run of a role — a job's
//! turn, a worker ask, a search, a translation, a probe, a login's renewal — as the interactive program in
//! a pane of its own, opened for the run and closed with it, on the session its role's setting gives it: a
//! new one, or the valid one the role's last run at the same path on the same provider and model left
//! (docs/sessions.md, 「A role's session」). The run's command is its line's; what it
//! is handed beside that is its own tool token and the variables its line set, and what comes back is the
//! door's reading of the program's record.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use harness_providers::claude_code::ClaudeCode;
use herdr::Herdr;
use policy::door_words::DOOR_WORDS;
use policy::resident_turn::RESIDENT_TURN;
use policy::run_lines::RUN_LINES;

use super::execution::{Identity, execution_root, own_unit};
use super::guard::launch_refusal;
use super::place_run::{begin_run, run_variables};
use super::record::{Reply, Tail, compacted_since};
use super::surface::interrupt_of;
use super::turn::{Ending, Quiet, Seat, Timings, Turn, Watch, put};
use crate::apart_sessions;
use crate::job_run::claude::failure::{Failure, RunEnding, of_errored};
use crate::lines::claude::{Command, line_environment};
use crate::lines::place::{Holds, Place, read_settings, resumed_command, write_place_settings};
use crate::lines::standing::status_root;
use crate::lines::token;
use crate::now_ms;
use crate::places::Ground;
use crate::places::trust::{config_file, mark_onboarded};
use crate::worker_session::fold::FOLD_WORD;
use crate::worker_session::told::SessionFolded;

// How long a run's tool session may stand if nothing takes its token back.
const TOKEN_BACKSTOP: Duration = Duration::from_secs(24 * 3_600);

/// One run as its caller names it: the name it runs under, the role it holds, the line and provider it goes
/// out on, what a person watching herdr reads it as, the line's command for it — its words opening `session`,
/// a new session the run resumes a kept one in place of where its role keeps sessions — the words it is told,
/// the model, effort and login its execution row and its session's key keep, and the run its execution is a
/// child of and the root it is counted under, where the caller names them rather than this process's own
/// unit. A run carries no clock: it lasts until it ends or is stopped.
pub struct Apart<'a> {
  pub agent: &'a str,
  pub role: &'a str,
  pub line: &'a str,
  pub provider: &'a str,
  pub title: String,
  pub command: Command,
  pub session: String,
  pub prompt: String,
  pub model: String,
  pub effort: String,
  pub account: Option<String>,
  pub parent: Option<String>,
  pub root: Option<String>,
  /// The run the pane's turn is, where the caller's turn is one of its own — a job's or another role's
  /// work turn kept in flight under its run (docs/sessions.md, 「A restart hands the session over」); the
  /// line and the session name it otherwise.
  pub run: Option<String>,
  /// The harness session the caller names for the run to continue, resumed while its record stands in place of
  /// the one its role keeps.
  pub resume: Option<String>,
  /// The words the run goes out with instead where the session `resume` names cannot be resumed.
  pub resume_else: Option<String>,
}

/// A new session for a run, which a later run resumes only where the run's role keeps sessions.
pub fn new_session() -> String {
  uuid::Uuid::new_v4().to_string()
}

/// Where the program of a command keeps its record: under the configuration directory its environment
/// names, or the machine's own.
pub fn records_home(ground: &Ground, command: &Command) -> PathBuf {
  command
    .env
    .get("CLAUDE_CONFIG_DIR")
    .cloned()
    .flatten()
    .filter(|dir| !dir.is_empty())
    .map_or_else(|| ClaudeCode::machine_config_dir(&ground.machine()), PathBuf::from)
}

/// The record a run apart writes, by the session it opens.
pub fn record_of(ground: &Ground, command: &Command, session: &str) -> PathBuf {
  ClaudeCode::records_under(&records_home(ground, command), Path::new(&command.cwd), session).swap_remove(0)
}

/// What the pane of a run is handed: what a pane running its line's command is handed by name, and every
/// variable the line set for the run over the launcher's own environment — its login, its thinking, a
/// gateway's road and the run's own settings — and never the rest of that environment.
pub fn pane_environment(
  process: &BTreeMap<String, String>,
  command: &Command,
  name: &str,
) -> BTreeMap<String, String> {
  let mut env = line_environment(command, name, None);

  for (variable, value) in &command.env {
    if let Some(value) = value
      && process.get(variable) != Some(value)
    {
      env.entry(variable.clone()).or_insert_with(|| value.clone());
    }
  }

  env
}

/// One run apart, to its ending: refused before anything stands where the job guard refuses its command,
/// its tool token issued for it and taken back once it ends, its session the kept one where its role keeps
/// sessions and one stands warm, and the turn put into a pane of its own on this machine's herdr, its record
/// handed to `watch` as it is written. A run whose role keeps sessions keeps the session it answered on.
pub async fn run<W: Watch>(ground: &Ground, mut apart: Apart<'_>, watch: &mut W) -> Result<Ending, String> {
  let herdr = ground
    .herdr
    .clone()
    .ok_or_else(|| format!("{}: this machine names no herdr to open the run's pane on", apart.line))?;
  let kept = if apart_sessions::setting(&ground.db, apart.role, &apart.command.cwd).await.keeps() {
    Some(apart_sessions::SessionKey {
      agent: apart_sessions::agent_at(&ground.db, apart.role, &apart.command.cwd).await,
      role: apart.role.to_owned(),
      path: apart.command.cwd.clone(),
      provider: apart.provider.to_owned(),
      model: apart.model.clone(),
      effort: apart.effort.clone(),
      account: apart.account.clone(),
      command: apart_sessions::key(apart.line, &apart.command, &records_home(ground, &apart.command)),
    })
  } else {
    None
  };

  let named = apart.resume.clone().filter(|named| record_of(ground, &apart.command, named).is_file());

  if let Some(named) = named {
    apart.command.args = resumed_command(&apart.command.args, &named);
    apart.session = named;
  } else if apart.resume.is_some()
    && let Some(instead) = apart.resume_else.clone()
  {
    // The named session's record is gone: a new session opens on the words that carry what it held.
    apart.prompt = instead;
  } else if let Some(key) = &kept {
    match apart_sessions::take(&ground.db, key, now_ms()).await {
      Ok(Some(held)) if record_of(ground, &apart.command, &held).is_file() => {
        apart.command.args = resumed_command(&apart.command.args, &held);
        apart.session = held;
      }
      Ok(_) => {}
      Err(why) => {
        eprintln!("{}: its kept session could not be read, so it opens a new one: {why:?}", apart.line)
      }
    }
  }

  let line = apart.line;
  let session = apart.session.clone();
  let opened_at = now_ms();
  let fold = Folding {
    agent: apart.agent,
    role: apart.role,
    line: apart.line,
    provider: apart.provider,
    command: apart.command.clone(),
    model: apart.model.clone(),
    effort: apart.effort.clone(),
    account: apart.account.clone(),
    parent: apart.parent.clone(),
    root: apart.root.clone(),
  };
  let ended = run_on(ground, herdr.clone(), apart, watch).await;

  if let Some(key) = &kept {
    let kept = match &ended {
      Ok(Ending::Answered { reply, .. }) => {
        let answered = apart_sessions::Answered {
          at: now_ms(),
          opened_at,
          context_tokens: reply.context_tokens,
          context_share: reply.context_ratio,
        };

        apart_sessions::keep(&ground.db, key, &session, answered).await
      }
      _ => apart_sessions::give_back(&ground.db, &session).await,
    };

    if let Err(why) = kept {
      eprintln!("{line}: the session it ran on was not kept: {why:?}");
    }

    if let Ok(Ending::Answered { reply, .. }) = &ended {
      let warm = fold.warm(&session);
      let cwd = fold.command.cwd.clone();

      fold.fold(ground, herdr, &session, reply.context_ratio).await;
      remember_warm(ground, warm, &cwd).await;
    }
  }

  ended
}

// The session a run kept remembered as its agent's warm one, where the agent it was let in on keeps its cache
// warm (docs/keepalive-schedule.md, 「An agent's kept session」): the run still holds its slot at its folder,
// which names the agent. An agent that does not keep its cache warm has nothing remembered of it.
async fn remember_warm(ground: &Ground, warm: crate::keepalive::agents::Warm, cwd: &str) {
  let seat = crate::role_agents::seated_at(&ground.db, Some(&warm.role), Some(cwd)).await;

  match seat {
    Ok(Some(agent)) if agent.keeps_cache => {
      crate::keepalive::agents::remember(crate::keepalive::agents::Warm {
        agent: agent.name,
        ..warm
      });
    }
    Ok(Some(agent)) => {
      crate::keepalive::agents::forget(&agent.name);
    }
    Ok(None) => {}
    Err(why) => eprintln!("{}: the agent its run was let in on could not be read: {why:?}", warm.role),
  }
}

/// One warm ping of an agent's kept session (docs/keepalive-schedule.md, 「An agent's kept session」): one
/// more turn into the session on the command its run went out on, the keepalive's word, the session held
/// while it goes. A ping that answered leaves the session valid from that moment; one that did not, or that
/// found the session held, leaves it as it stood. Whether it answered.
pub async fn ping(ground: &Ground, warm: &crate::keepalive::agents::Warm) -> bool {
  let Some(herdr) = ground.herdr.clone() else {
    eprintln!("keepalive {}: this machine names no herdr to open the ping's pane on", warm.agent);

    return false;
  };

  match apart_sessions::take_for_ping(&ground.db, &warm.session).await {
    Ok(true) => {}
    Ok(false) => return false,
    Err(why) => {
      eprintln!("keepalive {}: its session could not be taken: {why:?}", warm.agent);

      return false;
    }
  }

  let crate::keepalive::agents::How::Apart {
    line,
    run_name,
    effort,
    account,
    command,
    parent,
    root,
  } = &warm.how
  else {
    return false;
  };
  let mut command = command.clone();

  command.args = resumed_command(&command.args, &warm.session);

  let ping = Apart {
    agent: run_name,
    role: &warm.role,
    line,
    provider: &warm.provider,
    title: DOOR_WORDS.keepalive_title.clone(),
    command,
    session: warm.session.clone(),
    prompt: policy::keepalive::KEEPALIVE.prompt.clone(),
    model: warm.model.clone(),
    effort: effort.clone(),
    account: account.clone(),
    parent: parent.clone(),
    root: root.clone(),
    run: None,
    resume: None,
    resume_else: None,
  };
  let answered = matches!(run_on(ground, herdr, ping, &mut Quiet).await, Ok(Ending::Answered { .. }));
  let given = match answered {
    true => {
      println!("keepalive {} {}: warmed", warm.agent, warm.session);
      apart_sessions::warmed(&ground.db, &warm.session, now_ms()).await
    }
    false => {
      eprintln!(
        "keepalive {} {}: the ping did not answer, the session stands as it was",
        warm.agent, warm.session
      );
      apart_sessions::give_back(&ground.db, &warm.session).await
    }
  };

  if let Err(why) = given {
    eprintln!("keepalive {}: the session it pinged was not given back: {why:?}", warm.agent);
  }

  answered
}

/// What a run's fold goes out with: the run's own line, command, road and login, on the session it kept.
struct Folding<'a> {
  agent: &'a str,
  role: &'a str,
  line: &'a str,
  provider: &'a str,
  command: Command,
  model: String,
  effort: String,
  account: Option<String>,
  parent: Option<String>,
  root: Option<String>,
}

impl Folding<'_> {
  // What a warm ping of the session this run kept would go out on: the run as it went out, under no agent
  // yet.
  fn warm(&self, session: &str) -> crate::keepalive::agents::Warm {
    crate::keepalive::agents::Warm {
      agent: String::new(),
      session: session.to_owned(),
      role: self.role.to_owned(),
      provider: self.provider.to_owned(),
      model: self.model.clone(),
      how: crate::keepalive::agents::How::Apart {
        line: self.line.to_owned(),
        run_name: self.agent.to_owned(),
        effort: self.effort.clone(),
        account: self.account.clone(),
        command: self.command.clone(),
        parent: self.parent.clone(),
        root: self.root.clone(),
      },
    }
  }

  /// The session a run kept folded, where its turn left it at or past its role's compaction threshold
  /// (docs/sessions.md, 「Session compaction」): one more turn into the same session on the same command, the
  /// fold word, before the run gives its slot back. A fold that compacted leaves the session kept and valid
  /// with its context unread; one that did not leaves it as it stood. Nothing here fails the run.
  async fn fold(self, ground: &Ground, herdr: Herdr, session: &str, share: Option<f64>) {
    let line = self.line;
    let taken = match apart_sessions::take_for_fold(&ground.db, self.role, session, share).await {
      Ok(Some(taken)) => taken,
      Ok(None) => return,
      Err(why) => {
        eprintln!("{line}: whether its session is due a fold could not be read: {why:?}");
        return;
      }
    };
    let mut command = self.command;

    command.args = resumed_command(&command.args, &taken);

    let mut tail = Tail::at_end(record_of(ground, &command, &taken));
    let from = tail.count();
    let fold = Apart {
      agent: self.agent,
      role: self.role,
      line,
      provider: self.provider,
      title: DOOR_WORDS.fold_title.clone(),
      command,
      session: taken.clone(),
      prompt: FOLD_WORD.to_owned(),
      model: self.model,
      effort: self.effort,
      account: self.account,
      parent: self.parent,
      root: self.root,
      run: None,
      resume: None,
      resume_else: None,
    };
    let ended = run_on(ground, herdr, fold, &mut Quiet).await;
    let compacted = compacted_since(tail.look(), from);

    // Whatever a consumer sent the session to hold is folded away once the fold reached it, or may have.
    if compacted || ended.is_ok() {
      ground.events.publish(SessionFolded {
        session: taken.clone(),
      });
    }

    let given = if compacted {
      println!("compact {line} {taken}: compacted");
      apart_sessions::folded(&ground.db, &taken, now_ms()).await
    } else {
      eprintln!("compact {line} {taken}: no compacted marker, the session stands as it was");
      apart_sessions::give_back(&ground.db, &taken).await
    };

    if let Err(why) = given {
      eprintln!("{line}: the session it folded was not given back: {why:?}");
    }
  }
}

/// The place of a run's own its hooks write into, where its line holds a turn's end (docs/agent-control.md,
/// 「What a resident turn reads」): a pane of its own stands in no agent's place, so the stop hook would have no
/// run to hold. Named after the run's agent and session and marked as a thrown-away place, so the sweep as a
/// server starts takes down one its process left behind. The place's settings are the command's own with the
/// status line and hooks over them; the command handed back names them in place of its own. None for a line
/// that holds nothing.
pub fn hooks_place(ground: &Ground, apart: &Apart<'_>) -> Result<Option<(String, Vec<String>)>, String> {
  if !RUN_LINES.holds_stops(apart.line) {
    return Ok(None);
  }

  let mut args = apart.command.args.clone();
  let Some(at) = args
    .iter()
    .position(|arg| arg == "--settings")
    .map(|at| at + 1)
    .filter(|at| *at < args.len())
  else {
    return Err(format!("{}: the run's command names no settings to hold its stops by", apart.line));
  };
  let session = agent_session::SessionId(format!("{}@{}", apart.agent, apart.session));
  let name = crate::apart_places::place_name(&session);

  crate::apart_places::mark(ground, &name, &session, None)?;

  let given = read_settings(&Path::new(&apart.command.cwd).join(&args[at]));
  let place = Place::of(&ground.roots.data, &name);
  let settings = write_place_settings(&place.path, given, &status_root(ground), Holds::of_line(apart.line))
    .map_err(|why| why.detail)?;

  args[at] = settings.to_string_lossy().into_owned();

  Ok(Some((name, args)))
}

// The run on the session settled for it.
async fn run_on<W: Watch>(
  ground: &Ground,
  herdr: Herdr,
  apart: Apart<'_>,
  watch: &mut W,
) -> Result<Ending, String> {
  let name = apart
    .run
    .clone()
    .unwrap_or_else(|| format!("{}-{}", apart.line, apart.session.chars().take(8).collect::<String>()));
  let mut env = pane_environment(ground.env(), &apart.command, &name);

  if let Some(why) = launch_refusal(apart.line, &apart.command.args, &env, &ground.machine()) {
    return Err(why);
  }

  // A pane handed a folder that is not there starts its program in the home instead, whose record is not
  // the one the run is read from.
  if !Path::new(&apart.command.cwd).is_dir() {
    return Err(format!("{}: the run's folder {} is not there", apart.line, apart.command.cwd));
  }

  let resident = harness_providers::provider("claude-code")
    .and_then(|declared| declared.declared().resident.as_ref())
    .ok_or_else(|| "Claude Code stands no resident program".to_owned())?;
  let deadline = u64::try_from(now_ms()).unwrap_or_default() + TOKEN_BACKSTOP.as_millis() as u64;
  let token = token::issue_for_run(
    ground,
    apart.line,
    apart.agent,
    &apart.command.cwd,
    &env,
    Some((apart.provider, apart.model.as_str())),
    deadline,
  )
  .await
  .map_err(|why| why.detail)?;

  if let Some(token) = &token {
    env.insert(token::TOKEN_VARIABLE.to_owned(), token.clone());
  }

  let hooks = match hooks_place(ground, &apart) {
    Ok(hooks) => hooks,
    Err(why) => {
      if let Some(token) = &token {
        token::revoke(ground, token).await;
      }

      return Err(why);
    }
  };

  // A program brought up on a configuration directory nobody has opened before stops on its first-run
  // screens before it reads a word; the machine's own login keeps its configuration in the home.
  let configured = apart
    .command
    .env
    .get("CLAUDE_CONFIG_DIR")
    .cloned()
    .flatten()
    .filter(|dir| !dir.is_empty());

  if let Some(dir) = configured {
    mark_onboarded(&config_file(Path::new(&dir)));
  }

  let own = own_unit(ground.env());
  let turn = Turn {
    name,
    title: apart.title,
    seat: Seat::Own(herdr),
    cwd: apart.command.cwd.clone(),
    command: hooks.as_ref().map_or_else(|| apart.command.args.clone(), |(_, args)| args.clone()),
    env,
    prompt: apart.prompt,
    session: apart.session,
    identity: Identity {
      agent: None,
      root: apart.root.or_else(|| Some(execution_root(own.as_deref().unwrap_or_default()))),
      parent: apart.parent.or(own),
      role: Some(apart.role.to_owned()),
      model: Some(apart.model),
      effort: (!apart.effort.is_empty()).then_some(apart.effort),
    },
    line: apart.line.to_owned(),
    provider: apart.provider.to_owned(),
    lead: RESIDENT_TURN.apart_lead.clone(),
    // A run apart works in a folder of its line's own or one made for it, and a line told to ask no
    // permission there trusts the folder by the same decision.
    trusts_folder: true,
    clock: None,
    interrupt: interrupt_of(resident),
    timings: Timings::default(),
  };

  // The run goes into its hooks' place before its words go in, so the prompt the program takes is the bot's.
  if let Some((place, _)) = &hooks
    && let Err(why) = begin_run(
      &Place::of(&ground.roots.data, place).path,
      &turn.name,
      &turn.title,
      now_ms(),
      run_variables(&turn.env),
    )
  {
    eprintln!("{}: the place its hooks write into: {why}", turn.name);
  }

  let ended = put(ground, &turn, watch).await;

  if let Some(token) = &token {
    token::revoke(ground, token).await;
  }

  if let Some((place, _)) = &hooks {
    crate::apart_places::end(ground, place).await;
  }

  ended
}

/// How a run ended, as its run log's end says it: its answer where it gave one, why its words never became
/// a turn where they did not, and otherwise the word its ending goes by or why it never got under way.
pub fn said(ended: &Result<Ending, String>) -> Result<&str, &str> {
  match ended {
    Ok(Ending::Answered { reply, .. }) => Ok(&reply.text),
    Ok(Ending::Errored { error, .. }) => Err(error),
    Ok(Ending::Unsubmitted { why, .. })
    | Ok(Ending::ProgramLost { why, .. })
    | Ok(Ending::SilentPeer { why, .. }) => Err(why),
    Ok(other) => Err(other.word()),
    Err(why) => Err(why),
  }
}

/// A run's ending in the words a walk of logins reads: its reply where it answered, and otherwise what kept
/// it from answering — the program's own error read as a login's failure where it is one, a stop, the pane
/// or the program failing under it, a person's question, or a run that ended with no answer. `key` names the
/// run, which a gone login's failure names.
pub fn answered(ending: Ending, key: &str) -> Result<Reply, Failure> {
  let died = |message: String| Failure::Died {
    ending: RunEnding::Exit { code: 1 },
    said: None,
    message,
  };

  match ending {
    Ending::Answered { reply, .. } => Ok(*reply),
    Ending::Errored { error, .. } => Err(of_errored(&error, key)),
    Ending::Stopped { .. } => Err(Failure::Stopped),
    Ending::Vanished { .. } => Err(Failure::Transport {
      message: format!("{key}: the run's pane went away under it"),
    }),
    // The program the run was put into failed under it, as a pane gone does: the road and the work are
    // not what failed.
    Ending::ProgramLost { why, .. } | Ending::SilentPeer { why, .. } => Err(Failure::Transport {
      message: format!("{key}: {why}"),
    }),
    Ending::Blocked { .. } => Err(Failure::Person),
    Ending::Untrusted { folder, config, .. } => Err(Failure::Untrusted { folder, config }),
    Ending::Overran { .. } => Err(died(format!("{key}: the run was cut on its caller's verdict"))),
    Ending::Silent { .. } => {
      Err(died(format!("{key}: the run came back to rest with no answer in its record")))
    }
    Ending::Unsubmitted { why, .. } => {
      Err(died(format!("{key}: the program never took the run's words: {why}")))
    }
  }
}
