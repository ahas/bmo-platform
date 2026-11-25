//! Agency's work keys and the profile each starts on (docs/cache-fallback.md, 「The canonical name of a road
//! is the work key, not a Claude model name」).

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkKeys {
  pub profile: Vec<Profile>,
}

/// The Codex choice the tier store's first seed drew on.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodexChoice {
  pub model: String,
  pub effort: String,
}

/// One work key's profile: the defaults its work runs on where the owner has chosen nothing.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
  pub key: String,
  pub label: String,
  /// bmo, worker or gate.
  pub kind: String,
  /// The run line the work goes out on.
  pub line: String,
  /// The road taken where the owner has written no route for the key.
  pub default_route_mode: String,
  pub default_claude_model: String,
  pub default_codex: CodexChoice,
  /// The tier the key starts on where nobody has assigned one.
  pub tier: String,
  pub gate_answer_tokens: Option<i64>,
  /// How the gate's request with no tools samples, each setting by the engine's own name.
  pub gate_sampling: Option<BTreeMap<String, f64>>,
  /// The most programs the gate's pool stands on Claude Code, where it is not the pool's own default.
  pub gate_pool: Option<usize>,
  pub answer_floor: Option<i64>,
  pub thinking_answer_floor: Option<i64>,
  #[serde(default)]
  pub switchable: bool,
  #[serde(default)]
  pub holds_no_session: bool,
  /// The role whose saved prefix an engine that keeps role files starts its requests from (docs/own-llm.md,
  /// 「Role files」); none where the key's prompt is no fixed role.
  #[serde(default)]
  pub prefix_role: Option<String>,
}

impl WorkKeys {
  /// The profile of a key, where a profile declares it.
  pub fn profile(&self, key: &str) -> Option<&Profile> {
    self.profile.iter().find(|one| one.key == key)
  }

  /// The supervisor key: the profile of the supervisor's kind, whose road every supervisor's route is.
  pub fn supervisor(&self) -> Option<&Profile> {
    self.profile.iter().find(|one| one.kind == "bmo")
  }
}

crate::part!(pub WORK_KEYS: WorkKeys = "work-keys.toml");
