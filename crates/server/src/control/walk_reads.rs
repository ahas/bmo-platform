//! The providers as a consumer reads them (docs/api.md, 「providers, models and model files」): each with the
//! owner's switch, what it declares of itself and what it has left; the models an order's entry may name on
//! one; the choice a pin's value names; and whether a registered model's engine answers.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use harness_providers::ARCHIVE_PROVIDERS;
use serde::Deserialize;
use serde_json::{Value, json};

use super::claude_accounts::Judging;
use super::harness::{self, named, provider_enabled, set_provider_enabled};
use super::{Call, Refused, Route, answer, registry};
use crate::places::Ground;
use crate::queue::pins::pin_choice;

fn bad(error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, error)
}

/// One provider as the routes answer it: its switch, its declaration, why it cannot answer now and what it has
/// left as an order reads it.
async fn provider_view(ground: &Ground, name: &str) -> Result<Value, Refused> {
  let provider = named(name)?;
  let declared = provider.declared();
  let missing = harness::missing(&ground.db, &ground.machine(), provider).await?;
  let quota = crate::order::quota(ground, provider, None, None).await;

  Ok(json!({
    "name": name,
    "code": name,
    "label": declared.label,
    "enabled": provider_enabled(&ground.db, name).await?,
    "registered": provider.unregistered().is_some(),
    "gateway": provider.models_on_gateway(),
    "ownersEngine": declared.owners_engine,
    "takesAdvisor": provider.takes_advisor(),
    "advisorWords": provider.advisor_words(),
    "lines": declared.lines,
    "quota": quota,
    "reason": missing.as_ref().map(|missing| missing.text.clone()),
    "reasonCode": missing.as_ref().map(|missing| missing.code.clone()),
    "foldRatioDefault": declared.fold_ratio.unwrap_or(crate::order::DEFAULT_FOLD_RATIO),
    "keyFile": provider.key_file(),
  }))
}

/// `GET /providers`: every provider this build carries.
pub struct ListProviders;

impl Route for ListProviders {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/providers";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let mut listed: Vec<Value> = vec![];

    for name in ARCHIVE_PROVIDERS {
      listed.push(provider_view(call.ground(), name).await?);
    }

    Ok(answer(StatusCode::OK, json!({ "items": listed, "more": false })))
  }
}

crate::moved!(ListProviders);

/// `GET /providers/{providerName}`: one provider.
pub struct ProviderRoute;

impl Route for ProviderRoute {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/providers/{providerName}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let name = call.param("providerName").unwrap_or_default().to_owned();

    Ok(answer(StatusCode::OK, provider_view(call.ground(), &name).await?))
  }
}

crate::moved!(ProviderRoute);

/// `PATCH /providers/{providerName}`: the owner's switch on a provider thrown; the last provider left on is not
/// turned off.
pub struct ChangeProvider;

impl Route for ChangeProvider {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/providers/{providerName}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let name = call.param("providerName").unwrap_or_default().to_owned();
    let asked = call.body().await?;

    if !ARCHIVE_PROVIDERS.contains(&name.as_str()) {
      return Err(Refused::new(StatusCode::NOT_FOUND, format!("no provider named {name} in this build")));
    }

    if let Some(enabled) = asked.get("enabled").and_then(Value::as_bool) {
      set_provider_enabled(call.store(), &name, enabled)
        .await
        .map_err(|refused| Refused::new(StatusCode::CONFLICT, refused.error))?;
    }

    Ok(answer(StatusCode::OK, provider_view(call.ground(), &name).await?))
  }
}

crate::moved!(ChangeProvider);

/// `GET /provider-models/{providerName}`: the models an order's entry may name on a provider, as its own
/// catalogue offers them; a catalogue that cannot be read answers the provider's refusal.
pub struct ProviderModels;

impl Route for ProviderModels {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/provider-models/{providerName}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let name = call.param("providerName").unwrap_or_default().to_owned();
    let provider = named(&name)?;
    let offered = harness::offered(Judging::of(call.ground()), call.machine(), provider)
      .await
      .map_err(|why| Refused::new(StatusCode::SERVICE_UNAVAILABLE, why))?;
    let sizes = model_sizes(call.ground(), provider, &name).await?;
    let models: Vec<Value> = offered
      .models
      .iter()
      .map(|model| {
        let size = sizes.get(&model.id);

        json!({
          "id": model.id,
          "efforts": model.efforts,
          "free": model.free,
          "inService": model.in_service,
          "contextTokens": size.map(|size| size.0),
          "answerTokens": size.map(|size| size.1),
          "thinking": size.map(|size| size.2),
        })
      })
      .collect();

    Ok(answer(
      StatusCode::OK,
      json!({ "provider": name, "models": models, "staleReadAt": offered.stale_read_at }),
    ))
  }
}

crate::moved!(ProviderModels);

/// Each model's window, answer ceiling and thinking default, where its provider states them: the registry's
/// registrations, and a gateway road's members; none for a provider whose catalogue states none.
async fn model_sizes(
  ground: &Ground,
  provider: &dyn harness_providers::Provider,
  name: &str,
) -> Result<std::collections::HashMap<String, (i64, i64, bool)>, Refused> {
  if provider.unregistered().is_some() {
    return Ok(
      registry::registrations(&ground.db)
        .await?
        .into_iter()
        .map(|held| (held.name, (held.context_tokens, held.answer_tokens, held.thinking)))
        .collect(),
    );
  }

  if !provider.models_on_gateway() {
    return Ok(std::collections::HashMap::new());
  }

  Ok(
    harness_providers::member::road_members(&ground.machine(), name)
      .unwrap_or_default()
      .into_iter()
      .map(|held| (held.member.name, (held.context_tokens, held.answer_tokens, held.thinking)))
      .collect(),
  )
}

/// A pin's value, as a worker's pin is written.
#[derive(Deserialize)]
struct PinValue {
  value: String,
}

/// The choice a pin's value names, or why nothing could run on it.
pub struct CheckPin;

impl Route for CheckPin {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/pin-checks";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let asked: PinValue =
      serde_json::from_value(call.body().await?).map_err(|error| bad(format!("a pin's value: {error}")))?;
    let pin = pin_choice(call.ground(), &asked.value).await?.map_err(bad)?;

    Ok(answer(
      StatusCode::OK,
      match pin.provider {
        None => json!({ "kind": "auto", "provider": Value::Null, "model": Value::Null }),
        Some(provider) => json!({
          "kind": if pin.model.is_some() { "model" } else { "provider" },
          "provider": provider,
          "model": pin.model,
        }),
      },
    ))
  }
}

crate::moved!(CheckPin);

/// A model of a provider, by the name the path and the query give it: the provider and the model apart.
pub fn model_key(key: &str, provider: Option<&str>) -> (String, String) {
  if let Some(provider) = provider.filter(|provider| !provider.trim().is_empty()) {
    return (provider.to_owned(), key.to_owned());
  }

  key.split_once("::").or_else(|| key.split_once('/')).map_or_else(
    || (registry::PROVIDER.to_owned(), key.to_owned()),
    |(provider, model)| (provider.to_owned(), model.to_owned()),
  )
}

/// One model and its health: a registration of the registry's provider, or a model of a provider whose
/// catalogue is its own.
pub struct ModelHealth;

impl Route for ModelHealth {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/models/{modelKey}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let key = call.param("modelKey").unwrap_or_default();
    let (provider, model) = model_key(key, call.query("provider").as_deref());
    let asked = named(&provider).map_err(|_| {
      Refused::new(StatusCode::NOT_FOUND, format!("no provider named {provider} in this build"))
    })?;

    if asked.unregistered().is_some() {
      let view = registry::registered_model(call.store(), &call.data_root(), &model).await?;

      // A registration the reader does not reach is answered as none stood there (docs/api.md,
      // 「Who a caller acts for」).
      let mut reach =
        crate::organisation::resources::Reach::new(call.ground(), crate::access::Acting::of(&call)?);
      let reached = match &view {
        Some(view) => reach.reaches_row(view["account"].as_i64(), view["unit"].as_i64()).await?,
        None => true,
      };

      return match (view, reached) {
        (Some(view), true) => Ok(answer(StatusCode::OK, view)),
        _ => Err(Refused::new(StatusCode::NOT_FOUND, format!("no model is registered as {model}"))),
      };
    }

    Ok(answer(
      StatusCode::OK,
      json!({
        "key": key,
        "name": model,
        "provider": asked.name(),
        "model": model,
        "cardService": Value::Null,
        "roleFiles": false,
        "usedBy": [],
        "healthy": false,
        "lastAnsweredAt": Value::Null,
        "contextTokens": Value::Null,
        "answerTokens": Value::Null,
        "thinking": Value::Null,
      }),
    ))
  }
}

crate::moved!(ModelHealth);
