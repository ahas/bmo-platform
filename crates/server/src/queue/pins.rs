//! A worker's pin as the providers that would run it read it (docs/own-llm.md, 「A worker pinned to a
//! provider」): `auto` for none, a provider, or `<provider>::<model>` on a provider a pin may name a model
//! of. Each provider answers for itself which lines it carries and whether a pin names one of its models;
//! the registrations a model names are read from the store here, and a catalogue from where its provider
//! keeps it.

use harness_providers::{ARCHIVE_PROVIDERS, Provider};
use policy::fill_marks;
use policy::queue_words::QUEUE_WORDS;

use super::routes::job_line;
use crate::control::Refused;
use crate::control::claude_accounts::Judging;
use crate::control::harness::{self, named};
use crate::control::registry;
use crate::places::Ground;

/// What a worker is pinned to: a provider and one model of it, or neither for no pin.
#[derive(Debug, PartialEq, Eq)]
pub struct Pin {
  pub provider: Option<String>,
  pub model: Option<String>,
}

/// The pin a value names, or the sentence saying why nothing could run on it.
pub async fn pin_choice(ground: &Ground, value: &str) -> Result<Result<Pin, String>, Refused> {
  if value == "auto" {
    return Ok(Ok(Pin {
      provider: None,
      model: None,
    }));
  }

  // The provider and the piece after it, whatever follows a second separator left aside.
  let mut pieces = value.split("::");
  let name = pieces.next().unwrap_or_default();
  let model = pieces.next();

  if !ARCHIVE_PROVIDERS.contains(&name) {
    return Ok(Err(fill_marks(
      &QUEUE_WORDS.pins.unknown,
      &[("providers", &ARCHIVE_PROVIDERS.join(", "))],
    )));
  }

  let provider = named(name)?;
  let label = &provider.declared().label;

  if !provider.declared().carries(job_line()) {
    return Ok(Err(fill_marks(&QUEUE_WORDS.pins.no_line, &[("label", label)])));
  }

  let Some(model) = model else {
    return Ok(Ok(Pin {
      provider: Some(name.to_owned()),
      model: None,
    }));
  };

  if !provider.pins_models() {
    return Ok(Err(fill_marks(&QUEUE_WORDS.pins.no_model, &[("label", label)])));
  }

  if let Some(refusal) = model_refusal(ground, provider, model).await? {
    return Ok(Err(refusal));
  }

  Ok(Ok(Pin {
    provider: Some(name.to_owned()),
    model: Some(model.to_owned()),
  }))
}

/// Why a provider a pin may name a model of does not hold one by this name, or none where it holds it
/// (docs/model-tiers.md, 「An agent's own model」): the registry's registrations with an address, read from
/// the store; the models a provider with a road of its own declares; or the catalogue a provider keeps,
/// Claude Code's CLI catalogue. A catalogue that cannot be read says nothing against the name, so it
/// refuses nothing.
pub async fn model_refusal(
  ground: &Ground,
  provider: &dyn Provider,
  model: &str,
) -> Result<Option<String>, Refused> {
  if let Some(not_registered) = provider.not_registered() {
    let held = if provider.unregistered().is_some() {
      registry::registration(&ground.db, model).await?.is_some_and(|held| held.offered())
    } else {
      provider.holds_model(&ground.machine(), model)
    };

    return Ok((!held).then(|| format!("{not_registered}: {model}")));
  }

  match harness::offered(Judging::of(ground), &ground.machine(), provider).await {
    Ok(offered) if offered.models.iter().any(|one| one.id == model) => Ok(None),
    Ok(_) => Ok(Some(fill_marks(
      &QUEUE_WORDS.pins.unlisted,
      &[("label", &provider.declared().label), ("model", model)],
    ))),
    Err(why) => {
      eprintln!(
        "pin {}::{model}: its catalogue could not be read, so it is kept: {why}",
        provider.name()
      );

      Ok(None)
    }
  }
}
