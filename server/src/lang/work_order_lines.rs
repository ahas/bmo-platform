//! The words a work order reads as in lines (docs/work-orders.md), and what a resume adds where the queue is
//! held still.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkOrderLines {
  /// The round past the first: `{round}`.
  pub round: String,
  /// A task run past its first time: `{attempt}`.
  pub attempt: String,
  /// The order's purpose: `{intent}`.
  pub intent: String,
  /// The order's instruction: `{instruction}`.
  pub instruction: String,
  /// The tasks of earlier rounds, counted: `{count}`.
  pub history: String,
  pub holds_checkout: String,
  /// What letting the checkout go waits on: `{wait}`.
  pub release_wait: String,
  /// The owner's stop: `{owner}`, `{why}`, `{at}`.
  pub owner_stop: String,
  /// A wind-down under way: `{why}`, `{at}`.
  pub closing: String,
  /// The stops that were failures, counted: `{count}`.
  pub failed_stops: String,
  /// The last line: `{at}`.
  pub updated: String,
  pub resume: Resume,
}

/// What holds the queue still after a resume.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resume {
  pub owner: String,
  pub deploy: String,
}

crate::part!(pub WORK_ORDER_LINES: WorkOrderLines = "work-order-lines");
