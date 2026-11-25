//! One turn asked of the door from outside the service: what the `resident-turn` verb reads, the turn
//! in the TypeScript pane turn's own words for it (the agent package's `worker-pane.ts`), so a twin check
//! can put the same turn through either side.

use std::collections::BTreeMap;
use std::time::Duration;

use agent_host::place::ResidentCommand;
use serde::Deserialize;
use serde_json::Value;

use super::execution::Identity;
use super::looks::GoneConfirm;
use super::stand::Interrupt;
use super::surface::{Clocks, Run, SurfaceTurn};
use super::turn::{AsideOutcome, Seat, Timings, Turn, Watch, pasted_lead};
use crate::places::Ground;

/// The turn as the TypeScript pane turn is handed it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asked {
  pub agent: String,
  #[serde(default)]
  pub label: Option<String>,
  pub name: String,
  pub title: String,
  pub cwd: String,
  pub command: Vec<String>,
  #[serde(default)]
  pub env: BTreeMap<String, String>,
  pub prompt: String,
  pub session_id: String,
  #[serde(default)]
  pub line: Option<String>,
  #[serde(default)]
  pub provider: Option<String>,
  #[serde(default)]
  pub lead: Option<String>,
  #[serde(default)]
  pub trusts_folder: bool,
  /// The keys that interrupt the program, or none where its program is ended instead.
  #[serde(default)]
  pub interrupt: Option<Value>,
  #[serde(default)]
  pub identity: Option<AskedIdentity>,
  #[serde(default)]
  pub poll_ms: Option<u64>,
  #[serde(default)]
  pub redraw_ms: Option<u64>,
  #[serde(default)]
  pub submit_ms: Option<u64>,
  /// Words said to the turn while it works, one at each look at a working program until none is left.
  #[serde(default)]
  pub words: Vec<String>,
}

/// A caller that says the words it was given, one at each look, and keeps what became of each.
#[derive(Debug, Default)]
pub struct Scripted {
  pub words: std::collections::VecDeque<String>,
  pub said: Vec<&'static str>,
}

impl Watch for Scripted {
  async fn words(&mut self) -> Option<String> {
    self.words.pop_front()
  }

  async fn said(&mut self, outcome: AsideOutcome) {
    self.said.push(match outcome {
      AsideOutcome::Delivered => "delivered",
      AsideOutcome::Waiting => "waiting",
      AsideOutcome::Refused => "refused",
    });
  }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskedIdentity {
  #[serde(default)]
  pub root: Option<String>,
  #[serde(default)]
  pub parent: Option<String>,
  #[serde(default)]
  pub role: Option<String>,
  #[serde(default)]
  pub model: Option<String>,
  #[serde(default)]
  pub effort: Option<String>,
}

/// A turn of a surface that carries its own turns, as a TypeScript surface turn (`agents/agent`) is handed
/// it, with the command its program stands on as the provider composed it for this run.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceAsked {
  pub agent: String,
  #[serde(default)]
  pub label: Option<String>,
  pub provider: String,
  pub name: String,
  pub title: String,
  #[serde(default)]
  pub root: Option<String>,
  #[serde(default)]
  pub parent: Option<String>,
  pub run: Value,
  pub command: ResidentCommand,
  #[serde(default)]
  pub poll_ms: Option<u64>,
  /// Words said to the turn while it works, where its surface takes them.
  #[serde(default)]
  pub words: Vec<String>,
}

impl SurfaceAsked {
  /// The turn on this ground, in the agent's home on the ground's herdr, with the caller that says its
  /// words.
  pub async fn turn(self, ground: &Ground) -> Result<(SurfaceTurn, Scripted), String> {
    let run: Run = serde_json::from_value(self.run.clone()).map_err(|why| format!("the run: {why}"))?;
    let label = self.label.clone().unwrap_or_else(|| self.agent.clone());
    let home = ground
      .home_of(&self.agent, &label)
      .ok_or_else(|| "this machine names no herdr for the turn's home".to_owned())?;

    Ok((
      SurfaceTurn {
        run,
        given: self.run,
        home,
        name: self.name,
        title: self.title,
        root: self.root,
        parent: self.parent,
        command: self.command,
        clocks: Clocks {
          poll: self.poll_ms.map(Duration::from_millis),
        },
      },
      Scripted {
        words: self.words.into_iter().collect(),
        said: vec![],
      },
    ))
  }
}

impl Asked {
  /// The turn on this ground, in the agent's home on the ground's herdr, with the caller that says
  /// its words.
  pub async fn turn(self, ground: &Ground) -> Result<(Turn, Scripted), String> {
    let line = self.line.clone().unwrap_or_else(|| "worker".to_owned());
    let label = self.label.clone().unwrap_or_else(|| self.agent.clone());
    let home = ground
      .home_of(&self.agent, &label)
      .ok_or_else(|| "this machine names no herdr for the turn's home".to_owned())?;
    let ms = |given: Option<u64>, otherwise: Duration| given.map_or(otherwise, Duration::from_millis);
    let base = Timings::default();
    let interrupt = match &self.interrupt {
      Some(Value::Object(named)) if named.get("endProgram") == Some(&Value::Bool(true)) => {
        Interrupt::EndProgram
      }
      Some(Value::Object(named)) => Interrupt::Keys(
        named
          .get("keys")
          .and_then(Value::as_array)
          .map(|keys| keys.iter().filter_map(Value::as_str).map(str::to_owned).collect())
          .unwrap_or_default(),
      ),
      // The key Claude Code reads as an interruption, which leaves its session usable: the interrupt
      // of a turn whose caller names none.
      _ => Interrupt::Keys(vec!["escape".to_owned()]),
    };
    let identity = self.identity.map_or_else(Identity::default, |given| Identity {
      agent: None,
      root: given.root,
      parent: given.parent,
      role: given.role,
      model: given.model,
      effort: given.effort,
    });

    let scripted = Scripted {
      words: self.words.into_iter().collect(),
      said: vec![],
    };

    Ok((
      Turn {
        name: self.name,
        title: self.title,
        seat: Seat::Standing(home),
        cwd: self.cwd,
        command: self.command,
        env: self.env,
        prompt: self.prompt,
        session: self.session_id,
        identity,
        line,
        provider: self.provider.unwrap_or_else(|| "claude-code".to_owned()),
        lead: self.lead.unwrap_or_else(pasted_lead),
        trusts_folder: self.trusts_folder,
        clock: None,
        interrupt,
        timings: Timings {
          poll: ms(self.poll_ms, base.poll),
          redraw: ms(self.redraw_ms, base.redraw),
          submit: ms(self.submit_ms, base.submit),
          gone: GoneConfirm::default(),
        },
      },
      scripted,
    ))
  }
}
