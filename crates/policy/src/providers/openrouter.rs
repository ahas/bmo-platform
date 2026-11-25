//! What OpenRouter declares (docs/provider-presence.md).

use serde::Deserialize;

use super::Declared;

#[derive(Debug, Deserialize)]
pub struct OpenRouter {
  #[serde(flatten)]
  pub declared: Declared,
  pub presence: Presence,
  pub gateway: Gateway,
  pub cache: Cache,
  pub catalogue: Catalogue,
  pub balance: super::Balance,
  /// The Claude Code effort word a run of each OpenRouter word goes out with.
  #[serde(default)]
  pub claude: Option<super::Claude>,
}

/// How long a prefix stays cached on each upstream that publishes it, by the model names it begins.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cache {
  pub upstreams: Vec<UpstreamCache>,
}

/// One upstream's lifetime for the models whose names begin as given: the minutes a prefix stays cached
/// and the minutes a run may stand idle before a ping warms it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamCache {
  pub upstream: String,
  pub prefix: String,
  pub ttl_minutes: i64,
  pub keepalive_idle_minutes: i64,
}

impl Cache {
  /// The lifetime an upstream gives a model: its entry with the longest start the model begins with, and
  /// none where the upstream publishes none for it.
  pub fn of(&self, upstream: &str, model: &str) -> Option<&UpstreamCache> {
    self
      .upstreams
      .iter()
      .filter(|entry| entry.upstream == upstream && model.starts_with(&entry.prefix))
      .max_by_key(|entry| entry.prefix.len())
  }
}

/// Where its API answers, and the variable a stand-in's address is named in.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gateway {
  pub api: String,
  pub api_env: String,
}

impl Gateway {
  /// The API's address on a machine: the stand-in's its environment names, or the gateway's own.
  pub fn api_on<'a>(&'a self, vars: &'a std::collections::BTreeMap<String, String>) -> &'a str {
    vars
      .get(&self.api_env)
      .map(String::as_str)
      .filter(|named| !named.is_empty())
      .unwrap_or(&self.api)
  }
}

/// The key file its member names, under the data root's env directory.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Presence {
  pub key_file: String,
  pub missing_code: String,
  pub missing: String,
  pub not_held: String,
}

/// Where the model listing is read from, and the variable a stand-in's address is named in.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalogue {
  pub url: String,
  pub url_env: String,
}

crate::part!(pub OPENROUTER: OpenRouter = "providers/openrouter.toml");
