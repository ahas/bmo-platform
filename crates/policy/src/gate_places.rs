//! The places a gate's turns stand in on a road whose program carries its own turns (docs/agent-control.md,
//! 「A gate's places」): how many stand at once and for one program, and how many tokens its session may
//! hold.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatePlaces {
  pub places: usize,
  pub per_program: usize,
  pub session_tokens: i64,
}

crate::part!(pub GATE_PLACES: GatePlaces = "gate-places.toml");
