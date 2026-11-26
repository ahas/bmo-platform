//! A support checkout grown at a launch (docs/worker-slots.md, 「Growing a checkout at a launch」): a repository
//! set to `auto` whose checkouts are all held records its next support checkout and holds it for the item at
//! once, bounded by a free agent of the item's and by the machine's room, and by nothing else.

use axum::http::StatusCode;
use policy::workers::WORKERS;
use sea_orm::TransactionTrait;
use serde_json::Value;
use store::{all, one, run};

use super::Refused;
use super::work::{agent_carries, held, write_hold};
use crate::now_ms;
use crate::places::Ground;
use crate::queue::holds::{JOB_HOLDER, is_free};
use crate::queue::workers::{Worker, support_name, support_worker};
use crate::repositories::{AGENT_LABEL, MODEL_LABEL, Repository, named_by};
use crate::role_agents::{self, RoleAgent};

/// The agents a launch of an item would seat its job on: the item, its role, and the account of the
/// supervisor launching it.
pub(crate) struct Seating<'a> {
  pub item: &'a Value,
  pub role: &'a str,
  pub account: Option<i64>,
}

/// The hold a grown checkout is taken under.
pub(crate) struct Holding<'a> {
  pub item_id: i64,
  pub holder: &'a str,
  pub role: &'a str,
}

/// A checkout grown and held: the checkout, and its assignment as the routes answer it.
pub(crate) struct Grown {
  pub checkout: Worker,
  pub assignment: Value,
}

// The agents an item's labels name for its job, as its launch would read them: the agent its `agent` label names
// where that agent has a road of its own, else the candidates of its role for the model its `model` label names.
// None where it names neither or nothing answers, since no launch would seat it.
async fn candidates(ground: &Ground, seating: &Seating<'_>) -> Result<Vec<RoleAgent>, Refused> {
  let db = &ground.db;
  let labels = &seating.item["labels"];

  if let Some(agent) = named_by(labels, AGENT_LABEL) {
    return Ok(
      role_agents::named(db, agent)
        .await?
        .filter(|agent| agent.entry().is_some())
        .into_iter()
        .collect(),
    );
  }

  match named_by(labels, MODEL_LABEL) {
    Some(model) => Ok(role_agents::candidates(ground, seating.role, model, seating.account).await?),
    None => Ok(vec![]),
  }
}

// The holds of a role that no job has started on yet: each a launch about to seat an agent of it.
async fn launches_pending<C: sea_orm::ConnectionTrait>(db: &C, role: &str) -> Result<i64, Refused> {
  let row = one(
    db,
    r#"SELECT COUNT(*) AS "count" FROM "assignments" a
       WHERE a."state" = 'held' AND a."role" = $1 AND a."holder_name" NOT LIKE $2
         AND NOT EXISTS (SELECT 1 FROM "jobs" j WHERE j."assignment_id" = a."id")"#,
    vec![role.into(), format!("{JOB_HOLDER}%").into()],
  )
  .await?;

  Ok(row.map_or(Ok(0), |row| row.try_get::<i64>("", "count"))?)
}

// The lowest slot from the first that the repository has not recorded and that nothing holds.
async fn next_slot<C: sea_orm::ConnectionTrait>(db: &C, repository: &str) -> Result<i64, Refused> {
  let recorded: Vec<i64> = all(
    db,
    r#"SELECT "slot" FROM "support_checkouts" WHERE "repository" = $1"#,
    vec![repository.into()],
  )
  .await?
  .iter()
  .filter_map(|row| row.try_get::<i64>("", "slot").ok())
  .collect();
  let mut slot = WORKERS.first_support_slot;

  while recorded.contains(&slot) || !is_free(db, &support_name(repository, slot)).await? {
    slot += 1;
  }

  Ok(slot)
}

/// The next support checkout of a repository whose checkouts are all held, recorded and held for the item; none
/// where the repository does not grow or, given `seating`, its item names no agent a launch would seat. Refused
/// 409 `agent-busy` where every agent of `seating` is carrying a run or spoken for by a hold no job has started
/// on, and 422 `no-worker-free` where the machine has no room, naming each reason. A borrowing gives no
/// `seating`, and is bounded by the machine's room alone.
pub(crate) async fn grow(
  ground: &Ground,
  repository: &Repository,
  seating: Option<Seating<'_>>,
  holding: Holding<'_>,
) -> Result<Option<Grown>, Refused> {
  if !repository.grows() {
    return Ok(None);
  }

  let db = &ground.db;
  let agents = match &seating {
    Some(seating) => {
      let agents = candidates(ground, seating).await?;

      if agents.is_empty() {
        return Ok(None);
      }

      Some(agents)
    }
    None => None,
  };
  let room = crate::watches::machine::room(db, ground.peers.node.as_ref()).await?;

  if !room.held.is_empty() {
    return Err(
      Refused::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!(
          "every checkout of {} is held, and no other is grown while the machine is short: {}",
          repository.name,
          room.held.join("; ")
        ),
      )
      .with_code("no-worker-free"),
    );
  }

  // The candidates' rows and the repository's held for the transaction, in that order as a launch holds them,
  // so two launches at once never count one free agent twice nor record one slot twice.
  let written = db.begin().await.map_err(|why| Refused::failed(why.to_string()))?;

  if let Some(agents) = &agents {
    for agent in agents {
      one(
        &written,
        r#"SELECT 1 FROM "role_agents" WHERE "name" = $1 FOR UPDATE"#,
        vec![agent.name.clone().into()],
      )
      .await?;
    }
  }

  one(
    &written,
    r#"SELECT 1 FROM "repositories" WHERE "name" = $1 FOR UPDATE"#,
    vec![repository.name.clone().into()],
  )
  .await?;

  if let Some(agents) = agents {
    let mut free = 0;
    let mut carried = vec![];

    for agent in agents {
      match agent_carries(&written, &agent.name).await? {
        None => free += 1,
        Some(what) => carried.push(format!("{}, which carries {what}", agent.name)),
      }
    }

    let pending = launches_pending(&written, holding.role).await?;

    if free <= pending {
      let busy = if carried.is_empty() { "none".to_owned() } else { carried.join("; ") };

      return Err(
        Refused::new(
          StatusCode::CONFLICT,
          format!(
            "Item #{} waits for an agent before a checkout of {} is grown: {free} of its agents are free and \
             {pending} holds of the {} role wait for their jobs; busy: {busy}; launch it once one is free",
            holding.item_id, repository.name, holding.role
          ),
        )
        .with_code("agent-busy"),
      );
    }
  }

  let slot = next_slot(&written, &repository.name).await?;

  run(
    &written,
    r#"INSERT INTO "support_checkouts" ("repository", "slot", "made_at") VALUES ($1, $2, $3)"#,
    vec![repository.name.clone().into(), slot.into(), now_ms().into()],
  )
  .await?;

  let checkout = support_worker(&Worker::base(repository), slot);
  let id = write_hold(&written, holding.item_id, holding.holder, &checkout.name, holding.role).await?;

  written.commit().await.map_err(|why| Refused::failed(why.to_string()))?;
  println!(
    "{}: support checkout {} grown for item #{}",
    repository.name, checkout.name, holding.item_id
  );

  Ok(Some(Grown {
    checkout,
    assignment: held(ground, id).await?,
  }))
}
