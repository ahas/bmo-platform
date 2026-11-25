//! What a request cost, as the costs hook hands it to the front (docs/own-llm.md, 「What a request costs」),
//! written as one row of `gateway_requests` on a connection of its own: the only thing the launcher writes to
//! the store, one row a statement. A write that fails is logged and dropped.

use std::time::{SystemTime, UNIX_EPOCH};

use sea_orm::DatabaseConnection;
use serde::Deserialize;

/// One request's cost as the hook names it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cost {
  /// The payer the front named the request to the child in `x-bmo-payer`: an account, or `platform`.
  #[serde(default)]
  pub payer: Option<String>,
  /// The run its `x-bmo-run` header names.
  #[serde(default)]
  pub run: Option<String>,
  /// The program's session its Claude Code session header names.
  #[serde(default)]
  pub session: Option<String>,
  #[serde(default)]
  pub model: Option<String>,
  /// LiteLLM's cost of it, in US dollars.
  #[serde(default)]
  pub cost_usd: Option<f64>,
  #[serde(default)]
  pub input_tokens: Option<i64>,
  #[serde(default)]
  pub output_tokens: Option<i64>,
}

impl Cost {
  /// The account that pays, none for the platform.
  pub fn account(&self) -> Option<i64> {
    self.payer.as_deref().and_then(|payer| payer.trim().parse().ok())
  }

  /// The cost in millionths of a US dollar; nothing where none is named or it is no number.
  pub fn micros(&self) -> i64 {
    self.cost_usd.filter(|usd| usd.is_finite()).map_or(0, |usd| (usd * 1e6).round() as i64)
  }
}

fn blank_is_none(value: Option<&str>) -> Option<String> {
  value.map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned)
}

/// The connection the request records are written on.
pub struct Records {
  db: DatabaseConnection,
}

impl Records {
  /// Records on the store at the address, connecting only once the first is written.
  pub fn new(address: &str) -> Result<Records, String> {
    store::connect_lazy(address, store::pool(1))
      .map(|db| Records { db })
      .map_err(|fault| format!("the store at its address cannot be connected to: {fault}"))
  }

  /// One request's cost written down, a failure logged and dropped.
  pub async fn write(&self, cost: &Cost) {
    let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis() as i64);
    let written = store::run(
      &self.db,
      "INSERT INTO gateway_requests (at, account, model, run, session, input_tokens, output_tokens, \
       cost_micros) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
      vec![
        at.into(),
        cost.account().into(),
        blank_is_none(cost.model.as_deref()).unwrap_or_default().into(),
        blank_is_none(cost.run.as_deref()).into(),
        blank_is_none(cost.session.as_deref()).into(),
        cost.input_tokens.into(),
        cost.output_tokens.into(),
        cost.micros().into(),
      ],
    )
    .await;

    if let Err(fault) = written {
      eprintln!(
        "llm-gateway: the cost of a request on {} was not recorded: {fault}",
        cost.model.as_deref().unwrap_or("(none named)")
      );
    }
  }
}
