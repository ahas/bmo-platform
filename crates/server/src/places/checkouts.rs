//! A support worker's worktree made at its launch (docs/worker-slots.md, 「Names and places」), and a
//! renamed worker's checkout moved to the directory its rename names (「A worker of its own in a
//! worktree」), its row pointed there only once it stands there. A job of the worker at work holds the
//! checkout's lock and puts the move off to the next pass.

use std::path::Path;
use std::process::Stdio;

use policy::agent_renames::AGENT_RENAMES;
use store::run;

use super::{AgentRow, Ground, locks};

/// The command a repository sets a fresh worktree up with: the script of this name in its own
/// `package.json`. The repository knows what it needs installed and linked; this side only runs it.
pub const WORKTREE_SETUP_SCRIPT: &str = "worktree:setup";

/// A support worker's worktree made where it is missing, before its program is stood: detached at its
/// base checkout's main branch, the submodules first and the repository's own setup after them. A base
/// worker, a worker of its own and one on a node are left as they are, and a worktree already standing
/// is left as it stands.
pub async fn ensure_support_checkout(ground: &Ground, worker: &str) -> Result<(), String> {
  let Some(row) = AgentRow::named(&ground.db, worker).await.map_err(|refused| refused.detail)? else {
    return Ok(());
  };
  let Some(base) = row.support_of.as_deref().map(str::trim).filter(|base| !base.is_empty()) else {
    return Ok(());
  };
  let Some(cwd) = row.cwd.as_deref().filter(|cwd| !cwd.is_empty()) else { return Ok(()) };

  if row.node.is_some() || Path::new(cwd).join(".git").exists() {
    return Ok(());
  }

  let Some(base_row) = AgentRow::named(&ground.db, base).await.map_err(|refused| refused.detail)? else {
    return Ok(());
  };
  let Some(base_cwd) = base_row.cwd.as_deref().filter(|cwd| !cwd.is_empty()) else {
    return Ok(());
  };

  // Two launches never make one worktree at once; the one that waits, however long the other's making
  // takes, finds it standing when it looks.
  let lock = locks::lock_path(&ground.roots.data, cwd);
  let _held = locks::hold(lock).await.map_err(|why| format!("{worker}: the lock of {cwd}: {why}"))?;

  if Path::new(cwd).join(".git").exists() {
    return Ok(());
  }

  let bun = ground
    .machine()
    .program("bun")
    .map_or_else(|| "bun".to_owned(), |path| path.to_string_lossy().into_owned());
  let (base_cwd, cwd) = (base_cwd.to_owned(), cwd.to_owned());

  tokio::task::spawn_blocking(move || make_worktree(&base_cwd, &cwd, &bun))
    .await
    .map_err(|why| format!("{worker}: {why}"))?
}

// The worktree made and set up, or why it could not be: a directory holding anything that is not a
// checkout is somebody's, and a worktree whose setup failed is taken away so the next launch makes it
// afresh rather than working in a half-installed one.
fn make_worktree(base: &str, cwd: &str, bun: &str) -> Result<(), String> {
  let at = Path::new(cwd);

  if at.exists()
    && (!at.is_dir() || std::fs::read_dir(at).map_or(true, |mut entries| entries.next().is_some()))
  {
    return Err(format!(
      "{cwd} holds something that is not a worktree, so it cannot be a support worker's working path."
    ));
  }

  git_at(base, &["worktree", "prune"])?;

  let main = main_branch(base);

  git_at(base, &["worktree", "add", "--detach", cwd, &main])
    .map_err(|why| format!("the worktree at {cwd} could not be made from {main}: {why}"))?;

  if let Err(why) = set_up(at, bun, true) {
    let _ = git_at(base, &["worktree", "remove", "--force", cwd]);

    return Err(why);
  }

  Ok(())
}

// The repository's own setup of a checkout: its submodules brought to the commit's pins first, what is open
// inside them thrown away where `discard` says the tree is no one's yet, then the script its manifest names
// where it names one.
pub(super) fn set_up(at: &Path, bun: &str, discard: bool) -> Result<(), String> {
  bring_submodules(at, discard)?;

  if !setup_script(at) {
    return Ok(());
  }

  // The setup runs until its script exits, however long a cold install takes.
  let done = std::process::Command::new(bun)
    .args(["run", WORKTREE_SETUP_SCRIPT])
    .current_dir(at)
    .stdin(Stdio::null())
    .output()
    .map_err(|why| format!("{}: {why}", at.display()))?;
  let code = done.status.code();

  if code == Some(0) {
    return Ok(());
  }

  let whole = format!("{}{}", String::from_utf8_lossy(&done.stdout), String::from_utf8_lossy(&done.stderr));
  let lines: Vec<&str> = whole.trim().lines().collect();
  let said = lines[lines.len().saturating_sub(8)..].join("\n");

  Err(format!(
    "{WORKTREE_SETUP_SCRIPT} failed in {} ({}):\n{said}",
    at.display(),
    code.map_or_else(|| "ended by a signal".to_owned(), |code| code.to_string())
  ))
}

// The repository's main branch, which a support worker's worktree is detached at: the branch the base
// checkout stands on where that is not a job's; on a job's branch, the one it was put on from where that
// was written down and still exists; and otherwise the one the remote's head names, then `main`, then
// `master`.
pub(super) fn main_branch(base: &str) -> String {
  if let Ok(standing) = git_at(base, &["symbolic-ref", "--quiet", "--short", "HEAD"])
    && !standing.is_empty()
  {
    if !standing.starts_with("work/") {
      return standing;
    }

    let key = format!("branch.{standing}.bmo-main");

    if let Ok(from) = git_at(base, &["config", "--get", &key])
      && !from.is_empty()
      && !from.starts_with("work/")
      && branch_exists(base, &from)
    {
      return from;
    }
  }

  if let Ok(head) = git_at(base, &["symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"])
    && let Some((_, branch)) = head.split_once('/')
  {
    return branch.to_owned();
  }

  ["main", "master"]
    .into_iter()
    .find(|name| branch_exists(base, name))
    .unwrap_or("main")
    .to_owned()
}

pub(super) fn branch_exists(base: &str, branch: &str) -> bool {
  git_at(base, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_ok()
}

// Whether the repository names a setup script for its fresh worktrees.
fn setup_script(at: &Path) -> bool {
  std::fs::read_to_string(at.join("package.json"))
    .ok()
    .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    .is_some_and(|manifest| manifest["scripts"][WORKTREE_SETUP_SCRIPT].is_string())
}

// Git in a directory, asked nothing on its terminal and handed no editor: what it printed, or why it
// failed.
pub(crate) fn git_at(cwd: &str, args: &[&str]) -> Result<String, String> {
  let done = std::process::Command::new("git")
    .arg("-C")
    .arg(cwd)
    .args(args)
    .env("GIT_EDITOR", "true")
    .env("GIT_TERMINAL_PROMPT", "0")
    .stdin(Stdio::null())
    .output()
    .map_err(|why| format!("git: {why}"))?;

  if done.status.success() {
    return Ok(String::from_utf8_lossy(&done.stdout).trim().to_owned());
  }

  let said = String::from_utf8_lossy(&done.stderr).trim().to_owned();

  Err(if said.is_empty() { format!("git {} failed", args.join(" ")) } else { said })
}

/// The submodules the commit a checkout stands on pins, each cloned where it is missing and put on the
/// pinned commit (docs/worker-slots.md, 「Names and places」). A submodule that does not stand on its
/// pin has its pinned commit fetched first, from the checkout the place is a worktree of and
/// otherwise from the submodule's remote, and is moved again, so a pin standing on a branch outside
/// the submodule's default, or one its remote never received, comes up as it does in the base. A folder an older commit tracked where a submodule now
/// stands keeps only what git ignored in it, installs and builds, which would refuse the clone, so it
/// is emptied first. `discard` throws away what is open inside a submodule, for a tree no one works in;
/// without it git refuses to move a submodule over open work, and that refusal is the answer.
pub fn bring_submodules(cwd: &Path, discard: bool) -> Result<(), String> {
  let (declared, listed) =
    git_here(cwd, &["config", "--file", ".gitmodules", "--get-regexp", r"^submodule\..*\.path$"]);

  // No `.gitmodules`, or one declaring nothing: git answers no, and there is nothing to bring.
  if !declared {
    return Ok(());
  }

  if !git_here(cwd, &["submodule", "sync", "--quiet", "--recursive"]).0 {
    return Err("could not read the submodules' addresses".to_owned());
  }

  // The checkout this place is a worktree of, whose own submodules stand: where a pin's commit waits
  // when the submodule's remote carries no copy of it.
  let base = owning_checkout(cwd);

  for path in listed.lines().filter_map(|line| line.split_once(' ').map(|(_, path)| path)) {
    let Some(pin) = pin_at(cwd, path) else {
      continue;
    };

    let at = cwd.join(path);

    // A folder an older commit tracked where a submodule now stands would refuse the clone, so it is
    // emptied first.
    if !at.join(".git").exists()
      && let Ok(entries) = std::fs::read_dir(&at)
    {
      for entry in entries.flatten() {
        let gone = if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
          std::fs::remove_dir_all(entry.path())
        } else {
          std::fs::remove_file(entry.path())
        };

        if let Err(why) = gone {
          return Err(format!("{}: {why}", entry.path().display()));
        }
      }
    }

    // Cloned where missing and put on the pin, git's own fetch of a pin its remote carries included.
    let _ = bring_onto(cwd, path, discard);

    if stands_on(&at, &pin) {
      continue;
    }

    // A fresh clone holds the branches standing at its remote's heads, and a pinned commit may wait on
    // none of them, or on no remote at all: the checkout this place is a worktree of holds what its own
    // submodules stood on, so its objects are taken from there first, and otherwise from the remote.
    let base_copy = base.as_ref().map(|main| Path::new(main).join(path).to_string_lossy().into_owned());

    if !base_copy.is_some_and(|from| fetch(&at, &from, &pin)) {
      fetch(&at, "origin", &pin);
    }

    // The move tried again with the pin's objects in hand, its own submodules after it.
    if !bring_onto(cwd, path, discard) || !stands_on(&at, &pin) {
      return Err(format!("could not bring the submodule {} of {} to its pin", path, cwd.display()));
    }
  }

  Ok(())
}

// The commit a checkout's index pins at a path, where a commit is pinned there.
fn pin_at(cwd: &Path, path: &str) -> Option<String> {
  let (_, staged) = git_here(cwd, &["ls-files", "--stage", "--", path]);

  staged
    .lines()
    .find(|line| line.starts_with("160000 "))
    .and_then(|line| line.split_whitespace().nth(1))
    .map(str::to_owned)
}

// Whether a submodule's working tree stands on a commit.
fn stands_on(at: &Path, commit: &str) -> bool {
  git_here(at, &["rev-parse", "--verify", "--quiet", "HEAD"]).1 == commit
}

// A submodule asked for a commit from one source: another checkout's copy of it, by path, or the
// remote's name. Whether the source answers.
fn fetch(at: &Path, source: &str, commit: &str) -> bool {
  git_here(at, &["fetch", "--quiet", source, commit]).0
}

// One submodule brought onto the commit its parent records: initialized and cloned where missing, its
// own submodules after it, open work thrown away where `discard` says the tree is no one's.
fn bring_onto(cwd: &Path, path: &str, discard: bool) -> bool {
  let mut args = vec!["submodule", "update", "--quiet", "--init", "--recursive"];

  if discard {
    args.push("--force");
  }

  args.extend(["--", path]);

  git_here(cwd, &args).0
}

/// The checkout a linked worktree belongs to; nothing for a directory that is no linked worktree. The
/// setup asks it in its own blocking pass; `main_checkout_of` answers the same for the async roads.
fn owning_checkout(cwd: &Path) -> Option<String> {
  let (own_read, own) = git_here(cwd, &["rev-parse", "--path-format=absolute", "--git-dir"]);
  let (common_read, common) = git_here(cwd, &["rev-parse", "--path-format=absolute", "--git-common-dir"]);

  if !own_read || !common_read || own == common {
    return None;
  }

  common.strip_suffix("/.git").map(str::to_owned)
}

// Git in a place, asked nothing on its terminal and handed no editor: whether it worked, and what it said.
fn git_here(cwd: &Path, args: &[&str]) -> (bool, String) {
  let ran = std::process::Command::new("git")
    .args(args)
    .current_dir(cwd)
    .env("GIT_EDITOR", "true")
    .env("GIT_TERMINAL_PROMPT", "0")
    .stdin(Stdio::null())
    .output();

  match ran {
    Ok(done) => (done.status.success(), String::from_utf8_lossy(&done.stdout).trim().to_owned()),
    Err(_) => (false, String::new()),
  }
}

async fn git(cwd: &Path, args: &[&str]) -> Option<String> {
  let output = tokio::process::Command::new("git")
    .arg("-C")
    .arg(cwd)
    .args(args)
    .env("GIT_TERMINAL_PROMPT", "0")
    .stdin(std::process::Stdio::null())
    .output()
    .await
    .ok()?;

  output
    .status
    .success()
    .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The checkout a linked worktree belongs to; none for a directory that is no linked worktree.
async fn main_checkout_of(cwd: &Path) -> Option<String> {
  let own = git(cwd, &["rev-parse", "--path-format=absolute", "--git-dir"]).await?;
  let common = git(cwd, &["rev-parse", "--path-format=absolute", "--git-common-dir"]).await?;

  if Path::new(&own) == Path::new(&common) {
    return None;
  }

  common.strip_suffix("/.git").map(str::to_owned)
}

/// The worktree moved under its checkout's lock, taken without waiting; whether it now stands there.
async fn move_worktree(ground: &Ground, from: &str, to: &str) -> bool {
  let Some(main) = main_checkout_of(Path::new(from)).await else { return false };

  if Path::new(to).exists() {
    return false;
  }

  let Some(_held) = locks::try_hold(&locks::lock_path(&ground.roots.data, from)) else {
    return false;
  };

  git(Path::new(&main), &["worktree", "move", from, to]).await.is_some() && Path::new(to).exists()
}

/// One worker's checkout moved from the directory its row still names to the one its rename names, and
/// the row pointed there once it stands there; whether it moved.
pub async fn move_checkout(ground: &Ground, worker: &str, from: &str, to: &str) -> bool {
  match AgentRow::named(&ground.db, worker).await {
    Ok(Some(row)) if row.node.is_none() && row.cwd.as_deref() == Some(from) => {}
    Ok(_) => return false,
    Err(refusal) => {
      eprintln!("{worker}: {}", refusal.detail);

      return false;
    }
  }

  if !move_worktree(ground, from, to).await {
    return false;
  }

  match run(
    &ground.db,
    r#"UPDATE "repositories" SET "path" = $1 WHERE "name" = $2"#,
    vec![to.into(), worker.into()],
  )
  .await
  {
    Ok(_) => {
      println!("{worker}: checkout moved to {to}");

      true
    }
    Err(fault) => {
      eprintln!("{worker}: the moved checkout is not written: {fault}");

      false
    }
  }
}

/// Every renamed worker whose row still names its old directory, moved.
pub async fn move_renamed(ground: &Ground) {
  for rename in &AGENT_RENAMES.renames {
    if let Some(cwd) = &rename.cwd {
      move_checkout(ground, &rename.to, &cwd.from, &cwd.to).await;
    }
  }
}
