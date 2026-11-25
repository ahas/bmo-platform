//! A supervisor's reset: what a caller asks — the supervisor, the ranges and the rooms it names — and work
//! carried to its end whatever becomes of the caller.

use agency_server::control::supervisor_reset::rows::{Range, Scope, picks};
use agency_server::control::supervisor_reset::*;
use serde_json::json;

mod files;
mod rows;

/// The run log's condition for a supervisor answering the rooms of a scope.
fn supervisor_reset_picks(scope: &Scope) -> String {
  picks(scope, &[Range::Conversation])
    .into_iter()
    .find(|one| one.table == "agent_log")
    .unwrap()
    .condition
    .text
}

#[test]
fn an_ask_names_the_supervisor_its_ranges_and_the_rooms_the_caller_says_it_answers() {
  let asked = Asked::read(
    Some(&json!(" pal ")),
    Some(&json!(true)),
    Some(&json!("off")),
    (Some(&json!([-7, 12])), Some(&json!("12, 30"))),
  )
  .unwrap();

  assert_eq!(asked.agent, "pal");
  assert_eq!(asked.ranges, [Range::Conversation]);
  assert_eq!(asked.alone, [-7, 12]);
  assert_eq!(asked.shared, [30], "a room answered alone is no shared one");
  assert!(Asked::read(Some(&json!("")), Some(&json!(true)), None, (None, None)).is_err());
  assert!(Asked::read(Some(&json!("pal")), None, None, (None, None)).is_err());
}

#[tokio::test]
async fn work_carried_to_its_end_outlives_a_caller_that_stops_waiting() {
  let (began, beginning) = tokio::sync::oneshot::channel::<()>();
  let (release, released) = tokio::sync::oneshot::channel::<()>();
  let (done, finished) = tokio::sync::oneshot::channel::<()>();
  // The caller stands as a request's handler does, and is dropped as its connection closes.
  let caller = tokio::spawn(to_its_end(async move {
    let _ = began.send(());
    let _ = released.await;
    let _ = done.send(());
    Ok(())
  }));

  beginning.await.unwrap();
  caller.abort();
  assert!(caller.await.unwrap_err().is_cancelled());
  release.send(()).unwrap();
  assert!(finished.await.is_ok());
}
