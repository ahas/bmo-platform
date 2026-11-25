//! The point a supervisor's reset cuts it off at (docs/supervisors.md, 「What the reset cuts off」): a turn begun
//! at or before that point is overtaken, and nothing it does is kept. The point is kept in the store's settings
//! under the supervisor's name; a turn reads it under the hold its reset moves it under, so the reset never moves
//! the point between a turn's reading and its keeping.

use std::sync::Arc;

use sea_orm::ConnectionTrait;
use store::{one, run};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

use crate::queue::running::EXECUTION_CANCEL_PREFIX;
use crate::queue::settings;

pub use store::reset_point::{key as reset_point_key, overtakes};

/// The agent's reset point, where it was ever reset.
pub async fn point(db: &impl ConnectionTrait, agent: &str) -> Option<i64> {
  one(
    db,
    r#"SELECT "value" FROM "settings" WHERE "key" = $1"#,
    vec![reset_point_key(agent).into()],
  )
  .await
  .ok()
  .flatten()
  .and_then(|row| row.try_get::<String>("", "value").ok())
  .and_then(|value| value.trim().parse().ok())
}

/// Whether the agent's reset overtook a turn begun at `began`.
pub async fn overtaken(db: &impl ConnectionTrait, agent: &str, began: i64) -> bool {
  overtakes(point(db, agent).await, began)
}

/// What an overtaken run wrote under its own name taken back: its run log's lines, so the turn leaves no
/// record of itself, and the cancellation the reset stood over it.
pub async fn take_back(db: &impl ConnectionTrait, run_name: &str) {
  if let Err(fault) = run(db, r#"DELETE FROM "agent_log" WHERE "run" = $1"#, vec![run_name.into()]).await {
    eprintln!("run {run_name}: what it wrote after the reset overtook it was not taken back: {fault}");
  }

  let _ = settings::forget(db, &format!("{EXECUTION_CANCEL_PREFIX}{run_name}")).await;
}

/// The hold a turn keeps what it came to under and a reset moves the point under, for as long as the
/// process stands. Cloned freely: every clone is the one hold.
#[derive(Clone, Default)]
pub struct ResetHold(Arc<RwLock<()>>);

/// Held by a turn from its last reading of the reset point until what it came to is kept.
pub struct Keeping(#[allow(dead_code)] OwnedRwLockReadGuard<()>);

/// Held by a reset while it moves the point: had once no turn keeps, and holding off every turn after.
pub struct Cutting(#[allow(dead_code)] OwnedRwLockWriteGuard<()>);

impl ResetHold {
  /// The hold a turn keeps under.
  pub async fn keeping(&self) -> Keeping {
    Keeping(self.0.clone().read_owned().await)
  }

  /// The hold a reset moves the point under.
  pub async fn cutting(&self) -> Cutting {
    Cutting(self.0.clone().write_owned().await)
  }
}
