//! What agency's server seeds a fresh store with as it settles it (docs/store.md, 「Its seeds」): the account
//! a store holding none is given and the catalogue's entries; the roles are `roles.toml`'s.

use serde::Deserialize;

/// The one account a store holding none is given; its credential is the machine's own login, read where the
/// store is settled.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountSeed {
  pub id: String,
  pub name: String,
}

/// One model file of the catalogue, in the catalogue's own fields (docs/model-catalogue.md, 「An entry」).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueEntry {
  pub name: String,
  pub label: String,
  pub file: String,
  pub bytes: i64,
  pub sha256: String,
  #[serde(default)]
  pub source: Option<String>,
  #[serde(default)]
  pub engine: Option<String>,
  #[serde(default)]
  pub registration: Option<String>,
  #[serde(default)]
  pub thinking: Option<bool>,
  #[serde(default)]
  pub temperature: Option<f64>,
  #[serde(default)]
  pub top_p: Option<f64>,
  #[serde(default)]
  pub presence_penalty: Option<f64>,
  #[serde(default)]
  pub max_tokens: Option<i64>,
  #[serde(default)]
  pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seeds {
  pub account: AccountSeed,
  pub catalogue: Vec<CatalogueEntry>,
}

crate::part!(pub SEEDS: Seeds = "seeds.toml");
