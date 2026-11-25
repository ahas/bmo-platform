//! What a work order holding its checkout says (docs/work-orders.md), each line with the `{name}` marks
//! its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckoutHolds {
  pub waits: Waits,
  pub place: Place,
  pub notes: Notes,
  pub git: Git,
}

/// What a stopped order waits on before it lets its checkout go.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Waits {
  pub open_job: String,
  pub lock: String,
  pub ended_units: String,
  pub ended_record: String,
  pub ended_records: String,
  pub own_turns: String,
  pub checking: String,
  pub release: String,
}

/// Where a stopped order stands in its worker's line.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Place {
  pub line: String,
  pub failures: String,
  pub resume: String,
  pub passed: String,
  pub held: String,
  pub owner_stop: String,
}

/// What the order writes down of the change it leaves.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notes {
  pub unsettled: String,
  pub unpushed: String,
  pub uncommitted: String,
  pub left: String,
  pub parked: String,
  pub parked_stopped: String,
  pub not_parked: String,
  pub not_on_main: String,
  pub unread: String,
}

/// Where moving the checkout or a parked change failed.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Git {
  pub own_worktree: String,
  pub base_checkout: String,
  pub not_moved: String,
  pub not_merged: String,
  pub no_branch: String,
  pub not_fetched: String,
  pub add: String,
  pub commit: String,
  pub read_commit: String,
  pub read_head: String,
  pub write_tree: String,
  pub write_commit: String,
  pub move_branch: String,
  pub take_back: String,
  pub read_branch: String,
}

crate::part!(pub CHECKOUT_HOLDS: CheckoutHolds = "checkout-holds");
