//! The test server's connections as one test binary shares them out among the agency servers its tests
//! stand. The test server takes a fixed number of clients, and every agency server holds a pool of
//! `STORE_CONNECTIONS` of them beside the scratch store's own, so as many servers as the machine runs tests
//! side by side would ask more than the test server takes, and the test that asks past it is refused.
//!
//! A server stands on a seat: this binary holds at most as many seats as half the test server's ordinary
//! connections give a server's pool and its slack, and a test whose server finds none free waits for one
//! to be given back. The other half is left to the scratch stores, which the number of tests running at
//! once bounds, and to another run on the same test server.

#![allow(dead_code)]

use std::str::FromStr;
use std::sync::{Condvar, LazyLock, Mutex};

use sqlx::Connection as _;
use sqlx::postgres::{PgConnectOptions, PgConnection};

use crate::support::scratch::address_of;

// What one server takes beyond its pool: the commands and tools it starts that open a connection of their
// own while it stands.
const SLACK: u32 = 2;

struct Seats {
  free: Mutex<u32>,
  given_back: Condvar,
}

static SEATS: LazyLock<Seats> = LazyLock::new(|| Seats {
  free: Mutex::new(seats()),
  given_back: Condvar::new(),
});

// The connections the test server takes from ordinary clients, read off its settings. It goes on a thread
// of its own, since the first test to ask may be running inside a runtime.
fn ordinary_connections() -> u32 {
  std::thread::spawn(|| {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

    runtime.block_on(async {
      let options = PgConnectOptions::from_str(&address_of("postgres")).unwrap();
      let mut admin = PgConnection::connect_with(&options).await.unwrap();
      let (most, reserved): (i32, i32) = sqlx::query_as(
        "SELECT current_setting('max_connections')::int, current_setting('superuser_reserved_connections')::int",
      )
      .fetch_one(&mut admin)
      .await
      .unwrap();

      admin.close().await.unwrap();
      u32::try_from(most.saturating_sub(reserved)).unwrap_or(0)
    })
  })
  .join()
  .unwrap()
}

fn seats() -> u32 {
  (ordinary_connections() / 2 / (agency_server::STORE_CONNECTIONS + SLACK)).max(1)
}

/// A seat held for one agency server, given back when it goes.
pub struct Seat(());

impl Seat {
  /// A seat taken, waiting for one to be given back where none is free.
  pub fn take() -> Seat {
    let mut free = SEATS.free.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    while *free == 0 {
      free = SEATS.given_back.wait(free).unwrap_or_else(|poisoned| poisoned.into_inner());
    }

    *free -= 1;
    Seat(())
  }
}

impl Drop for Seat {
  fn drop(&mut self) {
    *SEATS.free.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) += 1;
    SEATS.given_back.notify_one();
  }
}
