//! The service's clocks: a pass after the first wait and on every period after, a failing pass not
//! stopping the ones after it.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use agency_server::clock::every;

#[tokio::test]
async fn a_clock_passes_after_its_first_wait_and_goes_on_past_a_failure() {
  let passes = Arc::new(AtomicU32::new(0));
  let counted = passes.clone();
  let clock =
    tokio::spawn(every("test clock", Duration::from_millis(150), Duration::from_millis(20), move || {
      let counted = counted.clone();

      async move {
        // The first pass fails; the clock goes on.
        match counted.fetch_add(1, Ordering::SeqCst) {
          0 => Err("the first pass fails".to_owned()),
          _ => Ok(()),
        }
      }
    }));

  tokio::time::sleep(Duration::from_millis(60)).await;
  assert_eq!(passes.load(Ordering::SeqCst), 0, "no pass before the first wait is over");

  tokio::time::sleep(Duration::from_millis(400)).await;
  clock.abort();

  assert!(passes.load(Ordering::SeqCst) >= 3, "passes went on after the failing one");
}
