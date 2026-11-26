//! Who a request is for and what it may reach, as the store says (docs/own-llm.md, 「The keys」, 「Personal
//! members」 and 「The gateway」): the call keys by their digests, the people's own keys for a gateway road,
//! the administrator, whose owned supply the gateway's own key reaches, which accounts hold an open
//! platform reservation, without which a call key reaches nothing of the platform's, and which hold an open
//! reservation of an organisation unit's supply, which opens that unit's names and no others. All of it is
//! read on the launcher's beat, on a connection opened read-only for each reading, and what a request finds
//! missing from the last reading is looked up in the store once more before it is refused, so a key issued
//! or a reservation opened a moment ago is taken.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use sqlx::{AnyConnection, Connection, Row};
use store::open_read;

/// Who pays for a request: the platform, on the gateway's own key, or the account a call key is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Payer {
  Platform,
  Account(i64),
}

impl Payer {
  /// The payer as the front names it to the child in `x-bmo-payer`.
  pub fn named(&self) -> String {
    match self {
      Payer::Platform => "platform".to_owned(),
      Payer::Account(account) => account.to_string(),
    }
  }
}

/// One person's or one organisation unit's own key for a gateway road, a row of personal supply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonalKey {
  pub id: i64,
  /// The person whose key it is, where a person's.
  pub owner_account: Option<i64>,
  /// The organisation unit whose key it is, where a unit's.
  pub owner_unit: Option<i64>,
  pub road: String,
  /// The secret's name under its owner's folder, `users/<owner_account>/` or `units/<owner_unit>/` under the data root's
  /// `env/`.
  pub secret: String,
  /// Its place among the person's keys of the road, the lowest taken first.
  pub place: i64,
  pub enabled: bool,
  /// When the provider was found refusing it, in milliseconds; none while it stands.
  pub refused_at: Option<i64>,
}

impl PersonalKey {
  /// Whose key it is: the person's or the unit's, none for a row that names neither.
  pub fn owner(&self) -> Option<crate::table::Owner> {
    match (self.owner_account, self.owner_unit) {
      (Some(user), _) => Some(crate::table::Owner::Personal(user)),
      (None, Some(unit)) => Some(crate::table::Owner::Organisation(unit)),
      (None, None) => None,
    }
  }

  /// Whether the key may be served: switched on and not found refused.
  pub fn open(&self) -> bool {
    self.enabled && self.refused_at.is_none()
  }
}

/// What one reading of the store holds for the gateway.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reading {
  /// Each unrevoked call key's digest, with the account it is.
  pub call_keys: HashMap<String, i64>,
  /// Every row of personal supply, open or not.
  pub personal: Vec<PersonalKey>,
  /// The account that administers the platform, where the settings name one.
  pub administrator: Option<i64>,
  /// Each account holding an open platform reservation, with the latest moment one of them expires, none
  /// where one never does.
  pub reservations: HashMap<i64, Option<i64>>,
  /// Each account holding an open reservation of an organisation unit's supply, with the units it names;
  /// these open the unit's names alone, never the platform's.
  pub unit_reservations: HashMap<i64, Vec<i64>>,
}

/// The digest the store keeps of a call key: the lowercase hex SHA-256 of the key trimmed.
pub fn digest(key: &str) -> String {
  format!("{:x}", Sha256::digest(key.trim().as_bytes()))
}

fn now_ms() -> i64 {
  SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis() as i64)
}

// Whether a reservation expiring at `expires_at` is open at `now`.
fn unexpired(expires_at: Option<i64>, now: i64) -> bool {
  expires_at.is_none_or(|at| at > now)
}

/// The store as the gateway reads it now.
pub async fn read(address: &str) -> Result<Reading, sqlx::Error> {
  let mut store = open_read(address).await?;
  let read = read_on(&mut store).await;
  let _ = store.close().await;
  read
}

async fn read_on(store: &mut AnyConnection) -> Result<Reading, sqlx::Error> {
  let call_keys = sqlx::query("SELECT account, key_hash FROM gateway_call_keys WHERE revoked_at IS NULL")
    .fetch_all(&mut *store)
    .await?
    .iter()
    .map(|row| Ok((row.try_get::<String, _>("key_hash")?, row.try_get::<i64, _>("account")?)))
    .collect::<Result<HashMap<_, _>, sqlx::Error>>()?;
  let personal = sqlx::query(
    "SELECT id, owner_account, owner_unit, road, secret, place, enabled, refused_at FROM personal_supply \
     ORDER BY owner_account, owner_unit, road, place, id",
  )
  .fetch_all(&mut *store)
  .await?
  .iter()
  .map(|row| {
    Ok(PersonalKey {
      id: row.try_get("id")?,
      owner_account: row.try_get("owner_account")?,
      owner_unit: row.try_get("owner_unit")?,
      road: row.try_get("road")?,
      secret: row.try_get("secret")?,
      place: row.try_get("place")?,
      enabled: row.try_get("enabled")?,
      refused_at: row.try_get("refused_at")?,
    })
  })
  .collect::<Result<Vec<_>, sqlx::Error>>()?;
  let administrator = store::settings::administrator(store).await?;
  let mut reservations: HashMap<i64, Option<i64>> = HashMap::new();
  let mut unit_reservations: HashMap<i64, Vec<i64>> = HashMap::new();
  for row in sqlx::query(
    "SELECT account, unit_id, expires_at FROM supply_reservations WHERE state = 'open' \
     AND (expires_at IS NULL OR expires_at > $1)",
  )
  .bind(now_ms())
  .fetch_all(&mut *store)
  .await?
  {
    let (account, unit, expires_at) = (
      row.try_get::<i64, _>("account")?,
      row.try_get::<Option<i64>, _>("unit_id")?,
      row.try_get::<Option<i64>, _>("expires_at")?,
    );

    match unit {
      // An organisation reservation opens its unit's names alone; the platform's door asks a reservation
      // of the platform's own (docs/own-llm.md, 「The gateway」).
      Some(unit) => {
        let held = unit_reservations.entry(account).or_default();

        if !held.contains(&unit) {
          held.push(unit);
        }
      }
      None => {
        let latest = reservations.entry(account).or_insert(expires_at);
        *latest = match (*latest, expires_at) {
          (Some(held), Some(read)) => Some(held.max(read)),
          _ => None,
        };
      }
    }
  }

  Ok(Reading {
    call_keys,
    personal,
    administrator,
    reservations,
    unit_reservations,
  })
}

/// The account whose unrevoked call key has the digest, as the store says now.
async fn account_of_digest(address: &str, digest: &str) -> Result<Option<i64>, sqlx::Error> {
  let mut store = open_read(address).await?;
  let read =
    sqlx::query("SELECT account FROM gateway_call_keys WHERE key_hash = $1 AND revoked_at IS NULL")
      .bind(digest)
      .fetch_optional(&mut store)
      .await
      .and_then(|row| row.map(|row| row.try_get::<i64, _>("account")).transpose());
  let _ = store.close().await;
  read
}

/// The latest expiry of the account's open platform reservations that have not expired, as the store says
/// now, none expiring where one never does; none where the account holds none.
async fn supplied_now(address: &str, account: i64) -> Result<Option<Option<i64>>, sqlx::Error> {
  let mut store = open_read(address).await?;
  let read = async {
    sqlx::query(
      "SELECT expires_at FROM supply_reservations WHERE account = $1 AND unit_id IS NULL AND state = 'open' \
       AND (expires_at IS NULL OR expires_at > $2) ORDER BY expires_at DESC NULLS FIRST LIMIT 1",
    )
    .bind(account)
    .bind(now_ms())
    .fetch_optional(&mut store)
    .await?
    .map(|row| row.try_get::<Option<i64>, _>("expires_at"))
    .transpose()
  }
  .await;
  let _ = store.close().await;
  read
}

/// Whether the account holds an open, unexpired reservation on the unit's own supply, as the store says now.
async fn unit_supplied_now(address: &str, account: i64, unit: i64) -> Result<bool, sqlx::Error> {
  let mut store = open_read(address).await?;
  let read = sqlx::query(
    "SELECT 1 AS held FROM supply_reservations WHERE account = $1 AND unit_id = $2 AND state = 'open' \
     AND (expires_at IS NULL OR expires_at > $3) LIMIT 1",
  )
  .bind(account)
  .bind(unit)
  .bind(now_ms())
  .fetch_optional(&mut store)
  .await
  .map(|row| row.is_some());
  let _ = store.close().await;
  read
}

/// The last reading the front judges its requests by, and the store it looks a miss up in.
pub struct Supply {
  /// The store's address, where a miss is looked up.
  address: String,
  held: Mutex<Reading>,
}

impl Supply {
  pub fn new(address: String) -> Supply {
    Supply {
      address,
      held: Mutex::new(Reading::default()),
    }
  }

  /// The reading the beat made, in place of the last.
  pub fn know(&self, reading: Reading) {
    *self.held.lock().unwrap() = reading;
  }

  /// The rows of personal supply the last reading holds.
  pub fn personal(&self) -> Vec<PersonalKey> {
    self.held.lock().unwrap().personal.clone()
  }

  /// The account a call key is: by its digest in the last reading, else as the store says now.
  pub async fn account_of(&self, key: &str) -> Option<i64> {
    let digest = digest(key);
    if let Some(account) = self.held.lock().unwrap().call_keys.get(&digest) {
      return Some(*account);
    }
    match account_of_digest(&self.address, &digest).await {
      Ok(Some(account)) => {
        self.held.lock().unwrap().call_keys.insert(digest, account);
        Some(account)
      }
      Ok(None) => None,
      Err(fault) => {
        eprintln!("llm-gateway: a call key could not be looked up: {fault}; it is refused");
        None
      }
    }
  }

  /// The account that administers the platform, as the last reading names it.
  pub fn administrator(&self) -> Option<i64> {
    self.held.lock().unwrap().administrator
  }

  /// Whether the account may reach an organisation unit's own members: only on an open reservation of that
  /// unit's supply, looked up in the store each time, since such a request is rare beside the platform's.
  pub async fn supplied_by_unit(&self, account: i64, unit: i64) -> bool {
    match unit_supplied_now(&self.address, account, unit).await {
      Ok(supplied) => supplied,
      Err(fault) => {
        eprintln!(
          "llm-gateway: the supply of account {account} on unit {unit} could not be looked up: {fault}; it is refused"
        );
        false
      }
    }
  }

  /// Whether the account may reach platform supply: only on an open reservation, looked up in the store once
  /// more where the last reading holds none.
  pub async fn supplied(&self, account: i64) -> bool {
    if self
      .held
      .lock()
      .unwrap()
      .reservations
      .get(&account)
      .is_some_and(|expires| unexpired(*expires, now_ms()))
    {
      return true;
    }
    match supplied_now(&self.address, account).await {
      Ok(Some(expires_at)) => {
        self.held.lock().unwrap().reservations.insert(account, expires_at);
        true
      }
      Ok(None) => false,
      Err(fault) => {
        eprintln!(
          "llm-gateway: the supply of account {account} could not be looked up: {fault}; it is refused"
        );
        false
      }
    }
  }
}
