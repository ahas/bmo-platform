//! What a run is refused with about the provider and the job model it goes out on
//! (`lang/<language>/providers.yaml`).

use serde::Deserialize;

/// Why a run is refused about its provider or its job model.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderWords {
  /// Its provider is switched off.
  pub off: String,
  /// A co-worker asked a provider serving one model alone.
  pub single_model: String,
  /// The job model it names, `{selection}`, is none.
  pub model_wrong: String,
  /// An automatic job finds no job model on any provider that is on.
  pub no_model: String,
}

crate::part!(pub PROVIDER_WORDS: ProviderWords = "providers");
