//! What finished items left in the workbenches swept away (docs/worker-bench.md, 「An item's scratch」): every
//! item's scratch folder and its own databases on the store's cluster, once no job of the item is open, with
//! every empty workbench no agent's row names. Which items have an open job is the job unit's, asked at the
//! open items point; where nobody answers, no item's scratch is touched.

use std::collections::BTreeSet;
use std::future::Future;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use policy::job_prompt::JOB_PROMPT;
use sea_orm::{ConnectionTrait, DatabaseConnection};
use serde::Deserialize;
use store::all;

use super::Ground;
use crate::Refusal;
use crate::hooks::HookPoint;

/// The items a job of which is open, as the job unit answers them.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct OpenItems {
  pub items: Vec<i64>,
}

/// The point the open items are asked at: the consumer that holds the jobs answers them, or none where it
/// cannot say, and no item's scratch is touched.
pub struct OpenItemsHook;

impl HookPoint for OpenItemsHook {
  type Ask = ();
  type Answer = Option<OpenItems>;
  const NAME: &'static str = "open-items";

  fn default_answer(_ask: ()) -> Option<OpenItems> {
    None
  }
}

crate::wired!(OpenItemsHook);

fn names(dir: &Path) -> Vec<String> {
  std::fs::read_dir(dir)
    .map(|entries| {
      entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
    })
    .unwrap_or_default()
}

// The item a scratch folder or a database name stands for: a whole number and nothing else.
fn item_of(name: &str) -> Option<i64> {
  (!name.is_empty() && name.bytes().all(|byte| byte.is_ascii_digit()))
    .then(|| name.parse().ok())
    .flatten()
}

/// The sweep's memory: the open items the last look acted on, so a look with nothing changed does nothing,
/// and a process starting acts once.
#[derive(Default)]
pub struct Sweep {
  acted_for: Mutex<Option<BTreeSet<i64>>>,
}

impl Sweep {
  /// One look: every empty workbench no agent's row names removed, and where the open items are answered and
  /// changed since the last look, every item's scratch folder and database whose item has no open job.
  /// What went is answered, and said on the process's output; what could not go is said on its errors.
  pub async fn sweep(
    &self,
    ground: &Ground,
    databases: &impl ItemDatabases,
  ) -> Result<Vec<String>, Refusal> {
    let benches = ground.roots.data.join("workbench");
    let mut removed = empty_benches(ground, &benches).await?;

    if let Some(open) = ground.hooks.ask::<OpenItemsHook>(()).await {
      let open: BTreeSet<i64> = open.items.into_iter().collect();
      let changed = {
        let mut acted = self.acted_for.lock().unwrap_or_else(PoisonError::into_inner);
        let changed = acted.as_ref() != Some(&open);

        *acted = Some(open.clone());
        changed
      };

      if changed {
        removed.extend(closed_scratches(&benches, &open));
        removed.extend(closed_databases(databases, &open).await?);
      }
    }

    if !removed.is_empty() {
      println!("agency: item scratch: removed what no open job needs: {}", removed.join(", "));
    }

    Ok(removed)
  }
}

// Every workbench no agent's row names that holds nothing; one that still holds anything stays, since what is
// inside is not agency's to judge.
async fn empty_benches(ground: &Ground, benches: &Path) -> Result<Vec<String>, Refusal> {
  let agents: BTreeSet<String> =
    crate::queue::workers::taken_names(&ground.db).await?.into_iter().collect();
  let mut removed = vec![];

  for bench in names(benches).into_iter().filter(|bench| !agents.contains(bench)) {
    let dir = benches.join(&bench);

    if dir.is_dir() && names(&dir).is_empty() && std::fs::remove_dir(&dir).is_ok() {
      removed.push(format!("{bench}/"));
    }
  }

  Ok(removed)
}

// Every item's scratch folder of every workbench whose item has no open job.
fn closed_scratches(benches: &Path, open: &BTreeSet<i64>) -> Vec<String> {
  let scratch = &JOB_PROMPT.workbench.scratch;
  let mut removed = vec![];

  for bench in names(benches) {
    let items = benches.join(&bench).join(scratch);

    for item in names(&items) {
      if !item_of(&item).is_some_and(|number| !open.contains(&number)) || !items.join(&item).is_dir() {
        continue;
      }

      match std::fs::remove_dir_all(items.join(&item)) {
        Ok(()) => removed.push(format!("{bench}/{scratch}/{item}")),
        Err(why) => eprintln!("agency: item scratch: {bench}/{scratch}/{item} could not be removed: {why}"),
      }
    }
  }

  removed
}

/// Where the items' own databases stand: listed by the name they begin with, and dropped one by one.
pub trait ItemDatabases: Sync {
  /// Every database whose name begins with the prefix.
  fn list<'a>(&'a self, prefix: &'a str) -> impl Future<Output = Result<Vec<String>, Refusal>> + Send + 'a;

  /// One database dropped.
  fn drop<'a>(&'a self, name: &'a str) -> impl Future<Output = Result<(), String>> + Send + 'a;
}

/// The store's own cluster, reached on the store's connection: an item's database stands beside it.
pub struct StoreCluster<'a>(pub &'a DatabaseConnection);

impl ItemDatabases for StoreCluster<'_> {
  async fn list<'a>(&'a self, prefix: &'a str) -> Result<Vec<String>, Refusal> {
    let pattern = format!("{}%", prefix.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
    let rows = all(
      self.0,
      r#"SELECT "datname"::text AS "name" FROM "pg_database" WHERE "datname" LIKE $1 ESCAPE '\'"#,
      vec![pattern.into()],
    )
    .await?;

    Ok(rows.iter().filter_map(|row| row.try_get::<String>("", "name").ok()).collect())
  }

  async fn drop<'a>(&'a self, name: &'a str) -> Result<(), String> {
    self
      .0
      .execute_unprepared(&format!(r#"DROP DATABASE IF EXISTS "{}""#, name.replace('"', "\"\"")))
      .await
      .map(|_| ())
      .map_err(|why| why.to_string())
  }
}

// Every item's own database whose item has no open job.
async fn closed_databases(
  databases: &impl ItemDatabases,
  open: &BTreeSet<i64>,
) -> Result<Vec<String>, Refusal> {
  let prefix = &JOB_PROMPT.workbench.item_database;
  let mut removed = vec![];

  for name in databases.list(prefix).await? {
    let item = name
      .strip_prefix(prefix.as_str())
      .and_then(|rest| rest.split_once('_'))
      .map(|(item, _)| item);

    if !item.and_then(item_of).is_some_and(|number| !open.contains(&number)) {
      continue;
    }

    match databases.drop(&name).await {
      Ok(()) => removed.push(name),
      Err(why) => eprintln!("agency: item scratch: the database {name} could not be dropped: {why}"),
    }
  }

  Ok(removed)
}
