//! A checkout lent to a job put on the job's branch, and parked there as the loan is given back
//! (docs/worker-slots.md, 「A checkout lent to a job」, 「Loans across a job's review」). It runs git in the
//! checkout itself, so it reaches only a checkout on this machine; one on a work node is lent as it stands.

use std::path::Path;

use policy::fill_marks;
use policy::workers::WORKERS;

use super::checkouts::{branch_exists, git_at, main_branch, set_up};

/// The branch a job of an item works on in a checkout lent to it.
pub fn job_branch(item: i64) -> String {
  format!("work/{item}")
}

/// The branch a checkout has checked out, none where it stands detached or cannot be read.
pub fn standing_branch(cwd: &str) -> Option<String> {
  git_at(cwd, &["symbolic-ref", "--quiet", "--short", "HEAD"])
    .ok()
    .filter(|branch| !branch.is_empty())
}

// Whether anything stands open in the checkout, tracked or not.
fn has_open(cwd: &str) -> Result<bool, String> {
  Ok(!git_at(cwd, &["status", "--porcelain"])?.is_empty())
}

// What is open in the checkout committed onto the branch it stands on, unchecked: parked work.
fn park(cwd: &str, branch: &str) -> Result<(), String> {
  if !has_open(cwd)? {
    return Ok(());
  }

  let message = fill_marks(&WORKERS.park_message, &[("checkout", cwd), ("branch", branch)]);

  git_at(cwd, &["add", "--all"])?;
  git_at(
    cwd,
    &["-c", "commit.gpgsign=false", "commit", "--no-verify", "--quiet", "--message", &message],
  )
  .map(|_| ())
  .map_err(|why| format!("what was open in {cwd} could not be parked on {branch}: {why}"))
}

/// Whether a checkout stands on main with something open in it: somebody's work, which no loan takes. Main is read
/// off `base`, the repository's base checkout.
pub fn open_on_main(cwd: &str, base: &str) -> bool {
  standing_branch(cwd).is_some_and(|standing| standing == main_branch(base))
    && has_open(cwd).unwrap_or(true)
}

/// A lent checkout put on the job's branch: what was open there on a branch other than main and the job's parked
/// onto that branch; the job's branch checked out where it exists, and otherwise made from main with main written
/// beside it in the repository's git config; and the repository's setup run where the branch moved. Main is read
/// off `base`, the repository's base checkout. Answers whether the checkout moved.
pub fn put_on_branch(cwd: &str, base: &str, branch: &str, bun: &str) -> Result<bool, String> {
  let standing = standing_branch(cwd);

  if standing.as_deref() == Some(branch) {
    return Ok(false);
  }

  let main = main_branch(base);

  if let Some(other) = standing.as_deref().filter(|other| *other != main) {
    park(cwd, other)?;
  }

  let moved = if branch_exists(cwd, branch) {
    git_at(cwd, &["switch", "--quiet", branch])
  } else {
    git_at(cwd, &["switch", "--quiet", "--create", branch, &main])
      .and_then(|_| git_at(cwd, &["config", &format!("branch.{branch}.bmo-main"), &main]))
  };

  moved.map_err(|why| format!("{cwd} could not be put on {branch}: {why}"))?;
  set_up(Path::new(cwd), bun, false)?;

  Ok(true)
}

/// A lent checkout parked as its loan is given back: what is open committed onto the job's branch, and the
/// repository's own checkout — the one git keeps the repository in — put back on main, the branch deleted where
/// main already holds all of it. A worktree stays on the branch, and a checkout the job moved off its branch is
/// left as it stands.
pub fn give_back(cwd: &str, branch: &str) -> Result<(), String> {
  if standing_branch(cwd).as_deref() != Some(branch) {
    return Ok(());
  }

  park(cwd, branch)?;

  if !Path::new(cwd).join(".git").is_dir() {
    return Ok(());
  }

  let main = main_branch(cwd);

  git_at(cwd, &["switch", "--quiet", &main])
    .map_err(|why| format!("{cwd} could not be put back on {main}: {why}"))?;
  // Refused while main lacks some of it, which keeps the branch.
  let _ = git_at(cwd, &["branch", "--delete", "--quiet", branch]);

  Ok(())
}
