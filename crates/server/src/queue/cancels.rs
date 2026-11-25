//! A stopped run's cancellation taken back once it is gone (docs/agent-control.md, 「What a cancellation
//! reaches, and how long it stands」). A marker is the signal that ends one run, never a verdict about a
//! name for ever: while it stands every launch under that name is refused, so it is dropped once the
//! work it was raised over has ended — the name asked for and every execution that name owns.

use std::collections::BTreeSet;
use std::time::Duration;

use sea_orm::DatabaseConnection;
use serde_json::Value;

use store::all;

use super::running::EXECUTION_CANCEL_PREFIX;
use super::settings;
use crate::Refusal;

/// The names one cancellation covers: the one asked for and every execution it owns — its own
/// descendants, and anything rooted in it. Never a root, which every run of the same place shares.
pub fn covered(rows: &[Value], parent: &str) -> BTreeSet<String> {
  let text = |row: &Value, key: &str| row.get(key).and_then(Value::as_str).map(str::to_owned);
  let mut owned = BTreeSet::from([parent.to_owned()]);
  let mut changed = true;

  while changed {
    changed = false;

    for row in rows {
      let Some(id) = text(row, "id") else { continue };
      let rooted = text(row, "root").as_deref() == Some(parent);
      let child = text(row, "parent").is_some_and(|of| owned.contains(&of));

      if (rooted || child) && owned.insert(id) {
        changed = true;
      }
    }
  }

  owned
}

/// Every execution's body, the latest first.
pub async fn executions(db: &DatabaseConnection) -> Result<Vec<Value>, Refusal> {
  Ok(
    all(db, r#"SELECT "body" FROM "agent_executions" ORDER BY rowid DESC"#, vec![])
      .await?
      .iter()
      .filter_map(|row| row.try_get::<String>("", "body").ok())
      .filter_map(|body| serde_json::from_str(&body).ok())
      .collect(),
  )
}

/// Every name a standing cancellation covers among these executions: each name a marker stands over,
/// and every execution that name owns.
pub async fn standing(db: &DatabaseConnection, rows: &[Value]) -> Result<BTreeSet<String>, Refusal> {
  let marked = settings::named(db, EXECUTION_CANCEL_PREFIX).await?;

  Ok(marked.iter().flat_map(|name| covered(rows, name)).collect())
}

/// Whether a cancellation stands over this name itself.
pub async fn standing_named(db: &DatabaseConnection, name: &str) -> Result<bool, Refusal> {
  Ok(settings::get(db, &format!("{EXECUTION_CANCEL_PREFIX}{name}")).await?.is_some())
}

/// How long a marker stands before the reconciliation's sweep takes it, whether or not anybody took it
/// back: longer than any stopped run takes to wind down.
pub const MARKER_GRACE: Duration = Duration::from_secs(60 * 60);

/// Every marker raised before `now` less the grace taken, the floor under the take-back; how many were. A
/// marker's value is the moment it was raised, in milliseconds.
pub async fn swept(db: &DatabaseConnection, now: i64) -> Result<u64, Refusal> {
  let raised_before = now - i64::try_from(MARKER_GRACE.as_millis()).unwrap_or(i64::MAX);
  let taken = all(
    db,
    r#"DELETE FROM "settings"
       WHERE "key" LIKE $1 ESCAPE '\'
         AND CASE WHEN "value" ~ '^[0-9]{1,18}$' THEN "value"::bigint END < $2
       RETURNING "key""#,
    vec![format!("{}%", EXECUTION_CANCEL_PREFIX.replace('_', "\\_")).into(), raised_before.into()],
  )
  .await?;

  Ok(taken.len() as u64)
}

/// Every marker the cancellation of this name raised taken back; the names actually dropped.
pub async fn take_back(db: &DatabaseConnection, parent: &str) -> Result<Vec<String>, Refusal> {
  let rows = executions(db).await?;
  let mut dropped = vec![];

  for name in covered(&rows, parent) {
    if settings::take(db, &format!("{EXECUTION_CANCEL_PREFIX}{name}")).await?.is_some() {
      dropped.push(name);
    }
  }

  Ok(dropped)
}
