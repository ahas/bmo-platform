//! Agency's tier ladder (docs/model-tiers.md).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tiers {
  /// Every tier, in the order the tiers are shown and compared.
  pub ladder: Vec<String>,
}

impl Tiers {
  pub fn holds(&self, tier: &str) -> bool {
    self.ladder.iter().any(|one| one == tier)
  }
}

crate::part!(pub TIERS: Tiers = "tiers.toml");
