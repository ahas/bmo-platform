//! The keepalive pass: what a ping says and the pass's clocks.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keepalive {
  pub prompt: String,
  pub start_delay_seconds: u64,
  pub tick_seconds: u64,
}

crate::part!(pub KEEPALIVE: Keepalive = "keepalive.toml");
