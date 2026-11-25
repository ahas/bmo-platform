//! Whether a worker's session is still warm: by the lifetime its provider declares for its model, and an hour
//! for a session set aside where its provider declares none. The judgement itself is the cache-siren
//! crate's, tested with it.

use agency_server::worker_session::warmth::{lifetime_ms, set_aside_lifetime_ms};
use policy::providers::claude_code::CLAUDE_CODE;

#[test]
fn the_lifetime_is_the_one_the_provider_declares_for_the_model() {
  let declared = CLAUDE_CODE.cache.ttl_minutes * 60_000;

  assert_eq!(lifetime_ms("claude-code", "model-a"), Some(declared));
  assert_eq!(lifetime_ms("codex", "gpt-5.6-sol"), None, "a member road declares none");
  assert_eq!(lifetime_ms("no-such-provider", "model-a"), None);
}

#[test]
fn a_session_set_aside_reads_an_hour_where_its_provider_declares_none() {
  assert_eq!(set_aside_lifetime_ms("no-such-provider", "model-a"), 60 * 60_000);
  assert_eq!(set_aside_lifetime_ms("claude-code", "model-a"), CLAUDE_CODE.cache.ttl_minutes * 60_000);
}
