//! The renewal of a login near its credential's end (docs/cache-fallback.md, 「A login that is about to
//! end」): a short step of its own before a run goes out on the login, never the run's turn. The run that
//! takes the account's own lock renews the login with one run of the CLI on it whose only purpose is that
//! renewal, and lets the lock go before its turn goes out; a run that finds the lock held waits for that
//! renewal and says so where its caller reads it, as it starts waiting and as it goes out. A renewal that
//! fails on the login is recorded against the credential it could not renew before the lock is let go, so
//! the runs waiting on it, and every run after them while that credential stands, end as that login's
//! failure without another renewal.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use policy::providers::claude_code::CLAUDE_CODE;
use policy::roles::ROLES;

use super::failure::{AccountFailure, Failure};
use crate::control::claude_accounts::{Account, Credential, SEED_ACCOUNT, credential_expiry};
use crate::door::apart::{self, Apart, answered, new_session};
use crate::door::turn::Quiet;
use crate::lines::Trees;
use crate::lines::claude::{ApartRun, Command, Runs, apart_args, command_env};
use crate::now_ms;
use crate::places::{Ground, locks};

/// How often a run waiting on another run's renewal looks at the credential and the lock again.
pub const LOOK: Duration = Duration::from_millis(500);

// The work, the line and the agent a login's renewal run goes out as.
const RENEWAL: &str = "login-renewal";

/// What a run near its login's end renews the login with, and where it says that it waits.
pub trait Renewing {
  /// One run on the login whose only purpose is that the CLI renews its token as it goes out: the CLI's
  /// renewal run, unless the renewer says otherwise.
  fn renew(&mut self, ground: &Ground, account: &Account) -> impl Future<Output = Result<(), Failure>> {
    run(ground, account)
  }

  /// One line where whoever the run is for reads it, said as a wait for the renewal starts and as the run
  /// goes out after it. Said nowhere, unless the renewer says otherwise.
  fn say(&mut self, _words: &str) -> impl Future<Output = ()> {
    async {}
  }

  /// How often a run waiting on another run's renewal looks again: `LOOK`, unless the renewer says
  /// otherwise.
  fn look(&self) -> Duration {
    LOOK
  }
}

/// The lock one account's renewal is held under, in the data root's `locks/`.
pub fn lock_path(data_root: &Path, account: &Account) -> PathBuf {
  data_root.join("locks").join(format!("claude-account-{}.lock", account.id))
}

// Where the refusal of one account's renewal is recorded beside its lock: the moment the credential it could
// not renew states, which no other credential states.
fn refusal_path(data_root: &Path, account: &Account) -> PathBuf {
  data_root.join("locks").join(format!("claude-account-{}.refused", account.id))
}

/// Whether the login is one a run renews, a directory account's, with its credential inside the margin the
/// CLI renews it within as a run goes out. A long-lived token holds nothing to renew with and an endpoint
/// account holds no login, so neither is ever due.
pub fn due(account: &Account) -> bool {
  let margin = CLAUDE_CODE.usage.renewal_margin_seconds as f64 * 1000.0;

  matches!(account.credential, Credential::Directory { .. })
    && credential_expiry(account).is_some_and(|expiry| expiry - now_ms() as f64 <= margin)
}

/// The renewal step before a run goes out on a login: nothing where the login is not near its end, or where
/// the run goes out on the account's token for the work nodes, which renews nothing; the renewal, under the
/// account's lock, where this run takes it; and otherwise the wait for the run that holds it, which ends once
/// the credential shows the renewal written or the lock is let go. A credential whose renewal was refused
/// ends the run as that login's failure; any other end of the step lets the run go out.
pub async fn renewed(
  ground: &Ground,
  account: &Account,
  renewing: &mut impl Renewing,
) -> Result<(), Failure> {
  if !due(account) || account.carrying_token(&ground.roots.data, now_ms()).is_some() {
    return Ok(());
  }

  let data = &ground.roots.data;
  let path = lock_path(data, account);

  if let Some(held) = locks::try_hold(&path) {
    let renewal = renew_holding(ground, account, renewing).await;

    drop(held);

    return renewal;
  }

  let id = &account.id;

  renewing
    .say(&format!("Waiting for another run to renew the login of Claude account {id}"))
    .await;

  loop {
    tokio::time::sleep(renewing.look()).await;

    if !due(account) {
      renewing
        .say(&format!(
          "Claude account {id}: another run renewed its login, going out on what it wrote"
        ))
        .await;

      return Ok(());
    }

    if !locks::held(&path) {
      if refused(data, account) {
        renewing
          .say(&format!(
            "Claude account {id}: the other run's renewal was refused, the login waits for a person"
          ))
          .await;

        return Err(refused_failure(account));
      }

      renewing
        .say(&format!("Claude account {id}: the other run's renewal ended, going out"))
        .await;

      return Ok(());
    }
  }
}

// The renewal by the run holding the account's lock. A credential whose renewal was refused already is not
// renewed again; a login the renewal found gone is recorded against its credential while the lock is still
// held, so a run waiting on it reads the record once the lock is free; a full window is the login's as a
// turn's would be; any other failure is said and the run goes out, its own CLI renewing as it goes.
async fn renew_holding(
  ground: &Ground,
  account: &Account,
  renewing: &mut impl Renewing,
) -> Result<(), Failure> {
  let id = &account.id;
  let data = &ground.roots.data;

  // Another run may have renewed it between the look and the lock.
  if !due(account) {
    return Ok(());
  }

  if refused(data, account) {
    return Err(refused_failure(account));
  }

  renewing
    .say(&format!("Renewing the login of Claude account {id} before going out"))
    .await;

  match renewing.renew(ground, account).await {
    Ok(()) => {
      renewing.say(&format!("Claude account {id}: its login is renewed, going out")).await;

      Ok(())
    }
    Err(failed) if failed.account_failure() == Some(AccountFailure::Login) => {
      if let Some(expiry) = credential_expiry(account) {
        std::fs::write(refusal_path(data, account), expiry.to_string())
          .map_err(|why| Failure::error(format!("claude account {id}: the renewal's refusal: {why}")))?;
      }

      Err(failed)
    }
    Err(failed) if failed.account_failure().is_some() => Err(failed),
    Err(failed) => {
      renewing
        .say(&format!(
          "Claude account {id}: the renewal of its login failed ({}), going out regardless",
          failed.message()
        ))
        .await;

      Ok(())
    }
  }
}

// Whether the renewal of the credential the account holds now was refused: the record beside its lock names
// the moment that credential states. A credential written since, by a renewal or a person signing in again,
// states another.
fn refused(data_root: &Path, account: &Account) -> bool {
  let recorded = std::fs::read_to_string(refusal_path(data_root, account)).ok();

  recorded
    .is_some_and(|recorded| credential_expiry(account).is_some_and(|expiry| recorded == expiry.to_string()))
}

// The failure a run ends as on a login whose renewal was refused: the login waits for a person.
fn refused_failure(account: &Account) -> Failure {
  Failure::Account {
    failure: AccountFailure::Login,
    detail: format!("claude account {}: its login could not be renewed and waits for a person", account.id),
  }
}

/// One run of Claude Code on a directory account's own login, so the CLI renews its token as it goes out: a
/// run apart under the account's configuration directory, on the model and effort the usage declaration
/// names, its one word put in with no tool shown and its answer thrown away. It holds no lock: its callers
/// hold the account's.
pub async fn run(ground: &Ground, account: &Account) -> Result<(), Failure> {
  let id = if account.id.is_empty() { SEED_ACCOUNT } else { &account.id };
  let usage = &CLAUDE_CODE.usage;
  let session = new_session();
  let args = apart_args(
    &ApartRun {
      line: RENEWAL,
      model: &usage.renewal_model,
      effort: &usage.renewal_effort,
      system_prompt: None,
      mcp_config: None,
      tools_shown: "",
      tools_allowed: "",
      host_commands: false,
      advisor: None,
    },
    &session,
  )
  .map_err(|why| Failure::error(why.detail))?;
  let env = command_env(ground.env(), Some(account), &Runs::OwnLogin, false)
    .map_err(|why| Failure::error(why.detail))?;
  let cwd = Trees::of(ground).agents.join(RENEWAL);

  std::fs::create_dir_all(&cwd).map_err(|why| Failure::error(why.to_string()))?;

  let apart = Apart {
    agent: RENEWAL,
    role: ROLES.renewal.as_str(),
    line: RENEWAL,
    provider: crate::lines::runner::PROVIDER,
    title: format!("{RENEWAL} · {id} · {}", usage.renewal_model),
    command: Command {
      args,
      cwd: cwd.to_string_lossy().into_owned(),
      env,
    },
    session,
    prompt: usage.renewal_words.clone(),
    model: usage.renewal_model.clone(),
    effort: usage.renewal_effort.clone(),
    account: None,
    parent: None,
    root: None,
    run: None,
    resume: None,
    resume_else: None,
  };
  let ending = apart::run(ground, apart, &mut Quiet)
    .await
    .map_err(|message| Failure::Transport { message })?;

  answered(ending, RENEWAL).map(drop)
}
