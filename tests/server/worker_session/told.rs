//! What a worker's session tells its consumers: the names and the shapes `docs/api.md` gives them.

use agency_server::events::Event;
use agency_server::worker_session::told::{SessionFolded, WorkerDowngraded};
use serde_json::json;

#[test]
fn a_downgrade_and_a_fold_travel_under_their_names_in_their_shapes() {
  assert_eq!(WorkerDowngraded::NAME, "worker.changed");
  assert_eq!(SessionFolded::NAME, "session.folded");
  assert_eq!(
    serde_json::to_value(WorkerDowngraded {
      worker: "worker-w2".into(),
      repository: Some("worker".into()),
      role: Some("worker".into()),
      launched: "claude-opus-5".into(),
      answered_on: vec!["claude-opus-4-7".into()],
      dropped: true,
      job: None,
    })
    .unwrap(),
    json!({ "worker": "worker-w2", "repository": "worker", "role": "worker", "launched": "claude-opus-5", "answeredOn": ["claude-opus-4-7"], "dropped": true, "job": null })
  );
  assert_eq!(
    serde_json::to_value(SessionFolded {
      session: "s-1".into()
    })
    .unwrap(),
    json!({ "session": "s-1" })
  );
}
