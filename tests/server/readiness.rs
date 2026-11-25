//! Readiness: the process answering and its code tree, ready once the places have stood.

use std::path::Path;

use agency_server::readiness::Readiness;

#[test]
fn readiness_names_this_process_and_its_tree_and_is_ready_once_the_places_stood() {
  let readiness = Readiness::default();
  let before = readiness.answer(Path::new("/releases/one"));

  assert_eq!(before.pid, std::process::id());
  assert_eq!(before.code, Path::new("/releases/one"));
  assert!(!before.ready);

  readiness.stood();

  assert!(readiness.answer(Path::new("/releases/one")).ready);
  assert_eq!(
    serde_json::to_value(readiness.answer(Path::new("/r"))).unwrap(),
    serde_json::json!({ "pid": std::process::id(), "code": "/r", "ready": true })
  );
}
