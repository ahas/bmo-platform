//! The rest a usage limit calls for.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rest {
  /// How long a login is put aside where its limit named no reset moment, in seconds.
  pub fallback_seconds: i64,
  /// What the owner is told as the rest that silenced the last open login begins.
  pub resting: String,
  /// How often the deployment is looked at for a silence no written rest told, in seconds.
  pub lift_check_seconds: u64,
}

crate::part!(pub REST: Rest = "rest.toml");
