#![allow(
  dead_code,
  unused_imports,
  unused_variables,
  clippy::needless_lifetimes,
  clippy::let_and_return,
  clippy::unnecessary_to_owned
)]
//! The two catalogues a fallback stand-in is picked from (docs/provider-presence.md): the one bundled with
//! the installed Codex executable, and OpenRouter's public listing. Neither reads the store.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use harness_providers::{codex, openrouter};
use serde_json::json;

use super::{Call, Refused, Route, answer};

/// The models the installed Codex executable offers, with the reasoning levels each accepts.
pub struct CodexModels;

impl Route for CodexModels {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/fallback/codex-models";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    match codex::models(call.machine()).await {
      Ok(models) => Ok(answer(StatusCode::OK, json!({ "models": models }))),
      Err(error) => Err(Refused::new(StatusCode::BAD_GATEWAY, listing_refusal("Codex", &error))),
    }
  }
}

/// Every model OpenRouter lists; the listing needs no key.
pub struct OpenRouterModels;

impl Route for OpenRouterModels {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/fallback/openrouter-models";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    match openrouter::models(call.machine()).await {
      Ok(models) => Ok(answer(StatusCode::OK, json!({ "models": models }))),
      Err(error) => Err(Refused::new(StatusCode::BAD_GATEWAY, listing_refusal("OpenRouter", &error))),
    }
  }
}

/// The sentence a provider's model listing that could not be read is refused with.
pub fn listing_refusal(provider: &str, error: &str) -> String {
  policy::fill_marks(
    &policy::control_words::CONTROL_WORDS.fallback.models_unread,
    &[("provider", provider), ("why", error)],
  )
}
