//! Which jobs stand in the machine's turn (docs/role-agents.md, 「When an agent is busy」): testing, waiting to
//! test, deploying or waiting for their deployment. The testbed's lanes, a testing close's line, the job
//! service's items under a close and the deployment of each commit a close handed over are read every few
//! seconds, and each standing job's mark is kept on its row, where every reading of a busy agent leaves the seat
//! of a marked job out.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use bmo_testbed_client::{Page, Queue};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::Value;
use store::{Fault, all, run};

use crate::order_run::dispatch::JOB_OWNER;
use crate::queue::orderer::JobService;
use crate::watches::closes::{self, Close};

/// The condition a slot request answers where it carries a run at work: no job standing in the machine's turn
/// owns it, a job's request being owned by `JOB_OWNER` and the job's id. Laid into every reading of the requests
/// that tells a busy agent.
pub const AT_WORK: &str = r#"NOT EXISTS (SELECT 1 FROM "jobs" AS "in_turn"
       WHERE "in_turn"."machine_turn" AND "role_slot_requests"."owner" = 'job-' || "in_turn"."id")"#;

/// The command whose status prints the testbed's lanes and a repository's close line.
const TESTBED: &str = "bmo-testbed";

/// The testbed declaration at a checkout's root, and the records a deployment keeps under its deploy root (the
/// testbed's docs/deploy.md, 「The places」).
const DECLARATION: &str = "testbed.toml";
const DEPLOYED: &str = "deployed.json";
const PROGRESS: &str = "deploy-progress.json";

/// The outcome a deployment that put its commit live ends with.
const LIVE: &str = "live";

/// What `bmo-testbed status --json` prints, as far as the machine's turn reads it: the page of both lanes, and
/// the close line of the repository it ran in, null outside one. The close line's entries are the testbed's own
/// shape, which its published client does not carry; only their checkout is read.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Status {
  queues: Page<Queue>,
  close_line: Option<CloseLine>,
}

#[derive(Deserialize)]
struct CloseLine {
  holder: Option<CloseEntry>,
  waiting: Vec<CloseEntry>,
}

#[derive(Deserialize)]
struct CloseEntry {
  checkout: String,
}

/// One reading of the machine's turn: the checkouts that hold a lane or a close line's turn or wait in line for
/// one, and the items whose close has handed over a commit that is not live yet. A reading that failed leaves its
/// part empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reading {
  pub checkouts: BTreeSet<String>,
  pub awaiting_live: BTreeSet<i64>,
}

impl Reading {
  /// The checkouts `bmo-testbed status --json` names: every lane's holder and waiters, and its close line's.
  pub fn testing_in(printed: &str) -> Result<BTreeSet<String>, String> {
    let status: Status = serde_json::from_str(printed).map_err(|why| why.to_string())?;
    let lanes = status
      .queues
      .items
      .iter()
      .flat_map(|lane| lane.holder.iter().chain(lane.waiting.iter()))
      .map(|entry| entry.checkout.as_str());
    let close = status
      .close_line
      .iter()
      .flat_map(|line| line.holder.iter().chain(line.waiting.iter()))
      .map(|entry| entry.checkout.as_str());

    Ok(lanes.chain(close).map(|checkout| trimmed(checkout).to_owned()).collect())
  }

  /// Whether a job working in `path` on `item` stands in the machine's turn: the testbed names its checkout or a
  /// folder under it, or its item's close waits for a commit that is not live yet.
  pub fn holds(&self, path: Option<&str>, item: i64) -> bool {
    if self.awaiting_live.contains(&item) {
      return true;
    }

    let Some(path) = path.map(trimmed).filter(|path| !path.is_empty()) else {
      return false;
    };

    self.checkouts.iter().any(|checkout| {
      checkout == path || checkout.strip_prefix(path).is_some_and(|under| under.starts_with('/'))
    })
  }
}

fn trimmed(path: &str) -> &str {
  path.trim().trim_end_matches('/')
}

/// Whether a commit a close handed over still waits to go live, by its repository's deployment record, naming the
/// commit that runs, and its progress record, the run under way or the last one ended: it waits until the
/// record's commit is it or, as `held` answers for that record's commit, holds it; and no longer once a run of
/// that very commit ended other than live, since the close then sends its job back to work. A record that does
/// not read and a `held` that cannot tell leave it waiting no longer, so its job is at work.
pub fn waits_live(
  commit: &str,
  deployed: Option<&str>,
  progress: Option<&Value>,
  held: Option<bool>,
) -> bool {
  let run_of_it = progress.filter(|run| run["commit"].as_str() == Some(commit));

  if run_of_it
    .and_then(|run| run["ending"]["outcome"].as_str())
    .is_some_and(|outcome| outcome != LIVE)
  {
    return false;
  }

  deployed.is_some_and(|deployed| deployed != commit) && held == Some(false)
}

/// The testbed declaration at a checkout's root, as far as the machine's turn reads it: the repository's name and
/// its deploy root.
#[derive(Deserialize)]
struct Declaration {
  name: Option<String>,
  deploy: Option<Deploy>,
}

#[derive(Deserialize)]
struct Deploy {
  root: Option<String>,
}

fn declaration(checkout: &Path) -> Option<Declaration> {
  toml::from_str(&std::fs::read_to_string(checkout.join(DECLARATION)).ok()?).ok()
}

/// The name the testbed declaration in `checkout` gives its repository.
pub(crate) fn declared_name(checkout: &Path) -> Option<String> {
  declaration(checkout)?.name.filter(|name| !name.is_empty())
}

/// The deploy root the testbed declaration in `checkout` names, a leading `~/` read against `home`.
pub(crate) fn deploy_root(checkout: &Path, home: &Path) -> Option<PathBuf> {
  let root = declaration(checkout)?.deploy?.root?;

  Some(root.strip_prefix("~/").map_or_else(|| PathBuf::from(&root), |rest| home.join(rest)))
}

/// The checkouts a repository's commits are read in, by the name it goes by: each base checkout `bases` names
/// with its repository row's name, by the name its testbed declaration gives, else by that row's name.
pub fn checkouts_by_name<'a>(
  bases: impl IntoIterator<Item = (&'a str, &'a Path)>,
) -> BTreeMap<String, PathBuf> {
  bases
    .into_iter()
    .map(|(row, base)| (declared_name(base).unwrap_or_else(|| row.to_owned()), base.to_path_buf()))
    .collect()
}

/// The base checkouts of the repositories on this machine, by the name each goes by; none where the rows do not
/// read.
pub(crate) async fn local_checkouts(db: &DatabaseConnection) -> BTreeMap<String, PathBuf> {
  match crate::repositories::every(db).await {
    Ok(rows) => checkouts_by_name(
      rows
        .iter()
        .filter(|row| row.node.is_none())
        .map(|row| (row.name.as_str(), Path::new(&row.path))),
    ),
    Err(why) => {
      eprintln!("machine turn: the repositories were not read: {}", why.detail);

      BTreeMap::new()
    }
  }
}

/// Whether any commit an item's closes handed over still waits to go live, each read in a checkout of its own
/// repository: the job's own checkout where its declaration gives that repository's name, else the one
/// `checkouts` names for it, and the job's own for the summing pair. A repository with no checkout here leaves
/// its commit waiting no longer.
pub async fn any_waiting_live(
  closes: &[Close],
  own: &Path,
  checkouts: &BTreeMap<String, PathBuf>,
  home: &Path,
) -> bool {
  let own_name = declared_name(own);

  for close in closes {
    let Some(commit) = close.waiting() else { continue };
    let checkout = match close.repository.as_deref() {
      None => Some(own),
      Some(repository) if own_name.as_deref() == Some(repository) => Some(own),
      Some(repository) => checkouts.get(repository).map(PathBuf::as_path),
    };

    if let Some(checkout) = checkout
      && waiting_live(checkout, commit, home).await
    {
      return true;
    }
  }

  false
}

pub(crate) fn read_json(path: &Path) -> Option<Value> {
  serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Whether `commit` is `deployed` or an ancestor of it, as git answers in `checkout`; none where git cannot
/// tell, a commit the checkout does not hold among them.
pub(crate) async fn held_by(checkout: &Path, commit: &str, deployed: &str) -> Option<bool> {
  let status = tokio::process::Command::new("git")
    .args(["merge-base", "--is-ancestor", commit, deployed])
    .current_dir(checkout)
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .status()
    .await
    .ok()?;

  match status.code() {
    Some(0) => Some(true),
    Some(1) => Some(false),
    _ => None,
  }
}

/// Whether `commit` is live, as the deployment record of the repository in `checkout` reads: the commit that runs
/// is it or, as git answers in `checkout`, holds it; none where the record or git cannot tell.
pub(crate) async fn live(checkout: &Path, commit: &str, home: &Path) -> Option<bool> {
  let root = deploy_root(checkout, home)?;
  let deployed = read_json(&root.join(DEPLOYED))?["commit"].as_str()?.to_owned();

  match deployed == commit {
    true => Some(true),
    false => held_by(checkout, commit, &deployed).await,
  }
}

/// Whether the commit a close in `checkout` handed over still waits to go live, by its repository's deployment.
pub async fn waiting_live(checkout: &Path, commit: &str, home: &Path) -> bool {
  let Some(root) = deploy_root(checkout, home) else {
    return false;
  };
  let deployed =
    read_json(&root.join(DEPLOYED)).and_then(|record| record["commit"].as_str().map(str::to_owned));
  let progress = read_json(&root.join(PROGRESS));
  let held = match deployed.as_deref() {
    Some(deployed) if deployed != commit => held_by(checkout, commit, deployed).await,
    _ => None,
  };

  waits_live(commit, deployed.as_deref(), progress.as_ref(), held)
}

/// One job a reading may mark: running or under review, or marked still.
struct Standing {
  id: i64,
  item: i64,
  state: String,
  path: Option<String>,
  marked: bool,
}

/// The jobs a reading may mark, each with the checkout its seat was let in at.
async fn standing(db: &DatabaseConnection) -> Result<Vec<Standing>, Fault> {
  all(
    db,
    r#"SELECT j."id", j."item_id", j."state", j."machine_turn",
              (SELECT r."path" FROM "role_slot_requests" r
                WHERE r."owner" = $1 || j."id" AND r."state" = 'active'
                ORDER BY r."sequence" DESC LIMIT 1) AS "path"
         FROM "jobs" j
        WHERE j."state" IN ('starting', 'running', 'reviewing') OR j."machine_turn""#,
    vec![JOB_OWNER.into()],
  )
  .await?
  .iter()
  .map(|row| {
    Ok(Standing {
      id: row.try_get("", "id")?,
      item: row.try_get("", "item_id")?,
      state: row.try_get("", "state")?,
      path: row.try_get("", "path")?,
      marked: row.try_get("", "machine_turn")?,
    })
  })
  .collect()
}

/// What the testbed's status prints run in `folder`: its lanes, and the close line of the repository the folder
/// is of.
async fn testing(folder: &Path) -> Result<BTreeSet<String>, String> {
  let printed = tokio::process::Command::new(TESTBED)
    .args(["status", "--json"])
    .current_dir(folder)
    .stdin(std::process::Stdio::null())
    .output()
    .await
    .map_err(|why| format!("{TESTBED} status did not run: {why}"))?;

  if !printed.status.success() {
    return Err(format!("{TESTBED} status ended {}", printed.status));
  }

  Reading::testing_in(&String::from_utf8_lossy(&printed.stdout))
}

/// The items the job service holds under a close, each with where its closes stand.
async fn closing(service: &JobService) -> Result<BTreeMap<i64, Vec<Close>>, String> {
  Ok(
    closes::items(service)
      .await?
      .iter()
      .filter_map(|item| Some((item["id"].as_i64()?, closes::of(&item["labels"]))))
      .collect(),
  )
}

/// One pass: the machine's turn read and each standing job marked by it. The lanes are read once; a running
/// job whose item's close in any repository is testing has its repository's close line read in its checkout,
/// and a job whose item's closes wait for their commits to go live has each commit's deployment read, the
/// repositories' checkouts read once a pass where a close names one. A test's ground reads nothing; a reading
/// that fails marks nothing from it, so a job no reading names is at work.
pub async fn pass(db: &DatabaseConnection, vars: &BTreeMap<String, String>, data_root: &Path, home: &Path) {
  if crate::job_end::test_ground(vars) {
    return;
  }

  let jobs = match standing(db).await {
    Ok(jobs) => jobs,
    Err(why) => return eprintln!("machine turn: the standing jobs were not read: {}", why.detail),
  };
  let closes = match JobService::of(vars) {
    Some(service) => closing(&service).await.unwrap_or_else(|why| {
      eprintln!("machine turn: the items under a close were not read: {why}");

      BTreeMap::new()
    }),
    None => BTreeMap::new(),
  };
  let mut reading = Reading::default();

  match testing(data_root).await {
    Ok(checkouts) => reading.checkouts.extend(checkouts),
    Err(why) => eprintln!("machine turn: the testbed's lanes were not read: {why}"),
  }

  let mut checkouts = None;

  for job in &jobs {
    let (Some(path), Some(closes)) = (job.path.as_deref(), closes.get(&job.item)) else {
      continue;
    };

    if job.state == "running" && closes.iter().any(Close::testing) {
      match testing(Path::new(path)).await {
        Ok(named) => reading.checkouts.extend(named),
        Err(why) => eprintln!("machine turn: the close line at {path} was not read: {why}"),
      }
    }

    if !closes.iter().any(|close| close.waiting().is_some()) {
      continue;
    }

    if checkouts.is_none() && closes.iter().any(|close| close.repository.is_some()) {
      checkouts = Some(local_checkouts(db).await);
    }

    if any_waiting_live(closes, Path::new(path), checkouts.as_ref().unwrap_or(&BTreeMap::new()), home).await
    {
      reading.awaiting_live.insert(job.item);
    }
  }

  if let Err(why) = mark(db, &reading).await {
    eprintln!("machine turn: the jobs were not marked: {}", why.detail);
  }
}

/// Each standing job marked by the reading, and a job no longer running or under review unmarked; answers the
/// jobs whose mark changed. A job's checkout is the path its seat was let in at.
pub async fn mark(db: &DatabaseConnection, reading: &Reading) -> Result<Vec<i64>, Fault> {
  let mut changed = vec![];

  for job in standing(db).await? {
    let in_turn =
      matches!(job.state.as_str(), "running" | "reviewing") && reading.holds(job.path.as_deref(), job.item);

    if in_turn != job.marked {
      run(
        db,
        r#"UPDATE "jobs" SET "machine_turn" = $2 WHERE "id" = $1"#,
        vec![job.id.into(), in_turn.into()],
      )
      .await?;
      changed.push(job.id);
    }
  }

  Ok(changed)
}
