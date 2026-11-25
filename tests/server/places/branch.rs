//! A standing branch checkout: the place a directory belongs to, its label, its agent and its pool.

use std::path::Path;

use agency_server::places::branch::{place_agent, place_label, place_of, pool_of};

#[test]
fn a_directory_inside_a_standing_checkout_is_that_place() {
  let data = Path::new("/data");

  assert_eq!(place_of(data, "/data/branch-agents/repo/3"), Some(("repo".to_owned(), 3)));
  assert_eq!(place_of(data, "/data/branch-agents/repo/12/server/./x"), Some(("repo".to_owned(), 12)));
  assert_eq!(place_of(data, "/data/branch-agents/repo/x"), None);
  assert_eq!(place_of(data, "/data/branch-agents/repo"), None);
  assert_eq!(place_of(data, "/data/branch-agents/../elsewhere/repo/1"), None);
  assert_eq!(place_of(data, "/repos/repo"), None);
}

#[test]
fn a_place_is_read_by_its_pool_and_its_number() {
  assert_eq!(place_label("repo", 3), "co-worker repo 03");
  assert_eq!(place_agent("repo-1a2b", 3), "branch-repo-1a2b-3");
  assert_eq!(place_agent("a.b", 1), "branch-a-b-1");
}

#[tokio::test]
async fn a_directory_that_is_no_repository_is_a_pool_of_its_own_path() {
  assert_eq!(pool_of("/nowhere/my repo").await, "my_repo-4c94ea6004");
  assert_eq!(pool_of("/nowhere/x.git").await, "x.git-82a34162a5");
}
