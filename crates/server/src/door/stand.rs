//! The resident program a turn goes into, stood before the turn and cut short when it must be
//! (docs/agent-control.md, 「One standing pane per agent」, 「When a program will not come to rest」,
//! 「Stopping, falling back and standing up again」). The launcher writes the program into the agent's
//! place and the head standing in its pane starts what it finds; the two talk through the place's files
//! and neither reaches into the other's process.

use std::time::Duration;

use agent_host::place::RunningProgram;
use herdr::Fault;
use policy::handoff::HANDOFF;
use policy::resident_turn::RESIDENT_TURN;
use serde::Deserialize;
use serde_json::json;

use super::looks::{GoneConfirm, Look, pane_listed, send_keys, status_of};
use super::place_run::read_run;
use crate::lines::place::{Place, Program, token_of};
use crate::lines::token;
use crate::now_ms;
use crate::places::Ground;
use crate::places::home::Home;

// How far back the starts of a place are remembered, and how many within it make a program one in a
// loop.
const RECENT_WINDOW_MS: i64 = 5 * 60_000;
const LOOPING_STARTS: usize = 6;
// How often one place may be rebuilt.
const REBUILD_EVERY_MS: i64 = 60_000;

/// How a working turn of the program is cut short: keys its surface reads as an interruption, which
/// leave its session usable, or — where no interrupt has been measured — the program ended, which its
/// loop answers by starting it again on the same session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Interrupt {
  Keys(Vec<String>),
  EndProgram,
}

/// A program standing, and where its words go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stood {
  pub pane: String,
  pub restarted: bool,
}

#[derive(Deserialize)]
struct StartFailure {
  identity: String,
  at: i64,
  reason: String,
}

fn fault(code: &str, message: String) -> Fault {
  Fault {
    code: code.to_owned(),
    message,
  }
}

// Whether the program `identity` names is in a loop or rests after one: its own starts, never those of a
// program it was written in place of.
fn looping(running: Option<&RunningProgram>, identity: &str, now: i64) -> bool {
  running.filter(|running| running.identity == identity).is_some_and(|running| {
    running.resting_since.is_some()
      || running.recent.iter().filter(|at| now - (**at as i64) < RECENT_WINDOW_MS).count() >= LOOPING_STARTS
  })
}

// The place torn down so it stands fresh: the program ended, its pane closed — the head and its loop go
// with it — and its start history cleared, so the pane the next stand makes starts it at once, on the
// command that resumes its session. At most once a minute per place; answers whether it was done.
async fn rebuild(place: &Place, home: &Home, pane: Option<&str>) -> bool {
  let stamp = place.path.join("rebuilt.json");
  let last: Option<serde_json::Value> = agent_host::disk::read_json(&stamp);

  if now_ms() - last.and_then(|last| last.get("at").and_then(serde_json::Value::as_i64)).unwrap_or(0)
    < REBUILD_EVERY_MS
  {
    return false;
  }

  let _ = agent_host::disk::private_dir(&place.path).and_then(|()| {
    agent_host::disk::publish_bytes(&stamp, json!({ "at": now_ms() }).to_string().as_bytes())
  });

  let running = place.running();

  if let Some(up) = place.program_alive() {
    place.end(up.pid).await;
  }

  if let Some(running) = &running {
    let cleared = RunningProgram {
      pid: 0,
      birth: None,
      recent: vec![],
      pending_until: None,
      resting_since: None,
      ..running.clone()
    };
    let _ = agent_host::place::Place(place.path.clone()).record_running(&cleared);
  }

  if let Some(pane) =
    running.as_ref().and_then(|running| running.pane_id.clone()).or(pane.map(str::to_owned))
  {
    let _ = home.herdr.call("pane.close", json!({ "pane_id": pane }), None).await;
  }

  eprintln!(
    "{}: its resident program would not stand ({}); the place was rebuilt",
    home.agent,
    running.as_ref().map_or_else(
      || "never started".to_owned(),
      |running| format!(
        "{} starts in five minutes",
        running.recent.iter().filter(|at| now_ms() - (**at as i64) < RECENT_WINDOW_MS).count()
      )
    )
  );

  true
}

// How long a thrown-away session's place's session token may stand where nothing takes it back first: the
// token is taken back as the session ends, and this is only the tool host's own sweep past that.
const RECORD_BACKSTOP: Duration = Duration::from_secs(24 * 3_600);

/// The program the turn needs, standing. A place already running it is left as it is, and so is one
/// running another program while its record says a turn of it works. A different program is written and
/// the one standing ended, which the loop answers by starting the new one in the
/// same pane; a pane that is not there is stood up. The turn then waits for the program to be up, for
/// its start failure to be written, or for `stop` to say the turn was stopped, however long that takes.
/// A program in a start loop has its place rebuilt and is waited on once more; one still in a loop
/// after that is not coming up. A head pane herdr says is gone is stood again once, since nothing in a pane
/// that is not there starts the program; one gone again after that is not coming up. A program about to be
/// written is handed its session token first, and a tool host that does not answer refuses the turn before
/// anything is written. The token's record is issued from the program's own environment with the run's
/// variables of `run_env` laid over it: a program's environment holds no turn's variables, and the role a run
/// holds and the record its caller named stand among them, so a record
/// issued from the program's environment alone would hold none of its role's bundles. `road` names the
/// provider and model the turn goes out on, for the record's `reads` label. Before anything, the apps the
/// program's person reaches are resolved again for the turn.
#[allow(clippy::too_many_arguments)]
pub async fn stand_resident<F, Fut>(
  ground: &Ground,
  place: &Place,
  home: &Home,
  program: &Program,
  run_env: &std::collections::BTreeMap<String, String>,
  line: &str,
  road: Option<(&str, &str)>,
  poll: Duration,
  stop: F,
) -> Result<Stood, Fault>
where
  F: Fn() -> Fut,
  Fut: Future<Output = bool>,
{
  // The apps the program's person reaches, resolved again as each turn begins, so a setting changed while the
  // session stands holds from this turn; a chat session in a room is told of a change (docs/tools.md, 「The apps
  // a person reaches」).
  let room = run_env.get("BMO_TURN_CHAT").and_then(|chat| chat.trim().parse::<i64>().ok());

  crate::apps::turn_begins(
    ground,
    &place.path,
    &home.agent,
    room.filter(|_| line == crate::lines::chat::LINE),
  )
  .await;

  // A program is stood afresh only between its turns: one whose own record says a turn works is left
  // standing, the turn asking for it going in as it stands, and the next stand after that turn replaces it.
  if let Some(up) = place.program_alive()
    && up.identity != program.identity
    && super::inbox::turn_working(&place.path)
  {
    let pane = match up.pane_id {
      Some(pane) => pane,
      None => home.stand().await?.head_pane,
    };

    return Ok(Stood {
      pane,
      restarted: false,
    });
  }

  let standing = place.program();
  let mut written = program.clone();

  if standing
    .as_ref()
    .is_none_or(|standing| standing.identity != program.identity || token_of(standing).is_none())
  {
    let checkout = token::record_checkout(ground, &place.path, &program.command.cwd.to_string_lossy());
    let deadline = u64::try_from(now_ms()).unwrap_or_default() + RECORD_BACKSTOP.as_millis() as u64;
    let issued = token::issue_for_program(
      ground,
      line,
      &home.agent,
      &place.path,
      &checkout,
      &program.command.env,
      run_env,
      road,
      deadline,
    )
    .await
    .map_err(|refused| fault("tool-host-down", refused.detail))?;

    if let Some(issued) = issued {
      for command in std::iter::once(&mut written.command).chain(written.restart.as_mut()) {
        command.env.insert(token::TOKEN_VARIABLE.to_owned(), issued.clone());
      }
    }
  }

  let (before, restarted) = place
    .replace(&written)
    .await
    .map_err(|refused| fault("program_not_written", refused.detail))?;

  // A token the place's program no longer carries is given up to the tool host, once no asynchronous call
  // asked under it still runs.
  if let Some(old) = before.as_ref().and_then(token_of)
    && Some(old) != token_of(&written)
    && place.program().as_ref().and_then(token_of) != Some(old)
  {
    token::let_go(ground, old, Duration::from_secs(HANDOFF.calls_look_seconds)).await;
  }

  // A start failure written from now on, for this program, is the answer; one written before is a head
  // that may since have been stood again on another environment.
  let asked_at = now_ms();
  let mut rebuilt =
    looping(place.running().as_ref(), &program.identity, now_ms()) && rebuild(place, home, None).await;
  let mut restood = false;

  loop {
    let stood = home.stand().await?;

    loop {
      if let Some(up) = place.program_alive()
        && up.identity == program.identity
      {
        return Ok(Stood {
          pane: up.pane_id.unwrap_or(stood.head_pane),
          restarted,
        });
      }

      let failed: Option<StartFailure> = agent_host::disk::read_json(&place.path.join("start-failed.json"));

      if let Some(failed) = failed
        && failed.identity == program.identity
        && failed.at >= asked_at
      {
        return Err(fault(
          "program_not_started",
          format!("the resident program of {} could not be started: {}", home.agent, failed.reason),
        ));
      }

      if stop().await {
        return Err(fault(
          "stopped",
          format!("the turn on {} was stopped while its program came up", home.agent),
        ));
      }

      // A program in a start loop has its place rebuilt once, as soon as the place may be rebuilt again;
      // one that loops again after that is not coming up.
      if looping(place.running().as_ref(), &program.identity, now_ms()) {
        if rebuilt {
          return Err(fault(
            "program_not_up",
            format!(
              "the resident program of {} keeps ending as it starts, its place rebuilt once already",
              home.agent
            ),
          ));
        }

        if rebuild(place, home, Some(&stood.head_pane)).await {
          rebuilt = true;
          break;
        }
      }

      // The pane the program was to come up in is gone, on herdr's own word.
      if pane_listed(&home.herdr, &stood.head_pane).await == Some(false) {
        if restood {
          return Err(fault(
            "pane_gone",
            format!(
              "the pane {}'s resident program was to come up in went away twice while it was stood",
              home.agent
            ),
          ));
        }

        restood = true;
        break;
      }

      tokio::time::sleep(poll).await;
    }
  }
}

/// A working turn of the place's program cut short by its surface's own interrupt. The pane stays
/// either way.
pub async fn interrupt_program(place: &Place, home: &Home, pane: &str, interrupt: &Interrupt) {
  match interrupt {
    Interrupt::Keys(keys) => {
      let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
      let _ = send_keys(&home.herdr, pane, &keys).await;
    }
    Interrupt::EndProgram => {
      if let Some(running) = place.program_alive() {
        place.end(running.pid).await;
        gone(place, &running).await;
      }
    }
  }
}

// A program just ended, waited on until its process is gone: until then the place still names it and
// reads it alive, and whatever reads the place next would take it for the one standing.
async fn gone(place: &Place, running: &RunningProgram) {
  while place.alive(running.pid, running.birth.as_deref()) {
    tokio::time::sleep(Duration::from_millis(RESIDENT_TURN.end_poll_ms)).await;
  }
}

/// Whether a program ended for a reset stands again: its place's record naming a different process from
/// the one just ended. The record is what says so, since a pane whose program is gone can still read as
/// it did a moment before. Waited for until it does; a turn cancelled while it waits stops here.
pub async fn stood_again<F, Fut>(
  place: &Place,
  was: Option<&RunningProgram>,
  every: Duration,
  stop: F,
) -> bool
where
  F: Fn() -> Fut,
  Fut: Future<Output = bool>,
{
  loop {
    if let Some(now) = place.running()
      && was.is_none_or(|was| now.pid != was.pid || now.started_at != was.started_at)
    {
      return true;
    }

    if stop().await {
      return false;
    }

    tokio::time::sleep(every).await;
  }
}

/// A person's turn working in the place's program, waited out: while the place names a person's turn
/// still open and the pane reads working or waiting on a question, a turn of the bot's neither stands,
/// replaces nor resets the program nor puts words in. A person's turn cut short by an Escape never has
/// its end recorded, so the pane reading neither working nor waiting on a question ends the wait as well.
/// Answers whether there was one to wait for; `check` refuses the wait where the turn was cancelled
/// meanwhile.
pub async fn person_turn_done<F, Fut>(
  place: &Place,
  home: &Home,
  every: Duration,
  check: F,
) -> Result<bool, String>
where
  F: Fn() -> Fut,
  Fut: Future<Output = Result<(), String>>,
{
  let mut waited = false;

  loop {
    let run = read_run(&place.path);
    let pane = place.program_alive().and_then(|running| running.pane_id);

    let (Some(run), Some(pane)) = (run, pane) else { return Ok(waited) };

    if !run.person || run.ended_at.is_some() {
      return Ok(waited);
    }

    // A herdr that did not answer has not said the person's turn is over.
    let now = status_of(&home.herdr, &pane, GoneConfirm::default()).await;

    if !(now.is("working") || now.is("blocked") || matches!(now, Look::Silent(_))) {
      return Ok(waited);
    }

    check().await?;
    waited = true;
    tokio::time::sleep(every).await;
  }
}
