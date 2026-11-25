//! A worker's directory worked out on the path's own words.

use std::path::PathBuf;

use agency_server::queue::paths::resolve;

#[test]
fn a_path_is_worked_out_on_its_own_words() {
  assert_eq!(resolve("/srv/infra/"), PathBuf::from("/srv/infra"));
  assert_eq!(resolve("/srv/./infra/../repo"), PathBuf::from("/srv/repo"));
  assert_eq!(resolve("/.."), PathBuf::from("/"));
}
