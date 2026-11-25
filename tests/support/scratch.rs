//! An empty store of a test's own as a database of its own: the store migrated once per test binary into a
//! template database, and each test handed a copy of it, which the server makes in a fraction of the time a
//! migration takes. A copy is dropped when its test is done, and the test fails if it cannot be. Before a
//! binary makes its first database, the templates and copies a binary that is gone left behind are dropped.
//!
//! The modules that need a store each use what they need of it, so what none of them uses is no fault.

#![allow(dead_code)]

use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement, Value};
use sqlx::postgres::{PgConnectOptions, PgConnection};
use sqlx::{Connection as _, Executor};

/// What a template's name begins with, followed by the process that made it.
const TEMPLATE: &str = "agency_rs_tmpl_";
/// What a copy's name begins with, followed by the process that made it and a count.
const COPY: &str = "agency_rs_copy_";
/// How many times a drop ends the database's sessions and tries again before it gives up.
const DROP_ATTEMPTS: usize = 3;

static COPIES: AtomicU32 = AtomicU32::new(0);
static SWEPT: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
static MADE: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();

/// The suite's PostgreSQL server, as the run names it, and the machine's own socket where nothing does.
pub fn test_address() -> String {
  std::env::var("BMO_TEST_POSTGRES")
    .unwrap_or_else(|_| "postgres:///bmo_test?host=/var/run/postgresql".into())
}

/// The test server's address with another database in place of its own.
pub fn address_of(database: &str) -> String {
  let address = test_address();
  let named = PgConnectOptions::from_str(&address)
    .unwrap()
    .get_database()
    .unwrap_or("bmo_test")
    .to_owned();
  let (base, query) = address
    .split_once('?')
    .map_or((address.as_str(), None), |(base, query)| (base, Some(query)));
  let base = base.strip_suffix(&named).unwrap_or(base);

  match query {
    Some(query) => format!("{base}{database}?{query}"),
    None => format!("{base}{database}"),
  }
}

async fn server() -> Result<PgConnection, sqlx::Error> {
  PgConnection::connect_with(&PgConnectOptions::from_str(&address_of("postgres"))?).await
}

/// The databases a dead process of this harness left behind dropped, once per binary and before it makes
/// any of its own.
async fn swept() {
  SWEPT.get_or_init(sweep).await;
}

/// Drops every template and copy a dead process of this harness left. A database is left alone while its
/// process runs on this machine or any session is on it, so a run of another checkout against the same
/// server keeps its own, and a name of another repository's harness is never read. A database the sweep
/// cannot drop is named on the error stream and left for the next run.
async fn sweep() {
  let mut admin = match server().await {
    Ok(admin) => admin,
    Err(error) => {
      eprintln!("the scratch sweep reaches no test server: {error}");
      return;
    }
  };
  let unattended: Vec<String> = match sqlx::query_scalar(
    "SELECT datname::text FROM pg_database d \
     WHERE NOT EXISTS (SELECT 1 FROM pg_stat_activity a WHERE a.datname = d.datname)",
  )
  .fetch_all(&mut admin)
  .await
  {
    Ok(names) => names,
    Err(error) => {
      eprintln!("the scratch sweep lists no databases: {error}");
      Vec::new()
    }
  };

  for name in unattended {
    if process_of(&name).is_none_or(lives) {
      continue;
    }

    if let Err(error) = drop_database(&mut admin, &name).await {
      eprintln!("the scratch sweep leaves {name}: {error}");
    }
  }

  let _ = admin.close().await;
}

/// The process id a template's or a copy's name carries, if the name is one of this harness's: its prefix
/// followed by the id alone or by the id and a count.
fn process_of(name: &str) -> Option<u32> {
  let rest = name.strip_prefix(TEMPLATE).or_else(|| name.strip_prefix(COPY))?;
  let (process, count) = rest.split_once('_').unwrap_or((rest, "0"));

  count.parse::<u32>().ok()?;
  process.parse().ok()
}

/// Whether process `id`, other than this one, is running on this machine. Where the process table cannot
/// be read every process counts as alive, so nothing is dropped on a guess.
fn lives(id: u32) -> bool {
  let table = std::path::Path::new("/proc");

  id != std::process::id() && (!table.join("self").exists() || table.join(id.to_string()).exists())
}

// The template this binary's copies are made from, migrated the first time a test asks.
async fn template() -> String {
  MADE
    .get_or_init(|| async {
      let mut admin = server().await.unwrap();
      let name = format!("{TEMPLATE}{}", std::process::id());

      admin
        .execute(
          format!(
            "CREATE DATABASE \"{name}\" TEMPLATE template0 ENCODING 'UTF8' LC_COLLATE 'C' LC_CTYPE 'C'"
          )
          .as_str(),
        )
        .await
        .unwrap();
      admin.close().await.unwrap();

      let db = connect(&name).await;

      store::migration::up(&db).await.unwrap();
      db.close().await.unwrap();

      name
    })
    .await
    .clone()
}

/// A connection to a database of the test server.
pub async fn connect(database: &str) -> DatabaseConnection {
  let mut options = ConnectOptions::new(address_of(database));

  options.sqlx_logging(false).max_connections(4);

  Database::connect(options).await.unwrap()
}

/// A new database of this binary's, made from `template`; its name.
async fn made_from(template: &str) -> String {
  let database = format!("{COPY}{}_{}", std::process::id(), COPIES.fetch_add(1, Ordering::SeqCst));
  let mut admin = server().await.unwrap();

  admin
    .execute(format!("CREATE DATABASE \"{database}\" TEMPLATE {template}").as_str())
    .await
    .unwrap();
  admin.close().await.unwrap();

  database
}

/// A new copy of this binary's migrated, empty store; its database's name.
async fn copy() -> String {
  swept().await;

  let template = template().await;

  made_from(&format!("\"{template}\"")).await
}

/// A new database holding nothing, not even the store's migrations; its name.
async fn empty_database() -> String {
  swept().await;
  made_from("template0 ENCODING 'UTF8' LC_COLLATE 'C' LC_CTYPE 'C'").await
}

/// Drops `database`. It first takes no new connection, since what a test started may still be running on its
/// runtime's other threads, a pool among it opening a session again as each is ended. This role's own sessions
/// on it are then ended, since a drop that forces every session off needs the right to end other roles'
/// processes, an autovacuum worker's among them, which the suite's role need not hold; the plain drop then stops
/// an autovacuum worker itself and waits a few seconds for the ended sessions to leave. A session that stays past
/// that is ended again on the next attempt.
async fn drop_database(admin: &mut PgConnection, database: &str) -> Result<(), sqlx::Error> {
  let mut attempt = 1;

  // A database already gone refuses the change, and the drop below reads it as dropped.
  let _ = admin
    .execute(format!("ALTER DATABASE \"{database}\" WITH ALLOW_CONNECTIONS false").as_str())
    .await;

  loop {
    sqlx::query(
      "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
       WHERE datname = $1 AND pid <> pg_backend_pid() AND usename = current_user",
    )
    .bind(database)
    .execute(&mut *admin)
    .await?;

    match admin.execute(format!("DROP DATABASE IF EXISTS \"{database}\"").as_str()).await {
      Ok(_) => return Ok(()),
      Err(error) if attempt == DROP_ATTEMPTS => return Err(error),
      Err(_) => attempt += 1,
    }
  }
}

/// A copy dropped, its connections ended with it; what kept it standing where it could not be. It goes on a
/// runtime of its own on a thread of its own, since a guard dropped inside a test's runtime may not block on
/// it.
fn drop_copy(database: String) -> Result<(), String> {
  std::thread::spawn(move || {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

    runtime.block_on(async move {
      let mut admin = server().await.map_err(|error| format!("{database}: {error}"))?;
      let dropped = drop_database(&mut admin, &database)
        .await
        .map_err(|error| format!("{database}: {error}"));
      let _ = admin.close().await;

      dropped
    })
  })
  .join()
  .unwrap_or_else(|_| Err("the drop's thread panicked".into()))
}

/// A migrated, empty store of the test's own, dropped when the guard goes.
pub struct Scratch {
  database: String,
  db: DatabaseConnection,
}

impl Scratch {
  pub async fn new() -> Scratch {
    let database = copy().await;
    let db = connect(&database).await;

    Scratch { database, db }
  }

  /// A database of the test's own that no migration has run in, dropped when the guard goes.
  pub async fn blank() -> Scratch {
    let database = empty_database().await;
    let db = connect(&database).await;

    Scratch { database, db }
  }

  /// The store's address, as a reader handed it opens it.
  pub fn address(&self) -> String {
    address_of(&self.database)
  }

  pub fn db(&self) -> &DatabaseConnection {
    &self.db
  }

  /// A statement run as written.
  pub async fn run(&self, sql: &str, values: Vec<Value>) {
    self
      .db
      .execute(Statement::from_sql_and_values(DbBackend::Postgres, sql, values))
      .await
      .unwrap();
  }

  /// A count a statement answers.
  pub async fn count(&self, sql: &str, values: Vec<Value>) -> i64 {
    let row = self
      .db
      .query_one(Statement::from_sql_and_values(DbBackend::Postgres, sql, values))
      .await
      .unwrap()
      .unwrap();

    row.try_get_by_index::<i64>(0).unwrap()
  }
}

impl Drop for Scratch {
  fn drop(&mut self) {
    if let Err(error) = drop_copy(std::mem::take(&mut self.database))
      && !std::thread::panicking()
    {
      panic!("the scratch database is dropped: {error}");
    }
  }
}
