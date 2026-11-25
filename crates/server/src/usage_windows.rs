//! The accounts' usage windows as agency keeps them (docs/usage-windows.md): the keeping on agency's store
//! with the readers this machine runs, the registered Claude logins those readers ask for, the run of the
//! CLI that renews a login the Claude reader finds expired, each provider's balance as its member declares
//! the reading, and a turn's answer kept under the login that answered.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, OnceLock, Weak};

use std::time::Duration;

use cache_siren::balance::{Asking, Balance, BalanceAsk, BalanceReader, BalanceSource, Requests};
use cache_siren::claude::{ClaudeAccounts, Login, Logins, Renewals};
use cache_siren::codex::CodexReader;
use cache_siren::{Keeping, Reader, Slot, Window};
use harness_providers::balance::{OffersBalance, key_in_file};
use harness_providers::{Machine, Provider};
use sea_orm::DatabaseConnection;
use serde_json::Value;

use crate::control::claude_accounts::{self, Account, Credential, SEED_ACCOUNT, accounts};
use crate::door::record::Reply;
use crate::job_run::claude::failure::AccountFailure;
use crate::job_run::claude::renewal;
use crate::keepalive::logins;
use crate::now_ms;
use crate::places::{Ground, locks};

const CODEX: &str = "codex";

/// Every registered Claude account with usage windows, on or off, as the store holds it now: an endpoint
/// account is paid by the request and is never asked of the reader.
struct StoreLogins(DatabaseConnection);

impl Logins for StoreLogins {
  fn logins(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Login>, String>> + Send + '_>> {
    Box::pin(async move { Ok(window_logins(accounts(&self.0).await.map_err(|refused| refused.error)?)) })
  }
}

/// The logins the Claude account reader asks of, one each account that has usage windows and whose
/// credential stands on this machine: a person's login signed in on their node is read off its turns'
/// answers alone (docs/supply.md, 「Signing a person's login in」).
pub fn window_logins(held: Vec<Account>) -> Vec<Login> {
  held
    .into_iter()
    .filter(|account| !account.on_own_node())
    .filter_map(|account| {
      Some(Login {
        credential: account.credential.window_credential()?,
        account: window_account(&account).to_owned(),
      })
    })
    .collect()
}

// The account a reader's slot names an account by.
fn window_account(account: &Account) -> &str {
  if account.id.is_empty() { SEED_ACCOUNT } else { &account.id }
}

/// The run of the CLI the Claude account reader asks on a directory account's login it finds expired or
/// refused (docs/usage-windows.md, 「The Claude account reader」), on the ground the readers pass on once
/// they stand on one; none runs before.
#[derive(Clone, Default)]
pub struct LoginRenewals(Arc<OnceLock<Weak<Ground>>>);

impl LoginRenewals {
  /// The ground the runs go out on, from now on.
  pub fn stand_on(&self, ground: &Arc<Ground>) {
    let _ = self.0.set(Arc::downgrade(ground));
  }
}

impl Renewals for LoginRenewals {
  fn renew(&self, account: &str) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'static>> {
    let ground = self.0.get().and_then(Weak::upgrade);
    let account = account.to_owned();

    Box::pin(async move {
      let ground = ground.ok_or("no ground stands to run the CLI on")?;

      renew_login(&ground, &account).await
    })
  }
}

/// One run of Claude Code on a directory account's own login, so the CLI renews its token as it goes out
/// (`renewal::run`), under the account's renewal lock, waited for where a run near its login's end holds it
/// for the same renewal. A token or endpoint account, a login waiting for a person and a login past its end
/// are never run on. A run its login refuses marks the account as waiting for a person and tells the owner
/// once, as a turn's refusal does (docs/cache-fallback.md, 「A login that is about to end」).
pub async fn renew_login(ground: &Ground, id: &str) -> Result<(), String> {
  let account = accounts(&ground.db)
    .await
    .map_err(|refused| refused.error)?
    .into_iter()
    .find(|account| window_account(account) == id)
    .ok_or("no such account is registered")?;

  if !matches!(account.credential, Credential::Directory { .. }) {
    return Err("it holds no login the CLI renews".to_owned());
  }

  let now = now_ms();

  if account.needs_login_at.is_some() && !account.login_back_here(&ground.roots.data, now) {
    return Err("its login waits for a person to sign in again".to_owned());
  }

  if claude_accounts::login_end(&account).is_some_and(|end| end <= now as f64) {
    return Err("its login has ended and waits for a person to sign in again".to_owned());
  }

  let _held = locks::hold(renewal::lock_path(&ground.roots.data, &account))
    .await
    .map_err(|why| format!("the account's renewal lock: {why}"))?;

  match renewal::run(ground, &account).await {
    Ok(()) => Ok(()),
    Err(failure) if failure.account_failure() == Some(AccountFailure::Login) => {
      let now = now_ms();
      let marked = async {
        logins::mark_gone(&ground.db, &account.id, now).await?;

        if let Some(held) = accounts(&ground.db)
          .await
          .map_err(|refused| crate::lines::fault(refused.error))?
          .into_iter()
          .find(|one| one.id == account.id)
        {
          logins::tell(&ground.db, &ground.events, &held, &ground.roots.data, now).await?;
        }

        Ok::<(), crate::Refusal>(())
      };

      if let Err(why) = marked.await {
        eprintln!("claude account {}: its refused login was not marked: {}", account.id, why.detail);
      }

      Err(failure.message())
    }
    Err(failure) => Err(failure.message()),
  }
}

/// One provider's balance as its member declares the reading: its own road where it can answer on this
/// machine, and every endpoint account of the store whose endpoint is the provider's, with the account's own
/// key file.
pub struct ProviderBalances {
  pub provider: &'static dyn Provider,
  pub offers: &'static dyn OffersBalance,
  pub machine: Machine,
  pub db: DatabaseConnection,
}

impl ProviderBalances {
  /// The provider's balance on this machine, where it offers one.
  pub fn of(provider: &'static dyn Provider, machine: &Machine, db: &DatabaseConnection) -> Option<Self> {
    Some(Self {
      provider,
      offers: provider.balance()?,
      machine: machine.clone(),
      db: db.clone(),
    })
  }
}

impl BalanceSource for ProviderBalances {
  fn mode(&self) -> &str {
    self.provider.name()
  }

  fn cadence(&self) -> Duration {
    Duration::from_secs(self.offers.declared().pass_seconds)
  }

  fn fresh(&self) -> Duration {
    self.cadence() * self.offers.declared().fresh_passes
  }

  fn asks(&self) -> Asking<'_> {
    Box::pin(async move {
      let url = self.offers.url(&self.machine);
      let requests_url = self.offers.requests_url(&self.machine);
      let mut asks = Vec::new();

      if self.provider.missing(&self.machine).is_none() {
        asks.push(BalanceAsk {
          account: String::new(),
          url: url.clone(),
          key: self.offers.key(&self.machine),
          requests_url: requests_url.clone(),
        });
      }

      for account in accounts(&self.db).await.map_err(|refused| refused.error)? {
        if let Credential::Endpoint {
          url: endpoint,
          key_file,
          ..
        } = &account.credential
          && self.offers.serves(endpoint, &self.machine)
        {
          asks.push(BalanceAsk {
            account: account.id.clone(),
            url: url.clone(),
            key: key_in_file(key_file),
            requests_url: requests_url.clone(),
          });
        }
      }

      Ok(asks)
    })
  }

  fn balance_of(&self, answer: &Value) -> Result<Balance, String> {
    self.offers.read(answer).map(|read| Balance {
      amount: read.amount,
      currency: read.currency,
      requests: None,
    })
  }

  fn requests_of(&self, answer: &Value) -> Option<Requests> {
    self.offers.requests(answer).map(|read| Requests {
      used: read.used,
      limit: read.limit,
      left: read.left,
      scope: read.scope,
    })
  }
}

/// The readers this machine runs: every Claude account's, asking `renewals` to renew a login it finds
/// expired, Codex's where Codex is installed, and the balance of every provider that offers one.
fn readers(db: &DatabaseConnection, machine: &Machine, renewals: &LoginRenewals) -> Vec<Arc<dyn Reader>> {
  let mut readers: Vec<Arc<dyn Reader>> = vec![Arc::new(
    ClaudeAccounts::of_provider(Arc::new(StoreLogins(db.clone()))).renewing(Arc::new(renewals.clone())),
  )];

  for provider in harness_providers::providers() {
    if let Some(balances) = ProviderBalances::of(provider, machine, db) {
      readers.push(Arc::new(BalanceReader::new(Arc::new(balances))));
    }
  }

  let codex = harness_providers::provider(CODEX)
    .filter(|codex| !codex.models_on_gateway() && codex.missing(machine).is_none());

  if codex.is_some() {
    let sessions = PathBuf::from(machine.var("HOME").unwrap_or_default()).join(".codex").join("sessions");
    let program = machine.program(CODEX).unwrap_or_else(|| PathBuf::from(CODEX));

    readers.push(Arc::new(CodexReader::new(sessions, program)));
  }

  readers
}

/// The keeping on agency's store, with the readers this machine runs, the Claude reader's renewals run by
/// `renewals`.
pub fn keeping(db: &DatabaseConnection, machine: &Machine, renewals: &LoginRenewals) -> Keeping {
  Keeping::new(db.clone(), readers(db, machine, renewals))
}

/// The windows a turn's answer read, kept at `at` in epoch seconds under the login that answered: the
/// empty account on a provider that holds no logins of agency's, and nothing written where the provider
/// holds logins and the answer names none. An answer that read no windows leaves the slot as it was, and a
/// keeping that cannot be written is said with its cause rather than failing the turn.
pub async fn keep_reply(keeping: &Keeping, reply: &Reply, at: i64) {
  if reply.windows.is_empty() {
    return;
  }

  let holds_logins =
    harness_providers::provider(&reply.mode).is_some_and(|provider| provider.holds_logins());
  let account = match reply.account.as_deref().filter(|account| !account.is_empty()) {
    Some(account) if holds_logins => account,
    None if holds_logins => return,
    _ => "",
  };
  let windows: Vec<Window> = reply.windows.iter().map(Window::from).collect();

  if let Err(fault) = keeping
    .replace(
      Slot {
        mode: &reply.mode,
        account,
      },
      &windows,
      at,
    )
    .await
  {
    eprintln!(
      "{} windows of {}: not kept: {fault}",
      reply.mode,
      if account.is_empty() { "no login" } else { account }
    );
  }
}
