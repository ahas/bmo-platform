//! The supply hook (docs/supply.md, 「The supply hook」; docs/api.md, 「Hook points」): the consumer that answers who
//! pays for a person's run is asked its order as the run starts, and asked to reserve, extend, capture and release
//! as it spends. Nobody registered, the order is none and nothing else is asked; registered and not answering, the
//! order is personal supply alone and nothing is reserved, so a person's run never goes out on the platform's
//! supply without a reservation. Its registration outlives a restart.

use serde::{Deserialize, Serialize};

use crate::hooks::{HookPoint, Hooks};

/// The supply hook's point.
pub struct SupplyHook;

/// Whether a run is a job's turn or a supervisor's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
  Job,
  Turn,
}

/// What a run asks for its order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderAsk {
  pub run: String,
  pub account: i64,
  pub provider: String,
  pub model: String,
  pub kind: Kind,
  pub job: Option<i64>,
  pub supervisor: Option<String>,
  pub room: Option<i64>,
}

/// One ask that moves money: its run, its step, the account and source it is for, the consumer's reservation
/// where one stands, the amount it names and its idempotency key.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoneyAsk {
  pub run: String,
  pub step: i64,
  pub account: i64,
  pub source: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reservation: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub spent_usd: Option<f64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cost_usd: Option<f64>,
  pub key: String,
}

/// One ask of the hook, by its `op`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum SupplyAsk {
  Order(OrderAsk),
  Reserve(MoneyAsk),
  Extend(MoneyAsk),
  Capture(MoneyAsk),
  Release(MoneyAsk),
}

impl SupplyAsk {
  /// The word the ask is known by, its idempotency key's last part.
  pub fn op(&self) -> &'static str {
    match self {
      Self::Order(_) => "order",
      Self::Reserve(_) => "reserve",
      Self::Extend(_) => "extend",
      Self::Capture(_) => "capture",
      Self::Release(_) => "release",
    }
  }
}

/// What the hook answers, each field read by the ask it answers: `sources` an order's, `accepted` with the
/// reservation, its whole amount and its expiry a reservation's or an extension's, `taken` a capture's or a
/// release's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplyAnswer {
  #[serde(default)]
  pub sources: Option<Vec<String>>,
  #[serde(default)]
  pub accepted: Option<bool>,
  #[serde(default)]
  pub reservation: Option<String>,
  #[serde(default)]
  pub amount_usd: Option<f64>,
  #[serde(default)]
  pub expires_at: Option<i64>,
  #[serde(default)]
  pub reopens_at: Option<i64>,
  #[serde(default)]
  pub taken: Option<bool>,
}

impl SupplyAnswer {
  fn refused() -> Self {
    Self {
      accepted: Some(false),
      taken: Some(false),
      ..Self::default()
    }
  }
}

impl HookPoint for SupplyHook {
  type Ask = SupplyAsk;
  type Answer = SupplyAnswer;

  const NAME: &'static str = "supply";
  const KEPT: bool = true;

  // Nobody registered: no order, so a person's run takes personal supply alone.
  fn default_answer(_ask: SupplyAsk) -> SupplyAnswer {
    SupplyAnswer::refused()
  }

  // Registered and silent: personal supply alone, and no money moves.
  fn unanswered(ask: SupplyAsk) -> SupplyAnswer {
    match ask {
      SupplyAsk::Order(_) => SupplyAnswer {
        sources: Some(vec![crate::supply::PERSONAL.to_owned()]),
        ..SupplyAnswer::refused()
      },
      _ => SupplyAnswer::refused(),
    }
  }
}

crate::wired!(SupplyHook);

/// Whether a consumer answers the supply hook now.
pub fn registered(hooks: &Hooks) -> bool {
  hooks.registered::<SupplyHook>()
}

/// The order the hook answers for a run: none where nobody is registered.
pub async fn order(hooks: &Hooks, ask: OrderAsk) -> Option<Vec<String>> {
  if !registered(hooks) {
    return None;
  }

  Some(hooks.ask::<SupplyHook>(SupplyAsk::Order(ask)).await.sources.unwrap_or_default())
}

/// The hook's answer to an ask that moves money; refused where nobody is registered.
pub async fn money(hooks: &Hooks, ask: SupplyAsk) -> SupplyAnswer {
  if !registered(hooks) {
    return SupplyAnswer::refused();
  }

  hooks.ask::<SupplyHook>(ask).await
}
