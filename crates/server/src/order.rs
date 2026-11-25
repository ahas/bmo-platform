//! A list of roads the road judgement walks (docs/provider-order.md, 「What a road is」): a supervisor's one road,
//! and the roads of a role's agents a run of the role may take, each provider once, with the model, the effort, the thinking and on the registry's
//! provider the answer ceiling it runs on there. This module reads an order off its row, judges one a write hands in, and reads what a
//! provider has left (「Quota: what a provider has left」), each provider answering out of its own member.

use harness_providers::balance::OffersBalance;
use harness_providers::{Machine, Provider};
use policy::reasoning::REASONING;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::control::claude_accounts::{self, Judging};
use crate::control::harness;
use crate::now_ms;
use crate::places::Ground;
use crate::repositories::Pin;
use cache_siren::{FULL, Slot};

/// One entry of an order: a provider and what the row runs on there.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
  pub provider: String,
  pub model: String,
  #[serde(default)]
  pub effort: Option<String>,
  /// `on`, `off`, or none for the provider's own.
  #[serde(default)]
  pub thinking: Option<String>,
  #[serde(default)]
  pub answer_tokens: Option<i64>,
  /// The model the run's program may consult, a model of the provider or a family word its program takes;
  /// none for none (docs/provider-order.md, 「The advisor」).
  #[serde(default)]
  pub advisor: Option<String>,
}

impl Entry {
  /// Whether this road is another than `other` in what its sessions stand on: its provider, model, effort,
  /// thinking or answer ceiling. The advisor stands outside the prefix a session's cache keeps, so a road that
  /// moved only its advisor has not moved (docs/provider-order.md, 「The advisor」).
  pub fn moved_from(&self, other: &Entry) -> bool {
    Entry {
      advisor: None,
      ..self.clone()
    } != Entry {
      advisor: None,
      ..other.clone()
    }
  }

  /// The advisor a run of this road consults: the one it names, where its provider takes an advisor.
  pub fn advisor_taken(&self) -> Option<String> {
    self.advisor.clone().filter(|_| {
      harness_providers::provider(&self.provider).is_some_and(|provider| provider.takes_advisor())
    })
  }

  /// Whether the entry asks the run to think, where it names it.
  pub fn thinks(&self) -> Option<bool> {
    match self.thinking.as_deref() {
      Some("on") => Some(true),
      Some("off") => Some(false),
      _ => None,
    }
  }
}

/// A row's order, and the provider the owner forced as the row's active provider where
/// one stands (docs/provider-order.md, 「What a road is」), which is kept beside the order rather
/// than in it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
  pub entries: Vec<Entry>,
  #[serde(skip)]
  pub forced: Option<Pin>,
}

impl Order {
  /// The order a row keeps: its entries as the JSON list it holds, a list that does not read being none.
  pub fn of_row(entries: &str) -> Self {
    Self {
      entries: serde_json::from_str(entries).unwrap_or_default(),
      forced: None,
    }
  }

  /// The order with the provider and model a row's columns force as its active provider, none where either
  /// is missing or empty.
  pub fn forcing(self, provider: Option<String>, model: Option<String>) -> Self {
    Self {
      forced: Pin::of_parts(provider, model),
      ..self
    }
  }

  /// The entry a run goes out on where its provider and model are fixed over the order — an item's model, a
  /// forced provider or a repository's pin: that provider and model, at the effort, thinking and answer
  /// ceiling the order's entry for that road names where it holds one, else its entry for the provider, else
  /// the provider's own.
  pub fn entry_on(&self, fixed: &Pin) -> Entry {
    let own = self.entry_of(&fixed.provider, &fixed.model);

    Entry {
      provider: fixed.provider.clone(),
      model: fixed.model.clone(),
      effort: own.and_then(|entry| entry.effort.clone()),
      thinking: own.and_then(|entry| entry.thinking.clone()),
      answer_tokens: own.and_then(|entry| entry.answer_tokens),
      advisor: own.and_then(|entry| entry.advisor.clone()),
    }
  }

  /// The entry of a provider, where the order names it.
  pub fn entry(&self, provider: &str) -> Option<&Entry> {
    self.entries.iter().find(|entry| entry.provider == provider)
  }

  /// The entry of a road, a provider and one model of it, where the order names it; else the entry of the
  /// provider. A role's roads are its agents', and several of them may stand on one provider.
  pub fn entry_of(&self, provider: &str, model: &str) -> Option<&Entry> {
    self
      .entries
      .iter()
      .find(|entry| entry.provider == provider && entry.model == model)
      .or_else(|| self.entry(provider))
  }

  /// Whether the order is empty, so the row's runs follow the supervisor that asks them.
  pub fn follows_supervisor(&self) -> bool {
    self.entries.is_empty()
  }

  /// The first entry, where the order holds one.
  pub fn first(&self) -> Option<&Entry> {
    self.entries.first()
  }

  /// The list as a row keeps it.
  pub fn entries_text(&self) -> String {
    serde_json::to_string(&self.entries).unwrap_or_else(|_| "[]".to_owned())
  }

  /// The compaction threshold a row added on this order is given where it names none: its first provider's
  /// declared default, 0.85 where it declares none (docs/supervisors.md, 「What a provider declares for a
  /// conversation」).
  pub fn default_fold_ratio(&self) -> f64 {
    self
      .first()
      .and_then(|entry| harness_providers::provider(&entry.provider))
      .and_then(|provider| provider.declared().fold_ratio)
      .unwrap_or(DEFAULT_FOLD_RATIO)
  }
}

/// The compaction threshold of a row whose first provider declares none.
pub const DEFAULT_FOLD_RATIO: f64 = 0.85;

/// The field an advisor a road may not name is refused under.
pub const ADVISOR: &str = "advisor";

/// Why an order a write hands in is refused: the field it names and one English sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
  pub field: String,
  pub error: String,
}

fn refused(field: impl Into<String>, error: impl Into<String>) -> Refusal {
  Refusal {
    field: field.into(),
    error: error.into(),
  }
}

/// Whether an empty list is an order: a supervisor's never is, and a role's is, its runs then following the
/// supervisor that asks them (docs/provider-order.md, 「What a road is」).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Empty {
  Refused,
  FollowsSupervisor,
}

/// An order a write hands in, judged for a row whose runs go out on `line` (docs/api.md, 「supervisors」): a
/// list of providers agency carries, empty only where `empty` takes one, each once, whose program carries the line, each on a model it
/// holds at an effort the model takes. A catalogue that cannot be read refuses
/// nothing.
pub async fn judge(
  judging: Judging<'_>,
  machine: &Machine,
  line: &str,
  entries: &Value,
  empty: Empty,
) -> Result<Order, Refusal> {
  let read: Vec<Entry> = serde_json::from_value(entries.clone())
    .map_err(|_| refused("order", "The order is a list of providers, each with its model."))?;

  if read.is_empty() && empty == Empty::Refused {
    return Err(refused("order", "An order names at least one provider."));
  }

  for (at, entry) in read.iter().enumerate() {
    let field = format!("order[{at}]");
    let Some(provider) = harness_providers::provider(&entry.provider) else {
      return Err(refused(field, format!("No provider named {} is carried here.", entry.provider)));
    };

    if read[..at].iter().any(|before| before.provider == entry.provider) {
      return Err(refused(field, format!("{} stands in the order twice.", entry.provider)));
    }

    if !provider.declared().carries(line) {
      return Err(refused(field, format!("{} does not carry the {line} line.", entry.provider)));
    }

    if entry.model.trim().is_empty() {
      return Err(refused(field, format!("The entry of {} names no model.", entry.provider)));
    }

    if let Some(why) = unheld(judging, machine, provider, &entry.model).await {
      return Err(refused(field, why));
    }

    if let Some(effort) = &entry.effort {
      if !REASONING.levels.contains(effort) {
        return Err(refused(field, format!("{effort} is no level of the effort scale.")));
      }

      let levels = harness::levels(judging, machine, provider, &entry.model).await;

      if !levels.contains(effort) {
        return Err(refused(
          field,
          format!("{} on {} does not take the effort {effort}.", entry.model, entry.provider),
        ));
      }
    }

    if let Some(thinking) = &entry.thinking
      && thinking != "on"
      && thinking != "off"
    {
      return Err(refused(field, "Thinking is on, off or left to the provider."));
    }

    if let Some(advisor) = &entry.advisor
      && let Some(why) = harness::advisor_refusal(judging, machine, provider, &entry.model, advisor).await
    {
      return Err(refused(ADVISOR, why));
    }
  }

  Ok(Order {
    entries: read,
    forced: None,
  })
}

/// A provider and one model of it a write names — a repository's pin — judged as one entry of an order for `line` is: a provider agency carries whose program
/// carries the line, on a model it holds. Answers the refusal's sentence.
pub async fn judge_one(
  judging: Judging<'_>,
  machine: &Machine,
  line: &str,
  named: &Value,
) -> Result<Pin, String> {
  let entry = serde_json::json!([{ "provider": named["provider"], "model": named["model"] }]);
  let judged = judge(judging, machine, line, &entry, Empty::Refused)
    .await
    .map_err(|refusal| refusal.error)?;

  judged
    .entries
    .into_iter()
    .next()
    .map(|first| Pin {
      provider: first.provider,
      model: first.model,
    })
    .ok_or_else(|| "A provider and a model are named.".to_owned())
}

// Why a provider does not hold a model, or none where it does or its catalogue cannot be read.
async fn unheld(
  judging: Judging<'_>,
  machine: &Machine,
  provider: &dyn Provider,
  model: &str,
) -> Option<String> {
  match harness::offered(judging, machine, provider).await {
    Ok(offered) if offered.models.iter().any(|one| one.id == model) => None,
    Ok(_) => Some(format!("{} does not hold the model {model}.", provider.name())),
    Err(_) => None,
  }
}

/// What a provider has left as an order reads it (docs/provider-order.md, 「Quota: what a provider has
/// left」): the fraction left by its window with the least left, where it reports windows; its balance,
/// where it keeps one; and whether anything of it is left.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quota {
  pub has: bool,
  pub left: Option<f64>,
  pub window: Option<String>,
  pub balance: Option<f64>,
  /// The currency the balance is in, when it was seen in epoch seconds, and the request allowance read
  /// with it.
  pub currency: Option<String>,
  pub balance_seen_at: Option<i64>,
  pub requests: Option<cache_siren::balance::Requests>,
  pub resets_at: Option<i64>,
  /// The login that counts first, on a provider that holds logins.
  #[serde(skip)]
  pub account: Option<String>,
}

/// One window's left: none where it has reset by now, else what it has not used, nothing once it is full.
pub fn window_left(used: f64, resets_at: Option<i64>, now: i64) -> Option<f64> {
  if resets_at.is_some_and(|at| at <= now) {
    return None;
  }

  Some((FULL - used).max(0.0))
}

/// The window with the least left of a reading, standing at `now` in epoch seconds.
pub fn least_left(reading: &cache_siren::Reading, now: i64) -> Option<(f64, String, Option<i64>)> {
  reading
    .standing(now)
    .filter_map(|kept| {
      let window = &kept.window;

      window_left(window.utilization, window.resets_at, now)
        .map(|left| (left, window.name.clone(), window.resets_at))
    })
    .min_by(|one, other| one.0.total_cmp(&other.0))
}

/// What a provider has left now, read afresh out of what agency keeps, quota while anything is left; of a
/// provider that holds logins, what the logins open to `model` have left, or every open login where none is
/// named; of a provider that offers a balance, quota whatever the balance where `model` is one it says costs
/// nothing.
pub async fn quota(
  ground: &Ground,
  provider: &dyn Provider,
  user: impl Into<claude_accounts::Carrying>,
  model: Option<&str>,
) -> Quota {
  let user = user.into();
  let now = now_ms().div_euclid(1000);

  if provider.holds_logins() {
    return logins_quota(ground, provider, user, model, now).await;
  }

  if let Some(offers) = provider.balance() {
    let kept = ground
      .windows
      .balance(Slot {
        mode: provider.name(),
        account: "",
      })
      .await
      .ok()
      .flatten();

    return match kept {
      Some(kept) => Quota {
        has: kept.balance.amount > 0.0 || !balance_closes(ground, offers, model),
        balance: Some(kept.balance.amount),
        currency: Some(kept.balance.currency),
        balance_seen_at: Some(kept.seen_at),
        requests: kept.balance.requests,
        ..Quota::default()
      },
      None => Quota {
        has: true,
        ..Quota::default()
      },
    };
  }

  let reading = ground
    .windows
    .read(Slot {
      mode: provider.name(),
      account: "",
    })
    .await
    .unwrap_or_default();

  match least_left(&reading, now) {
    Some((left, window, resets_at)) => Quota {
      has: left > 0.0,
      left: Some(left),
      window: Some(window),
      resets_at,
      ..Quota::default()
    },
    None => Quota {
      has: true,
      ..Quota::default()
    },
  }
}

/// Whether a provider's spent balance closes a run on `model` (docs/provider-order.md, 「Quota: what a
/// provider has left」): a model the provider says costs nothing goes out on it, and a read that names no
/// model asks of the provider as a whole, which it closes.
pub fn balance_closes(ground: &Ground, offers: &dyn OffersBalance, model: Option<&str>) -> bool {
  model.is_none_or(|model| offers.spent_closes(&ground.machine(), model))
}

// A provider that holds logins has quota while one of its open logins has any left by its own window with the
// least left; that login counts first (docs/provider-order.md, 「Quota: what a provider has
// left」).
async fn logins_quota(
  ground: &Ground,
  provider: &dyn Provider,
  user: claude_accounts::Carrying,
  model: Option<&str>,
  now: i64,
) -> Quota {
  let judging = Judging::of(ground);
  let Ok(open) = claude_accounts::walk_for_user(judging, user, model).await else {
    return Quota::default();
  };
  let mut best: Option<Quota> = None;

  for account in open {
    let reading = ground
      .windows
      .read(Slot {
        mode: provider.name(),
        account: &account.id,
      })
      .await
      .unwrap_or_default();
    let quota = match least_left(&reading, now) {
      Some((left, window, resets_at)) => Quota {
        has: left > 0.0,
        left: Some(left),
        window: Some(window),
        resets_at,
        account: Some(account.id.clone()),
        ..Quota::default()
      },
      None => Quota {
        has: true,
        account: Some(account.id.clone()),
        ..Quota::default()
      },
    };

    if quota.has {
      return quota;
    }

    if best
      .as_ref()
      .is_none_or(|best| best.resets_at.is_none() || quota.resets_at < best.resets_at)
    {
      best = Some(quota);
    }
  }

  best.unwrap_or_default()
}

/// The kinds of event after which what a provider has left may have moved: a usage reading, an account's
/// change, a rest and a road's fact.
pub const QUOTA_MOVES: [&str; 4] = ["usage.changed", "account.changed", "rest.changed", "road.changed"];

/// A watch on the events after which a run waiting for quota asks its order again: a wait for quota ends on one
/// of them, never on a clock (docs/provider-order.md, 「Whether a road can answer」).
pub fn quota_watch(ground: &Ground) -> crate::events::WireSubscription {
  ground
    .events
    .subscribe_wire(Some(QUOTA_MOVES.iter().map(|kind| (*kind).to_owned()).collect()))
}

/// The entry a role's order names for a provider, read afresh: what a run of the role goes out on there.
pub async fn role_entry<C: sea_orm::ConnectionTrait>(
  db: &C,
  role: &str,
  provider: &str,
) -> Result<Entry, String> {
  let held = crate::agent_roles::named(db, role).await.map_err(|why| why.detail)?;
  let held = held.ok_or_else(|| crate::agent_roles::unknown(role))?;

  held
    .order
    .entry(provider)
    .cloned()
    .ok_or_else(|| format!("the {role} role's order holds no entry for {provider}"))
}

/// A supervisor's one road, read afresh, where it stands on `provider`: what its turns go out on there.
pub async fn supervisor_entry<C: sea_orm::ConnectionTrait>(
  db: &C,
  supervisor: &str,
  provider: &str,
) -> Result<Entry, String> {
  let held = crate::supervisors::named(db, supervisor).await.map_err(|why| why.detail)?;
  let held = held.ok_or_else(|| format!("No supervisor named {supervisor} is held"))?;

  held
    .road
    .filter(|road| road.provider == provider)
    .ok_or_else(|| format!("{supervisor}'s road is not on {provider}"))
}

/// The entry the row a run names — a supervisor's, else a role's of that name — holds for a provider. A role
/// whose order is empty follows the supervisor that asks it (docs/provider-order.md, 「Runs on another's road」):
/// its entry is that supervisor's for the provider, and a run of such a role that names no supervisor is refused.
/// A role whose agents follow their asker and name roads of their own takes the supervisor's entry on that
/// supervisor's provider and its own elsewhere.
pub async fn row_entry<C: sea_orm::ConnectionTrait>(
  db: &C,
  row: &str,
  provider: &str,
  supervisor: Option<&str>,
) -> Result<Entry, String> {
  if crate::supervisors::named(db, row).await.map_err(|why| why.detail)?.is_some() {
    return supervisor_entry(db, row, provider).await;
  }

  let held = crate::agent_roles::named(db, row).await.map_err(|why| why.detail)?;
  let held = held.ok_or_else(|| crate::agent_roles::unknown(row))?;

  if held.order.follows_supervisor() {
    return match supervisor {
      Some(supervisor) => supervisor_entry(db, supervisor, provider).await,
      None => Err(format!("the {row} role follows the supervisor that asks it, and this run names none")),
    };
  }

  // A role whose agents follow their asker while naming roads of their own goes by the asking supervisor's entry
  // on that supervisor's provider, and by its agents' own entry on any other, the road it stands on where the
  // supervisor's cannot answer (docs/role-agents.md, 「An agent that follows its asker」).
  if held.runs_follow()
    && let Some(supervisor) = supervisor
    && let Ok(entry) = supervisor_entry(db, supervisor, provider).await
  {
    return Ok(entry);
  }

  held
    .order
    .entry(provider)
    .cloned()
    .ok_or_else(|| format!("the {row} role's order holds no entry for {provider}"))
}
