//! The co-worker pool's warming of the places its reservations will use (docs/coworker-pool.md,
//! 「Reserving places and keepalive」): the margin left off a published lifetime, what each kind of place
//! declares, and why a ping warmed nothing.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolWarm {
  pub margin_minutes: i64,
  pub branch: Branch,
  pub native: Refused,
  pub pings: PingWords,
}

/// The standing branch checkouts' pings: how long one may take, and what it is titled as in the place's
/// pane.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Branch {
  pub title: String,
}

/// A kind of place with nothing that can be warmed.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Refused {
  pub refusal: String,
}

/// Why a ping came back without warming its place.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PingWords {
  pub off: String,
  pub no_prefix: String,
  pub no_account: String,
  pub no_pane: String,
  pub no_key: String,
  pub no_upstream: String,
  pub other_upstream: String,
}

crate::part!(pub POOL_WARM: PoolWarm = "pool-warm.toml");
