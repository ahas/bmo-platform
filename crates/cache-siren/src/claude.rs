//! Each registered Claude account's own windows, asked of the account-usage endpoint with that account's
//! own login (docs/usage-windows.md, 「The Claude account reader」), so an account nothing has run on lately
//! still has its own figures and never another's.
//!
//! The token is only ever in the request: read out of the account's own credential file, put in the one
//! header that asks, and dropped — never kept, returned, said or renewed. Renewing a login is the CLI's: a
//! directory account's token found expired or refused marks the account unread, so its last reading stops
//! standing until a reading lifts the mark, and asks the caller for one run of the CLI on that login, which
//! renews it as it goes out; one such run of an account at a time, and at a pace while it keeps failing.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use policy::providers::claude_code::CLAUDE_CODE;
use serde_json::Value;

use crate::reader::{Passing, Reader, Said};
use crate::windows::{Keeping, Slot, Window};

/// The provider the windows are kept under.
pub const MODE: &str = "claude-code";

// The failures that say the login's token cannot be read with until a run renews it: one found expired in
// its file, and one the endpoint refused as unauthorised.
const TOKEN_EXPIRED: &str = "token expired";
const TOKEN_REFUSED: &str = "HTTP 401";

/// Where a login's token is read from: a directory account's credential file, which holds it beside its
/// expiry, or a token account's file, which is the token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Credential {
  Directory(PathBuf),
  Token(PathBuf),
}

/// One registered account: the id its windows are kept under and where its token is read from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Login {
  pub account: String,
  pub credential: Credential,
}

/// The registered accounts, on or off, as the caller that keeps them answers on each pass.
pub trait Logins: Send + Sync {
  fn logins(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Login>, String>> + Send + '_>>;
}

/// A run of the CLI on one account's own login, as the caller that starts the CLI answers it: a future that
/// is the run, ending when the run has ended, with why it failed where it did. The CLI renews the login's
/// token for itself as the run goes out; the reader never touches it.
pub trait Renewals: Send + Sync {
  fn renew(&self, account: &str) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'static>>;
}

// An account's last renewal: the moment it was asked, in epoch seconds, and whether its run has ended.
#[derive(Clone, Copy, Debug)]
struct Renewal {
  asked_at: i64,
  ended: bool,
}

type Renewing = Arc<Mutex<HashMap<String, Renewal>>>;

fn locked<T>(held: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
  held.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The windows an answer states, each as the fraction used and the moment it resets. A window with no
/// figure says nothing and is left out rather than written as nought.
pub fn windows_of(answer: &Value) -> Vec<Window> {
  CLAUDE_CODE
    .usage
    .windows
    .iter()
    .filter_map(|name| {
      let window = answer.get(name).filter(|window| window.is_object())?;
      let used = window.get("utilization").and_then(Value::as_f64)?;
      let resets_at = window
        .get("resets_at")
        .and_then(Value::as_str)
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.timestamp());

      Some(Window {
        name: name.clone(),
        utilization: used / 100.0,
        resets_at,
        duration_minutes: None,
        limit_name: None,
      })
    })
    .collect()
}

// The access token a login holds now, or why there is none to ask with.
fn access_token(credential: &Credential, now_ms: i64) -> Result<String, &'static str> {
  let (Credential::Directory(file) | Credential::Token(file)) = credential;
  let text = std::fs::read_to_string(file).map_err(|_| "credential file unreadable")?;

  if let Credential::Token(_) = credential {
    let token = text.trim();

    return if token.is_empty() { Err("no token") } else { Ok(token.to_owned()) };
  }

  let held: Value = serde_json::from_str(&text).map_err(|_| "credential file unreadable")?;
  let login = held.get("claudeAiOauth");
  let token = login
    .and_then(|login| login.get("accessToken"))
    .and_then(Value::as_str)
    .filter(|token| !token.is_empty())
    .ok_or("no token")?;

  if login
    .and_then(|login| login.get("expiresAt"))
    .and_then(Value::as_f64)
    .is_some_and(|expires| expires <= now_ms as f64)
  {
    return Err(TOKEN_EXPIRED);
  }

  Ok(token.to_owned())
}

/// The reader on the clock: where it asks, who it asks for, each account's failure as last said, the
/// moment, in epoch seconds, before which an account the endpoint refused for asking too often is not asked,
/// who runs the CLI that renews a login, where anybody does, and each account's last renewal.
pub struct ClaudeAccounts {
  url: String,
  http: reqwest::Client,
  logins: Arc<dyn Logins>,
  said: Said,
  paused: Mutex<HashMap<String, i64>>,
  renewals: Option<Arc<dyn Renewals>>,
  renewing: Renewing,
}

impl ClaudeAccounts {
  /// The reader asking `url` for every login `logins` answers.
  pub fn new(url: impl Into<String>, logins: Arc<dyn Logins>) -> Self {
    Self {
      url: url.into(),
      http: reqwest::Client::new(),
      logins,
      said: Said::default(),
      paused: Mutex::default(),
      renewals: None,
      renewing: Renewing::default(),
    }
  }

  fn paused(&self) -> std::sync::MutexGuard<'_, HashMap<String, i64>> {
    locked(&self.paused)
  }

  /// The same reader asking `renewals` for a run of the CLI on a directory account's login it finds
  /// expired or refused.
  pub fn renewing(mut self, renewals: Arc<dyn Renewals>) -> Self {
    self.renewals = Some(renewals);
    self
  }

  /// The reader asking the provider's own account-usage endpoint.
  pub fn of_provider(logins: Arc<dyn Logins>) -> Self {
    Self::new(CLAUDE_CODE.usage.url.as_str(), logins)
  }

  // One ask for an account's windows, waiting for its answer however long it takes: its end is the answer,
  // or the connection refused or broken. A refusal for asking too often pauses the account until the moment
  // the refusal's `Retry-After` names, in seconds, or for the policy's pause where it names none.
  async fn ask(&self, token: &str, account: &str, now: i64) -> Result<Vec<Window>, String> {
    let answer = self
      .http
      .get(&self.url)
      .bearer_auth(token)
      .send()
      .await
      .map_err(|_| "the request failed".to_owned())?;

    if answer.status().as_u16() == 429 {
      let after = answer
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(i64::try_from(CLAUDE_CODE.usage.refused_pause_seconds).unwrap_or(300));

      self.paused().insert(account.to_owned(), now + after);

      return Err(format!("HTTP 429; not asked again for {after} seconds"));
    }

    if !answer.status().is_success() {
      return Err(format!("HTTP {}", answer.status().as_u16()));
    }

    let read: Value = answer.json().await.map_err(|_| "the answer is not JSON".to_owned())?;

    Ok(windows_of(&read))
  }

  // The windows the account last read full, not yet reset at `now`, that this reading finds short of full:
  // a limit lifted early, by a quota reset or the provider itself.
  async fn came_back_early(
    keeping: &Keeping,
    account: &str,
    windows: &[Window],
    now: i64,
  ) -> Result<Vec<String>, String> {
    let held = keeping
      .read(Slot {
        mode: MODE,
        account,
      })
      .await
      .map_err(|fault| fault.detail)?;

    Ok(
      held
        .windows
        .iter()
        .filter(|kept| {
          kept.window.is_full()
            && kept.window.resets_at.is_some_and(|reset| reset > now)
            && windows.iter().any(|window| window.name == kept.window.name && !window.is_full())
        })
        .map(|kept| kept.window.name.clone())
        .collect(),
    )
  }

  // A run of the CLI asked on an account's login, at `now` in epoch seconds: none while the last one asked
  // runs, and none sooner than the pause after it was asked. The pass goes on without waiting for the run;
  // its start is said as it is asked and its outcome as it ends.
  fn renew(&self, account: &str, now: i64) {
    let Some(renewals) = &self.renewals else { return };
    let pause = i64::try_from(CLAUDE_CODE.usage.renewal_pause_seconds).unwrap_or(300);

    {
      let mut renewing = locked(&self.renewing);

      if renewing.get(account).is_some_and(|last| !last.ended || now < last.asked_at + pause) {
        return;
      }

      renewing.insert(
        account.to_owned(),
        Renewal {
          asked_at: now,
          ended: false,
        },
      );
    }

    eprintln!("claude usage for {account}: its login is renewed by one run of the CLI on it");

    let running = renewals.renew(account);
    let renewing = self.renewing.clone();
    let account = account.to_owned();

    tokio::spawn(async move {
      let outcome = running.await;

      if let Some(last) = locked(&renewing).get_mut(&account) {
        last.ended = true;
      }

      match outcome {
        Ok(()) => println!("claude usage for {account}: the run renewing its login ended"),
        Err(why) => eprintln!("claude usage for {account}: the run renewing its login failed: {why}"),
      }
    });
  }

  // One account read and its windows written; why not where they were not. A token that cannot be read
  // with marks the account so its last reading stops standing, and asks a run of the CLI on a directory
  // account's login; a reading taken lifts the mark, and lets go of a renewal that has ended.
  async fn read(&self, keeping: &Keeping, login: &Login, now_ms: i64) -> Result<Vec<Window>, String> {
    let now = now_ms.div_euclid(1000);
    let account = login.account.as_str();

    if let Some(until) = self.paused().get(account).copied().filter(|until| *until > now) {
      return Err(format!("paused after the endpoint refused the pace, until {until}"));
    }

    let asked = match access_token(&login.credential, now_ms) {
      Ok(token) => self.ask(&token, account, now).await,
      Err(why) => Err(why.to_owned()),
    };
    let windows = match asked {
      Ok(windows) if windows.is_empty() => return Err("no usable windows".to_owned()),
      Ok(windows) => windows,
      Err(failure) => {
        if failure == TOKEN_EXPIRED || failure == TOKEN_REFUSED {
          keeping.mark_unread(account, now).await.map_err(|fault| fault.detail)?;

          if let Credential::Directory(_) = login.credential {
            self.renew(account, now);
          }
        }

        return Err(failure);
      }
    };
    let early = Self::came_back_early(keeping, account, &windows, now).await?;

    keeping
      .replace(
        Slot {
          mode: MODE,
          account,
        },
        &windows,
        now,
      )
      .await
      .map_err(|fault| fault.detail)?;
    keeping.lift_unread(account).await.map_err(|fault| fault.detail)?;

    let mut renewing = locked(&self.renewing);

    if renewing.get(account).is_some_and(|last| last.ended) {
      renewing.remove(account);
    }

    drop(renewing);

    for name in early {
      println!(
        "claude usage for {account}: its {name} window came back before the reset its last reading named"
      );
    }

    Ok(windows)
  }

  /// Every registered account asked once, on or off, each reading replacing that account's previous
  /// windows and nobody else's, and each account answered with the windows it was written with or why
  /// nothing was written; an account that could not be asked keeps what it had.
  pub async fn read_each(
    &self,
    keeping: &Keeping,
    now_ms: i64,
  ) -> Result<Vec<(String, Result<Vec<Window>, String>)>, String> {
    let mut outcomes = Vec::new();

    for login in self.logins.logins().await? {
      let outcome = self.read(keeping, &login, now_ms).await;

      outcomes.push((login.account, outcome));
    }

    Ok(outcomes)
  }
}

impl Reader for ClaudeAccounts {
  fn mode(&self) -> &str {
    MODE
  }

  fn cadence(&self) -> Duration {
    Duration::from_secs(CLAUDE_CODE.usage.pass_seconds)
  }

  fn fresh(&self) -> Duration {
    self.cadence() * CLAUDE_CODE.usage.fresh_passes
  }

  fn pass<'a>(&'a self, keeping: &'a Keeping, now_ms: i64) -> Passing<'a> {
    Box::pin(async move {
      match self.read_each(keeping, now_ms).await {
        Ok(outcomes) => {
          self.said.outcome("the claude accounts", &Ok(()));

          for (account, outcome) in outcomes {
            self.said.outcome(&format!("claude usage for {account}"), &outcome.map(drop));
          }
        }
        Err(why) => {
          self.said.outcome("the claude accounts", &Err(why));
        }
      }
    })
  }
}
