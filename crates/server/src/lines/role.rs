//! A role session's turn on a road other than Claude Code's own, as its line is composed before the road's
//! program is stood (docs/api.md, 「sessions」; docs/launch-lines.md, 「What a line's program is allowed, by
//! road」): the runner line it names with the instructions its caller handed in, or else the worker line whole —
//! its prompt, rules, settings, plugin, co-workers and MCP configuration — which a program that is Claude Code
//! stands on in its own words on every road.

use std::collections::BTreeMap;
use std::path::PathBuf;

use agent_session::{Apart, Mapping};
use sha2::{Digest, Sha256};

use super::Trees;
use super::chat::{ChatRun, RoomContext};
use super::worker::{Reasoning, WorkerRun};
use crate::Refusal;
use crate::places::Ground;

/// What a role's run on a road other than Claude Code's own stands on: its line, effort and reasoning, its whole
/// system prompt, the tools a program told what it is shown and allowed is shown and allowed, its MCP
/// configuration and its folder; and, on the worker line, the line whole, which a program that is Claude Code
/// is started on in the line's own words.
pub struct RoleLine {
  pub line: &'static str,
  pub effort: String,
  pub tier: Option<String>,
  pub reasoning: Reasoning,
  pub thinking: bool,
  pub system_path: PathBuf,
  pub tools_shown: String,
  pub tools_allowed: String,
  pub mcp_config: PathBuf,
  pub cwd: String,
  pub worker: Option<Box<WorkerRun>>,
}

/// The line a role's session's turn goes out on (docs/api.md, 「sessions」): the runner line the session names
/// with the instructions its caller handed in, or else the worker line opening on the platform's harness and the
/// role's prompt, with the repository's own where its folder is a repository's checkout.
pub async fn compose(
  ground: &Ground,
  trees: &Trees,
  apart: &Apart,
  role: &str,
  mapping: &Mapping,
) -> Result<RoleLine, Refusal> {
  let effort = mapping.effort.as_deref().unwrap_or_default();
  let tier = mapping.tier.as_deref().unwrap_or_default();

  if let (Some(line), Some(instructions)) = (apart.line.as_deref(), apart.system_prompt.as_deref()) {
    let run =
      super::runner::given_run(ground, trees, line, instructions, (&apart.model, effort, tier)).await?;
    let thinking = super::worker::reasoning_of(ground, line).await?.thinking;

    return Ok(RoleLine {
      line: run.line,
      effort: run.effort.clone(),
      tier: Some(run.tier.clone()).filter(|tier| !tier.is_empty()),
      reasoning: Reasoning {
        thinking,
        level: None,
      },
      thinking: run.thinking,
      system_path: run.system_prompt.clone(),
      tools_shown: String::new(),
      tools_allowed: run.tools_allowed.to_owned(),
      mcp_config: run.mcp_config.clone(),
      cwd: run.cwd.to_string_lossy().into_owned(),
      worker: None,
    });
  }

  let checkout = crate::queue::workers::checkout_at(&ground.db, &apart.cwd).await?;
  let mut line = super::worker::role_run(
    ground,
    trees,
    role,
    checkout.as_deref(),
    None,
    &apart.model,
    &apart.cwd,
    &apart.provider,
  )
  .await?;
  line.effort = mapping.effort.clone().unwrap_or(line.effort);
  line.tier.clone_from(&mapping.tier);

  Ok(RoleLine {
    line: super::worker::LINE,
    effort: line.effort.clone(),
    tier: line.tier.clone(),
    reasoning: line.reasoning.clone().unwrap_or_default(),
    thinking: line.thinking,
    // The role's whole system prompt: the harness opening and the role's prompt before the rules.
    system_path: line.system_prompt.clone().unwrap_or_else(|| line.harnessed.clone()),
    tools_shown: String::new(),
    // A program told its tools takes every working row of its road; the list only has to be non-empty for the
    // road to hand it them rather than none.
    tools_allowed: "Bash".to_owned(),
    mcp_config: line.mcp_config.clone(),
    cwd: apart.cwd.clone(),
    worker: Some(Box::new(line)),
  })
}

/// The turn's own parts a role's run is composed with beside its line.
pub struct Turned {
  /// The place the run stands in, whose name its work key, lock and records stand under.
  pub place: String,
  pub model: String,
  pub prompt: String,
  pub resume: Option<String>,
  pub env: BTreeMap<String, String>,
  pub cards: crate::cards::Ask,
}

impl RoleLine {
  /// The run the line makes of a turn: no room's block and no persona, the line's tools and configuration, and
  /// the worker line whole where the run is on it.
  pub fn run(self, turned: Turned) -> ChatRun {
    let system_hash = std::fs::read_to_string(&self.system_path)
      .map(|text| format!("{:x}", Sha256::digest(text.as_bytes())))
      .unwrap_or_default();

    ChatRun {
      line: self.line,
      agent: turned.place,
      advisor: None,
      model: turned.model,
      effort: self.effort,
      tier: self.tier,
      reasoning: self.reasoning,
      thinking: self.thinking,
      system_path: self.system_path,
      system_hash,
      system_parts: None,
      room: RoomContext {
        chat_id: 0,
        text: String::new(),
        mark: String::new(),
      },
      tools_shown: self.tools_shown,
      tools_allowed: self.tools_allowed,
      tools_denied: None,
      tools_kept: None,
      mcp_config: self.mcp_config,
      plugin_dir: None,
      mcp_tools: None,
      prompt: turned.prompt,
      persona_unused: None,
      fold: false,
      ping: false,
      resume: turned.resume,
      cwd: self.cwd,
      env: turned.env,
      cards: turned.cards,
      worker: self.worker,
    }
  }
}
