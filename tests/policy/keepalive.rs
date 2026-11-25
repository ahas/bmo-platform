use policy::keepalive::*;
use policy::providers::claude_code::CLAUDE_CODE;

#[test]
fn the_first_pass_and_a_ping_land_inside_the_cache_they_hold() {
  let idle = CLAUDE_CODE.cache.keepalive_idle_minutes as u64 * 60;
  let life = CLAUDE_CODE.cache.ttl_minutes as u64 * 60;

  assert!(idle + KEEPALIVE.start_delay_seconds + KEEPALIVE.tick_seconds < life);
  assert!(!KEEPALIVE.prompt.trim().is_empty());
}
