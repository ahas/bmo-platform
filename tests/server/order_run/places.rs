//! The standing branch checkouts: a warm place reused oldest first, then a cold one, then a new number; a
//! line whose prefix moved dying like one past its life; a place going past the cap or rested past the keep;
//! the places taken counted from what the consumer hands in; and a line's life the provider's published
//! lifetime less the keepalive schedule's one margin, none where nothing is published.

use std::collections::HashSet;

use agency_server::order_run::places::{Agent, CAP, Held, KEEP_MS, busy_slots, choose, prunable, ttl_ms};
use policy::pool_warm::POOL_WARM;
use policy::providers::claude_code::CLAUDE_CODE;

#[test]
fn a_lines_life_is_the_published_lifetime_less_the_one_margin_and_none_where_none_is_published() {
  let margin = POOL_WARM.margin_minutes * 60_000;

  assert_eq!(
    ttl_ms("claude-code", "opus", None),
    Some(CLAUDE_CODE.cache.ttl_minutes * 60_000 - margin)
  );
  assert_eq!(ttl_ms("codex", "gpt-5.6-sol", None), None, "a member road publishes no lifetime");
  assert_eq!(ttl_ms("openrouter", "anthropic/model", None), None, "nor does an unrecorded upstream");
}

fn agent(slot: i64, model: &str, at: i64) -> Agent {
  Agent {
    slot,
    mode: "claude-code".into(),
    model: model.into(),
    role: "scout".into(),
    upstream: String::new(),
    prefix_digest: "prefix".into(),
    last_reply_at: at,
  }
}

#[test]
fn a_warm_place_is_reused_oldest_first_then_a_cold_one_then_a_new_number() {
  let now = 10_000_000;
  let at = now / 1000 - 60;
  let agents = [agent(2, "opus", at), agent(1, "opus", at - 10), agent(3, "sonnet", at)];
  let busy = HashSet::from([4]);
  let choice = choose(&agents, &[1, 2, 3], &busy, 4, ("claude-code", "opus"), now, |_| "prefix".into());

  assert_eq!(choice.reuse, [1, 2]);
  assert_eq!(choice.cold, [3]);
  assert_eq!(choice.opened, [5]);
  assert!(choice.expired.is_empty());
}

#[test]
fn a_row_whose_prefix_moved_dies_like_one_past_its_life() {
  let now = 10_000_000;
  let agents = [agent(1, "opus", now / 1000 - 60)];
  let choice = choose(&agents, &[1], &HashSet::new(), 1, ("claude-code", "opus"), now, |_| "moved".into());

  assert!(choice.reuse.is_empty());
  assert_eq!(choice.cold, [1]);
  assert_eq!(choice.expired, agents);
}

#[test]
fn a_place_goes_past_the_cap_or_rested_past_the_keep() {
  let now = 30 * 24 * 60 * 60 * 1000;
  let old = agent(2, "opus", (now - KEEP_MS) / 1000 - 1);
  let slots: Vec<i64> = (1..=10).collect();

  assert_eq!(prunable(&[old], &slots, &HashSet::from([9]), now, CAP, KEEP_MS), [2, 10]);
}

#[tokio::test]
async fn the_places_taken_are_the_ones_the_consumer_names_in_the_pool_its_orders_checkout_sets() {
  let held = [
    Held {
      cwd: "/nowhere/one".into(),
      slot: 1,
    },
    Held {
      cwd: "/nowhere/two".into(),
      slot: 2,
    },
    Held {
      cwd: String::new(),
      slot: 3,
    },
  ];
  let one = agency_server::places::branch::pool_of("/nowhere/one").await;

  assert_eq!(busy_slots(None, &held).await, HashSet::from([1, 2]));
  assert_eq!(busy_slots(Some(&one), &held).await, HashSet::from([1]));
}
