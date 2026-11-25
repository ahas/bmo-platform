//! An idle base checkout's main brought up to the remote's (docs/worker-slots.md, 「The base checkout's main
//! brought up to the remote's」): fast-forwarded in place where the base has it checked out, the branch alone
//! where nothing does, and left where something holds the base, an open file stands in the way, or main is
//! ahead of the remote's.

use std::path::Path;

use agency_server::places::base_mains::{Pass, Remote, pass};

use crate::support::git::{at, git};
use crate::support::grounds::ground_on;
use crate::support::rows::repository;
use crate::support::scratch::Scratch;

// A base checkout on main with one commit, following a bare remote that holds the same.
fn base_with_remote(home: &Path) -> (String, String) {
  let base = at(&home.join("repo"));
  let remote = at(&home.join("remote.git"));

  git(&["init", "-q", "-b", "main", &base]);
  git(&["-C", &base, "commit", "-q", "--allow-empty", "-m", "start"]);
  git(&["clone", "-q", "--bare", &base, &remote]);
  git(&["-C", &base, "remote", "add", "origin", &remote]);
  git(&["-C", &base, "fetch", "-q", "origin"]);
  git(&["-C", &base, "branch", "-q", "-u", "origin/main", "main"]);

  (base, remote)
}

// A commit adding `file`, made in `checkout` and pushed onto the remote's main; the commit.
fn pushed(checkout: &str, file: &str) -> String {
  std::fs::write(Path::new(checkout).join(file), "landed\n").unwrap();
  git(&["-C", checkout, "add", file]);
  git(&["-C", checkout, "commit", "-q", "-m", file]);
  git(&["-C", checkout, "push", "-q", "origin", "HEAD:main"]);

  git(&["-C", checkout, "rev-parse", "HEAD"])
}

fn main_of(base: &str) -> String {
  git(&["-C", base, "rev-parse", "refs/heads/main"])
}

#[tokio::test]
async fn a_close_from_a_support_worktree_moves_the_idle_base_checkout_unless_something_stands_in_its_way() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let (base, _) = base_with_remote(home.path());
  let support = format!("{base}-w2");

  git(&["-C", &base, "worktree", "add", "-q", "--detach", &support, "main"]);
  repository(scratch.db(), "repo", &base, &[]).await;

  let landed = pushed(&support, "first.txt");

  assert_ne!(main_of(&base), landed, "the push leaves the base's main behind");

  // Held: the base stays where it is.
  scratch
    .run(
      r#"INSERT INTO "assignments" ("item_id", "holder_name", "worker_name", "state", "created_at", "changed_at")
         VALUES (5, 'harness', 'repo', 'held', 0, 0)"#,
      vec![],
    )
    .await;
  assert_eq!(pass(&ground, Remote::AsKnown).await.unwrap(), Pass::default());
  assert_ne!(main_of(&base), landed, "a held base is not moved");

  scratch.run(r#"DELETE FROM "assignments""#, vec![]).await;
  std::fs::write(Path::new(&base).join("draft.txt"), "open\n").unwrap();

  assert_eq!(
    pass(&ground, Remote::AsKnown).await.unwrap(),
    Pass {
      moved: vec!["repo".into()],
      refused: vec![],
    }
  );
  assert_eq!(
    main_of(&base),
    landed,
    "the idle base's main is fast-forwarded from what the push recorded"
  );
  assert!(Path::new(&base).join("first.txt").exists(), "the checkout moved with its main");
  assert!(Path::new(&base).join("draft.txt").exists(), "what was open stays open");

  // An open file the next commit would overwrite: git moves nothing, and the base is named.
  let next = pushed(&support, "second.txt");

  std::fs::write(Path::new(&base).join("second.txt"), "mine\n").unwrap();
  assert_eq!(
    pass(&ground, Remote::AsKnown).await.unwrap(),
    Pass {
      moved: vec![],
      refused: vec!["repo".into()],
    }
  );
  assert_eq!(main_of(&base), landed);

  std::fs::remove_file(Path::new(&base).join("second.txt")).unwrap();
  pass(&ground, Remote::AsKnown).await.unwrap();
  assert_eq!(main_of(&base), next);

  // A main ahead of the remote's is left as it stands.
  git(&["-C", &base, "commit", "-q", "--allow-empty", "-m", "not pushed"]);

  let ahead = main_of(&base);

  assert_eq!(pass(&ground, Remote::Fetched).await.unwrap(), Pass::default());
  assert_eq!(main_of(&base), ahead);
}

#[tokio::test]
async fn a_push_from_elsewhere_is_fetched_and_a_main_checked_out_nowhere_moves_alone() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let (base, remote) = base_with_remote(home.path());
  let elsewhere = at(&home.path().join("elsewhere"));

  git(&["clone", "-q", &remote, &elsewhere]);
  // The base given back on a job's branch, as a loan leaves it.
  git(&["-C", &base, "switch", "-q", "-c", "work/7"]);
  git(&["-C", &base, "config", "branch.work/7.bmo-main", "main"]);
  repository(scratch.db(), "repo", &base, &[]).await;

  let landed = pushed(&elsewhere, "far.txt");

  assert_eq!(
    pass(&ground, Remote::AsKnown).await.unwrap(),
    Pass::default(),
    "a push this machine has not fetched is unknown to it"
  );

  let before = git(&["-C", &base, "rev-parse", "HEAD"]);

  assert_eq!(
    pass(&ground, Remote::Fetched).await.unwrap(),
    Pass {
      moved: vec!["repo".into()],
      refused: vec![],
    }
  );
  assert_eq!(main_of(&base), landed, "main is brought up once fetched");
  assert_eq!(git(&["-C", &base, "rev-parse", "HEAD"]), before, "the base's own branch is not touched");
  assert!(!Path::new(&base).join("far.txt").exists());
}
