//! An agent's place: how soon a program started often is started again, counting that program's own
//! starts alone.

use agent_host::identity::ProcessIdentity;
use agent_host::place::{RunningProgram, in_a_loop, recent_starts, restart_delay};

#[test]
fn three_starts_a_minute_go_at_once_and_then_the_wait_doubles() {
  let now = 1_000_000;
  let starts = |n: u64| (0..n).map(|i| now - 1_000 * (i + 1)).collect::<Vec<_>>();

  assert_eq!(restart_delay(&starts(2), now), 0);
  assert_eq!(restart_delay(&starts(3), now), 2_000);
  assert_eq!(restart_delay(&starts(4), now), 4_000);
  assert_eq!(restart_delay(&starts(5), now), 8_000);
  assert_eq!(restart_delay(&starts(9), now), 30_000);
  // Older than a minute counts for the loop, not the wait.
  assert_eq!(restart_delay(&[now - 120_000; 6], now), 0);
}

#[test]
fn a_program_written_in_place_of_another_starts_its_history_afresh() {
  let now = 1_000_000;
  let running = RunningProgram {
    identity: "first".into(),
    pid: 1,
    birth: Some("1".into()),
    head: ProcessIdentity {
      pid: 1,
      birth: "1".into(),
    },
    starts: 5,
    started_at: now - 1_000,
    pane_id: None,
    recent: (1..=5).map(|i| now - 1_000 * i).collect(),
    pending_until: None,
    resting_since: None,
  };

  // The same program starting again is held back as its own starts say.
  assert_eq!(recent_starts(Some(&running), "first", now).len(), 5);
  assert_eq!(restart_delay(&recent_starts(Some(&running), "first", now), now), 8_000);

  // Another program written in its place goes at once, however often the place's programs were swapped.
  assert!(recent_starts(Some(&running), "second", now).is_empty());
  assert_eq!(restart_delay(&recent_starts(Some(&running), "second", now), now), 0);
  assert!(recent_starts(None, "first", now).is_empty());
}

#[test]
fn six_starts_within_five_minutes_are_a_loop_and_older_ones_are_not() {
  let now = 1_000_000;

  assert!(!in_a_loop(&(1..=5).map(|i| now - 1_000 * i).collect::<Vec<_>>(), now));
  assert!(in_a_loop(&(1..=6).map(|i| now - 40_000 * i).collect::<Vec<_>>(), now));
  assert!(!in_a_loop(&[now - 300_000; 6], now));
}
