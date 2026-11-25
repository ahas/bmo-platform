//! Agency's server draining on its stop (docs/sessions.md, 「A restart hands the session over」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drain {
  /// How long the calls in hand at the stop may go on before the server exits, in seconds.
  pub short_calls_seconds: u64,
  /// What a call refused while the server drains is told to wait before it asks again, in seconds.
  pub retry_after_seconds: u64,
}

crate::part!(pub DRAIN: Drain = "drain.toml");
