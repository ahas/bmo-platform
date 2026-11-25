//! A checkout's submodules brought onto the commits its index pins (docs/worker-slots.md, 「Names and
//! places」): a pin standing on a branch outside the submodule's default branch, and a pin no remote
//! holds while the base checkout's own copy of the submodule does, are fetched and landed where the
//! place is a worktree grown off that base.

use agency_server::places::checkouts::bring_submodules;

use crate::integration::common::alone::alone;
use crate::support::git::{at, git};

use std::path::Path;

// The commits a base checkout's main pins for its three submodules: `plain` on its remote's default
// branch, `side` on another branch of its remote, and `unpushed` at a commit made in the base's own
// copy and never pushed to its remote.
struct Pins {
  plain: String,
  side: String,
  unpushed: String,
}

// A base checkout with those three submodules standing at their pins, and the bare remotes that hold
// what they were pushed.
fn base_with_pins(home: &Path) -> (String, Pins) {
  let repo = home.join("repo");

  git(&["init", "-q", "-b", "main", &at(&repo)]);
  git(&["-C", &at(&repo), "commit", "-q", "--allow-empty", "-m", "start"]);

  // One leaf with a default branch and a side branch, and a bare remote for each of the submodules.
  let leaf = home.join("leaf");

  git(&["init", "-q", "-b", "main", &at(&leaf)]);
  git(&["-C", &at(&leaf), "commit", "-q", "--allow-empty", "-m", "leaf start"]);

  let plain = git(&["-C", &at(&leaf), "rev-parse", "HEAD"]);

  git(&["-C", &at(&leaf), "checkout", "-q", "-b", "side"]);
  git(&["-C", &at(&leaf), "commit", "-q", "--allow-empty", "-m", "on the side"]);

  let side = git(&["-C", &at(&leaf), "rev-parse", "HEAD"]);

  git(&["init", "-q", "--bare", "-b", "main", &at(&home.join("plain.git"))]);
  git(&["init", "-q", "--bare", "-b", "main", &at(&home.join("side.git"))]);

  git(&["-C", &at(&leaf), "push", "-q", &at(&home.join("plain.git")), "main"]);
  git(&["-C", &at(&leaf), "push", "-q", &at(&home.join("side.git")), "main"]);
  git(&["-C", &at(&leaf), "push", "-q", &at(&home.join("side.git")), "side"]);

  // Each submodule added at its remote, then stood where its pin will be recorded: the side branch, and
  // a commit its remote never receives.
  git(&["-C", &at(&repo), "submodule", "add", "-q", &at(&home.join("plain.git")), "plain"]);
  git(&["-C", &at(&repo), "submodule", "add", "-q", &at(&home.join("side.git")), "side"]);
  git(&["-C", &at(&repo), "submodule", "add", "-q", &at(&home.join("plain.git")), "unpushed"]);

  git(&["-C", &at(&repo.join("side")), "checkout", "-q", "side"]);
  git(&["-C", &at(&repo.join("unpushed")), "commit", "-q", "--allow-empty", "-m", "never pushed"]);

  git(&["-C", &at(&repo), "add", "plain", "side", "unpushed"]);
  git(&["-C", &at(&repo), "commit", "-q", "-m", "pins three"]);

  let unpushed = git(&["-C", &at(&repo.join("unpushed")), "rev-parse", "HEAD"]);
  let pins = Pins {
    plain,
    side,
    unpushed,
  };

  (at(&repo), pins)
}

#[test]
fn a_grown_place_lands_each_pin_whatever_branch_it_stands_on() {
  let Some(_run) = alone(module_path!(), "a_grown_place_lands_each_pin_whatever_branch_it_stands_on")
  else {
    return;
  };

  // Submodule remotes on this machine are local paths, whose transport git lets only when it is told
  // so: the allowance stands in this process's own environment, so it reaches the setup's git runs too.
  unsafe { std::env::set_var("GIT_ALLOW_PROTOCOL", "file") };

  let home = tempfile::tempdir().unwrap();
  let (repo, pins) = base_with_pins(home.path());
  let place = home.path().join("repo-w2");

  git(&["-C", &repo, "worktree", "add", "-q", "--detach", &at(&place), "main"]);

  bring_submodules(&place, true).unwrap();

  for (name, pin) in [("plain", pins.plain), ("side", pins.side), ("unpushed", pins.unpushed)] {
    assert_eq!(
      git(&["-C", &at(&place.join(name)), "rev-parse", "HEAD"]),
      pin,
      "{name} stands at the commit its pin records",
    );
  }
}
