//! What a registration and its addresses are held to (docs/own-llm.md, 「What a registration holds」).

use serde::Deserialize;

/// The whole numbers a field may hold, both ends taken.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WholeBounds {
  pub min: i64,
  pub max: i64,
}

impl WholeBounds {
  pub fn holds(&self, value: i64) -> bool {
    (self.min..=self.max).contains(&value)
  }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryRules {
  /// The pattern a registration's name and a node's label match.
  pub name_pattern: String,
  /// The pattern a model matches.
  pub model_pattern: String,
  /// The pattern a key file's name under the data root's `env/` matches.
  pub key_file_pattern: String,
  /// The longest a label may be, in characters.
  pub label_most: usize,
  /// The pattern a language a registration names matches.
  pub language_pattern: String,
  /// The most languages a registration may name.
  pub languages_most: usize,
  pub context_tokens: WholeBounds,
  pub answer_tokens: WholeBounds,
}

crate::part!(pub REGISTRY: RegistryRules = "registry.toml");
