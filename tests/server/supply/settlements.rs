//! A reservation's settlement (docs/supply.md, 「Reservations」): settled, its capture with the cost and its
//! release are owed in the supply outbox and its row closed captured; a row the hook does not take is tried again
//! after a wait that doubles, one it takes is let go; and an open reservation past its expiry is owed a capture of
//! what the store last recorded it covered.

use agency_server::now_ms;
use agency_server::supply::reservations::{held, send_due, settle, settle_expired};
use policy::supply::SUPPLY;
use serde_json::json;

use super::{Consumer, PERSON, owed, reserved, row_of, state_of};
use crate::support::grounds::ground_on;
use crate::support::scratch::Scratch;

// The wait before a settlement's next try after it was tried `tries` times.
fn wait_after(tries: u32) -> i64 {
  (SUPPLY.retry_first_seconds << (tries - 1)).min(SUPPLY.retry_ceiling_seconds) * 1000
}

#[tokio::test(flavor = "multi_thread")]
async fn a_settled_reservation_owes_its_capture_and_release_until_the_hook_takes_them() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let consumer = Consumer::stand().await;
  let mut chosen = reserved(&ground, &consumer, &[("plan", 1.0)]).await;
  let row = row_of(&chosen);
  let kept = held(scratch.db(), row).await.unwrap().unwrap();
  let (run, step) = (chosen.run.clone(), chosen.step);

  let before = now_ms();
  settle(&ground, &mut chosen, &kept, 420_000).await.unwrap();
  let after = now_ms();

  assert_eq!(state_of(scratch.db(), row).await, ("captured".to_owned(), 420_000));

  let settlements = owed(scratch.db()).await;
  let keys: Vec<&str> = settlements.iter().map(|owed| owed.key.as_str()).collect();

  assert_eq!(keys, [format!("{run}:{step}:capture"), format!("{run}:{}:release", step + 1)]);
  assert_eq!(
    (
      &settlements[0].ask["op"],
      &settlements[0].ask["costUsd"],
      &settlements[0].ask["reservation"]
    ),
    (&json!("capture"), &json!(0.42), &json!("r-plan"))
  );
  assert_eq!(
    (&settlements[1].ask["op"], settlements[1].ask.get("costUsd")),
    (&json!("release"), None)
  );
  assert_eq!(&consumer.ops()[2..], ["capture", "release"], "each is tried as it is owed");

  for owed in &settlements {
    assert_eq!((owed.tries, owed.failure.as_deref()), (1, Some("the hook did not take it")));
    assert!((before + wait_after(1)..=after + wait_after(1)).contains(&owed.due_at), "{owed:?}");
  }

  // Due again and still not taken, each waits twice as long.
  scratch.run(r#"UPDATE "supply_settlements" SET "due_at" = 0"#, vec![]).await;

  let before = now_ms();
  assert_eq!(send_due(&ground).await, 0);
  let after = now_ms();

  for owed in owed(scratch.db()).await {
    assert_eq!(owed.tries, 2);
    assert!((before + wait_after(2)..=after + wait_after(2)).contains(&owed.due_at), "{owed:?}");
  }

  // Taken, each is let go once it is due.
  consumer.answers("capture", json!({ "taken": true }));
  consumer.answers("release", json!({ "taken": true }));

  assert_eq!(send_due(&ground).await, 0, "nothing is due yet");
  assert_eq!(owed(scratch.db()).await.len(), 2);

  scratch.run(r#"UPDATE "supply_settlements" SET "due_at" = 0"#, vec![]).await;

  assert_eq!(send_due(&ground).await, 2);
  assert!(owed(scratch.db()).await.is_empty());
  consumer.down().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_open_reservation_past_its_expiry_owes_a_capture_of_what_it_covered() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let now = now_ms();

  for (run, state, expires) in [
    ("job-9-1", "open", now - 1_000),
    ("job-9-2", "open", now + 3_600_000),
    ("job-9-3", "captured", now - 1_000),
  ] {
    scratch
      .run(
        r#"INSERT INTO "supply_reservations"
             ("run", "opened_step", "last_step", "account", "source", "reservation", "amount_micros", "unit_micros",
              "covered_micros", "expires_at", "state")
           VALUES ($1, 1, 4, $2, 'plan', $1, 1000000, 1000000, 1234, $3, $4)"#,
        vec![run.into(), PERSON.into(), expires.into(), state.into()],
      )
      .await;
  }

  assert_eq!(settle_expired(&ground).await.unwrap(), 1);

  let settlements = owed(scratch.db()).await;
  let keys: Vec<&str> = settlements.iter().map(|owed| owed.key.as_str()).collect();

  assert_eq!(keys, ["job-9-1:5:capture", "job-9-1:6:release"]);
  assert_eq!(settlements[0].ask["costUsd"], json!(0.001234));
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "supply_reservations" WHERE "run" = 'job-9-1' AND "state" = 'captured'
             AND "covered_micros" = 1234"#,
        vec![]
      )
      .await,
    1
  );
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "supply_reservations" WHERE "state" = 'open'"#, vec![])
      .await,
    1,
    "the reservation still in time stays open"
  );
}
