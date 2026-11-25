//! Whether a worker's session is still warm (docs/sessions.md, 「A worker's session per model」): the time
//! since its last answer against the cache lifetime its provider declares for its model, judged by the
//! `cache-siren` crate. Agency keeps no cache measurement of its own.

use harness_providers::provider;

pub use cache_siren::lifetime::{record_ttl_ms, warm};

// The minutes a provider declares a model's cache lives for; none where it declares none.
fn declared_minutes(mode: &str, model: &str) -> Option<i64> {
  provider(mode).and_then(|provider| provider.cache_minutes(model))
}

/// The cache lifetime of a session on a provider and model, in milliseconds: the one the provider declares
/// for the model; none where it declares none.
pub fn lifetime_ms(mode: &str, model: &str) -> Option<i64> {
  cache_siren::lifetime::lifetime_ms(declared_minutes(mode, model))
}

/// The lifetime a session set aside is judged warm by: its cache lifetime, or an hour where none is
/// declared.
pub fn set_aside_lifetime_ms(mode: &str, model: &str) -> i64 {
  cache_siren::lifetime::set_aside_lifetime_ms(declared_minutes(mode, model))
}
