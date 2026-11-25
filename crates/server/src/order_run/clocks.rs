//! The role slots' own clocks (docs/role-slots.md, 「The line」): every five seconds a reservation's head slot
//! raised into room another process gave back; every minute from
//! the start the leases bound to a launched child read against what stands, one whose child was seen standing
//! and stands no longer given back, and the
//! demand a lease no longer active held on the arbiter's master given back with it; from the start
//! and every ten minutes what runs and places left in herdr swept. Beside them, each reader of the accounts' usage windows
//! passes on the cadence it declares (docs/usage-windows.md). Each runs for as long as the process stands,
//! never beside itself.

use std::sync::Arc;
use std::time::Duration;

use sea_orm::DatabaseConnection;

use super::dispatch::{announce, promote_reservations, reclaim_bound};
use crate::clock::every;
use crate::events::Bus;
use crate::now_ms;
use crate::places::Ground;
use crate::queue::leftovers::sweep_everywhere;
use crate::queue::running::Running;

/// How often the line raises its reservations' head slots.
pub const POOL_PASS: Duration = Duration::from_secs(5);

/// How often the leases bound to a launched child are read against whether that child stands. A reading, never
/// a bound on any lease: a lease is given back only once its child was seen standing and then is not.
pub const BOUND_PASS: Duration = Duration::from_secs(60);

/// How often what runs and places left in herdr is swept beside the sweep each job's end asks for.
pub const LEFTOVERS_PASS: Duration = Duration::from_secs(10 * 60);

/// The line's clock: a reservation's slot at the head of its role's line let into room given back.
pub async fn keep_pool(db: DatabaseConnection, events: Bus) {
  every("role slots", Duration::ZERO, POOL_PASS, move || {
    let db = db.clone();
    let events = events.clone();

    async move {
      for request in promote_reservations(&db, now_ms()).await? {
        announce(&events, &request);
      }

      Ok(())
    }
  })
  .await;
}

/// The bound leases' clock, from the process's start: a lease whose launched child was seen standing and has
/// ended — its unit, its pane and every open execution naming it gone — given back, so a turn whose release
/// never came does not hold its place for good.
pub async fn keep_bound(db: DatabaseConnection, events: Bus, running: Running) {
  every("role slot bound leases", Duration::ZERO, BOUND_PASS, move || {
    let db = db.clone();
    let events = events.clone();
    let running = running.clone();

    async move {
      let released = reclaim_bound(&db, &events, &running).await?;

      if released > 0 {
        println!("role slots: {released} lease(s) whose launched child ended given back");
      }

      Ok(())
    }
  })
  .await;
}

/// The card demands' clock, from the process's start: a demand a seat's lease held on the arbiter's master
/// given back once that lease is no longer active, whatever path ended it (docs/coworker-pool.md,
/// 「Concurrency limits」). A process that names no master keeps no such clock.
pub async fn keep_card_demands(ground: Arc<Ground>) {
  let Some(master) = crate::cards::master_of(&ground) else { return };

  every("card demands", Duration::ZERO, BOUND_PASS, move || {
    let db = ground.db.clone();
    let master = master.clone();

    async move {
      let given = crate::cards::give_back_with_leases(&db, &master).await.map_err(|why| why.detail)?;

      if given > 0 {
        println!("cards: {given} demand(s) whose lease ended given back");
      }

      Ok(())
    }
  })
  .await;
}

/// The leftovers' clock, from the process's start: what runs and places left in herdr swept on this machine and
/// every ready node. It is started once the turns the last process left in flight are picked up, so a run
/// adopted is carried by this process before the first pass reads what carries it.
pub async fn keep_leftovers(ground: Arc<Ground>) {
  every("leftover sweep", Duration::ZERO, LEFTOVERS_PASS, move || {
    let ground = ground.clone();

    async move { sweep_everywhere(&ground).await.map_err(|refusal| refusal.detail) }
  })
  .await;
}

/// The usage windows' clocks: each reader the keeping knows passing at once and then on its own cadence,
/// every reading it takes replacing its slot's last, and a login renewal its reader asks run on this
/// ground.
pub async fn keep_usage_windows(ground: Arc<Ground>) {
  ground.renewals.stand_on(&ground);

  let clocks = ground.windows.readers().iter().cloned().map(|reader| {
    let ground = ground.clone();

    async move {
      every("usage windows", Duration::ZERO, reader.cadence(), move || {
        let ground = ground.clone();
        let reader = reader.clone();

        async move {
          reader.pass(&ground.windows, now_ms()).await;
          // A reading that came in may have given a provider its quota back: what waits for quota asks its
          // order again on it.
          ground.events.publish_named(crate::events::Named {
            id: 0,
            at: 0,
            name: std::borrow::Cow::Borrowed("usage.changed"),
            body: serde_json::json!({ "mode": reader.mode() }),
          });

          Ok(())
        }
      })
      .await;
    }
  });

  futures_util::future::join_all(clocks).await;
}
