//! Whether a session is still warm (docs/sessions.md, 「A worker's session per model」): the time since its
//! last answer against the cache lifetime its provider declares for its model, which the caller reads off
//! the provider member and hands in.

const MINUTE_MS: i64 = 60_000;

// The lifetime a session set aside is judged warm by where its provider declares none: the subscription's
// hour.
const SET_ASIDE_DEFAULT_MS: i64 = 60 * MINUTE_MS;

/// The cache lifetime in milliseconds of the minutes a provider declares for a model; none where it
/// declares none.
pub fn lifetime_ms(declared_minutes: Option<i64>) -> Option<i64> {
  declared_minutes.map(|minutes| minutes * MINUTE_MS)
}

/// The lifetime a session set aside is judged warm by: its cache lifetime, or an hour where none is
/// declared.
pub fn set_aside_lifetime_ms(declared_minutes: Option<i64>) -> i64 {
  lifetime_ms(declared_minutes).unwrap_or(SET_ASIDE_DEFAULT_MS)
}

/// Whether a session last answered at `last_at` is warm at `now`, both in epoch milliseconds: inside the
/// lifetime, and cold from the lifetime on.
pub fn warm(last_at: i64, now: i64, lifetime: i64) -> bool {
  now - last_at < lifetime
}

/// How long a kept session's record stands unused before it is retired: the cache lifetime, or the life
/// the deployment gives a session where that is sooner; none where neither names one.
pub fn record_ttl_ms(configured: Option<i64>, lifetime: Option<i64>) -> Option<i64> {
  match (configured, lifetime) {
    (Some(configured), Some(lifetime)) => Some(configured.min(lifetime)),
    (configured, lifetime) => configured.or(lifetime),
  }
}
