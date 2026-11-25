//! An idle base checkout's main brought up to the remote's (docs/worker-slots.md, 「The base checkout's main
//! brought up to the remote's」): the close moves the remote's main alone, and the base is what other
//! repositories build against and what a new support worktree is detached at. Only a fast-forward moves it,
//! and only where nothing holds the base.

use std::path::Path;
use std::sync::Arc;

use super::checkouts::{git_at, main_branch, set_up};
use super::{Ground, locks};
use crate::queue::holds::is_free;
use crate::repositories::{self, Repository};

/// What a pass knows of each remote's main: fetched first, or as git on this machine last recorded it, which a
/// close run here leaves current since its push records the remote's main.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Remote {
  Fetched,
  AsKnown,
}

/// What one pass did, by repository: the bases whose main it moved, and those git refused to move.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Pass {
  pub moved: Vec<String>,
  pub refused: Vec<String>,
}

// What became of one base.
enum Outcome {
  Moved,
  Left,
}

/// One pass over every repository whose base stands on this machine, each base passed over that something
/// holds, and each refusal said on the error stream.
pub async fn pass(ground: &Ground, remote: Remote) -> Result<Pass, String> {
  let mut pass = Pass::default();

  for repository in repositories::every(&ground.db).await.map_err(|fault| fault.to_string())? {
    if repository.umbrella || repository.node.as_deref().is_some_and(|node| !node.is_empty()) {
      continue;
    }

    match bring_up(ground, &repository, remote).await {
      Ok(Outcome::Moved) => pass.moved.push(repository.name),
      Ok(Outcome::Left) => {}
      Err(why) => {
        eprintln!("{}: its main was not brought up to the remote's: {why}", repository.name);
        pass.refused.push(repository.name);
      }
    }
  }

  for name in &pass.moved {
    println!("{name}: its main brought up to the remote's");
  }

  Ok(pass)
}

/// A pass from what git already knows, started on its own once a job's turn has ended, so a close that turn ran
/// reaches the base at once. Only on agency's own data root: a copy moves no checkout.
pub fn after_turn(ground: &Arc<Ground>) {
  if !ground.roots.own_data_root() {
    return;
  }

  let ground = ground.clone();

  tokio::spawn(async move {
    if let Err(why) = pass(&ground, Remote::AsKnown).await {
      eprintln!("agency: the bases' mains were not brought up: {why}");
    }
  });
}

// One base brought up where nothing holds it: no hold of the checkout, a job at work in it among them, and no
// lock of its own or its agent's held. The locks stay held while git moves it.
async fn bring_up(ground: &Ground, repository: &Repository, remote: Remote) -> Result<Outcome, String> {
  if !is_free(&ground.db, &repository.name).await.map_err(|refusal| refusal.detail)? {
    return Ok(Outcome::Left);
  }

  let data = &ground.roots.data;
  let cwd = crate::queue::paths::resolve(&repository.path).to_string_lossy().into_owned();

  // The fetch takes no lock, so a remote slow to answer holds up no launch on the base.
  if remote == Remote::Fetched {
    let at = cwd.clone();

    tokio::task::spawn_blocking(move || fetch(&at)).await.map_err(|why| why.to_string())??;
  }

  let Some(_checkout) = locks::try_hold(&locks::lock_path(data, &cwd)) else {
    return Ok(Outcome::Left);
  };
  let Some(_agent) = locks::try_hold(&locks::agent_lock_path(data, &repository.name)) else {
    return Ok(Outcome::Left);
  };
  let bun = ground
    .machine()
    .program("bun")
    .map_or_else(|| "bun".to_owned(), |path| path.to_string_lossy().into_owned());

  tokio::task::spawn_blocking(move || fast_forward(&cwd, &bun))
    .await
    .map_err(|why| why.to_string())?
}

// The repository's main and the remote it follows, else `origin`, where the base is a checkout and that
// remote is one it has.
fn main_and_remote(base: &str) -> Option<(String, String)> {
  if !Path::new(base).join(".git").exists() {
    return None;
  }

  let main = main_branch(base);
  let follows = git_at(base, &["config", "--get", &format!("branch.{main}.remote")])
    .ok()
    .filter(|name| !name.is_empty() && name != ".")
    .unwrap_or_else(|| "origin".to_owned());

  git_at(base, &["remote", "get-url", &follows]).ok().map(|_| (main, follows))
}

// The remote's main fetched into what git here knows of it.
fn fetch(base: &str) -> Result<(), String> {
  let Some((main, follows)) = main_and_remote(base) else { return Ok(()) };

  git_at(base, &["fetch", "--quiet", &follows, &main]).map(drop)
}

// The base's main fast-forwarded to the remote's where it stands under it: in place where the base has it
// checked out, the branch alone where nothing does, and nothing where another worktree has it.
fn fast_forward(base: &str, bun: &str) -> Result<Outcome, String> {
  let Some((main, follows)) = main_and_remote(base) else { return Ok(Outcome::Left) };
  let local = format!("refs/heads/{main}");
  let (Ok(ours), Ok(theirs)) = (
    git_at(base, &["rev-parse", "--verify", "--quiet", &local]),
    git_at(base, &["rev-parse", "--verify", "--quiet", &format!("refs/remotes/{follows}/{main}")]),
  ) else {
    return Ok(Outcome::Left);
  };

  if ours == theirs || git_at(base, &["merge-base", "--is-ancestor", &ours, &theirs]).is_err() {
    return Ok(Outcome::Left);
  }

  let standing = git_at(base, &["branch", "--list", &main, "--format=%(worktreepath)"])?;

  if standing.is_empty() {
    git_at(base, &["update-ref", "-m", "brought up to the remote's main", &local, &theirs, &ours])?;

    return Ok(Outcome::Moved);
  }

  if !same_place(&standing, base) {
    return Ok(Outcome::Left);
  }

  git_at(base, &["merge", "--ff-only", "--quiet", &theirs])?;

  if let Err(why) = set_up(Path::new(base), bun, false) {
    eprintln!("{base}: its main moved, and its setup failed: {why}");
  }

  Ok(Outcome::Moved)
}

// Whether two paths name one directory, however each is spelled.
fn same_place(one: &str, other: &str) -> bool {
  match (std::fs::canonicalize(one), std::fs::canonicalize(other)) {
    (Ok(one), Ok(other)) => one == other,
    _ => one == other,
  }
}
