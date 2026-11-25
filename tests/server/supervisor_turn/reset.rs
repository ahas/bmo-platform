//! What a supervisor's reset tells the units holding the rest of it, by the names and shapes the boundary
//! gives them.

use agency_server::events::Event;
use agency_server::supervisor_turn::reset::{SupervisorCutOff, SupervisorReset};
use serde_json::json;

#[test]
fn a_cut_off_and_a_reset_are_told_in_their_boundary_shapes() {
  assert_eq!(SupervisorCutOff::NAME, "supervisor-reset.created");
  assert_eq!(SupervisorReset::NAME, "supervisor-reset.created");
  assert_eq!(
    serde_json::to_value(SupervisorCutOff {
      agent: "pal".into(),
      point: 1_790_000_000_000,
    })
    .unwrap(),
    json!({ "agent": "pal", "point": 1_790_000_000_000_i64 })
  );
  assert_eq!(
    serde_json::to_value(SupervisorReset {
      agent: "pal".into(),
      ranges: vec!["conversation".into()],
      alone: vec![-7],
      shared: vec![12],
      runs: vec!["chat:12:1790000000".into()],
      by: "console".into(),
      at: 1_790_000_000,
    })
    .unwrap(),
    json!({
      "agent": "pal",
      "ranges": ["conversation"],
      "alone": [-7],
      "shared": [12],
      "runs": ["chat:12:1790000000"],
      "by": "console",
      "at": 1_790_000_000,
    })
  );
}
