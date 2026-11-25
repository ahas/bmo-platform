//! The support checkouts of the repositories set to `auto`, kept and let go (docs/worker-slots.md, 「An idle
//! support checkout」): a worktree standing at a slot's path with no record recorded, and one idle for the
//! policy's hours, that nothing holds and with nothing open in it, taken away with its record.

use std::path::{Path, PathBuf};

use policy::workers::WORKERS;
use store::{all, one, run};

use super::{Ground, locks};
use crate::queue::holds::is_free;
use crate::queue::workers::{Worker, support_worker};
use crate::repositories::{self, Repository};

/// What one pass did: the support checkouts it recorded and the ones it took away, by name.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Pass {
  pub recorded: Vec<String>,
  pub removed: Vec<String>,
}

// The slots a worktree stands at beside the base, read off the directory the base stands in: each entry named
// after the base's folder, `-w` and a number from the first slot, holding a checkout.
fn standing_slots(base: &Path) -> Vec<i64> {
  let (Some(parent), Some(folder)) = (base.parent(), base.file_name().and_then(|name| name.to_str()))
  else {
    return vec![];
  };
  let prefix = format!("{folder}-w");
  let Ok(entries) = std::fs::read_dir(parent) else { return vec![] };
  let mut slots: Vec<i64> = entries
    .flatten()
    .filter(|entry| entry.path().join(".git").exists())
    .filter_map(|entry| entry.file_name().to_str()?.strip_prefix(&prefix)?.parse::<i64>().ok())
    .filter(|slot| *slot >= WORKERS.first_support_slot)
    .collect();

  slots.sort_unstable();
  slots
}

// The moment a support checkout was last used: the latest change of an assignment of it, else when it was
// recorded.
async fn last_used(ground: &Ground, name: &str, made_at: i64) -> Result<i64, String> {
  let row = one(
    &ground.db,
    r#"SELECT MAX("changed_at") AS "at" FROM "assignments" WHERE "worker_name" = $1"#,
    vec![name.into()],
  )
  .await
  .map_err(|fault| fault.to_string())?;
  let changed = row.and_then(|row| row.try_get::<Option<i64>>("", "at").ok().flatten());

  Ok(changed.map_or(made_at, |at| at.max(made_at)))
}

// Git in a directory, asked nothing on its terminal: what it printed, or why it failed.
async fn git(cwd: &Path, args: &[&str]) -> Result<String, String> {
  let done = tokio::process::Command::new("git")
    .arg("-C")
    .arg(cwd)
    .args(args)
    .env("GIT_TERMINAL_PROMPT", "0")
    .stdin(std::process::Stdio::null())
    .output()
    .await
    .map_err(|why| format!("git: {why}"))?;

  if done.status.success() {
    return Ok(String::from_utf8_lossy(&done.stdout).trim().to_owned());
  }

  Err(String::from_utf8_lossy(&done.stderr).trim().to_owned())
}

// One idle support checkout taken away where nothing holds it and nothing is open in it; whether it went.
async fn take_away(ground: &Ground, base: &str, checkout: &Worker) -> Result<bool, String> {
  if !is_free(&ground.db, &checkout.name).await.map_err(|refusal| refusal.detail)? {
    return Ok(false);
  }

  let data = &ground.roots.data;
  let Some(_job) = locks::try_hold(&locks::lock_path(data, &checkout.cwd)) else {
    return Ok(false);
  };
  let Some(_agent) = locks::try_hold(&locks::agent_lock_path(data, &checkout.name)) else {
    return Ok(false);
  };
  let cwd = PathBuf::from(&checkout.cwd);

  if cwd.join(".git").exists() {
    let open = git(&cwd, &["status", "--porcelain"]).await?;

    if !open.is_empty() {
      eprintln!("{}: idle, and kept since something is open in {}", checkout.name, checkout.cwd);

      return Ok(false);
    }

    git(Path::new(base), &["worktree", "remove", "--force", &checkout.cwd]).await?;
  }

  run(
    &ground.db,
    r#"DELETE FROM "support_checkouts" WHERE "repository" = $1 AND "slot" = $2"#,
    vec![checkout.repository().into(), checkout.support_slot.unwrap_or_default().into()],
  )
  .await
  .map_err(|fault| fault.to_string())?;

  Ok(true)
}

// One repository's pass: its standing worktrees recorded, then its idle ones taken away.
async fn settle(ground: &Ground, repository: &Repository, now: i64, pass: &mut Pass) -> Result<(), String> {
  let base = Worker::base(repository);
  let base_path = crate::queue::paths::resolve(&repository.path);
  let fault = |fault: store::Fault| fault.to_string();

  for slot in standing_slots(&base_path) {
    let wrote = run(
      &ground.db,
      r#"INSERT INTO "support_checkouts" ("repository", "slot", "made_at") VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING"#,
      vec![repository.name.clone().into(), slot.into(), now.into()],
    )
    .await
    .map_err(fault)?;

    if wrote > 0 {
      pass.recorded.push(support_worker(&base, slot).name);
    }
  }

  let idle_ms = WORKERS.support_idle_hours * 3_600_000;
  let rows = all(
    &ground.db,
    r#"SELECT "slot", "made_at" FROM "support_checkouts" WHERE "repository" = $1 ORDER BY "slot""#,
    vec![repository.name.clone().into()],
  )
  .await
  .map_err(fault)?;

  for row in rows {
    let slot: i64 = row.try_get("", "slot").map_err(|why| why.to_string())?;
    let made_at: i64 = row.try_get("", "made_at").map_err(|why| why.to_string())?;
    let checkout = support_worker(&base, slot);

    if now - last_used(ground, &checkout.name, made_at).await? < idle_ms {
      continue;
    }

    if take_away(ground, &base_path.to_string_lossy(), &checkout).await? {
      pass.removed.push(checkout.name);
    }
  }

  Ok(())
}

/// One pass over every repository set to `auto` whose base stands on this machine, at `now`.
pub async fn pass(ground: &Ground, now: i64) -> Result<Pass, String> {
  let mut pass = Pass::default();

  for repository in repositories::every(&ground.db).await.map_err(|fault| fault.to_string())? {
    if !repository.grows() {
      continue;
    }

    if let Err(why) = settle(ground, &repository, now, &mut pass).await {
      eprintln!("{}: its support checkouts were not settled: {why}", repository.name);
    }
  }

  for name in &pass.removed {
    println!("{name}: an idle support checkout taken away");
  }

  Ok(pass)
}
