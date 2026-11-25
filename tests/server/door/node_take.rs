//! The bound a program on a node is held to before it takes its turn (docs/work-nodes.md, 「When a node drops」),
//! looked at on moments given rather than waited out, and the bound of its own a program on a PC whose far end could
//! not listen on a port is held to.

use std::time::Duration;

use agency_server::door::node_take::{NodeTake, Take};
use tokio::time::Instant;

const NOTICE: Duration = Duration::from_secs(60);
const BOUND: Duration = Duration::from_secs(300);
const LISTEN_BOUND: Duration = Duration::from_secs(30);

#[test]
fn the_notice_is_said_once_and_the_bound_ends_the_turn_naming_the_node() {
  let mut take = NodeTake::new("u4", NOTICE, BOUND, LISTEN_BOUND);
  let start = Instant::now();

  assert_eq!(take.look(true, None, start), Take::Within);
  assert_eq!(take.look(true, None, start + Duration::from_secs(59)), Take::Within);

  let Take::Notice { reason, stood } = take.look(true, None, start + NOTICE) else {
    panic!("the notice is said as it passes");
  };

  assert!(reason.contains("u4") && reason.contains("60 seconds"), "{reason}");
  assert_eq!(stood, NOTICE);
  assert!(take.noticed());
  assert_eq!(take.look(true, None, start + Duration::from_secs(120)), Take::Within, "said once");

  let Take::Overdue(why) = take.look(true, None, start + BOUND) else {
    panic!("the bound ends the turn");
  };

  assert!(why.contains("u4") && why.contains("300 seconds"), "{why}");
}

#[test]
fn a_while_the_node_s_link_is_not_heard_is_the_link_s_and_the_count_starts_again_after_it() {
  let mut take = NodeTake::new("u4", NOTICE, BOUND, LISTEN_BOUND);
  let start = Instant::now();

  assert_eq!(take.look(true, None, start), Take::Within);
  assert_eq!(take.look(false, None, start + Duration::from_secs(290)), Take::Within);
  // Heard again: the count begins at this look, so the bound is five minutes from here.
  assert_eq!(take.look(true, None, start + Duration::from_secs(400)), Take::Within);
  assert!(matches!(take.look(true, None, start + Duration::from_secs(460)), Take::Notice { .. }));
  assert_eq!(take.look(true, None, start + Duration::from_secs(699)), Take::Within);
  assert!(matches!(take.look(true, None, start + Duration::from_secs(700)), Take::Overdue(_)));
}

// "a port the far end could not listen on is named at once, ends the turn past its own bound, and its waiting is
// cleared once the far end listens again"
#[test]
fn a_port_the_far_end_could_not_listen_on_is_named_at_once_and_ends_the_turn_past_its_bound() {
  let mut take = NodeTake::new("u4", NOTICE, BOUND, LISTEN_BOUND);
  let start = Instant::now();
  let refused = Some((13123, "Address in use"));

  let Take::Notice { reason, .. } = take.look(true, refused, start) else {
    panic!("the refused port is named at once");
  };

  assert!(
    reason.contains("u4") && reason.contains("13123") && reason.contains("Address in use"),
    "{reason}"
  );
  assert!(take.noticed());
  assert_eq!(take.look(true, refused, start + Duration::from_secs(29)), Take::Within, "said once");
  assert_eq!(take.look(true, None, start + Duration::from_secs(29)), Take::Cleared);

  // Refused again: its own bound counts from here.
  assert!(matches!(take.look(true, refused, start + Duration::from_secs(40)), Take::Notice { .. }));

  let Take::Overdue(why) = take.look(true, refused, start + Duration::from_secs(70)) else {
    panic!("the listen bound ends the turn");
  };

  assert!(why.contains("13123") && why.contains("ended"), "{why}");
}
