//! The words of the one road resolution (docs/cache-fallback.md, 「One road resolution」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Roads {
  pub reasons: Reasons,
  pub logins: Logins,
  pub closed: Closed,
  pub job: Job,
  pub blocks: Blocks,
}

/// Why a provider cannot answer now.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reasons {
  /// A provider the owner turned off.
  pub off: String,
  /// No quota left, with `{window}`, the window read full.
  pub quota: String,
  /// A balance at or below zero, with `{amount}` and `{currency}`.
  pub balance: String,
  /// No login of a provider that holds logins is open.
  pub no_login: String,
  /// A provider on a named road whose program does not carry the key's line, with `{line}`.
  pub uncarried: String,
  /// A provider on a named road that does not list the model named, with `{model}`.
  pub unlisted: String,
}

/// Why one login cannot answer now, each with `{account}`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Logins {
  pub off: String,
  /// With `{until}`.
  pub resting: String,
  /// One model resting on the login, with `{model}` and `{until}`.
  pub model_resting: String,
  pub person: String,
  /// A login never signed in on this machine, or one whose credential file is gone.
  pub no_login: String,
  pub key: String,
  pub balance: String,
}

/// The sentence a closed road is refused or held with.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Closed {
  pub head: String,
  /// The head where the caller named the road.
  pub named: String,
  /// With `{provider}` and `{reason}`.
  pub provider: String,
  /// With `{at}`.
  pub reopens: String,
  /// With `{provider}` and `{command}`.
  pub person: String,
  /// With `{providers}`, `{line}` and `{needs}`.
  pub uncarried: String,
}

/// A job launched again on the next road.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
  /// With `{from}` and `{why}`.
  pub continued: String,
  /// With `{roads}`.
  pub moved: String,
  /// With `{road}` and `{why}`.
  pub road: String,
  /// How often a job waiting for a road asks again, in seconds.
  pub wait_seconds: u64,
}

/// How long a road a walk left for its provider's failure stays blocked, and the words of its block.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blocks {
  /// An outage, in seconds.
  pub outage_seconds: i64,
  /// A refused access, in seconds.
  pub access_seconds: i64,
  /// A limit whose failure names no reset, in seconds.
  pub limit_seconds: i64,
  /// With `{kind}` and `{why}`.
  pub reason: String,
  /// With `{until}` and `{reason}`.
  pub passed: String,
}

crate::part!(pub ROADS: Roads = "roads.toml");
