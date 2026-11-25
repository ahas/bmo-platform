//! The keeping of the accounts' usage windows (docs/usage-windows.md): each slot's last reading in the
//! store, replaced whole by the next, and read back with how long its reader says it stands and the
//! account's unread mark; and beside them each paid slot's last balance.

use std::sync::Arc;
use std::time::Duration;

use sea_orm::{DatabaseConnection, QueryResult, TransactionTrait};
use serde::{Deserialize, Serialize};
use store::{Fault, all, one, run};

use crate::balance::{Balance, KeptBalance, Requests};
use crate::reader::Reader;

// The settings family an account's unread mark stands in, by the account (docs/store.md, 「The settings
// table」).
const UNREAD: &str = "usage_windows_unread";

/// Whose windows a reading is: the provider it was read of, and the login under it, or the empty account
/// on a provider that holds no logins of agency's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot<'a> {
  pub mode: &'a str,
  pub account: &'a str,
}

/// One window of an account's limit as a reading states it: the fraction used, when it resets in epoch
/// seconds, and how long it runs and what its provider calls its limit where the provider says.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
  pub name: String,
  pub utilization: f64,
  pub resets_at: Option<i64>,
  pub duration_minutes: Option<i64>,
  pub limit_name: Option<String>,
}

/// The used fraction at which a window is full: all of it used, and no share short of it.
pub const FULL: f64 = 1.0;

impl Window {
  /// Whether the reading finds the window full.
  pub fn is_full(&self) -> bool {
    self.utilization >= FULL
  }
}

/// A window as the keeping holds it: the window, and when the reading it came in was seen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeptWindow {
  #[serde(flatten)]
  pub window: Window,
  pub seen_at: i64,
}

/// A slot's last reading as it is read back: its windows, how long a reading of its provider stands, and
/// the moment its account was marked unread, where it was.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reading {
  pub windows: Vec<KeptWindow>,
  pub fresh: Duration,
  pub unread_at: Option<i64>,
}

impl Reading {
  /// Whether a window was seen by a reading that stands at `now`, in epoch seconds: younger than its
  /// reader's span and later than the account's unread mark.
  pub fn stands(&self, window: &KeptWindow, now: i64) -> bool {
    let fresh_after = now - i64::try_from(self.fresh.as_secs()).unwrap_or(i64::MAX);

    window.seen_at >= fresh_after && self.unread_at.is_none_or(|unread| window.seen_at > unread)
  }

  /// The windows seen by a reading that stands at `now`.
  pub fn standing(&self, now: i64) -> impl Iterator<Item = &KeptWindow> {
    self.windows.iter().filter(move |window| self.stands(window, now))
  }

  /// The window of a name, standing or not.
  pub fn named(&self, name: &str) -> Option<&KeptWindow> {
    self.windows.iter().find(|kept| kept.window.name == name)
  }
}

/// The keeping: the store the windows stand in, and the readers that say how long each provider's reading
/// stands.
#[derive(Clone)]
pub struct Keeping {
  db: DatabaseConnection,
  readers: Vec<Arc<dyn Reader>>,
}

impl Keeping {
  pub fn new(db: DatabaseConnection, readers: Vec<Arc<dyn Reader>>) -> Self {
    Self { db, readers }
  }

  /// How long a reading of a provider's windows stands: what its reader declares, and nothing where no
  /// reader reads that provider.
  pub fn fresh(&self, mode: &str) -> Duration {
    self
      .readers
      .iter()
      .find(|reader| reader.mode() == mode)
      .map_or(Duration::ZERO, |reader| reader.fresh())
  }

  /// The readers the keeping knows, for the clocks that run them.
  pub fn readers(&self) -> &[Arc<dyn Reader>] {
    &self.readers
  }

  /// Every window the slot holds replaced by `windows`, seen at `seen_at` in epoch seconds, in one
  /// transaction.
  pub async fn replace(&self, slot: Slot<'_>, windows: &[Window], seen_at: i64) -> Result<(), Fault> {
    let transaction = self.db.begin().await?;

    run(
      &transaction,
      r#"DELETE FROM "usage_windows" WHERE "mode" = $1 AND "account" = $2"#,
      vec![slot.mode.into(), slot.account.into()],
    )
    .await?;

    for window in windows {
      run(
        &transaction,
        r#"INSERT INTO "usage_windows"
             ("mode", "account", "name", "utilization", "resets_at", "seen_at", "duration_minutes", "limit_name")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           ON CONFLICT ("mode", "account", "name") DO UPDATE SET
             "utilization" = excluded."utilization",
             "resets_at" = excluded."resets_at",
             "seen_at" = excluded."seen_at",
             "duration_minutes" = excluded."duration_minutes",
             "limit_name" = excluded."limit_name""#,
        vec![
          slot.mode.into(),
          slot.account.into(),
          window.name.clone().into(),
          window.utilization.into(),
          window.resets_at.into(),
          seen_at.into(),
          window.duration_minutes.into(),
          window.limit_name.clone().into(),
        ],
      )
      .await?;
    }

    transaction.commit().await?;

    Ok(())
  }

  /// The slot's last reading, its windows in name order, with its provider's freshness and its account's
  /// unread mark.
  pub async fn read(&self, slot: Slot<'_>) -> Result<Reading, Fault> {
    let rows = all(
      &self.db,
      r#"SELECT "name", "utilization", "resets_at", "seen_at", "duration_minutes", "limit_name"
         FROM "usage_windows" WHERE "mode" = $1 AND "account" = $2 ORDER BY "name""#,
      vec![slot.mode.into(), slot.account.into()],
    )
    .await?;
    let windows = rows.iter().map(kept).collect::<Result<_, _>>()?;

    Ok(Reading {
      windows,
      fresh: self.fresh(slot.mode),
      unread_at: self.unread_at(slot.account).await?,
    })
  }

  /// The moment an account was first found unreadable, where a reading has not lifted it since.
  pub async fn unread_at(&self, account: &str) -> Result<Option<i64>, Fault> {
    let row = one(
      &self.db,
      r#"SELECT "value" FROM "settings" WHERE "key" = $1"#,
      vec![unread_key(account).into()],
    )
    .await?;

    Ok(
      row
        .map(|row| row.try_get::<String>("", "value"))
        .transpose()
        .map_err(|why| Fault::new(format!("the unread mark of {account}: {why}")))?
        .and_then(|value| value.trim().parse().ok()),
    )
  }

  /// The account marked unread at `at`, in epoch seconds, unless a mark already stands: the mark keeps the
  /// moment it was first found so.
  pub async fn mark_unread(&self, account: &str, at: i64) -> Result<(), Fault> {
    run(
      &self.db,
      r#"INSERT INTO "settings" ("key", "value") VALUES ($1, $2) ON CONFLICT ("key") DO NOTHING"#,
      vec![unread_key(account).into(), at.to_string().into()],
    )
    .await?;

    Ok(())
  }

  /// The account's unread mark lifted, where one stands.
  pub async fn lift_unread(&self, account: &str) -> Result<(), Fault> {
    run(&self.db, r#"DELETE FROM "settings" WHERE "key" = $1"#, vec![unread_key(account).into()]).await?;

    Ok(())
  }
}

impl Keeping {
  /// The slot's balance replaced by `balance`, seen at `seen_at` in epoch seconds (docs/usage-windows.md,
  /// 「Balances」).
  pub async fn replace_balance(
    &self,
    slot: Slot<'_>,
    balance: &Balance,
    seen_at: i64,
  ) -> Result<(), Fault> {
    run(
      &self.db,
      r#"INSERT INTO "usage_balances"
           ("mode", "account", "amount", "currency", "seen_at", "requests_used", "requests_limit", "requests_left",
            "requests_scope")
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT ("mode", "account") DO UPDATE SET
           "amount" = excluded."amount",
           "currency" = excluded."currency",
           "seen_at" = excluded."seen_at",
           "requests_used" = excluded."requests_used",
           "requests_limit" = excluded."requests_limit",
           "requests_left" = excluded."requests_left",
           "requests_scope" = excluded."requests_scope""#,
      vec![
        slot.mode.into(),
        slot.account.into(),
        balance.amount.into(),
        balance.currency.clone().into(),
        seen_at.into(),
        balance.requests.as_ref().map(|requests| requests.used).into(),
        balance.requests.as_ref().map(|requests| requests.limit).into(),
        balance.requests.as_ref().map(|requests| requests.left).into(),
        balance.requests.as_ref().and_then(|requests| requests.scope.clone()).into(),
      ],
    )
    .await?;

    Ok(())
  }

  /// The slot's last balance, or none where none was read.
  pub async fn balance(&self, slot: Slot<'_>) -> Result<Option<KeptBalance>, Fault> {
    one(
      &self.db,
      r#"SELECT "amount", "currency", "seen_at", "requests_used", "requests_limit", "requests_left",
                "requests_scope"
         FROM "usage_balances" WHERE "mode" = $1 AND "account" = $2"#,
      vec![slot.mode.into(), slot.account.into()],
    )
    .await?
    .as_ref()
    .map(kept_balance)
    .transpose()
  }

  /// An account's last balance, whichever provider read it: an account's endpoint is one provider's, so
  /// its balance stands under one. None where none was read, and none for the empty account.
  pub async fn balance_of_account(&self, account: &str) -> Result<Option<KeptBalance>, Fault> {
    if account.is_empty() {
      return Ok(None);
    }

    one(
      &self.db,
      r#"SELECT "amount", "currency", "seen_at", "requests_used", "requests_limit", "requests_left",
                "requests_scope"
         FROM "usage_balances" WHERE "account" = $1
         ORDER BY "seen_at" DESC LIMIT 1"#,
      vec![account.into()],
    )
    .await?
    .as_ref()
    .map(kept_balance)
    .transpose()
  }
}

fn kept_balance(row: &QueryResult) -> Result<KeptBalance, Fault> {
  let field = |why: sea_orm::DbErr| Fault::new(format!("a kept balance: {why}"));

  let figure = |name: &str| row.try_get::<Option<i64>>("", name);
  let requests = match (figure("requests_used"), figure("requests_limit"), figure("requests_left")) {
    (Ok(Some(used)), Ok(Some(limit)), Ok(Some(left))) => Some(Requests {
      used,
      limit,
      left,
      scope: row.try_get::<Option<String>>("", "requests_scope").ok().flatten(),
    }),
    _ => None,
  };

  Ok(KeptBalance {
    balance: Balance {
      amount: row.try_get("", "amount").map_err(field)?,
      currency: row.try_get("", "currency").map_err(field)?,
      requests,
    },
    seen_at: row.try_get("", "seen_at").map_err(field)?,
  })
}

fn unread_key(account: &str) -> String {
  format!("{UNREAD}:{account}")
}

fn kept(row: &QueryResult) -> Result<KeptWindow, Fault> {
  let field = |why: sea_orm::DbErr| Fault::new(format!("a kept usage window: {why}"));

  Ok(KeptWindow {
    window: Window {
      name: row.try_get("", "name").map_err(field)?,
      utilization: row.try_get("", "utilization").map_err(field)?,
      resets_at: row.try_get("", "resets_at").map_err(field)?,
      duration_minutes: row.try_get("", "duration_minutes").map_err(field)?,
      limit_name: row.try_get("", "limit_name").map_err(field)?,
    },
    seen_at: row.try_get("", "seen_at").map_err(field)?,
  })
}
