//! A turn's caller key (docs/api.md, 「sessions」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnKeys {
  /// How long a keyed turn's end is kept after it ended, in seconds.
  pub kept_seconds: u64,
}

crate::part!(pub TURN_KEYS: TurnKeys = "turn-keys.toml");
