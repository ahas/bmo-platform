//! A person's run's platform reservations and their settlement (docs/supply.md, 「Reservations」): reserved before
//! the run goes out on a platform source, at the size the hook answers; extended as its cost nears the amount; and
//! captured with the actual cost and the rest released as it ends, through the supply outbox, one row an ask keyed
//! by its idempotency key, sent until the hook takes it. A reservation left open past its expiry while agency was
//! down is captured as the server starts, with what the store last recorded it covered.

use policy::supply::SUPPLY;
use sea_orm::ConnectionTrait;
use serde_json::json;
use store::{all, one, run};

use super::hook::{self, MoneyAsk, SupplyAsk};
use super::{Chosen, Source};
use crate::Refusal;
use crate::places::Ground;

/// Millionths of a US dollar in one.
const MICROS: f64 = 1_000_000.0;

/// An amount in millionths of a dollar.
pub fn micros(usd: f64) -> i64 {
  (usd * MICROS).round() as i64
}

/// An amount in dollars.
pub fn usd(micros: i64) -> f64 {
  micros as f64 / MICROS
}

fn key(run: &str, step: i64, op: &str) -> String {
  format!("{run}:{step}:{op}")
}

/// One reservation as the store keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
  pub id: i64,
  pub run: String,
  pub account: i64,
  pub source: String,
  pub reservation: String,
  pub amount_micros: i64,
  pub covered_micros: i64,
  pub expires_at: Option<i64>,
  pub last_step: i64,
  /// The amount the reservation was opened at: the unit a run that runs out goes on for once more.
  pub unit_micros: i64,
}

fn held_of(row: &sea_orm::QueryResult) -> Result<Held, Refusal> {
  Ok(Held {
    id: row.try_get("", "id")?,
    run: row.try_get("", "run")?,
    account: row.try_get("", "account")?,
    source: row.try_get("", "source")?,
    reservation: row.try_get("", "reservation")?,
    amount_micros: row.try_get("", "amount_micros")?,
    covered_micros: row.try_get("", "covered_micros")?,
    expires_at: row.try_get("", "expires_at")?,
    last_step: row.try_get("", "last_step")?,
    unit_micros: row.try_get("", "unit_micros")?,
  })
}

/// The reservation the store keeps under a row.
pub async fn held<C: ConnectionTrait>(db: &C, id: i64) -> Result<Option<Held>, Refusal> {
  one(db, r#"SELECT * FROM "supply_reservations" WHERE "id" = $1"#, vec![id.into()])
    .await?
    .as_ref()
    .map(held_of)
    .transpose()
}

/// A reservation asked of the hook for the run's source at its next step: the store's row where it was
/// accepted, or the moment the hook says the source reopens where it was refused. A reservation of an
/// organisation's own supply names the unit whose credential the run goes out on, which the gateway reads to let
/// the person's call key reach that unit's members (docs/own-llm.md, 「Personal members」).
pub async fn reserve(
  ground: &Ground,
  chosen: &mut Chosen,
  source: &str,
  unit: Option<i64>,
) -> Result<Result<i64, Option<i64>>, Refusal> {
  let Some(account) = chosen.account else { return Ok(Err(None)) };
  let step = chosen.step;

  chosen.step += 1;

  let answer = hook::money(
    &ground.hooks,
    SupplyAsk::Reserve(MoneyAsk {
      run: chosen.run.clone(),
      step,
      account,
      source: source.to_owned(),
      reservation: None,
      spent_usd: None,
      cost_usd: None,
      key: key(&chosen.run, step, "reserve"),
    }),
  )
  .await;
  // An accepted reservation names what it holds and how much; one that names neither cannot be metered, and is
  // taken as refused.
  let amount = answer.amount_usd.map(micros).filter(|amount| *amount > 0);
  let (Some(true), Some(reservation), Some(amount)) =
    (answer.accepted, answer.reservation.filter(|id| !id.is_empty()), amount)
  else {
    return Ok(Err(answer.reopens_at));
  };
  let row = one(
    &ground.db,
    r#"INSERT INTO "supply_reservations"
       ("run", "opened_step", "last_step", "account", "source", "reservation", "amount_micros", "unit_micros",
        "expires_at", "unit_id")
       VALUES ($1, $2, $2, $3, $4, $5, $6, $6, $7, $8) RETURNING "id""#,
    vec![
      chosen.run.clone().into(),
      step.into(),
      account.into(),
      source.into(),
      reservation.into(),
      amount.into(),
      answer.expires_at.into(),
      unit.into(),
    ],
  )
  .await?
  .ok_or_else(|| Refusal::failed("the store kept no reservation row"))?;

  Ok(Ok(row.try_get("", "id")?))
}

/// What a reservation has covered so far written down, the run's running cost as its meter read it.
pub async fn cover<C: ConnectionTrait>(db: &C, id: i64, covered_micros: i64) -> Result<(), Refusal> {
  run(
    db,
    r#"UPDATE "supply_reservations" SET "covered_micros" = GREATEST("covered_micros", $2) WHERE "id" = $1"#,
    vec![id.into(), covered_micros.into()],
  )
  .await?;

  Ok(())
}

/// A reservation extended at the run's next step: whether the hook accepted, its amount and expiry written where
/// it did.
pub async fn extend(
  ground: &Ground,
  chosen: &mut Chosen,
  held: &Held,
  spent_micros: i64,
) -> Result<bool, Refusal> {
  let step = chosen.step;

  chosen.step += 1;

  let answer = hook::money(
    &ground.hooks,
    SupplyAsk::Extend(MoneyAsk {
      run: held.run.clone(),
      step,
      account: held.account,
      source: held.source.clone(),
      reservation: Some(held.reservation.clone()),
      spent_usd: Some(usd(spent_micros)),
      cost_usd: None,
      key: key(&held.run, step, "extend"),
    }),
  )
  .await;

  if answer.accepted != Some(true) {
    return Ok(false);
  }

  let amount = answer.amount_usd.map(micros).unwrap_or(held.amount_micros).max(held.amount_micros);

  run(
    &ground.db,
    r#"UPDATE "supply_reservations" SET "amount_micros" = $2, "expires_at" = COALESCE($3, "expires_at"),
       "last_step" = $4 WHERE "id" = $1"#,
    vec![held.id.into(), amount.into(), answer.expires_at.into(), step.into()],
  )
  .await?;

  Ok(true)
}

/// A reservation settled: captured with what it cost and the rest released, each owed in the supply outbox and
/// tried at once; the row closed as captured.
pub async fn settle(
  ground: &Ground,
  chosen: &mut Chosen,
  held: &Held,
  cost_micros: i64,
) -> Result<(), Refusal> {
  let capture = chosen.step;
  let release = chosen.step + 1;

  chosen.step += 2;
  owe_settlement(ground, held, capture, release, cost_micros).await?;
  send_due(ground).await;

  Ok(())
}

// The capture and the release of a reservation written into the outbox and its row closed, in one transaction.
async fn owe_settlement(
  ground: &Ground,
  held: &Held,
  capture: i64,
  release: i64,
  cost_micros: i64,
) -> Result<(), Refusal> {
  let ask = |op: &str, step: i64, cost: Option<f64>| {
    let money = MoneyAsk {
      run: held.run.clone(),
      step,
      account: held.account,
      source: held.source.clone(),
      reservation: Some(held.reservation.clone()),
      spent_usd: None,
      cost_usd: cost,
      key: key(&held.run, step, op),
    };

    match op {
      "capture" => SupplyAsk::Capture(money),
      _ => SupplyAsk::Release(money),
    }
  };
  let now = crate::now_ms();
  let txn = sea_orm::TransactionTrait::begin(&ground.db).await?;

  for asked in [ask("capture", capture, Some(usd(cost_micros))), ask("release", release, None)] {
    let key = match &asked {
      SupplyAsk::Capture(money) | SupplyAsk::Release(money) => money.key.clone(),
      _ => unreachable!("a settlement is a capture or a release"),
    };

    run(
      &txn,
      r#"INSERT INTO "supply_settlements" ("key", "ask", "owed_at", "due_at") VALUES ($1, $2, $3, $3)
         ON CONFLICT ("key") DO NOTHING"#,
      vec![key.into(), json!(asked).to_string().into(), now.into()],
    )
    .await?;
  }

  run(
    &txn,
    r#"UPDATE "supply_reservations" SET "state" = 'captured', "covered_micros" = $2, "last_step" = $3
       WHERE "id" = $1 AND "state" = 'open'"#,
    vec![held.id.into(), cost_micros.into(), release.into()],
  )
  .await?;
  txn.commit().await?;

  Ok(())
}

/// Every settlement due tried once, in the order owed: a row the hook takes is let go, one it does not is tried
/// again after a wait that doubles up to the policy's ceiling. Answers how many were taken.
pub async fn send_due(ground: &Ground) -> usize {
  let now = crate::now_ms();
  let Ok(rows) = all(
    &ground.db,
    r#"SELECT "key", "ask", "tries" FROM "supply_settlements" WHERE "due_at" <= $1 ORDER BY "owed_at", "key""#,
    vec![now.into()],
  )
  .await
  else {
    return 0;
  };
  let mut taken = 0;

  for row in rows {
    let (Ok(key), Ok(text), Ok(tries)) = (
      row.try_get::<String>("", "key"),
      row.try_get::<String>("", "ask"),
      row.try_get::<i64>("", "tries"),
    ) else {
      continue;
    };
    let answer = match serde_json::from_str::<SupplyAsk>(&text) {
      Ok(ask) if hook::registered(&ground.hooks) => Some(hook::money(&ground.hooks, ask).await),
      Ok(_) => None,
      Err(why) => {
        eprintln!("agency: supply settlement {key} does not read: {why}");
        None
      }
    };

    if answer.as_ref().and_then(|answer| answer.taken) == Some(true) {
      if let Err(why) = run(
        &ground.db,
        r#"DELETE FROM "supply_settlements" WHERE "key" = $1"#,
        vec![key.clone().into()],
      )
      .await
      {
        eprintln!("agency: supply settlement {key} was taken and not let go: {why}");
      }

      taken += 1;
      continue;
    }

    let wait = (SUPPLY.retry_first_seconds << tries.clamp(0, 20)).min(SUPPLY.retry_ceiling_seconds) * 1000;
    let failure = if answer.is_some() { "the hook did not take it" } else { "no supply hook answers" };
    if let Err(why) = run(
      &ground.db,
      r#"UPDATE "supply_settlements" SET "tries" = "tries" + 1, "due_at" = $2, "failure" = $3 WHERE "key" = $1"#,
      vec![key.clone().into(), (now + wait).into(), failure.into()],
    )
    .await
    {
      eprintln!("agency: supply settlement {key}'s try was not written: {why}");
    }
  }

  taken
}

/// Every reservation left open past its expiry, captured with what the store last recorded it covered, as the
/// server starts. Answers how many were owed.
pub async fn settle_expired(ground: &Ground) -> Result<usize, Refusal> {
  let rows = all(
    &ground.db,
    r#"SELECT * FROM "supply_reservations" WHERE "state" = 'open' AND "expires_at" IS NOT NULL AND "expires_at" < $1"#,
    vec![crate::now_ms().into()],
  )
  .await?;
  let mut owed = 0;

  for row in &rows {
    let held = held_of(row)?;

    owe_settlement(ground, &held, held.last_step + 1, held.last_step + 2, held.covered_micros).await?;
    owed += 1;
  }

  Ok(owed)
}

/// Every reservation still open that belongs to an account, or that names an organisation unit's supply, captured
/// with what the store last recorded it covered, as an expired one is: what an owner removed for good leaves open
/// (docs/api.md, 「owners and organisation units removed for good」). Answers how many were owed.
pub async fn settle_open_of(ground: &Ground, owner: Owner) -> Result<usize, Refusal> {
  let (column, id) = match owner {
    Owner::Account(id) => ("account", id),
    Owner::Unit(id) => ("unit_id", id),
  };
  let rows = all(
    &ground.db,
    &format!(r#"SELECT * FROM "supply_reservations" WHERE "state" = 'open' AND "{column}" = $1"#),
    vec![id.into()],
  )
  .await?;

  for row in &rows {
    let held = held_of(row)?;

    owe_settlement(ground, &held, held.last_step + 1, held.last_step + 2, held.covered_micros).await?;
  }

  Ok(rows.len())
}

/// Whose reservations are meant: an account's own, or those naming an organisation unit's supply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
  Account(i64),
  Unit(i64),
}

/// The settlement outbox and the expired reservations kept: those expired captured once as the server starts,
/// then every settlement due sent every few seconds.
pub async fn keep(ground: std::sync::Arc<Ground>) {
  match settle_expired(&ground).await {
    Ok(0) => {}
    Ok(owed) => println!("agency: {owed} expired supply reservations are captured from the store"),
    Err(why) => eprintln!("agency: the expired supply reservations: {why}"),
  }

  loop {
    send_due(&ground).await;
    tokio::time::sleep(std::time::Duration::from_secs(SUPPLY.retry_first_seconds.max(1) as u64)).await;
  }
}

/// The reservation a run's source holds now, where it holds one.
pub async fn of_source(ground: &Ground, source: &Source) -> Result<Option<Held>, Refusal> {
  match source.reservation() {
    Some(reservation) => held(&ground.db, reservation).await,
    None => Ok(None),
  }
}
