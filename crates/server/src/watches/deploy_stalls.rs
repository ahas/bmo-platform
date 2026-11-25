//! Commits waiting for their repositories' batch deployments that are not going live, told by the deploy queue's
//! flush (docs/agent-control.md, 「The deploy queue is flushed every hour」): a repository no deployment of this
//! machine puts live, told at once for each set of its waiting commits, and a commit still waiting
//! `deploy_stale_minutes` after the first flush read it waiting, told once.

use std::collections::{BTreeMap, BTreeSet};

use policy::fill_marks;
use policy::job_notices::JOB_NOTICES;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::job_notices;
use crate::outbox::{self, Keep, Notice};
use crate::queue::orderer::JobService;
use crate::queue::settings;

/// The settings row what the flush has read of the waiting commits stands in.
pub const STATE_KEY: &str = "deploy_stalls";
/// The kind of the notice of a repository nothing deploys.
pub const UNDEPLOYABLE: &str = "deploy-undeployable";
/// The kind of the notice of commits waiting too long.
pub const STALE: &str = "deploy-stale";

const HOUR_MS: i64 = 3_600_000;

/// One commit waiting for a repository's batch deployment: its item, as the job service holds it, and the commit.
#[derive(Clone, Debug)]
pub struct Wait {
  pub item: Value,
  pub commit: String,
}

impl Wait {
  /// The item's number.
  pub fn id(&self) -> i64 {
    self.item["id"].as_i64().unwrap_or_default()
  }
}

/// Why a repository's waiting commits are not going live, as one flush read it, and whether no deployment of this
/// machine puts the repository live at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stall {
  pub why: String,
  pub undeployable: bool,
}

/// What the flush keeps between reads: when each waiting commit was first read waiting, the waiting commits told
/// as waiting too long, and the waiting commits each repository nothing deploys was told for, each commit keyed by
/// its repository, its item and the commit.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
  #[serde(default)]
  pub since: BTreeMap<String, i64>,
  #[serde(default)]
  pub stale: BTreeSet<String>,
  #[serde(default)]
  pub undeployable: BTreeMap<String, String>,
}

fn key(repository: &str, wait: &Wait) -> String {
  format!("{repository}|{}|{}", wait.id(), wait.commit)
}

fn listed(waits: &[&Wait]) -> String {
  waits
    .iter()
    .map(|wait| {
      let commit = wait.commit.get(..12).unwrap_or(&wait.commit);

      fill_marks(
        &JOB_NOTICES.deploy_stalled_item,
        &[
          ("item", &wait.id().to_string()),
          ("title", wait.item["title"].as_str().unwrap_or_default()),
          ("commit", commit),
        ],
      )
    })
    .collect::<Vec<_>>()
    .join("; ")
}

/// What one stalled notice tells: its kind, the repository it is about and its summary.
struct Telling<'a> {
  event: &'a str,
  repository: &'a str,
  summary: String,
}

/// One notice `telling` owed to the orderer of each of `waits`, one instruction a room, its order built by `order`
/// from the room's waits listed; the owner's private room for an item whose orderer is not read.
async fn owe(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  service: &JobService,
  telling: &Telling<'_>,
  waits: &[&Wait],
  order: impl Fn(&str) -> String,
) -> Result<(), String> {
  let mut rooms: BTreeMap<Option<i64>, Vec<&Wait>> = BTreeMap::new();

  for wait in waits {
    let room = service.orderer_of(&wait.item).await.map(|orderer| orderer.room).unwrap_or_default();

    rooms.entry(room).or_default().push(wait);
  }

  let several = rooms.len() > 1;

  for (room, waits) in rooms {
    let ids: Vec<i64> = waits.iter().map(|wait| wait.id()).collect();
    let body = job_notices::deploy_stalled(
      telling.event,
      telling.repository,
      &telling.summary,
      &order(&listed(&waits)),
      &ids,
      room,
    );
    let notice = Notice::instruction(body, Keep::Once);
    let notice = match several {
      true => {
        let key =
          format!("{}@{}", notice.key, room.map_or_else(|| "owner".to_owned(), |room| room.to_string()));

        notice.keyed(key)
      }
      false => notice,
    };

    outbox::owe(db, vars, notice).await?;
  }

  Ok(())
}

/// The waiting commits of one flush at `now` read against what the flush kept: each commit timed from the first
/// flush that read it waiting, a repository nothing deploys told once for each set of its waiting commits, and the
/// commits still waiting `deploy_stale_minutes` after that first read told once, each with why its repository is
/// not going live as `stalls` gives it. A commit that no longer waits leaves the row. A notice the store would not
/// take is told again by the next flush.
pub async fn told(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  service: &JobService,
  waiting: &BTreeMap<String, Vec<Wait>>,
  stalls: &BTreeMap<String, Stall>,
  now: i64,
) -> Result<(), String> {
  let kept: State = settings::get(db, STATE_KEY)
    .await
    .map_err(|why| why.detail)?
    .and_then(|kept| serde_json::from_str(&kept).ok())
    .unwrap_or_default();
  let bound = JOB_NOTICES.deploy_stale_minutes * 60_000;
  let mut state = State::default();

  for (repository, waits) in waiting {
    let Some(stall) = stalls.get(repository) else { continue };
    let mut overdue: Vec<&Wait> = vec![];
    let mut longest = 0;

    for wait in waits {
      let key = key(repository, wait);
      let first = kept.since.get(&key).copied().unwrap_or(now);

      state.since.insert(key.clone(), first);

      if kept.stale.contains(&key) {
        state.stale.insert(key);
      } else if now - first >= bound {
        overdue.push(wait);
        longest = longest.max(now - first);
      }
    }

    if stall.undeployable {
      let mark = waits.iter().map(|wait| key(repository, wait)).collect::<Vec<_>>().join(",");
      let all: Vec<&Wait> = waits.iter().collect();
      let told_before = kept.undeployable.get(repository) == Some(&mark);
      let telling = Telling {
        event: UNDEPLOYABLE,
        repository,
        summary: format!("Nothing on this machine deploys {repository}"),
      };
      let order = |items: &str| {
        fill_marks(
          &JOB_NOTICES.deploy_undeployable_order,
          &[("repository", repository), ("why", &stall.why), ("items", items)],
        )
      };

      match told_before {
        true => {
          state.undeployable.insert(repository.clone(), mark);
        }
        false => match owe(db, vars, service, &telling, &all, order).await {
          Ok(()) => {
            println!("deploy flush: {repository}, which nothing here deploys, owed for {mark}");
            state.undeployable.insert(repository.clone(), mark);
          }
          Err(why) => {
            eprintln!("deploy flush: {repository}, which nothing here deploys, was not owed: {why}")
          }
        },
      }
    }

    if overdue.is_empty() {
      continue;
    }

    let hours = (longest / HOUR_MS).to_string();
    let order = |items: &str| {
      fill_marks(
        &JOB_NOTICES.deploy_stale_order,
        &[("repository", repository), ("why", &stall.why), ("hours", &hours), ("items", items)],
      )
    };

    let telling = Telling {
      event: STALE,
      repository,
      summary: format!("Commits have waited {hours} hours for {repository}'s batch deployment"),
    };

    match owe(db, vars, service, &telling, &overdue, order).await {
      Ok(()) => {
        println!("deploy flush: {} commit(s) of {repository} waiting {hours} hours, owed", overdue.len());
        state.stale.extend(overdue.iter().map(|wait| key(repository, wait)));
      }
      Err(why) => eprintln!("deploy flush: {repository}'s commits waiting too long were not owed: {why}"),
    }
  }

  let written = serde_json::to_string(&state).map_err(|why| why.to_string())?;

  settings::set(db, STATE_KEY, &written).await.map_err(|why| why.detail)
}
