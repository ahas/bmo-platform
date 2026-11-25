//! The support checkouts of a repository set to `auto`, kept and let go (docs/worker-slots.md, 「An idle support
//! checkout」): a worktree standing at a slot's path recorded, and one idle past the hours taken away unless
//! something holds it or something is open in it.

use agency_server::places::idle_supports::{Pass, pass};
use policy::workers::WORKERS;

use crate::support::git::{at, git};
use crate::support::grounds::ground_on;
use crate::support::rows::repository;
use crate::support::scratch::Scratch;

#[tokio::test]
async fn an_idle_support_checkout_goes_unless_it_is_held_or_has_work_open() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let base = home.path().join("repo");
  let ground = ground_on(scratch.db().clone(), home.path());
  let hour = 3_600_000;
  let now = 1_000 * hour;

  git(&["init", "-q", &at(&base)]);
  git(&["-C", &at(&base), "commit", "-q", "--allow-empty", "-m", "start"]);

  for slot in [2, 3, 4] {
    git(&["-C", &at(&base), "worktree", "add", "-q", "--detach", &format!("{}-w{slot}", at(&base))]);
  }

  repository(scratch.db(), "repo", &at(&base), &[]).await;
  repository(
    scratch.db(),
    "fixed",
    &at(&home.path().join("fixed")),
    &[("support_count", 0i64.into())],
  )
  .await;

  assert_eq!(
    pass(&ground, now).await.unwrap(),
    Pass {
      recorded: vec!["repo-w2".into(), "repo-w3".into(), "repo-w4".into()],
      removed: vec![],
    },
    "the worktrees standing at a slot's path are recorded, none idle yet"
  );

  // repo-w2 held by a hold left long ago, repo-w3 with a file open, repo-w4 idle and clean.
  scratch
    .run(
      r#"INSERT INTO "assignments" ("item_id", "holder_name", "worker_name", "state", "created_at", "changed_at")
         VALUES (5, 'harness', 'repo-w2', 'held', 0, 0)"#,
      vec![],
    )
    .await;
  std::fs::write(base.with_file_name("repo-w3").join("draft.txt"), "open").unwrap();

  let later = now + WORKERS.support_idle_hours * hour;

  assert_eq!(
    pass(&ground, later).await.unwrap(),
    Pass {
      recorded: vec![],
      removed: vec!["repo-w4".into()],
    }
  );
  assert!(base.with_file_name("repo-w2").join(".git").exists(), "a held checkout stays");
  assert!(base.with_file_name("repo-w3").join(".git").exists(), "a checkout with work open stays");
  assert!(!base.with_file_name("repo-w4").exists(), "the idle one's worktree is taken away");
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "support_checkouts" WHERE "repository" = 'repo'"#, vec![])
      .await,
    2
  );
}
