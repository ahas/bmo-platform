//! What the registered models declare (docs/provider-presence.md).

use serde::Deserialize;

use super::Declared;

#[derive(Debug, Deserialize)]
pub struct Registered {
  #[serde(flatten)]
  pub declared: Declared,
  pub gateway: Gateway,
  pub presence: Presence,
}

/// Where the model gateway answers by default (docs/own-llm.md, 「The gateway」).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gateway {
  /// What the gateway writes into what the model reads.
  pub own_words: Vec<String>,
  pub url: String,
}

/// The gateway's key file under the data root's env directory, and what the registry says where it
/// cannot answer.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Presence {
  pub key_file: String,
  pub missing_code: String,
  pub missing: String,
  pub no_archive: String,
  pub no_address_code: String,
  pub no_address: String,
  pub not_registered: String,
  /// What a run naming a node the registration no longer has says: `{name}` and `{node}`.
  pub node_missing: String,
}

crate::part!(pub REGISTERED: Registered = "providers/registered.toml");
