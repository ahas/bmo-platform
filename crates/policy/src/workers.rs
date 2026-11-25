//! What a job worker may be (docs/worker-slots.md).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerRules {
  /// The pattern a worker's name matches.
  pub name_pattern: String,
  /// The longest a worker's label may be, in characters.
  pub label_most: usize,
  /// The longest a worker's preamble may be, in characters.
  pub preamble_most: usize,
  /// The longest a worker's dev server command may be, in characters.
  pub dev_command_most: usize,
  /// How many support workers one base may have where its count is a number.
  pub support_most: i64,
  /// How long an idle support checkout of a repository set to `auto` stands before it is taken away, in
  /// hours.
  pub support_idle_hours: i64,
  /// The first support worker's number.
  pub first_support_slot: i64,
  /// How many checkouts one job may hold at once, its own and those lent to it.
  pub job_checkouts_most: usize,
  /// How often a launch or a borrowing waiting for a checkout to lend looks again unwoken, in seconds.
  pub loan_look_seconds: u64,
  /// How long a borrowing that waits holds its ask before it answers that it still waits, in seconds.
  pub borrow_wait_seconds: u64,
  /// How long a loan's give-back stands begun before another read takes it up again, in seconds.
  pub loan_return_seconds: i64,
  /// The message of the commit that parks a lent checkout, with `{checkout}` and `{branch}`.
  pub park_message: String,
}

crate::part!(pub WORKERS: WorkerRules = "workers.toml");
