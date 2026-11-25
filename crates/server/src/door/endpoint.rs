//! One turn of Claude Code on another's endpoint, standing in the agent's pane (docs/own-llm.md, 「Claude
//! Code in the pane」): the transcript door's turn on the session the command names — the one the
//! program stands on where the turn goes on in it, one of its own otherwise, which stands the program up
//! again on it — under a configuration home of the road's own whose first-run questions are answered
//! before the program comes up, and an answer that is the road's rather than a Claude login's.

use std::path::PathBuf;

use policy::providers::Resident;
use policy::resident_turn::RESIDENT_TURN;

use super::record::Reply;
use super::surface::{SurfaceEnding, SurfaceTurn, interrupt_of};
use super::turn::{Ending, Seat, Timings, Turn, Watch, pasted_lead, put};
use crate::places::Ground;
use crate::places::trust::{config_file, mark_onboarded, trust_folder};

const RUN_NAME: &str = "BMO_RUN_NAME";
// The line whose turns are a room's rounds, which carry no typed lead.
const ROUND_LINE: &str = "chat";

/// The session a program's command opens or resumes.
pub fn session_named(args: &[String]) -> Option<&str> {
  let at = args.iter().position(|word| word == "--session-id" || word == "--resume")?;

  args.get(at + 1).map(String::as_str)
}

/// A reply of the road's: its provider's, with nothing spent in money and no Claude login named.
pub fn road_reply(provider: &str, reply: Reply) -> Reply {
  Reply {
    mode: provider.to_owned(),
    account: None,
    spend: reply
      .spend
      .into_iter()
      .map(|mut row| {
        row.mode = provider.to_owned();
        row.cost = 0.0;
        row
      })
      .collect(),
    ..reply
  }
}

pub async fn turn<W: Watch>(
  ground: &Ground,
  provider: &str,
  resident: &Resident,
  asked: &SurfaceTurn,
  watch: &mut W,
) -> Result<SurfaceEnding, String> {
  let command = &asked.command;
  let session = session_named(&command.args)
    .ok_or_else(|| format!("{}: the command names no session for the turn to go on in", asked.name))?
    .to_owned();
  let home = command
    .env
    .get("CLAUDE_CONFIG_DIR")
    .filter(|home| !home.is_empty())
    .map(PathBuf::from)
    .ok_or_else(|| format!("{}: the command names no home of the road's own", asked.name))?;

  std::fs::create_dir_all(&home).map_err(|why| format!("{}: {why}", home.display()))?;
  mark_onboarded(&config_file(&home));
  trust_folder(&config_file(&home), &asked.run.cwd);

  let mut env = command.env.clone();

  env.insert(RUN_NAME.to_owned(), asked.name.clone());

  let base = Timings::default();
  let turn = Turn {
    name: asked.name.clone(),
    title: asked.title.clone(),
    seat: Seat::Standing(asked.home.clone()),
    cwd: command.cwd.to_string_lossy().into_owned(),
    command: command.args.clone(),
    env,
    prompt: asked.run.prompt.clone(),
    session,
    // The run's own effort, none where it names none, never the word its command hands the program.
    identity: super::execution::Identity {
      effort: Some(asked.run.effort.clone()),
      ..asked.identity()
    },
    line: asked.run.line.clone(),
    provider: provider.to_owned(),
    lead: if asked.run.line == ROUND_LINE {
      RESIDENT_TURN.round_lead.clone()
    } else {
      pasted_lead()
    },
    trusts_folder: true,
    clock: None,
    interrupt: interrupt_of(resident),
    timings: Timings {
      poll: asked.clocks.poll.unwrap_or(base.poll),
      ..base
    },
  };

  // The program's own words where it wrote them in the answer's place; a screen is never words to hand
  // on, so every other ending is named by what it was (docs/agent-control.md, 「What a failed turn may
  // say」).
  Ok(match put(ground, &turn, watch).await? {
    Ending::Answered { reply, .. } => SurfaceEnding::Reply(Box::new(road_reply(
      provider,
      Reply {
        model: Some(asked.run.model.clone()),
        ..*reply
      },
    ))),
    Ending::Stopped { .. } => SurfaceEnding::Stopped(format!("{}: the turn was stopped", asked.name)),
    Ending::Errored { error, kind } => SurfaceEnding::Errored { error, kind },
    Ending::Unsubmitted { why, .. } => SurfaceEnding::Unsubmitted(why),
    other => SurfaceEnding::Error(format!(
      "Claude Code's turn ended {} with no answer",
      serde_json::to_value(&other)
        .ok()
        .and_then(|value| value.get("outcome").and_then(|outcome| outcome.as_str().map(str::to_owned)))
        .unwrap_or_default()
    )),
  })
}
