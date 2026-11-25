//! One turn of Codex's interactive program standing in its agent's pane (docs/agent-control.md, 「What a
//! resident turn reads」): the program stood under a Codex home of agency's own that shares the user's
//! login and threads, the turn typed in as a person would, and read back out of the thread's rollout
//! until the task it started has completed or been aborted. A thread the program opened for itself is
//! its thread from then on: the program standing is relabelled as the one on it.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use agent_host::place::ResidentCommand;
use cache_siren::codex::windows_of_rollout;
use policy::providers::Resident;
use policy::resident_turn::RESIDENT_TURN;
use serde_json::{Value, json};

use super::guard::prompt_refusal;
use super::looks::{GoneConfirm, Look, screen_of, status_of};
use super::place_run::{begin_run, end_run, resident_environment, run_variables};
use super::record::{ModelSpend, Reply, ReplyWindow, RequestUsage};
use super::stand::{interrupt_program, stand_resident};
use super::surface::{Execution, Run, SurfaceEnding, SurfaceTurn, interrupt_of};
use super::turn::{Watch, run_header};
use crate::lines::place::{Place, Program, command_digest};
use crate::now_ms;
use crate::places::Ground;

const RUN_NAME: &str = "BMO_RUN_NAME";
// How many of the newest days' rollouts are looked through for the thread a program opened now.
const OPENED_DAYS: usize = 2;
static NOTHING: Value = Value::Null;

/// The two Codex homes: the one the program runs under, agency's own under the data root, and the user's,
/// whose login and threads it shares.
pub struct Homes {
  pub resident: PathBuf,
  pub user: PathBuf,
}

impl Homes {
  pub fn of(ground: &Ground) -> Self {
    Self {
      resident: ground.roots.data.join("codex-home"),
      user: PathBuf::from(ground.machine().var("HOME").unwrap_or_default()).join(".codex"),
    }
  }
}

/// The resident home made ready for a program started in `cwd`: the shared login and threads linked,
/// and the folder trusted in its own configuration, which keeps the trust screen from coming up — the
/// screen appears even with a trust override on the command line, and answering it writes this same
/// entry (measured).
pub fn prepare_home(cwd: &str, homes: &Homes) -> std::io::Result<()> {
  agent_host::disk::private_dir(&homes.resident)?;
  agent_host::disk::private_dir(&homes.user.join("sessions"))?;

  for shared in ["auth.json", "sessions"] {
    let link = homes.resident.join(shared);

    if std::fs::symlink_metadata(&link).is_err() {
      std::os::unix::fs::symlink(homes.user.join(shared), &link)?;
    }
  }

  let config = homes.resident.join("config.toml");
  let held = std::fs::read_to_string(&config).unwrap_or_default();
  let trusted = held
    .parse::<toml::Table>()
    .ok()
    .and_then(|table| table.get("projects")?.as_table()?.get(cwd).cloned())
    .is_some();

  if trusted {
    return Ok(());
  }

  let key = serde_json::to_string(cwd).map_err(std::io::Error::other)?;
  let between = if held.is_empty() || held.ends_with('\n') { "" } else { "\n" };

  agent_host::disk::publish_bytes(
    &config,
    format!("{held}{between}[projects.{key}]\ntrust_level = \"trusted\"\n").as_bytes(),
  )
}

fn sessions_of(home: &Path) -> PathBuf {
  home.join("sessions")
}

/// The rollout of one thread, wherever the day it began filed it.
pub fn rollout_of(root: &Path, thread: &str) -> Option<PathBuf> {
  if thread.is_empty() || !thread.chars().all(|one| one.is_ascii_alphanumeric() || one == '-') {
    return None;
  }

  let pattern = format!("{}/**/rollout-*{thread}.jsonl", glob::Pattern::escape(&root.to_string_lossy()));
  let found: Vec<PathBuf> = glob::glob(&pattern).ok()?.filter_map(Result::ok).collect();

  match found.as_slice() {
    [one] => Some(one.clone()),
    _ => None,
  }
}

fn newest_first(path: &Path) -> Vec<String> {
  let mut names: Vec<String> = std::fs::read_dir(path)
    .map(|entries| {
      entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
    })
    .unwrap_or_default();

  names.sort();
  names.reverse();
  names
}

/// Every rollout written in the newest few days, which is where the rollout of a thread opened now is.
fn recent_rollouts(root: &Path, days: usize) -> Vec<PathBuf> {
  let mut found = vec![];
  let mut seen = 0;

  for year in newest_first(root) {
    for month in newest_first(&root.join(&year)) {
      for day in newest_first(&root.join(&year).join(&month)) {
        if seen >= days {
          return found;
        }

        seen += 1;

        let dir = root.join(&year).join(&month).join(&day);

        found.extend(
          newest_first(&dir)
            .into_iter()
            .filter(|name| name.starts_with("rollout-"))
            .map(|name| dir.join(name)),
        );
      }
    }
  }

  found
}

fn rollout_lines(path: &Path) -> Vec<Value> {
  read_lines(&std::fs::read_to_string(path).unwrap_or_default())
}

fn read_lines(text: &str) -> Vec<Value> {
  text
    .split('\n')
    .filter(|line| !line.trim().is_empty())
    .filter_map(|line| serde_json::from_str(line).ok())
    .collect()
}

/// The rollout's lines as the program wrote them, each one it has ended: what follows the last newline is a
/// line still being written, left for the next look.
pub fn whole_lines(text: &str) -> Vec<&str> {
  let mut lines: Vec<&str> = text.split('\n').collect();

  lines.pop();
  lines.into_iter().filter(|line| !line.trim().is_empty()).collect()
}

fn kind(line: &Value) -> Option<&str> {
  line.get("type").and_then(Value::as_str)
}

fn modified_ms(path: &Path) -> Option<i64> {
  let modified = std::fs::metadata(path).ok()?.modified().ok()?;

  Some(modified.duration_since(UNIX_EPOCH).ok()?.as_millis() as i64)
}

/// The rollout of the thread a program opened for itself: one that was not there before the prompt,
/// begun in the directory the program stands in.
pub fn opened_rollout(root: &Path, before: &HashSet<PathBuf>, cwd: &str, since: i64) -> Option<PathBuf> {
  recent_rollouts(root, OPENED_DAYS).into_iter().find(|path| {
    !before.contains(path)
      && modified_ms(path).is_some_and(|at| at >= since - 1_000)
      && rollout_lines(path)
        .iter()
        .find(|line| kind(line) == Some("session_meta"))
        .is_some_and(|meta| meta.pointer("/payload/cwd").and_then(Value::as_str) == Some(cwd))
  })
}

/// Where a turn read out of a rollout stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
  Working,
  Ended,
  Interrupted,
}

/// A turn read out of what the rollout added since it went in.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
  pub state: State,
  /// The program's own words where it wrote them in the answer's place.
  pub error: Option<String>,
  pub reply: Reply,
}

fn count(value: Option<&Value>) -> i64 {
  value
    .and_then(Value::as_f64)
    .filter(|value| value.is_finite() && *value > 0.0)
    .map_or(0, |value| value.floor() as i64)
}

/// What a failed turn's error says, in the words the fallback road reads for a limit or an outage: the
/// program's own code for it where it wrote one, and its message beside it.
pub fn error_text(error: &Value) -> String {
  if let Some(text) = error.as_str() {
    return text.to_owned();
  }

  let said: Vec<&str> = ["codex_error_info", "message"]
    .iter()
    .filter_map(|key| error.get(key).and_then(Value::as_str))
    .collect();

  if said.is_empty() { error.to_string() } else { said.join(": ") }
}

/// The turn the rollout's lines since the prompt hold: the last task started and what it came to. The
/// rollout records nothing of a word held on the screen, and a task never completes with one still
/// held, so an ended task has nothing left queued (measured).
pub fn read_rollout(lines: &[Value], run: &Run, provider: &str) -> Reading {
  let events: Vec<&Value> = lines
    .iter()
    .filter(|line| kind(line) == Some("event_msg"))
    .map(|line| line.get("payload").unwrap_or(&NOTHING))
    .collect();
  let turn = events
    .iter()
    .rposition(|event| kind(event) == Some("task_started"))
    .map_or(events.as_slice(), |start| &events[start..]);
  let completed = turn.iter().rev().find(|event| kind(event) == Some("task_complete"));
  let aborted = turn.iter().any(|event| kind(event) == Some("turn_aborted"));
  let said = turn
    .iter()
    .filter(|event| kind(event) == Some("agent_message"))
    .filter_map(|event| event.get("message").and_then(Value::as_str))
    .next_back();
  let text = completed
    .and_then(|event| event.get("last_agent_message").and_then(Value::as_str))
    .or(said)
    .unwrap_or_default();
  let failed = completed
    .and_then(|event| event.get("error"))
    .filter(|error| !error.is_null())
    .map(error_text);
  // This turn's own counts: a failed request writes one with no figures, and a compaction inside the
  // turn one with none of input, and neither is a request.
  let counts: Vec<&&Value> = turn.iter().filter(|event| kind(event) == Some("token_count")).collect();
  let requests: Vec<RequestUsage> = counts
    .iter()
    .filter_map(|event| event.pointer("/info/last_token_usage"))
    .filter(|usage| count(usage.get("input_tokens")) != 0)
    .map(|usage| {
      let input = count(usage.get("input_tokens"));
      let read = count(usage.get("cached_input_tokens"));
      let write = count(usage.get("cache_write_input_tokens"));

      RequestUsage {
        input_tokens: (input - read - write).max(0),
        cache_read_tokens: read,
        cache_write_tokens: write,
        output_tokens: count(usage.get("output_tokens")),
      }
    })
    .collect();
  let input: i64 = requests.iter().map(|one| one.input_tokens).sum();
  let read: i64 = requests.iter().map(|one| one.cache_read_tokens).sum();
  let write: i64 = requests.iter().map(|one| one.cache_write_tokens).sum();
  let output: i64 = requests.iter().map(|one| one.output_tokens).sum();
  // The windows the newest count carried, which a failed request writes too.
  let windows = counts
    .iter()
    .rev()
    .find_map(|event| event.get("rate_limits").filter(|limits| !limits.is_null()))
    .map(windows_of_rollout)
    .unwrap_or_default();
  let model = lines
    .iter()
    .filter(|line| kind(line) == Some("turn_context"))
    .filter_map(|line| line.pointer("/payload/model").and_then(Value::as_str))
    .next_back()
    .map_or_else(|| run.model.clone(), str::to_owned);
  let session = lines
    .iter()
    .filter(|line| kind(line) == Some("session_meta"))
    .filter_map(|line| line.pointer("/payload/id").and_then(Value::as_str))
    .next_back()
    .map(str::to_owned)
    .or_else(|| run.resume_session_id.clone());
  let tokens = input + output + read + write;

  Reading {
    state: if aborted {
      State::Interrupted
    } else if completed.is_some() {
      State::Ended
    } else {
      State::Working
    },
    reply: Reply {
      mode: provider.to_owned(),
      model: Some(model.clone()),
      effort: Some(run.effort.clone()),
      text: if failed.is_none() { text.trim().to_owned() } else { String::new() },
      session_id: session,
      context_ratio: None,
      context_tokens: None,
      session_usage: None,
      windows: windows.into_iter().map(ReplyWindow::Account).collect(),
      spend: if tokens == 0 {
        vec![]
      } else {
        vec![ModelSpend {
          mode: provider.to_owned(),
          model,
          tokens,
          input_tokens: input,
          output_tokens: output,
          cache_read_tokens: read,
          cache_write_tokens: write,
          cost: 0.0,
        }]
      },
      live_tasks: vec![],
      workflows: vec![],
      account: None,
      last_request_usage: requests.last().cloned(),
      request_usages: (!requests.is_empty()).then_some(requests),
      answers: Vec::new(),
    },
    error: failed,
  }
}

/// The command a program that opened a thread for itself is started on once it is on that thread.
pub fn resumed(command: &ResidentCommand, thread: &str) -> ResidentCommand {
  let mut args = command.args.clone();

  args.splice(1.min(args.len())..1.min(args.len()), ["resume".to_owned(), thread.to_owned()]);

  ResidentCommand {
    args,
    ..command.clone()
  }
}

fn program_of(command: ResidentCommand, restarts: bool) -> Program {
  Program {
    identity: command_digest(&command),
    restart: restarts.then(|| command.clone()),
    command,
    opened: None,
  }
}

/// One turn of the provider's Codex program, from standing it to its answer, its rollout handed to the
/// watch as it is written.
pub async fn turn<W: Watch>(
  ground: &Ground,
  provider: &str,
  resident: &Resident,
  turn: &SurfaceTurn,
  watch: &mut W,
) -> Result<SurfaceEnding, String> {
  let homes = Homes::of(ground);

  prepare_home(&turn.run.cwd, &homes).map_err(|why| format!("{}: {why}", homes.resident.display()))?;

  let place = Place::of(&ground.roots.data, &turn.home.agent);
  // The program stands across many runs, so it is started without this run's own variables and with
  // the place its processes read them from; the run puts them there before its prompt.
  let mut env = turn.command.env.clone();

  env.insert("CODEX_HOME".to_owned(), homes.resident.to_string_lossy().into_owned());

  let standing = ResidentCommand {
    env: resident_environment(&env, &place.path),
    ..turn.command.clone()
  };
  let mut own = run_variables(&turn.run.env);

  own.insert(RUN_NAME.to_owned(), turn.name.clone());

  let program = program_of(standing.clone(), turn.run.resume_session_id.is_some());
  let mut execution =
    Execution::begin(ground, provider, turn, turn.run.resume_session_id.as_deref(), &standing.args).await?;

  execution.save(ground, turn).await;
  watch.opened(execution.row.id()).await;

  let ended = put(ground, provider, resident, turn, &place, &program, own, &mut execution, watch).await;

  let _ = end_run(&place.path, now_ms());
  execution.close(ground, turn, ended.is_err()).await;
  ended
}

#[allow(clippy::too_many_arguments)]
async fn put<W: Watch>(
  ground: &Ground,
  provider: &str,
  resident: &Resident,
  turn: &SurfaceTurn,
  place: &Place,
  program: &Program,
  own: BTreeMap<String, String>,
  execution: &mut Execution,
  watch: &mut W,
) -> Result<SurfaceEnding, String> {
  let herdr = &turn.home.herdr;
  let every = turn.clocks.poll.unwrap_or(Duration::from_millis(RESIDENT_TURN.poll_ms));
  let root = sessions_of(&Homes::of(ground).resident);
  let interrupt = interrupt_of(resident);
  let shared: &Execution = execution;
  let stood = match stand_resident(
    ground,
    place,
    &turn.home,
    program,
    &turn.run.env,
    &turn.run.line,
    Some((provider, turn.run.model.as_str())),
    every,
    || shared.cancelled(ground, turn),
  )
  .await
  {
    Ok(stood) => stood,
    Err(fault) => {
      return Ok(SurfaceEnding::Error(format!(
        "codex's program did not stand: {}: {}",
        fault.code, fault.message
      )));
    }
  };
  let pane = stood.pane;

  execution.set("paneId", json!(pane));

  // Ready once herdr reads its input box — idle, or done after a turn that ended, failed or was
  // interrupted — and the screen is not the trust question herdr reads the same way. Waited for until
  // then, the program stands at a question, its pane is gone or the turn is stopped.
  loop {
    if execution.cancelled(ground, turn).await {
      execution.ending = "cancelled";

      return Ok(SurfaceEnding::Error("the turn was stopped before it went in".to_owned()));
    }

    match status_of(herdr, &pane, GoneConfirm::default()).await {
      Look::Gone => return Err("codex's pane went away before its turn".to_owned()),
      // Herdr answering none of the looks says nothing of the program, and never that a person is asked
      // something: it is waited out.
      Look::Silent(_) => {}
      now if now.at_rest() => {
        let screen = screen_of(herdr, &pane).await;
        let flat = screen.split_whitespace().collect::<Vec<_>>().join(" ");

        if !resident
          .trust_question
          .as_ref()
          .is_some_and(|question| flat.contains(question.as_str()))
        {
          break;
        }

        execution.ending = "cancelled";

        return Ok(SurfaceEnding::Error(format!("codex's program is waiting on a person: {screen}")));
      }
      now => {
        if now.is("blocked") {
          execution.ending = "cancelled";

          return Ok(SurfaceEnding::Error(format!(
            "codex's program is waiting on a person: {}",
            screen_of(herdr, &pane).await
          )));
        }
      }
    }

    tokio::time::sleep(every).await;
  }

  let started = place.running();
  let mut path = turn.run.resume_session_id.as_deref().and_then(|thread| rollout_of(&root, thread));
  let held = path.as_deref().map(std::fs::read_to_string).and_then(Result::ok).unwrap_or_default();
  let mut from = read_lines(&held).len();
  // How many of the rollout's whole lines the watch has been handed, or stood before the prompt.
  let mut fed = whole_lines(&held).len();
  let before: HashSet<PathBuf> = if path.is_none() {
    recent_rollouts(&root, OPENED_DAYS).into_iter().collect()
  } else {
    HashSet::new()
  };
  let prompted_at = now_ms();
  let resident_of = |path: &Option<PathBuf>, from: usize| {
    json!({
      "place": place.path,
      "record": path.as_ref().map_or_else(String::new, |path| path.to_string_lossy().into_owned()),
      "from": from,
      "pid": started.as_ref().map(|running| running.pid),
      "birth": started.as_ref().and_then(|running| running.birth.clone()),
    })
  };

  if execution.cancelled(ground, turn).await {
    execution.ending = "cancelled";

    return Ok(SurfaceEnding::Error("the turn was stopped before it went in".to_owned()));
  }

  execution.set("resident", resident_of(&path, from));

  let _ = begin_run(&place.path, &turn.name, &turn.title, prompted_at, own);

  if let Some(why) = prompt_refusal(&ground.machine()) {
    return Err(why);
  }

  let text = format!("{}\n\n{}", run_header(&turn.name, &turn.title, prompted_at), turn.run.prompt);

  if let Err(fault) = herdr.call("agent.prompt", json!({ "target": pane, "text": text }), None).await {
    execution.ending = "cancelled";

    return Ok(SurfaceEnding::Error(format!(
      "codex's program would not take the prompt: {}: {}",
      fault.code, fault.message
    )));
  }

  execution.set("state", json!("running"));
  execution.save(ground, turn).await;

  let reading = loop {
    if execution.cancelled(ground, turn).await {
      interrupt_program(place, &turn.home, &pane, &interrupt).await;
      execution.ending = "cancelled";

      return Ok(SurfaceEnding::Error("the turn was stopped".to_owned()));
    }

    let now = status_of(herdr, &pane, GoneConfirm::default()).await;

    if now == Look::Gone {
      return Err("codex's pane went away during the turn".to_owned());
    }

    // A herdr that does not answer is waited out while the rollout is read on, however long it is
    // silent, and never read as a pane gone.
    let alive = place.program_alive();

    if alive.is_none()
      || started
        .as_ref()
        .is_some_and(|started| alive.as_ref().is_some_and(|alive| alive.pid != started.pid))
    {
      return Err("codex's program went away during the turn".to_owned());
    }

    if path.is_none() {
      path = opened_rollout(&root, &before, &turn.run.cwd, prompted_at);
      from = 0;
      fed = 0;

      if path.is_some() {
        execution.set("resident", resident_of(&path, from));
        execution.save(ground, turn).await;
      }
    }

    if let Some(path) = &path {
      let text = std::fs::read_to_string(path).unwrap_or_default();
      let written = whole_lines(&text);

      // What the rollout gained since the last look is the turn's record as it goes.
      if written.len() > fed {
        let gained: Vec<String> = written[fed..].iter().map(|line| (*line).to_owned()).collect();

        watch.record(&gained).await;
        fed = written.len();
      }

      let reading = read_rollout(read_lines(&text).get(from..).unwrap_or_default(), &turn.run, provider);

      if reading.state != State::Working {
        break reading;
      }
    }

    tokio::time::sleep(every).await;
  };

  execution.ending = if reading.state == State::Interrupted { "cancelled" } else { "exited" };

  // A thread the program opened for itself is its thread from now on: the program standing is relabelled
  // as the one on it, so the next turn resuming it finds it rather than restarting it.
  if turn.run.resume_session_id.is_none()
    && let Some(thread) = &reading.reply.session_id
  {
    let on = program_of(resumed(&program.command, thread), true);

    place.adopt(&program.identity, &on).map_err(|why| why.detail)?;
  }

  Ok(if reading.state == State::Interrupted {
    SurfaceEnding::Error("the turn was interrupted".to_owned())
  } else if let Some(error) = reading.error {
    SurfaceEnding::Error(error)
  } else if reading.reply.text.is_empty() {
    SurfaceEnding::Error("codex wrote no answer".to_owned())
  } else {
    SurfaceEnding::Reply(Box::new(reading.reply))
  })
}
