//! The worker's run laid on a stand-in road a job's walk sends it to (docs/cache-fallback.md): the worker's
//! line on the model and effort the walk chose, described in the fields and the order the TypeScript
//! harness (`agents/agent`) describes a run in, so that what is kept of a turn — a Codex thread, an
//! OpenRouter run to warm — is resumed and warmed from one description, and so that the deployment's loop
//! is handed the run it goes out on. A kept run reads back as the worker's line it was laid from, which a
//! ping goes out on again.

use std::path::PathBuf;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::worker::{LINE, Reasoning, WorkerRun};

/// The worker's own choice of thinking and effort, carried with the run.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RunReasoning {
  pub thinking: Option<bool>,
  pub effort: Option<String>,
}

/// What the worker's line is handed beyond the program's own words.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Extensions {
  pub settings_path: PathBuf,
  pub plugin_dir: PathBuf,
  pub rules_path: PathBuf,
  /// The whole system prompt Claude Code is handed where no base prompt is chosen: the platform's harness
  /// opening with the rules after it; none in a kept run that does not name it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub harnessed_path: Option<PathBuf>,
}

/// How a road takes the run: the way its prompt reaches the program, and whether the session it opens is
/// kept.
#[derive(Clone, Copy, Debug)]
pub struct Taking {
  pub prompt_delivery: &'static str,
  pub keep_session: bool,
}

/// The worker's run on a stand-in road, less the prompt, the variables and the session it resumes, which
/// each turn supplies afresh.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StandInRun {
  pub line: String,
  pub model: String,
  pub effort: String,
  pub system_prompt_path: Option<PathBuf>,
  pub tools_shown: Option<Value>,
  pub tools_allowed: Option<Value>,
  pub tools_denied: String,
  pub mcp_config_path: PathBuf,
  pub prompt_delivery: String,
  pub keep_session: bool,
  pub host_commands: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reasoning: Option<RunReasoning>,
  pub thinking: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sampling: Option<IndexMap<String, Value>>,
  pub answer_shape: String,
  pub open_session_id: Option<Value>,
  pub cwd: String,
  pub account: Option<Value>,
  pub extensions: Extensions,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub thread: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tier: Option<String>,
  #[serde(default)]
  pub ephemeral: bool,
}

impl StandInRun {
  /// The worker's line laid on the model and effort the walk chose, on the item's thread, as the road
  /// takes it.
  pub fn of(
    line: &WorkerRun,
    sampling: Option<IndexMap<String, Value>>,
    thread: Option<String>,
    taking: Taking,
  ) -> Self {
    Self {
      line: LINE.to_owned(),
      model: line.model.clone(),
      effort: line.effort.clone(),
      system_prompt_path: line.system_prompt.clone(),
      tools_shown: None,
      tools_allowed: None,
      tools_denied: line.tools_denied.clone(),
      mcp_config_path: line.mcp_config.clone(),
      prompt_delivery: taking.prompt_delivery.to_owned(),
      keep_session: taking.keep_session,
      host_commands: true,
      reasoning: line.reasoning.as_ref().map(|reasoning| RunReasoning {
        thinking: reasoning.thinking,
        effort: reasoning.level.clone(),
      }),
      thinking: line.thinking,
      sampling,
      answer_shape: "text".to_owned(),
      open_session_id: None,
      cwd: line.cwd.clone(),
      account: None,
      extensions: Extensions {
        settings_path: line.settings.clone(),
        plugin_dir: line.plugin_dir.clone(),
        rules_path: line.rules.clone(),
        harnessed_path: Some(line.harnessed.clone()),
      },
      thread,
      tier: line.tier.clone(),
      ephemeral: false,
    }
  }

  /// The run as it is kept beside what its turn left, which a later turn and a ping go out by.
  pub fn stored(&self) -> String {
    serde_json::to_string(self).unwrap_or_default()
  }

  /// The worker's line the run was laid from, on the model, effort and tier it went out on, where it is a
  /// worker's run.
  pub fn worker_line(&self) -> Option<WorkerRun> {
    (self.line == LINE).then(|| WorkerRun {
      model: self.model.clone(),
      effort: self.effort.clone(),
      tier: self.tier.clone(),
      thinking: self.thinking,
      reasoning: self.reasoning.as_ref().map(|reasoning| Reasoning {
        thinking: reasoning.thinking,
        level: reasoning.effort.clone(),
      }),
      system_prompt: self.system_prompt_path.clone(),
      // A kept run that names no harnessed prompt stands on its rules alone.
      harnessed: self
        .extensions
        .harnessed_path
        .clone()
        .unwrap_or_else(|| self.extensions.rules_path.clone()),
      rules: self.extensions.rules_path.clone(),
      mcp_config: self.mcp_config_path.clone(),
      settings: self.extensions.settings_path.clone(),
      plugin_dir: self.extensions.plugin_dir.clone(),
      tools_denied: self.tools_denied.clone(),
      cwd: self.cwd.clone(),
      advisor: None,
    })
  }
}
