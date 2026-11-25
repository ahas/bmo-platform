//! The checkouts jobs work in, as the repositories hold them (docs/worker-slots.md): each repository's base
//! checkout, the support worktrees its count asks for beside it or, for a repository set to `auto`, the ones it
//! has grown, and the checkouts it keeps on the work nodes. A checkout is a place, never an agent: it is read off
//! its repository's row and the support checkouts it has recorded.

use std::path::Path;

use policy::workers::WORKERS;
use sea_orm::{DatabaseConnection, QueryResult};
use serde::Serialize;
use store::all;

use crate::Refusal;
use crate::events::Event;
use crate::repositories::{self, Repository};

/// A repository removed (docs/api.md, 「Events」): a consumer lets go what it lent its checkouts, such as a
/// checkout lease.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRemoved {
  pub worker: String,
}

impl Event for WorkerRemoved {
  const NAME: &'static str = "repository.deleted";
}

/// The checkout of the code tree agency itself runs from, which stands whether or not a repository names it.
pub const BUILTIN_WORKER: &str = "worker";

/// One checkout as the queue reads it: its name, the label a screen shows, its path, the repository it is a
/// support worktree or a node checkout of, its slot among the support worktrees and the node it stands on.
#[derive(Clone, Debug)]
pub struct Worker {
  pub name: String,
  pub label: String,
  pub cwd: String,
  pub builtin: bool,
  pub support_of: Option<String>,
  pub support_slot: Option<i64>,
  /// The work node it stands on, none for this machine.
  pub node: Option<String>,
  /// Whether it is an umbrella's checkout, whose own tree its jobs never write (docs/worker-slots.md, 「An
  /// umbrella repository」).
  pub umbrella: bool,
}

impl Worker {
  /// Whether it is a support worktree or a node checkout rather than a base.
  pub fn is_support(&self) -> bool {
    self.support_of.as_deref().is_some_and(|base| !base.is_empty())
  }

  /// The repository it is a checkout of: its base's for a support worktree or a node checkout, its own name for
  /// a base.
  pub fn repository(&self) -> &str {
    self.support_of.as_deref().filter(|base| !base.is_empty()).unwrap_or(&self.name)
  }

  /// Whether it answers an assignee, the name an assignment or an item gives (docs/api.md, 「assignments and
  /// jobs」).
  pub fn answers_to(&self, assignee: &str) -> bool {
    answers_to(&self.name, self.repository(), assignee)
  }

  /// Whether it is a repository's checkout on a node rather than a numbered worktree here (docs/work-nodes.md,
  /// 「A checkout on a node」).
  pub fn is_node_checkout(&self) -> bool {
    self.node.is_some() && self.support_slot.is_none()
  }

  /// A repository's base checkout.
  pub fn base(repository: &Repository) -> Self {
    Self {
      name: repository.name.clone(),
      label: repository.label.clone().unwrap_or_else(|| repository.name.clone()),
      cwd: repository.path.clone(),
      builtin: false,
      support_of: None,
      support_slot: None,
      node: repository.node.clone().filter(|node| !node.is_empty()),
      umbrella: repository.umbrella,
    }
  }
}

/// Whether a checkout, by its name and its repository's, answers an assignee: the checkout it names, or, where it
/// names a repository, any checkout of that repository — its base, a support worktree or a node checkout.
pub fn answers_to(checkout: &str, repository: &str, assignee: &str) -> bool {
  checkout == assignee || repository == assignee
}

/// A support worktree's name.
pub fn support_name(base: &str, slot: i64) -> String {
  format!("{base}-w{slot}")
}

/// A support worktree's directory, beside the base's checkout.
fn support_path(base_cwd: &str, slot: i64) -> String {
  format!("{}-w{slot}", super::paths::resolve(base_cwd).display())
}

/// What a support worktree is called on a screen.
fn support_label(base_label: &str, slot: i64) -> String {
  format!("{base_label}#{slot}")
}

/// What a repository's checkout on a node is called on a screen.
pub fn node_checkout_label(base_label: &str, node_label: &str) -> String {
  format!("{base_label}@{node_label}")
}

/// A repository's checkout on a node's name.
pub fn node_checkout_name(base: &str, node: &str) -> String {
  format!("{base}@{node}")
}

/// The repository a checkout's name says it belongs to: the repository's own name for its base, the name
/// before `-w<n>` for a support worktree and before `@<node>` for a node checkout, whichever names a
/// repository `held` holds; none for a checkout no repository holds. The name alone settles it, so a
/// checkout the repository no longer stands, such as a support worktree past its present count, still
/// belongs to it.
pub fn repository_named_by(checkout: &str, held: impl Fn(&str) -> bool) -> Option<&str> {
  let support_base = checkout
    .rsplit_once("-w")
    .filter(|(_, slot)| slot.parse::<i64>().is_ok())
    .map(|(base, _)| base);
  let node_base = checkout.split_once('@').map(|(base, _)| base);

  [Some(checkout), support_base, node_base].into_iter().flatten().find(|name| held(name))
}

/// What a repository's support worktree is when it is the one at `slot`.
pub fn support_worker(base: &Worker, slot: i64) -> Worker {
  Worker {
    name: support_name(&base.name, slot),
    label: support_label(&base.label, slot),
    cwd: support_path(&base.cwd, slot),
    builtin: false,
    support_of: Some(base.name.clone()),
    support_slot: Some(slot),
    node: base.node.clone(),
    umbrella: false,
  }
}

/// The support checkouts every repository set to `auto` has recorded, as their rows read.
const RECORDED: &str =
  r#"SELECT "repository", "slot" FROM "support_checkouts" ORDER BY "repository", "slot""#;

// The slots a repository's support worktrees stand at: within its count, or the ones it recorded where it is
// set to `auto`.
fn support_slots(repository: &Repository, recorded: &[QueryResult]) -> Vec<i64> {
  match repository.support_count.stored() {
    Some(count) => (WORKERS.first_support_slot..WORKERS.first_support_slot + count).collect(),
    None => recorded
      .iter()
      .filter(|row| row.try_get::<String>("", "repository").is_ok_and(|of| of == repository.name))
      .filter_map(|row| row.try_get::<i64>("", "slot").ok())
      .collect(),
  }
}

// A repository's checkouts after its base: its support worktrees by slot, then its node checkouts.
fn supports(
  repository: &Repository,
  base: &Worker,
  recorded: &[QueryResult],
  nodes: &[QueryResult],
) -> Vec<Worker> {
  let mut held: Vec<Worker> = support_slots(repository, recorded)
    .into_iter()
    .map(|slot| support_worker(base, slot))
    .collect();

  for row in nodes {
    let (Ok(repository), Ok(node), Ok(path)) = (
      row.try_get::<String>("", "repository"),
      row.try_get::<String>("", "node"),
      row.try_get::<String>("", "path"),
    ) else {
      continue;
    };

    if repository == base.name {
      held.push(Worker {
        name: node_checkout_name(&base.name, &node),
        label: node_checkout_label(&base.label, &node),
        cwd: path,
        builtin: false,
        support_of: Some(base.name.clone()),
        support_slot: None,
        node: Some(node),
        umbrella: false,
      });
    }
  }

  held
}

/// Every checkout, each repository's base followed by its support worktrees and node checkouts; the code tree
/// agency runs from stands first, as the built-in checkout, where no repository's base is there.
pub async fn workers(db: &DatabaseConnection, source: &Path) -> Result<Vec<Worker>, Refusal> {
  let nodes =
    all(db, r#"SELECT * FROM "repository_node_checkouts" ORDER BY "repository", "node""#, vec![]).await?;
  let recorded = all(db, RECORDED, vec![]).await?;
  let mut workers = vec![];

  for repository in repositories::every(db).await? {
    let base = Worker::base(&repository);
    let beside = supports(&repository, &base, &recorded, &nodes);

    workers.push(base);
    workers.extend(beside);
  }

  if !workers.iter().any(|worker| worker.name == BUILTIN_WORKER) {
    workers.insert(
      0,
      Worker {
        name: BUILTIN_WORKER.into(),
        label: BUILTIN_WORKER.into(),
        cwd: source.to_string_lossy().into_owned(),
        builtin: true,
        support_of: None,
        support_slot: None,
        node: None,
        umbrella: false,
      },
    );
  }

  Ok(workers)
}

/// Whether a checkout of that name stands.
pub async fn is_worker(db: &DatabaseConnection, name: &str) -> Result<bool, Refusal> {
  Ok(worker_row(db, name, Path::new("")).await?.is_some())
}

/// One checkout as it stands, the built-in at the source tree.
pub async fn worker_row(
  db: &DatabaseConnection,
  name: &str,
  source: &Path,
) -> Result<Option<Worker>, Refusal> {
  Ok(workers(db, source).await?.into_iter().find(|worker| worker.name == name))
}

/// The checkout standing at a path, by name, where a repository's checkout stands there: what a run working at
/// that path is handed the repository's own for (docs/repositories.md, 「What a run in a repository is
/// handed」).
pub async fn checkout_at(db: &DatabaseConnection, path: &str) -> Result<Option<String>, Refusal> {
  let at = super::paths::resolve(path);

  Ok(
    workers(db, Path::new(""))
      .await?
      .into_iter()
      .find(|worker| !worker.builtin && super::paths::resolve(&worker.cwd) == at)
      .map(|worker| worker.name),
  )
}

/// The repository a checkout is of: the base's own name, or the repository a support worktree or a node
/// checkout stands beside.
pub async fn repository_of(db: &DatabaseConnection, checkout: &str) -> Result<Option<Repository>, Refusal> {
  let Some(worker) = worker_row(db, checkout, Path::new("")).await? else {
    return Ok(None);
  };
  let base = worker.support_of.unwrap_or(worker.name);

  Ok(repositories::named(db, &base).await?)
}

/// The repository every checkout is of, by the checkout's name: a base's own name, or the repository a support
/// worktree or a node checkout stands beside.
pub async fn repositories_by_checkout(
  db: &DatabaseConnection,
) -> Result<std::collections::HashMap<String, String>, Refusal> {
  Ok(
    workers(db, Path::new(""))
      .await?
      .into_iter()
      .filter(|worker| !worker.builtin)
      .map(|worker| {
        let repository = worker.support_of.clone().unwrap_or_else(|| worker.name.clone());

        (worker.name, repository)
      })
      .collect(),
  )
}

/// Every name a repository or a supervisor stands under, with every checkout's: the names a new one may not
/// take.
pub async fn taken_names(db: &DatabaseConnection) -> Result<Vec<String>, Refusal> {
  let mut names: Vec<String> =
    workers(db, Path::new("")).await?.into_iter().map(|worker| worker.name).collect();

  for row in all(db, r#"SELECT "name" FROM "supervisors""#, vec![]).await? {
    names.push(row.try_get("", "name")?);
  }

  names.sort();
  names.dedup();

  Ok(names)
}

/// The checkouts a repository's settings ask for now, by name: its support worktrees within its count and its
/// node checkouts. The support checkouts follow the row, so nothing is written to settle them.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Settled {
  pub added: Vec<String>,
  pub removed: Vec<String>,
}

/// Every checkout's name, read in whatever connection a caller holds, the built-in left out: each repository's
/// base, its support worktrees and its node checkouts.
pub async fn workers_in<C: sea_orm::ConnectionTrait>(db: &C) -> Result<Vec<String>, Refusal> {
  let nodes =
    all(db, r#"SELECT * FROM "repository_node_checkouts" ORDER BY "repository", "node""#, vec![]).await?;
  let rows = all(db, r#"SELECT * FROM "repositories" ORDER BY "name""#, vec![]).await?;
  let recorded = all(db, RECORDED, vec![]).await?;
  let mut names = vec![];

  for row in &rows {
    let repository = Repository::of(row)?;
    let base = Worker::base(&repository);

    names.push(base.name.clone());
    names.extend(supports(&repository, &base, &recorded, &nodes).into_iter().map(|worker| worker.name));
  }

  Ok(names)
}
