//! The connections a process opens on the store: a pool for the service and its libraries, and one
//! read-only connection for a tool or a host handed the store's address.

use std::future::Future;
use std::str::FromStr;
use std::time::Duration;

use sea_orm::{DatabaseConnection, SqlxPostgresConnector};
use sqlx::any::{AnyRow, install_default_drivers};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AnyConnection, ConnectOptions, Connection, Row};

use crate::statements::opened_before_move;

/// A pool of at most `max` connections on the store, which never hands out a connection opened before
/// the store's shape last moved under this process.
pub fn pool(max: u32) -> PgPoolOptions {
  PgPoolOptions::new()
    .max_connections(max)
    .before_acquire(|_, meta| Box::pin(async move { Ok(!opened_before_move(meta.age)) }))
}

fn options(address: &str) -> Result<PgConnectOptions, sqlx::Error> {
  Ok(PgConnectOptions::from_str(address)?.disable_statement_logging())
}

/// The store at a PostgreSQL address, through a pool `pool` built.
pub async fn connect(address: &str, pool: PgPoolOptions) -> Result<DatabaseConnection, sqlx::Error> {
  let pool = pool.connect_with(options(address)?).await?;

  Ok(SqlxPostgresConnector::from_sqlx_postgres_pool(pool))
}

/// The same, connecting nothing until the first statement asks.
pub fn connect_lazy(address: &str, pool: PgPoolOptions) -> Result<DatabaseConnection, sqlx::Error> {
  Ok(SqlxPostgresConnector::from_sqlx_postgres_pool(
    pool.connect_lazy_with(options(address)?),
  ))
}

// The server's own codes for going down, not being up yet, or having no connection left to give.
const GONE_STATES: [&str; 4] = ["57P01", "57P02", "57P03", "53300"];

/// Whether an error is the database server going away rather than the statement's own fault: the
/// connection refused, absent or cut, a connection exception, or the server's codes for a restart.
pub fn server_went_away(error: &sqlx::Error) -> bool {
  match error {
    sqlx::Error::Io(_) => true,
    sqlx::Error::Database(fault) => fault
      .code()
      .is_some_and(|code| code.starts_with("08") || GONE_STATES.contains(&code.as_ref())),
    _ => false,
  }
}

// The pause between attempts doubles from the first to the last and stays there.
const FIRST_PAUSE_MS: u64 = 100;
const LAST_PAUSE_MS: u64 = 2_000;

/// Work run again whole wherever the server went away under it, with a doubling pause between
/// attempts, until the server answers it: a restart is waited out however long it takes. Every other
/// error is answered as it came. The work opens its own connection, so each attempt stands on a fresh
/// one.
pub async fn retrying<T, F, Fut>(mut work: F) -> Result<T, sqlx::Error>
where
  F: FnMut() -> Fut,
  Fut: Future<Output = Result<T, sqlx::Error>>,
{
  let mut pause = Duration::from_millis(FIRST_PAUSE_MS);
  let mut attempt = 1;

  loop {
    match work().await {
      Err(error) if server_went_away(&error) => {
        attempt += 1;
        eprintln!(
          "store: the database went away ({error}); running the work again in {} ms, attempt {attempt}",
          pause.as_millis()
        );
        tokio::time::sleep(pause).await;
        pause = (pause * 2).min(Duration::from_millis(LAST_PAUSE_MS));
      }
      answer => return answer,
    }
  }
}

/// A connection that reads the store at a PostgreSQL address and never writes it: every transaction
/// on it is read-only. A statement read this way numbers its placeholders `$1`, `$2`.
pub async fn open_read(address: &str) -> Result<AnyConnection, sqlx::Error> {
  install_default_drivers();

  retrying(|| async {
    let mut store = AnyConnection::connect(address).await?;

    sqlx::query("SET default_transaction_read_only = on").execute(&mut store).await?;

    Ok(store)
  })
  .await
}

/// A column read as a number whichever way it was stored: a timestamp written as an integer by one
/// writer and as a real by another reads the same.
pub fn number(row: &AnyRow, column: &str) -> Result<Option<f64>, sqlx::Error> {
  match row.try_get::<Option<i64>, _>(column) {
    Ok(value) => Ok(value.map(|n| n as f64)),
    Err(_) => row.try_get::<Option<f64>, _>(column),
  }
}
