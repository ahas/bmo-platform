//! A supervisor's run going in a room: its start published as it begins and its end as it is let go.

use agency_server::events::Bus;
use agency_server::supervisor_turn::going::{GoingRuns, SupervisorRunGoing, going};

#[test]
fn a_run_is_published_going_and_then_ended_once_let_go() {
  let bus = Bus::default();
  let mut seen = bus.subscribe::<SupervisorRunGoing>();
  let held = going(&bus, "pal", "handoff:7:1", 7);

  assert_eq!(
    seen.ready(),
    Some(SupervisorRunGoing {
      agent: "pal".into(),
      run: "handoff:7:1".into(),
      chat_id: 7,
      going: true,
    })
  );
  assert_eq!(seen.ready(), None);

  drop(held);

  assert_eq!(seen.ready().map(|ended| ended.going), Some(false));
}

#[test]
fn the_runs_going_are_those_started_and_not_yet_ended() {
  let bus = Bus::default();
  let runs = GoingRuns::on(&bus);
  let first = going(&bus, "pal", "turn:7:1", 7);
  let _second = going(&bus, "pal", "turn:8:1", 8);
  let _other = going(&bus, "friend", "turn:7:2", 7);

  assert_eq!(runs.of("pal"), ["turn:7:1", "turn:8:1"]);

  drop(first);

  assert_eq!(runs.of("pal"), ["turn:8:1"]);
  assert_eq!(runs.of("friend"), ["turn:7:2"]);
  assert!(runs.of("nobody").is_empty());
}
