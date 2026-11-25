//! The meter on a person's reserved run (docs/supply.md, 「Reservations」 and 「Running out mid-turn」), one look
//! at a time over the gateway's request records of the run: the cost written on the reservation, an extension
//! asked at the policy's share of the amount, an extension refused handing the run to the next platform source
//! with what it covered captured and the move published, and with no source left one more unit of grace before
//! the meter says the run ran out.

use agency_server::now_ms;
use agency_server::supply::meter::{Reading, Shared, look, metered};
use agency_server::supply::reservations::held;
use agency_server::supply::told::SupplyMoved;
use agency_server::supply::{Chosen, chosen_for};
use policy::fill_marks;
use policy::supply::SUPPLY;
use serde_json::json;

use super::{AGENT, Consumer, reserved, row_of, state_of};
use crate::support::grounds::ground_on;
use crate::support::scratch::Scratch;

// A request the gateway recorded for a run at a moment, at a cost.
async fn spend(scratch: &Scratch, run: &str, at: i64, cost_micros: i64) {
  scratch
    .run(
      r#"INSERT INTO "gateway_requests" ("at", "model", "run", "cost_micros") VALUES ($1, 'deepseek-flash', $2, $3)"#,
      vec![at.into(), run.into(), cost_micros.into()],
    )
    .await;
}

// The meter on a run, its cost read from the gateway's records of the agent's run since a moment.
fn meter_on(chosen: &Chosen, since: i64) -> Shared {
  let reading = Reading::Gateway {
    run: AGENT.to_owned(),
    since,
  };

  metered(chosen, reading, AGENT, Some(41), None).expect("a reserved run is metered")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_meter_writes_the_cost_extends_near_the_amount_and_hands_a_refused_extension_on() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let consumer = Consumer::stand().await;
  let chosen = reserved(&ground, &consumer, &[("first", 1.0), ("second", 0.5)]).await;
  let (run, first) = (chosen.run.clone(), row_of(&chosen));
  let since = now_ms() - 1_000;
  let shared = meter_on(&chosen, since);

  // Below the share, the cost is written and nothing is asked; another run's records and older ones count for
  // nothing.
  spend(&scratch, AGENT, since, 300_000).await;
  spend(&scratch, "another-run", since, 5_000_000).await;
  spend(&scratch, AGENT, since - 1, 5_000_000).await;

  assert_eq!(look(&ground, &shared).await.unwrap(), None);
  assert_eq!(state_of(scratch.db(), first).await, ("open".to_owned(), 300_000));
  assert_eq!(consumer.ops(), ["order", "reserve"]);

  // At the share, the reservation is extended to the whole the hook answers.
  consumer.answers("extend", json!({ "accepted": true, "amountUsd": 2.0 }));
  spend(&scratch, AGENT, since, 600_000).await;

  assert_eq!(look(&ground, &shared).await.unwrap(), None);

  let extended = held(scratch.db(), first).await.unwrap().unwrap();
  let heard = consumer.heard();
  let extend = &heard.last().unwrap().ask;

  assert_eq!((extended.amount_micros, extended.unit_micros), (2_000_000, 1_000_000));
  assert_eq!(
    (&extend["op"], &extend["spentUsd"], &extend["reservation"]),
    (&json!("extend"), &json!(0.9), &json!("r-first"))
  );

  // Refused, the run moves to the next platform source, what the first covered captured there.
  let mut moves = ground.events.subscribe::<SupplyMoved>();

  consumer.answers("extend", json!({ "accepted": false }));
  spend(&scratch, AGENT, since, 800_000).await;

  assert_eq!(look(&ground, &shared).await.unwrap(), None);
  assert_eq!(state_of(scratch.db(), first).await, ("captured".to_owned(), 1_700_000));

  let metered = shared.lock().await;
  let second = row_of(&metered.chosen);

  assert_eq!(metered.chosen.source.word(), "second");
  assert_eq!(state_of(scratch.db(), second).await, ("open".to_owned(), 0));
  assert_eq!(
    metered.offset_micros, 1_700_000,
    "the next source pays from the cost it took the run at"
  );
  assert_eq!(chosen_for(&ground.roots.data, AGENT).as_ref(), Some(&metered.chosen));

  let moved = moves.ready().expect("the move was published");

  assert_eq!(
    (moved.fact, moved.run.as_str(), moved.from.as_str(), moved.to.as_str(), moved.job),
    ("supply-moved", run.as_str(), "first", "second", Some(41))
  );
  assert_eq!(moved.reason, fill_marks(&SUPPLY.words.ran_out, &[("source", "first")]));
  assert!(moves.ready().is_none(), "the move is published once");

  // Each ask that moves money is keyed by the run and a step that rises with each.
  assert_eq!(
    consumer.keys(),
    ["1:reserve", "2:extend", "3:extend", "4:reserve", "5:capture", "6:release"]
      .map(|key| format!("{run}:{key}"))
  );

  drop(metered);
  consumer.down().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn with_no_source_left_the_meter_gives_one_unit_of_grace_then_says_the_run_ran_out() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let consumer = Consumer::stand().await;
  let chosen = reserved(&ground, &consumer, &[("plan", 1.0)]).await;
  let row = row_of(&chosen);
  let since = now_ms() - 1_000;
  let shared = meter_on(&chosen, since);

  consumer.answers("extend", json!({ "accepted": true, "amountUsd": 1.5 }));
  spend(&scratch, AGENT, since, 850_000).await;
  assert_eq!(look(&ground, &shared).await.unwrap(), None);

  // Refused with nothing after it, the run goes on for the unit it was reserved at past the whole amount.
  consumer.answers("extend", json!({ "accepted": false }));
  spend(&scratch, AGENT, since, 400_000).await;

  assert_eq!(look(&ground, &shared).await.unwrap(), None);
  assert_eq!(
    shared.lock().await.grace.as_ref().map(|(limit, _)| *limit),
    Some(2_500_000),
    "the amount, 1.5, and one unit, 1.0"
  );

  spend(&scratch, AGENT, since, 1_240_000).await;

  assert_eq!(look(&ground, &shared).await.unwrap(), None);

  spend(&scratch, AGENT, since, 10_000).await;

  let closed = look(&ground, &shared).await.unwrap().expect("the grace is spent");

  assert_eq!(
    (closed.source.as_str(), closed.reason, closed.reopens_at),
    ("plan", fill_marks(&SUPPLY.words.ran_out, &[("source", "plan")]), None)
  );
  assert_eq!(state_of(scratch.db(), row).await, ("open".to_owned(), 2_500_000));
  consumer.down().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_s_running_cost_is_read_off_the_status_line_in_its_session_s_own_place() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let since = now_ms() - 1_000;
  let status = |place: &std::path::Path, total: f64| {
    std::fs::create_dir_all(place).unwrap();
    let line = json!({ "at": since + 10, "payload": { "cost": { "total_cost_usd": total } } });
    let mut written = std::fs::read_to_string(place.join("status.jsonl")).unwrap_or_default();

    written.push_str(&format!("{line}\n"));
    std::fs::write(place.join("status.jsonl"), written).unwrap();
  };
  let reading = Reading::JobStatusLine {
    job: 41,
    worker: "u7-work".to_owned(),
    since,
  };

  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "item_agent", "run_name", "session_id", "room_id", "state", "started_at", "prompt")
         VALUES (41, 3, 9, 'harness', 10, 'u7-work', 'coder-01', 'job-41', 'u7-work@abc-1', 5, 'running', $1, 'work')"#,
      vec![since.into()],
    )
    .await;

  // With no thrown-away place standing yet, the checkout's own place is read.
  status(&ground.roots.place("u7-work"), 1.0);
  status(&ground.roots.place("u7-work"), 1.25);
  assert_eq!(reading.cost(&ground).await.unwrap(), 250_000);

  // Once the session's own place stands, its status line is the turn's.
  let apart = ground.roots.place(&agency_server::apart_places::place_name(&agent_session::SessionId(
    "u7-work@abc-1".to_owned(),
  )));

  status(&apart, 3.0);
  status(&apart, 3.5);
  assert_eq!(reading.cost(&ground).await.unwrap(), 500_000);
}
