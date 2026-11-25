//! A path read across the roots a spec names.

use node_link::spec::*;

#[test]
fn the_first_root_holding_a_path_takes_it() {
  let roots = [("/h/a/.local/share/bmo/data", "/n/root/data"), ("/h/a", "/n")];

  assert_eq!(
    rewrite("/h/a/.local/share/bmo/data/agent-places/w", roots.into_iter()),
    "/n/root/data/agent-places/w"
  );
  assert_eq!(rewrite("/h/a/.claude", roots.into_iter()), "/n/.claude");
  assert_eq!(rewrite("/h/ab", roots.into_iter()), "/h/ab");
  assert_eq!(rewrite("/h/a", roots.into_iter()), "/n");
}
