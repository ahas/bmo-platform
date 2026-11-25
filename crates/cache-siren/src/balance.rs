//! The balance reader (docs/usage-windows.md, 「Balances」): what each paid account has left, asked on a
//! clock of the address its provider declares with the key the account holds, and kept beside the windows.
//! The reader knows no provider: its source is one provider's member, answering who is asked, where, with
//! what key, and what an answer states.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::reader::{Passing, Reader, Said};
use crate::windows::{Keeping, Slot};

/// What an account has left: an amount in a currency.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
  pub amount: f64,
  pub currency: String,
  /// The request allowance read with it, where its provider states one.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub requests: Option<Requests>,
}

/// A request allowance as a provider states it: how many requests of the day are used, the day's ceiling
/// and how many are left, and what the allowance covers where the provider declares it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Requests {
  pub used: i64,
  pub limit: i64,
  pub left: i64,
  #[serde(default)]
  pub scope: Option<String>,
}

/// A balance as the keeping holds it: the balance, and when it was seen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeptBalance {
  #[serde(flatten)]
  pub balance: Balance,
  pub seen_at: i64,
}

/// One account to ask on a pass: the account its balance is kept under, the empty one for the provider's
/// own road; the address asked; and the key it is asked with, or why there is none.
pub struct BalanceAsk {
  pub account: String,
  pub url: String,
  pub key: Result<String, String>,
  /// The second address, asked with the same key for the request allowance, where the provider declares one.
  pub requests_url: Option<String>,
}

/// The accounts asked being answered.
pub type Asking<'a> = Pin<Box<dyn Future<Output = Result<Vec<BalanceAsk>, String>> + Send + 'a>>;

/// One provider's balance as its member declares the reading.
pub trait BalanceSource: Send + Sync {
  /// The provider whose balance it is, as a slot names it.
  fn mode(&self) -> &str;
  /// How often it is read.
  fn cadence(&self) -> Duration;
  /// How long a reading stands after it was seen.
  fn fresh(&self) -> Duration;
  /// The accounts asked on this pass.
  fn asks(&self) -> Asking<'_>;
  /// The balance an answer states, or why it states none.
  fn balance_of(&self, answer: &Value) -> Result<Balance, String>;
  /// The request allowance the second address's answer states, or none.
  fn requests_of(&self, _answer: &Value) -> Option<Requests> {
    None
  }
}

/// The reader on the clock: its source, the client it asks on, and each account's failure as last said.
pub struct BalanceReader {
  source: Arc<dyn BalanceSource>,
  http: reqwest::Client,
  said: Said,
}

impl BalanceReader {
  pub fn new(source: Arc<dyn BalanceSource>) -> Self {
    Self {
      http: reqwest::Client::new(),
      source,
      said: Said::default(),
    }
  }

  // One address asked with the key, answered with what it states as JSON.
  async fn answer_of(&self, url: &str, key: &str) -> Result<Value, String> {
    // The ask waits for its answer; its end is the answer, or the connection refused or broken.
    let answer = self
      .http
      .get(url)
      .bearer_auth(key)
      .send()
      .await
      .map_err(|_| "the request failed".to_owned())?;

    if !answer.status().is_success() {
      return Err(format!("HTTP {}", answer.status().as_u16()));
    }

    answer.json().await.map_err(|_| "the answer is not JSON".to_owned())
  }

  // The balance asked, and the request allowance after it where a second address is declared: one that
  // cannot be read leaves the balance with none.
  async fn ask(&self, ask: &BalanceAsk, key: &str) -> Result<Balance, String> {
    let mut balance = self.source.balance_of(&self.answer_of(&ask.url, key).await?)?;

    if let Some(url) = &ask.requests_url {
      balance.requests =
        self.answer_of(url, key).await.ok().and_then(|answer| self.source.requests_of(&answer));
    }

    Ok(balance)
  }

  // One account asked and its balance kept; why not where it was not.
  async fn read(&self, keeping: &Keeping, ask: &BalanceAsk, now_ms: i64) -> Result<Balance, String> {
    let key = ask.key.as_deref().map_err(|why| why.to_owned())?;
    let balance = self.ask(ask, key).await?;
    let slot = Slot {
      mode: self.source.mode(),
      account: &ask.account,
    };

    keeping
      .replace_balance(slot, &balance, now_ms.div_euclid(1000))
      .await
      .map_err(|fault| fault.detail)?;

    Ok(balance)
  }

  /// Every account the source names asked once, each reading replacing that account's balance and nobody
  /// else's, and each account answered with the balance kept or why nothing was; an account that could not
  /// be asked keeps what it had.
  pub async fn read_each(
    &self,
    keeping: &Keeping,
    now_ms: i64,
  ) -> Result<Vec<(String, Result<Balance, String>)>, String> {
    let mut outcomes = Vec::new();

    for ask in self.source.asks().await? {
      let outcome = self.read(keeping, &ask, now_ms).await;

      outcomes.push((ask.account, outcome));
    }

    Ok(outcomes)
  }

  // What a failure of one account is said under.
  fn what(&self, account: &str) -> String {
    let mode = self.source.mode();

    if account.is_empty() {
      format!("{mode} balance")
    } else {
      format!("{mode} balance for {account}")
    }
  }
}

impl Reader for BalanceReader {
  fn mode(&self) -> &str {
    self.source.mode()
  }

  fn cadence(&self) -> Duration {
    self.source.cadence()
  }

  fn fresh(&self) -> Duration {
    self.source.fresh()
  }

  fn pass<'a>(&'a self, keeping: &'a Keeping, now_ms: i64) -> Passing<'a> {
    Box::pin(async move {
      let accounts = format!("the {} balances", self.source.mode());

      match self.read_each(keeping, now_ms).await {
        Ok(outcomes) => {
          self.said.outcome(&accounts, &Ok(()));

          for (account, outcome) in outcomes {
            self.said.outcome(&self.what(&account), &outcome.map(drop));
          }
        }
        Err(why) => {
          self.said.outcome(&accounts, &Err(why));
        }
      }
    })
  }
}
