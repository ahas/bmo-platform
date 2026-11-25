//! The agents' sleep (docs/sleep.md): the deployment's clock, the hour and window a sleep goes out in, the pass's
//! clocks, the budget and the instruction.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sleep {
  pub utc_offset_hours: i32,
  pub hour: u32,
  pub window_hours: u32,
  pub tick_seconds: u64,
  pub start_delay_seconds: u64,
  pub budget_tokens: u64,
  pub instruction: String,
}

crate::part!(pub SLEEP: Sleep = "sleep.toml");
