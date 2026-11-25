//! When a worker's session is full: past the fold ratio of its model's window, and never on a turn that
//! reports no share.

use agency_server::worker_session::fold::worker_session_full;

#[test]
fn a_worker_session_is_full_past_the_fold_ratio_of_its_models_window_alone() {
  assert!(worker_session_full(Some(0.85), 0.85));
  assert!(!worker_session_full(Some(0.84), 0.85));
  assert!(worker_session_full(Some(0.5), 0.5));
  assert!(!worker_session_full(None, 0.5));
}
