//! The step a turn on a road other than Claude Code's own session goes out on (docs/cache-fallback.md, 「A
//! supervisor's turn on a road other than Claude Code」), and a role's run in a place of its own on any road:
//! its model, effort and answer room, resolved for its key.

use harness_providers::Machine;
use harness_providers::registered::Registered;
use policy::work_keys::WORK_KEYS;
use sea_orm::DatabaseConnection;

use crate::control::claude_accounts::Judging;
use crate::control::own_llm::{self, OwnLlmError};
use crate::control::registry::{self, Mapping};
use crate::lines::worker::{Reasoning, effort_of};
use crate::places::Ground;

/// Claude Code, the provider whose own session carries a turn rather than a step of this module.
pub const CLAUDE: &str = "claude-code";

/// What a step on a road goes out on: its provider, the model and effort its road names, and on a registered
/// model the room its answer is given and whether it thinks.
pub struct Step {
  pub provider: String,
  pub model: String,
  pub effort: String,
  pub thinking: bool,
  pub answer_tokens: Option<i64>,
}

/// A step's model and effort, as the key is mapped on the provider with its agent's own thinking and
/// effort — the mapping of the supervisor that asked the run where the key is a role that follows it
/// (docs/role-slots.md, 「A role that follows its supervisor」); on a registered model, the registration's
/// own run settings with the answer room the key's work is given — never below the floor its own work
/// declares, nor, where the run thinks, below its thinking floor as far as a quarter of the registration's
/// window allows.
pub async fn step_of(
  ground: &Ground,
  machine: &Machine,
  key: &str,
  provider: &str,
  reasoning: &Reasoning,
  supervisor: Option<&str>,
) -> Result<Step, String> {
  let db = &ground.db;
  let mapping = crate::order::row_entry(db, key, provider, supervisor).await?;
  let effort =
    effort_of(ground, machine, provider, &mapping.model, mapping.effort.as_deref(), Some(reasoning))
      .await
      .map_err(|why| why.detail)?
      .unwrap_or_default();

  if provider != registry::PROVIDER {
    let held = held_thinking(ground, machine, provider, &mapping.model).await?;
    let thinking = mapping
      .thinks()
      .unwrap_or_else(|| if effort.is_empty() { held.unwrap_or(true) } else { effort != "none" });

    return Ok(Step {
      provider: provider.to_owned(),
      model: mapping.model,
      thinking,
      effort,
      answer_tokens: mapping.answer_tokens,
    });
  }

  let asked = Mapping {
    model: mapping.model.clone(),
    thinking: mapping.thinks(),
    effort: Some(effort).filter(|effort| !effort.is_empty()),
    answer_tokens: mapping.answer_tokens,
  };

  registered_step(db, key, &asked).await
}

/// Whether a provider holds the model a run in a place of its own goes out on (docs/provider-order.md, 「Models
/// a provider holds」), and the model's thinking default where it states one. A provider whose models are the
/// gateway's holds a member of its own road, which states its default; any other holds a model its own
/// catalogue names, read under the login the walk would take first, and states none, so the line's own
/// stands. A catalogue that cannot be read holds nothing against the model.
pub async fn held_thinking(
  ground: &Ground,
  machine: &Machine,
  provider: &str,
  model: &str,
) -> Result<Option<bool>, String> {
  let found = harness_providers::provider(provider)
    .ok_or_else(|| format!("{provider} is no provider of this build"))?;

  if found.models_on_gateway() {
    return harness_providers::member::road_members(machine, provider)
      .map_err(|why| format!("{provider}: {why}"))?
      .into_iter()
      .find(|one| one.member.name == model)
      .map(|one| Some(one.thinking))
      .ok_or_else(|| format!("{provider}: {model} is no member of its road"));
  }

  match crate::control::harness::offered(Judging::of(ground), machine, found).await {
    Ok(offered) if !offered.models.iter().any(|one| one.id == model) => {
      Err(format!("{provider}: {model} is not in its catalogue"))
    }
    _ => Ok(None),
  }
}

/// A step on the registration a caller names rather than the one the key maps to: the registration's own
/// run settings, with the effort the key's agent's own reasoning makes of it, and the answer room the key's
/// work is given, as the key's mapped step is.
pub async fn registration_step(
  ground: &Ground,
  key: &str,
  registration: &str,
  reasoning: &Reasoning,
) -> Result<Step, String> {
  let effort =
    effort_of(ground, &ground.machine(), registry::PROVIDER, registration, None, Some(reasoning))
      .await
      .map_err(|why| why.detail)?;
  let asked = Mapping {
    model: registration.to_owned(),
    thinking: None,
    effort: effort.filter(|effort| !effort.is_empty()),
    answer_tokens: None,
  };

  registered_step(&ground.db, key, &asked).await
}

// A step on a registered model: its run settings, never below the answer floor the key's work declares.
async fn registered_step(db: &DatabaseConnection, key: &str, asked: &Mapping) -> Result<Step, String> {
  let settings = registry::run_settings(db, Some(asked)).await.map_err(|why| why.error)?;
  let profile = WORK_KEYS.profile(key);
  let thinking_floor = match (
    settings.thinking,
    profile.and_then(|one| one.thinking_answer_floor),
    settings.context_tokens,
  ) {
    (true, Some(floor), Some(window)) => floor.min(window / 4),
    _ => 0,
  };
  let floor = profile.and_then(|one| one.answer_floor).unwrap_or(0).max(thinking_floor);

  Ok(Step {
    provider: registry::PROVIDER.to_owned(),
    model: settings.model.clone(),
    effort: settings.effort.clone().unwrap_or_default(),
    thinking: settings.thinking,
    answer_tokens: Some(settings.answer_tokens.max(floor)),
  })
}

/// The registration held to the gateway's list, which must offer it as the service's own.
pub async fn listed(ground: &Ground, service: &registry::OwnLlmService, model: &str) -> Result<(), String> {
  let held = async {
    let key = own_llm::credential(&Registered::key_file(&ground.machine()))?;

    own_llm::offers(service, &own_llm::models(service, &key).await?, model)
  };

  held.await.map_err(|why: OwnLlmError| why.said(service))
}
