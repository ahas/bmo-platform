//! A node's pane retired by a deployment (docs/work-nodes.md, 「In herdr」): the verb ends for its loop to start
//! the deploy link's binary once the link names another release of the same deploy tree, and nothing else
//! retires it.

use std::path::Path;

use agency_server::verbs::node_pane::retired_by;

#[test]
fn a_pane_run_from_an_older_release_of_the_tree_is_retired_by_the_links_newer_one() {
  let releases = Path::new("/deploy-releases");

  assert!(retired_by(&releases.join("old/bin/bmo-agency"), &releases.join("new/bin/bmo-agency")));
}

#[test]
fn a_pane_run_from_the_links_own_release_is_not_retired() {
  let binary = Path::new("/deploy-releases/live/bin/bmo-agency");

  assert!(!retired_by(binary, binary));
}

#[test]
fn a_binary_of_no_release_beside_the_links_is_never_retired() {
  let live = Path::new("/deploy-releases/live/bin/bmo-agency");

  assert!(!retired_by(Path::new("/checkout/target/debug/bmo-agency"), live));
  assert!(!retired_by(Path::new("bmo-agency"), live));
}
