//! A running job's lease, kept on the harness's behalf (docs/api.md, 「assignments and jobs」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobLeases {
  /// How often the leases of the running jobs are renewed, in seconds.
  pub renew_seconds: u64,
  /// The span each renewal, or a lease taken again, asks for, in milliseconds.
  pub ttl_ms: i64,
}

crate::part!(pub JOB_LEASES: JobLeases = "job-leases.toml");
