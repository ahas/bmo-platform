//! The harness providers as the routes ask them (docs/provider-presence.md): the owner's switch on each,
//! whether each can answer on this machine, and what each offers an order's entry. The machine's answers are
//! the harness-providers crate's; what the store holds — the switches, the registrations — is read here.

use harness_providers::{ARCHIVE_PROVIDERS, Machine, Offered, OfferedModel, Provider, Reason, provider};
use policy::control_words::CONTROL_WORDS;
use policy::fill_marks;
use policy::reasoning::REASONING;
use sea_orm::DatabaseConnection;

use super::claude_accounts::{self, Judging};
use super::{Refused, set_setting, setting};
use crate::now_ms;

// Where the owner's switch on a provider is kept, by the provider (docs/store.md, 「The settings table」).
const SWITCH: &str = "provider_switch";

/// The scale's levels a run of this model on this provider takes, in the scale's order: the model's own
/// efforts where the provider's catalogue names them, and the provider's words otherwise.
pub async fn levels(
  judging: Judging<'_>,
  machine: &Machine,
  provider: &dyn Provider,
  model: &str,
) -> Vec<String> {
  let dir = if provider.catalogue_under_login() {
    claude_accounts::catalogue_dir(judging, machine).await.ok()
  } else {
    None
  };
  let words = match provider.model_efforts(machine, dir.as_deref(), model).await {
    Some(words) => words,
    None => provider.declared().effort.words.clone(),
  };

  REASONING.levels.iter().filter(|level| words.contains(level)).cloned().collect()
}

/// Why a road of the provider on `model` may not name `advisor`, or nothing where it may
/// (docs/provider-order.md, 「The advisor」): the provider's own answer, read against the catalogue of the login
/// the account walk would take first where its catalogue is kept under a login.
pub async fn advisor_refusal(
  judging: Judging<'_>,
  machine: &Machine,
  provider: &dyn Provider,
  model: &str,
  advisor: &str,
) -> Option<String> {
  let dir = if provider.catalogue_under_login() {
    claude_accounts::catalogue_dir(judging, machine).await.ok()
  } else {
    None
  };

  provider.advisor_refusal(machine, dir.as_deref(), model, advisor).await
}

/// A provider this build carries.
pub fn named(name: &str) -> Result<&'static dyn Provider, Refused> {
  provider(name).ok_or_else(|| Refused::failed(format!("no provider named {name} in this build")))
}

/// Whether the owner has a provider on: its switch, on where it was never thrown.
pub async fn provider_enabled(db: &DatabaseConnection, name: &str) -> Result<bool, Refused> {
  Ok(setting(db, &format!("{SWITCH}:{name}")).await?.as_deref() != Some("off"))
}

/// The providers the owner has on, in the order this build carries them.
pub async fn enabled_providers(db: &DatabaseConnection) -> Result<Vec<String>, Refused> {
  let mut on = vec![];

  for name in ARCHIVE_PROVIDERS {
    if provider_enabled(db, name).await? {
      on.push(name.to_owned());
    }
  }

  Ok(on)
}

/// The owner's switch on a provider thrown (docs/provider-order.md, 「The owner's switch on a provider」): the
/// last provider left on is not turned off, since nothing would answer.
pub async fn set_provider_enabled(db: &DatabaseConnection, name: &str, on: bool) -> Result<(), Refused> {
  named(name)?;

  if !on && enabled_providers(db).await?.iter().all(|one| one == name) {
    return Err(Refused::new(axum::http::StatusCode::CONFLICT, CONTROL_WORDS.fallback.all_off.as_str()));
  }

  set_setting(db, &format!("{SWITCH}:{name}"), if on { "on" } else { "off" }).await
}

/// The registrations a run may name, in the order they were added: each with an address, or naming a card
/// service the arbiter's master places, whether or not an instance of it stands at this moment
/// (docs/own-llm.md, 「An engine the arbiter stands」).
pub async fn offered_registrations(db: &DatabaseConnection) -> Result<Vec<String>, Refused> {
  Ok(
    super::registry::registrations(db)
      .await?
      .into_iter()
      .filter(super::registry::Registration::offered)
      .map(|held| held.name)
      .collect(),
  )
}

/// The code of the reason a provider whose program does not carry a line gives.
pub const NOT_CARRIED: &str = "line-not-carried";

/// Why this machine cannot use a provider now, or nothing where it can: the machine's answer, then the
/// registry's where the provider's models are registrations.
pub async fn missing(
  db: &DatabaseConnection,
  machine: &Machine,
  provider: &dyn Provider,
) -> Result<Option<Reason>, Refused> {
  if let Some(missing) = provider.missing(machine) {
    return Ok(Some(missing));
  }

  match provider.unregistered() {
    Some(reason) if offered_registrations(db).await?.is_empty() => Ok(Some(reason)),
    _ => Ok(None),
  }
}

/// Why a row on this line may not be put on a provider, or nothing where it may: the provider
/// cannot answer here, or its program does not carry the line.
pub async fn offered_reason(
  db: &DatabaseConnection,
  machine: &Machine,
  provider: &dyn Provider,
  line: &str,
) -> Result<Option<Reason>, Refused> {
  if let Some(missing) = missing(db, machine, provider).await? {
    return Ok(Some(missing));
  }

  let declared = provider.declared();

  Ok((!declared.carries(line)).then(|| Reason {
    code: NOT_CARRIED.to_owned(),
    text: fill_marks(&CONTROL_WORDS.fallback.not_carried, &[("provider", &declared.label)]),
  }))
}

/// What a provider offers an order's entry, read afresh: the registrations a run may name, each taking
/// the provider's whole effort vocabulary, for a provider whose models they are; otherwise the
/// provider's own catalogue, under the walking login's directory where it keeps it there. A catalogue
/// that cannot be read is the refusal it gave.
pub async fn offered(
  judging: Judging<'_>,
  machine: &Machine,
  provider: &dyn Provider,
) -> Result<Offered, String> {
  if provider.unregistered().is_some() {
    let efforts: Vec<Option<String>> = provider
      .declared()
      .effort
      .vocabulary()
      .into_iter()
      .map(|word| word.map(str::to_owned))
      .collect();
    let models = offered_registrations(judging.db).await.map_err(|refused| refused.error)?;

    return Ok(Offered {
      models: models
        .into_iter()
        .map(|id| OfferedModel {
          id,
          efforts: efforts.clone(),
          free: None,
          in_service: None,
        })
        .collect(),
      stale_read_at: None,
    });
  }

  let dir = if provider.catalogue_under_login() {
    Some(
      claude_accounts::catalogue_dir(judging, machine)
        .await
        .map_err(|refused| refused.error)?,
    )
  } else {
    None
  };

  provider.offered(machine, dir.as_deref(), now_ms() as f64).await
}
