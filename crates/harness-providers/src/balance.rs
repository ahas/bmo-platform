//! What a provider that offers a balance declares of its reading (docs/usage-windows.md, 「Balances」): the
//! address asked, the key its own road asks with, which endpoint accounts it answers for, and what its
//! answer states. A provider that offers none declares nothing, and no reader asks it.

use std::collections::BTreeMap;

use policy::providers::Balance as Declared;
use serde_json::Value;

use crate::Machine;

/// What an account has left, as its provider's answer states it.
#[derive(Clone, Debug, PartialEq)]
pub struct Balance {
  pub amount: f64,
  pub currency: String,
}

/// A request allowance as a provider's answer states it: how many requests of the day are used, the day's
/// ceiling and how many are left, and what the allowance covers where the provider declares it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requests {
  pub used: i64,
  pub limit: i64,
  pub left: i64,
  pub scope: Option<String>,
}

/// How one provider's balance is read.
pub trait OffersBalance: Sync {
  /// What its policy file declares of the reading.
  fn declared(&self) -> &'static Declared;

  /// The key its own road asks with on this machine, or why there is none. The key goes into the one
  /// request and is never kept.
  fn key(&self, machine: &Machine) -> Result<String, String>;

  /// The balance an answer states, or why it states none.
  fn read(&self, answer: &Value) -> Result<Balance, String>;

  /// The address asked on this machine: the stand-in's its environment names, or the provider's own.
  fn url(&self, machine: &Machine) -> String {
    let vars: BTreeMap<String, String> =
      machine.every_var().map(|(name, value)| (name.to_owned(), value.to_owned())).collect();

    self.declared().url_on(&vars).to_owned()
  }

  /// The second address asked on this machine, where the provider declares one for a request allowance.
  fn requests_url(&self, machine: &Machine) -> Option<String> {
    let vars: BTreeMap<String, String> =
      machine.every_var().map(|(name, value)| (name.to_owned(), value.to_owned())).collect();

    self.declared().requests_url_on(&vars).map(str::to_owned)
  }

  /// Whether a spent balance closes a run on `model` on this machine. Every model costs unless its
  /// provider answers that this one costs nothing.
  fn spent_closes(&self, machine: &Machine, model: &str) -> bool {
    let _ = (machine, model);

    true
  }

  /// The request allowance the second address's answer states, or none.
  fn requests(&self, _answer: &Value) -> Option<Requests> {
    None
  }

  /// Whether an endpoint account whose base address is `endpoint` is one of this provider's: its address
  /// stands on the same origin as the balance address.
  fn serves(&self, endpoint: &str, machine: &Machine) -> bool {
    let origin = |address: &str| url::Url::parse(address).ok().map(|parsed| parsed.origin());

    match (origin(endpoint), origin(&self.url(machine))) {
      (Some(endpoint), Some(balance)) => endpoint.is_tuple() && endpoint == balance,
      _ => false,
    }
  }
}

/// A key file's key: its content trimmed, or why there is none.
pub fn key_in_file(path: &std::path::Path) -> Result<String, String> {
  let text = std::fs::read_to_string(path).map_err(|_| "the key file is unreadable".to_owned())?;
  let key = text.trim();

  if key.is_empty() {
    Err("the key file is empty".to_owned())
  } else {
    Ok(key.to_owned())
  }
}

/// A figure an answer states as a number or as the text of one.
pub fn figure(value: Option<&Value>) -> Option<f64> {
  let value = value?;

  value.as_f64().or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
}
