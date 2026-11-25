//! What holds a checkout now (docs/api.md, 「assignments and jobs」): an assignment `held` or `running`, or a
//! job at work in it. `POST /assignments`, a job's borrowing and the queue's notices read a checkout as free by
//! this one reading, and the assignments whose hold has ended are released before any of them reads it. Its
//! half of jobs at work is what a checkout's dev server and the running workers are read by.

use sea_orm::DatabaseConnection;
use store::{all, one, run};

use policy::workers::WORKERS;

use crate::Refusal;
use crate::now_ms;
use crate::places::loan_branch;

/// What opens the holder's name of an assignment a job took for itself, the job's number after it: a checkout it
/// borrowed from another repository.
pub const JOB_HOLDER: &str = "job:";

// The checkouts a job is at work in, one row a job: starting, running, or under review with its checkout kept,
// through every stage and round and the pauses between their turns. A macro, so the holding reading below is
// put together from it at compile time.
macro_rules! at_work {
  () => {
    r#"SELECT "worker_name" FROM "jobs"
   WHERE "state" IN ('starting', 'running') OR ("state" = 'reviewing' AND NOT "review_parked")"#
  };
}

const AT_WORK: &str = at_work!();

// The checkouts held now, one row a hold: every assignment held or running, and every job at work.
const HOLDING: &str = concat!(
  r#"SELECT "worker_name" FROM "assignments" WHERE "state" IN ('held', 'running') UNION ALL "#,
  at_work!()
);

/// The assignments whose hold has ended, released: a running one whose job has ended, one lent to a job that gave
/// its checkout up, given back, and a held one a newer assignment of the same item follows, since an item is held on one checkout
/// at a time. No clock lets any other hold go.
pub async fn release_ended(db: &DatabaseConnection) -> Result<(), Refusal> {
  let now = now_ms();
  run(
    db,
    r#"UPDATE "assignments" a SET "state" = 'released', "changed_at" = $1
       WHERE a."state" = 'running'
         AND EXISTS (SELECT 1 FROM "jobs" j WHERE j."assignment_id" = a."id")
         AND NOT EXISTS (SELECT 1 FROM "jobs" j
               WHERE j."assignment_id" = a."id"
                 AND (j."state" IN ('starting', 'running') OR (j."state" = 'reviewing' AND NOT j."review_parked")))"#,
    vec![now.into()],
  )
  .await?;
  // A checkout lent to a job is given back once that job gives its checkout up (docs/worker-slots.md, 「Loans
  // across a job's review」): parked onto the job's branch first, held meanwhile.
  give_back_loans(db, now).await?;
  // A hold its item was assigned again after, under whichever holder, is one no job will start on: a hold left
  // under a name no session ends with goes here. A job's loan is no assignment of its item on either side.
  run(
    db,
    r#"UPDATE "assignments" a SET "state" = 'released', "changed_at" = $1
       WHERE a."state" = 'held'
         AND a."holder_name" NOT LIKE $2
         AND EXISTS (SELECT 1 FROM "assignments" b
               WHERE b."item_id" = a."item_id" AND (b."created_at", b."id") > (a."created_at", a."id")
                 AND b."holder_name" NOT LIKE $2)"#,
    vec![now.into(), format!("{JOB_HOLDER}%").into()],
  )
  .await?;

  Ok(())
}

/// The held assignments of an item that holders other than `holder` left, released as `holder` asks for the
/// item, since an item is held on one checkout at a time. A job's loan is no assignment of its item.
pub async fn release_others(db: &DatabaseConnection, item_id: i64, holder: &str) -> Result<(), Refusal> {
  run(
    db,
    r#"UPDATE "assignments" SET "state" = 'released', "changed_at" = $1
       WHERE "state" = 'held' AND "item_id" = $2 AND "holder_name" <> $3 AND "holder_name" NOT LIKE $4"#,
    vec![now_ms().into(), item_id.into(), holder.into(), format!("{JOB_HOLDER}%").into()],
  )
  .await?;

  Ok(())
}

// The loans whose job gave its checkout up, each given back: claimed by marking when its give-back began, so one
// reader alone parks it and nothing takes it half parked, then parked on this machine and released. A claim a
// stop of agency cut short is taken up again once the policy's seconds have passed.
async fn give_back_loans(db: &DatabaseConnection, now: i64) -> Result<(), Refusal> {
  let claimed = all(
    db,
    r#"UPDATE "assignments" a SET "returning_at" = $1
       WHERE a."state" = 'held'
         AND a."holder_name" LIKE $2
         AND (a."returning_at" IS NULL OR a."returning_at" < $4)
         AND NOT EXISTS (SELECT 1 FROM "jobs" j
               WHERE $3 || j."id"::text = a."holder_name"
                 AND (j."state" IN ('starting', 'running') OR (j."state" = 'reviewing' AND NOT j."review_parked")))
       RETURNING a."id", a."worker_name", a."item_id""#,
    vec![
      now.into(),
      format!("{JOB_HOLDER}%").into(),
      JOB_HOLDER.into(),
      (now - WORKERS.loan_return_seconds * 1000).into(),
    ],
  )
  .await?;

  for loan in claimed {
    let id: i64 = loan.try_get("", "id")?;
    let worker: String = loan.try_get("", "worker_name")?;
    let item: i64 = loan.try_get("", "item_id")?;
    let checkout = crate::queue::workers::worker_row(db, &worker, std::path::Path::new("")).await?;

    if let Some(checkout) = checkout.filter(|checkout| checkout.node.is_none() && !checkout.cwd.is_empty())
    {
      let branch = loan_branch::job_branch(item);
      let parked =
        tokio::task::spawn_blocking(move || loan_branch::give_back(&checkout.cwd, &branch)).await;

      match parked {
        Ok(Ok(())) => {}
        Ok(Err(why)) => eprintln!("loan {id} of {worker}: {why}; it is released as it stands"),
        Err(why) => eprintln!("loan {id} of {worker}: its give-back did not run: {why}"),
      }
    }

    run(
      db,
      r#"UPDATE "assignments" SET "state" = 'released', "changed_at" = $1 WHERE "id" = $2 AND "state" = 'held'"#,
      vec![crate::now_ms().into(), id.into()],
    )
    .await?;
  }

  Ok(())
}

/// Whether nothing holds a checkout now, read in whatever connection a caller holds.
pub async fn is_free<C: sea_orm::ConnectionTrait>(db: &C, checkout: &str) -> Result<bool, Refusal> {
  let held = one(
    db,
    &format!(r#"SELECT 1 AS "held" FROM ({HOLDING}) h WHERE h."worker_name" = $1 LIMIT 1"#),
    vec![checkout.into()],
  )
  .await?;

  Ok(held.is_none())
}

/// The checkouts something holds now, by name.
pub async fn held_checkouts(db: &DatabaseConnection) -> Result<Vec<String>, Refusal> {
  names(db, HOLDING).await
}

/// Whether a job is at work in a checkout now, in any stage or round of it (docs/worker-bench.md, 「It stands
/// only while a job runs」).
pub async fn job_at_work(db: &DatabaseConnection, checkout: &str) -> Result<bool, Refusal> {
  let at_work = one(
    db,
    &format!(r#"SELECT 1 AS "at_work" FROM ({AT_WORK}) w WHERE w."worker_name" = $1 LIMIT 1"#),
    vec![checkout.into()],
  )
  .await?;

  Ok(at_work.is_some())
}

/// The checkouts a job is at work in now, by name.
pub async fn checkouts_at_work(db: &DatabaseConnection) -> Result<Vec<String>, Refusal> {
  names(db, AT_WORK).await
}

// The checkouts a reading of checkout rows names, each once.
async fn names(db: &DatabaseConnection, rows: &str) -> Result<Vec<String>, Refusal> {
  Ok(
    all(db, &format!(r#"SELECT DISTINCT "worker_name" FROM ({rows}) h"#), vec![])
      .await?
      .iter()
      .filter_map(|row| row.try_get::<String>("", "worker_name").ok())
      .collect(),
  )
}
