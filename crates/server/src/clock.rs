//! The service's own clocks: a pass run on a period for as long as the process stands.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use tokio::time::MissedTickBehavior;

/// One pass of a clock, boxed so passes of different kinds stand in one list.
pub type Passing = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

/// One pass once the first wait is over and once a period after, a pass never overlapping the one
/// before it; a pass that fails is said on the process's own output under its name and the next one
/// goes on.
pub async fn every<Pass, Passed>(name: &'static str, first: Duration, period: Duration, pass: Pass)
where
  Pass: Fn() -> Passed,
  Passed: Future<Output = Result<(), String>>,
{
  let mut clock = tokio::time::interval_at(tokio::time::Instant::now() + first, period);

  clock.set_missed_tick_behavior(MissedTickBehavior::Delay);

  loop {
    clock.tick().await;

    if let Err(why) = pass().await {
      eprintln!("agency: the {name}: {why}");
    }
  }
}
