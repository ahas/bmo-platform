//! The scale an agent's own effort is named on (docs/model-tiers.md, 「An agent's own thinking and
//! effort」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasoningScale {
  /// The levels, lowest first.
  pub levels: Vec<String>,
}

impl ReasoningScale {
  pub fn holds(&self, level: &str) -> bool {
    self.levels.iter().any(|one| one == level)
  }
}

crate::part!(pub REASONING: ReasoningScale = "reasoning.toml");
