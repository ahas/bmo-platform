//! A session's warmth judged by the lifetime its provider declares, an hour for a session set aside where
//! none is declared, and a kept record's life the sooner of the lifetime and the deployment's.

use cache_siren::lifetime::{lifetime_ms, record_ttl_ms, set_aside_lifetime_ms, warm};

const MINUTE: i64 = 60_000;

#[test]
fn the_lifetime_is_the_declared_minutes_and_none_where_none_is_declared() {
  assert_eq!(lifetime_ms(Some(30)), Some(30 * MINUTE));
  assert_eq!(lifetime_ms(None), None);
}

#[test]
fn a_session_set_aside_reads_an_hour_where_its_provider_declares_none() {
  assert_eq!(set_aside_lifetime_ms(None), 60 * MINUTE);
  assert_eq!(set_aside_lifetime_ms(Some(5)), 5 * MINUTE);
}

#[test]
fn a_session_is_warm_inside_its_lifetime_and_cold_from_the_lifetime_on() {
  assert!(warm(1_000, 1_000 + 5 * MINUTE - 1, 5 * MINUTE));
  assert!(!warm(1_000, 1_000 + 5 * MINUTE, 5 * MINUTE));
}

#[test]
fn a_kept_record_lives_the_sooner_of_the_lifetime_and_the_deployments_life() {
  assert_eq!(record_ttl_ms(Some(10), Some(20)), Some(10));
  assert_eq!(record_ttl_ms(Some(30), Some(20)), Some(20));
  assert_eq!(record_ttl_ms(None, Some(20)), Some(20));
  assert_eq!(record_ttl_ms(Some(10), None), Some(10));
  assert_eq!(record_ttl_ms(None, None), None);
}
