//! One turn of a pane program that carries its own turns (docs/agent-control.md, 「What a resident turn
//! reads」): the terminal client stood on its command, the turn put into its inbox, and its record read until that turn
//! has ended. The pane program keeps one dsh session across turns; the turn carries only its own variables,
//! the key is the program's, read from its own environment, and nothing else of the caller's goes into the
//! inbox.

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

use policy::providers::Resident;
use policy::resident_turn::RESIDENT_TURN;
use serde_json::{Value, json};

use super::place_run::{begin_run, end_run, run_variables};
use super::record::{ModelSpend, Reply};
use super::stand::{interrupt_program, stand_resident};
use super::surface::{Execution, Run, SurfaceEnding, SurfaceTurn, interrupt_of};
use super::turn::Watch;
use crate::lines::dsh;
use crate::lines::place::{Place, Program, command_digest, is_turn_variable};
use crate::now_ms;
use crate::places::Ground;
use crate::session_tools;

const RUN_NAME: &str = "BMO_RUN_NAME";
/// The gateway address a turn's tap stands at, which travels with the turn rather than the pane program.
pub const LOCAL_URL: &str = "BMO_DSH_LOCAL_URL";

/// The record the pane program keeps in its agent's place.
pub fn record_path(place: &Path) -> PathBuf {
  place.join("loop.jsonl")
}

fn inbox_dir(place: &Path) -> PathBuf {
  place.join("inbox")
}

/// Whether a variable belongs to one turn rather than to the pane program: the gateway address its tap
/// stands at, and the run's own variables. They travel with the turn through the inbox, so the program's
/// own command — and so its identity — stays the same from one turn to the next.
pub fn is_inbox_turn_variable(name: &str) -> bool {
  name == LOCAL_URL || is_turn_variable(name)
}

/// The lines of the pane program's record.
pub fn read_record(place: &Path) -> Vec<Value> {
  record_of(place).1
}

/// The lines of the pane program's record as it wrote them and as they read, each one it has ended: what follows
/// the last newline is a line still being written, left for the next look.
pub fn record_of(place: &Path) -> (Vec<String>, Vec<Value>) {
  let text = std::fs::read_to_string(record_path(place)).unwrap_or_default();
  let mut lines: Vec<&str> = text.split('\n').collect();

  lines.pop();
  lines
    .into_iter()
    .filter(|line| !line.trim().is_empty())
    .filter_map(|line| serde_json::from_str(line).ok().map(|read| (line.to_owned(), read)))
    .unzip()
}

fn kind(record: &Value) -> Option<&str> {
  record.get("kind").and_then(Value::as_str)
}

/// Where the lines of the record that belong to the last turn put in under a name stand: from its start to
/// the next turn's start.
pub fn span_named(records: &[Value], name: &str) -> Option<Range<usize>> {
  let start = records.iter().rposition(|record| {
    kind(record) == Some("start") && record.get("name").and_then(Value::as_str) == Some(name)
  })?;
  let next = records[start + 1..].iter().position(|record| kind(record) == Some("start"));

  Some(start..next.map_or(records.len(), |next| start + 1 + next))
}

/// Where the lines of the record that belong to a turn put in under a name stand, the turns put in under that
/// name before it being `before`: from its start to the next turn's start.
pub fn span_named_after(records: &[Value], name: &str, before: usize) -> Option<Range<usize>> {
  let start = records
    .iter()
    .enumerate()
    .filter(|(_, record)| {
      kind(record) == Some("start") && record.get("name").and_then(Value::as_str) == Some(name)
    })
    .nth(before)?
    .0;
  let next = records[start + 1..].iter().position(|record| kind(record) == Some("start"));

  Some(start..next.map_or(records.len(), |next| start + 1 + next))
}

/// The context the pane program's session stands at, as its record says of its last answer, and the window
/// the record's latest context names (docs/sessions.md, 「Session compaction」): the `used` the answer's context
/// names, or where it names none — the program writes it only on a turn of several requests — the input,
/// cache-read and output tokens of the answer's one request. None where the record holds no answer or names
/// no window.
pub fn latest_context(place: &Path) -> Option<(u64, u64)> {
  let records = read_record(place);
  let context = |record: &Value, name: &str| {
    record
      .get("context")
      .and_then(|context| context.get(name))
      .and_then(Value::as_u64)
      .filter(|n| *n > 0)
  };
  let window = records
    .iter()
    .rev()
    .filter(|record| kind(record) == Some("answer"))
    .find_map(|record| context(record, "size"))?;
  let last = records.iter().rev().find(|record| kind(record) == Some("answer"))?;
  let carried = context(last, "used").or_else(|| {
    let usage = last.get("usage")?;
    let tokens = |name: &str| usage.get(name).and_then(Value::as_u64).unwrap_or(0);
    let sum = tokens("inputTokens") + tokens("cacheReadTokens") + tokens("outputTokens");

    (sum > 0).then_some(sum)
  })?;

  Some((carried, window))
}

/// Whether the place's pane program is working on a turn, as its record says: its last turn, the bot's or
/// a person's, started and has not ended. A place whose program keeps no record says nothing of the kind.
pub fn turn_working(place: &Path) -> bool {
  let records = read_record(place);

  records
    .iter()
    .rposition(|record| kind(record) == Some("start"))
    .is_some_and(|start| read_turn_on(&records[start..], "", "").0 == State::Working)
}

/// Whether a turn put in under a name still waits in the inbox, the program not having taken it yet.
pub fn waiting_in_inbox(place: &Path, name: &str) -> bool {
  std::fs::read_dir(inbox_dir(place)).into_iter().flatten().flatten().any(|entry| {
    std::fs::read(entry.path())
      .ok()
      .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
      .is_some_and(|turn| turn.get("name").and_then(Value::as_str) == Some(name))
  })
}

/// The provider a turn's answer names in its model choice, `[provider, model]`, where it names one.
pub fn provider_named(records: &[Value]) -> Option<String> {
  let choice = records
    .iter()
    .rev()
    .find(|record| kind(record) == Some("answer"))?
    .get("model")?
    .as_str()?
    .to_owned();

  serde_json::from_str::<Vec<String>>(&choice).ok()?.into_iter().next()
}

/// Where the lines of the record that belong to one turn stand: from its start to the next turn's start.
pub fn span_of(records: &[Value], id: &str) -> Option<Range<usize>> {
  let start = records.iter().position(|record| {
    kind(record) == Some("start") && record.get("id").and_then(Value::as_str) == Some(id)
  })?;
  let next = records[start + 1..].iter().position(|record| kind(record) == Some("start"));

  Some(start..next.map_or(records.len(), |next| start + 1 + next))
}

/// The lines of the record that belong to one turn.
pub fn lines_of<'a>(records: &'a [Value], id: &str) -> Option<&'a [Value]> {
  span_of(records, id).map(|span| &records[span])
}

/// A turn put into the inbox: written whole, under a name that sorts it after every turn put in
/// before it.
fn put_turn(place: &Path, turn: &Value, id: &str) -> std::io::Result<()> {
  let inbox = inbox_dir(place);

  agent_host::disk::private_dir(&inbox)?;

  let mut bytes = serde_json::to_vec(turn).map_err(std::io::Error::other)?;

  bytes.push(b'\n');
  agent_host::disk::publish_bytes(&inbox.join(format!("{:015}-{id}.json", now_ms())), &bytes)
}

/// Where a turn read out of the pane program's record stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
  Working,
  Ended,
  Interrupted,
}

/// A turn of the pane program out of the lines its record holds of it, as the provider the program
/// stands for carried it. The program keeps one dsh session across turns and takes nothing while a turn
/// works, so nothing is ever left queued; the record carries the answer's words and model and no figures,
/// so a turn read from it reports no tokens, money or window.
pub fn read_turn(records: &[Value], run: &Run, provider: &str) -> (State, Option<String>, Reply) {
  read_turn_on(records, &run.model, provider)
}

/// A turn of the pane program out of the lines its record holds of it, as `read_turn` reads one, with the
/// model a reply names where the record names none.
pub fn read_turn_on(records: &[Value], model: &str, provider: &str) -> (State, Option<String>, Reply) {
  let turn = records
    .iter()
    .rposition(|record| kind(record) == Some("start"))
    .map_or(records, |start| &records[start..]);
  let failed = turn.iter().rev().find(|record| kind(record) == Some("failed"));
  let answer = turn.iter().rev().find(|record| kind(record) == Some("answer"));
  // The record names the model as the session's choice, `[provider, model]`, where the program was started
  // on one; the model's own id is what a reply and a spend row carry.
  let model = answer
    .and_then(|answer| answer.get("model").and_then(Value::as_str))
    .filter(|choice| !choice.is_empty())
    .map_or_else(
      || model.to_owned(),
      |choice| {
        serde_json::from_str::<Vec<String>>(choice)
          .ok()
          .and_then(|parts| parts.get(1).cloned())
          .unwrap_or_else(|| choice.to_owned())
      },
    );
  // Any finite number the record wrote counts, cut to its whole part, since a spend holds whole tokens.
  let tokens = |name: &str| {
    answer
      .and_then(|answer| answer.get("usage"))
      .and_then(|usage| usage.get(name))
      .and_then(Value::as_f64)
      .filter(|count| count.is_finite())
      .map_or(0, |count| count as i64)
  };
  // The share read from the cache counts toward the turn's tokens as the progress line shows it, and is
  // none where the upstream reported none.
  let (input, output, cache_read) =
    (tokens("inputTokens"), tokens("outputTokens"), tokens("cacheReadTokens"));
  let state = if turn.iter().any(|record| kind(record) == Some("interrupted")) {
    State::Interrupted
  } else if turn.iter().any(|record| matches!(kind(record), Some("end" | "failed"))) {
    State::Ended
  } else {
    State::Working
  };
  let error =
    failed.map(|failed| failed.get("text").and_then(Value::as_str).unwrap_or("dsh failed").to_owned());
  let text = if failed.is_none() {
    answer
      .and_then(|answer| answer.get("text").and_then(Value::as_str))
      .unwrap_or_default()
      .trim()
      .to_owned()
  } else {
    String::new()
  };
  // How full the session stood when the turn ended, where the program's answer says.
  let context = answer.and_then(|answer| answer.get("context"));
  let used = context.and_then(|context| context.get("used")).and_then(Value::as_i64);
  let size = context.and_then(|context| context.get("size")).and_then(Value::as_i64);

  (
    state,
    error,
    Reply {
      mode: provider.to_owned(),
      model: Some(model.clone()),
      effort: None,
      text,
      session_id: None,
      context_ratio: used
        .zip(size)
        .filter(|(_, size)| *size > 0)
        .map(|(used, size)| used as f64 / size as f64),
      context_tokens: used,
      session_usage: None,
      windows: vec![],
      spend: if input + output + cache_read == 0 {
        vec![]
      } else {
        vec![ModelSpend {
          mode: provider.to_owned(),
          model,
          tokens: input + output + cache_read,
          input_tokens: input,
          output_tokens: output,
          cache_read_tokens: cache_read,
          cache_write_tokens: 0,
          cost: 0.0,
        }]
      },
      live_tasks: vec![],
      workflows: vec![],
      account: None,
      last_request_usage: None,
      request_usages: None,
      answers: Vec::new(),
    },
  )
}

/// One turn of the provider's pane program, from standing it to its answer, the turn's lines of the
/// record handed to the watch as they are written.
pub async fn turn<W: Watch>(
  ground: &Ground,
  provider: &str,
  resident: &Resident,
  turn: &SurfaceTurn,
  watch: &mut W,
) -> Result<SurfaceEnding, String> {
  let place = Place::of(&ground.roots.data, &turn.home.agent);
  let program = Program {
    identity: command_digest(&turn.command),
    command: turn.command.clone(),
    restart: None,
    opened: None,
  };
  // A pane program's run names no effort of its own in the row: the session holds it.
  let mut execution = {
    let mut asked = turn.clone();

    asked.run.effort = String::new();
    Execution::begin(ground, provider, &asked, None, &program.command.args).await?
  };

  execution.save(ground, turn).await;
  watch.opened(execution.row.id()).await;

  let ended = put(ground, provider, resident, turn, &place, &program, &mut execution, watch).await;

  kept_errors(ground, turn, &place, watch).await;

  let _ = end_run(&place.path, now_ms());
  execution.close(ground, turn, ended.is_err()).await;
  ended
}

// What the program's kept error stream gained during the turn, written into its run log; and, on a chat
// line, the servers its harness said did not activate, with the ones the tool host saw no client reach,
// told to the session once per program started.
async fn kept_errors<W: Watch>(ground: &Ground, turn: &SurfaceTurn, place: &Place, watch: &mut W) {
  let news = session_tools::error_news(&place.path, RESIDENT_TURN.stderr_lines);

  for line in &news {
    watch.note(&session_tools::error_line(line)).await;
  }

  let room = turn.run.env.get("BMO_TURN_CHAT").and_then(|chat| chat.trim().parse::<i64>().ok());

  session_tools::look(
    ground,
    place,
    &turn.home.agent,
    &turn.run.line,
    room,
    &dsh::inactive_servers(&news),
    watch,
  )
  .await;
}

#[allow(clippy::too_many_arguments)]
async fn put<W: Watch>(
  ground: &Ground,
  provider: &str,
  resident: &Resident,
  turn: &SurfaceTurn,
  place: &Place,
  program: &Program,
  execution: &mut Execution,
  watch: &mut W,
) -> Result<SurfaceEnding, String> {
  let every = turn.clocks.poll.unwrap_or(Duration::from_millis(RESIDENT_TURN.inbox_poll_ms));
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
        "the pane program did not stand: {}: {}",
        fault.code, fault.message
      )));
    }
  };
  let pane = stood.pane;
  let started = place.running();
  let from = read_record(&place.path).len();
  // Only the variables that are this turn's travel with it, so nothing of the caller's environment —
  // the key least of all — is ever written into the inbox.
  let mut env: BTreeMap<String, String> = turn
    .run
    .env
    .iter()
    .filter(|(name, _)| is_inbox_turn_variable(name) && *name != RUN_NAME)
    .map(|(name, value)| (name.clone(), value.clone()))
    .collect();

  env.insert(RUN_NAME.to_owned(), turn.name.clone());
  execution.set("paneId", json!(pane));
  execution.set(
    "resident",
    json!({
      "place": place.path,
      "record": record_path(&place.path),
      "from": from,
      "pid": started.as_ref().map(|running| running.pid),
      "birth": started.as_ref().and_then(|running| running.birth.clone()),
    }),
  );

  if execution.cancelled(ground, turn).await {
    execution.ending = "cancelled";

    return Ok(SurfaceEnding::Error("the turn was stopped before it went in".to_owned()));
  }

  let prompted_at = now_ms();
  let id = uuid::Uuid::new_v4().to_string();
  let mut run = turn.given.clone();

  // The turn as the record will name it, so a server started after this one finds the row it closes.
  execution.set("inboxTurn", json!({ "id": id, "name": turn.name }));

  run["env"] = json!(env);
  run["resident"] = Value::Null;

  let _ = begin_run(&place.path, &turn.name, &turn.title, prompted_at, run_variables(&env));
  put_turn(
    &place.path,
    &json!({
      "id": id,
      "name": turn.name,
      "title": turn.title,
      "run": run,
    }),
    &id,
  )
  .map_err(|why| format!("the pane program's inbox: {why}"))?;
  execution.set("state", json!("running"));
  execution.save(ground, turn).await;

  let read = |records: &[Value]| {
    lines_of(records.get(from..).unwrap_or_default(), &id)
      .map(|lines| read_turn(lines, &turn.run, provider))
  };
  // How many of the turn's own lines the watch has been handed.
  let mut fed = 0;
  // The program this turn was put under, as the place first showed it: a program seen after it, or none
  // at all once one stood, is the turn's own gone and the turn ended rather than waited on forever. A
  // place that has named no program yet is waited on until it does.
  let mut known = started.as_ref().map(|running| (running.pid, running.birth.clone()));

  loop {
    // A turn stopped is interrupted and waited on until it has written that it was, or its program is
    // gone.
    if execution.cancelled(ground, turn).await {
      interrupt_program(place, &turn.home, &pane, &interrupt_of(resident)).await;

      while place.program_alive().is_some()
        && read(&read_record(&place.path)).is_some_and(|(state, _, _)| state == State::Working)
      {
        tokio::time::sleep(every).await;
      }

      execution.ending = "cancelled";

      return Ok(SurfaceEnding::Error("the turn was stopped".to_owned()));
    }

    let alive = place.program_alive();
    let same = match (&known, &alive) {
      (Some((pid, birth)), Some(alive)) => alive.pid == *pid && alive.birth == *birth,
      (None, Some(alive)) => {
        known = Some((alive.pid, alive.birth.clone()));

        true
      }
      (None, None) => true,
      (Some(_), None) => false,
    };

    if !same {
      return Err("the pane program went away during the turn".to_owned());
    }

    let (written, records) = record_of(&place.path);
    let after = from.min(records.len());
    let reading = match span_of(&records[after..], &id) {
      None => None,
      Some(span) => {
        let lines = &written[after..][span.clone()];

        // What the turn's lines gained since the last look is its record as it goes.
        if lines.len() > fed {
          watch.record(&lines[fed..]).await;
          fed = lines.len();
        }

        Some(read_turn(&records[after..][span], &turn.run, provider))
      }
    };

    if let Some((state, error, reply)) = reading
      && state != State::Working
    {
      execution.ending = if state == State::Interrupted { "cancelled" } else { "exited" };

      return Ok(if state == State::Interrupted {
        SurfaceEnding::Error("the turn was interrupted".to_owned())
      } else if let Some(error) = error {
        SurfaceEnding::Error(error)
      } else if reply.text.is_empty() {
        SurfaceEnding::Error("the pane program wrote no answer".to_owned())
      } else {
        SurfaceEnding::Reply(Box::new(reply))
      });
    }

    tokio::time::sleep(every).await;
  }
}
