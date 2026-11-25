//! What a model catalogue entry is held to (docs/model-catalogue.md, 「An entry」).

use serde::Deserialize;

/// The range one number may take: both ends taken unless the lowest is refused, a whole number where
/// it says so.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumberBounds {
  pub min: f64,
  /// The highest value, or none where something outside the policy sets it.
  #[serde(default)]
  pub max: Option<f64>,
  #[serde(default)]
  pub above_min: bool,
  #[serde(default)]
  pub whole: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueDefaults {
  pub temperature: NumberBounds,
  pub top_p: NumberBounds,
  pub presence_penalty: NumberBounds,
  pub max_tokens: NumberBounds,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueRules {
  pub name_pattern: String,
  pub file_pattern: String,
  pub sha256_pattern: String,
  /// The pattern a source that is a path under the data root matches.
  pub path_pattern: String,
  /// The pattern an engine's and a node's name match.
  pub word_pattern: String,
  pub label_most: usize,
  pub note_most: usize,
  pub defaults: CatalogueDefaults,
}

crate::part!(pub MODEL_CATALOGUE: CatalogueRules = "model-catalogue.toml");
