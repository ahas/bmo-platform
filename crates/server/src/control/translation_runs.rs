//! A translation asked of the translator runner (docs/api.md, 「translations」; docs/launch-lines.md, 「Translate
//! runner」): answered whole with the road it went out on, or refused.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use bmo_agency_client::{TranslationRun, TranslationRunAsk};

use super::{Call, Refused, Route, answer};
use crate::translate::{self, Refused as Translation};

/// One text translated on the `translate` road.
pub struct CreateTranslationRun;

impl Route for CreateTranslationRun {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/translation-runs";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let asked: TranslationRunAsk = serde_json::from_value(call.body().await?).map_err(|why| {
      Refused::new(StatusCode::BAD_REQUEST, format!("a translation ask: {why}")).with_code("bad-request")
    })?;

    match translate::translate(
      call.ground(),
      &asked.purpose,
      &asked.language,
      (asked.note.as_deref(), &asked.terms),
      &asked.text,
    )
    .await
    {
      Ok(done) => Ok(answer(
        StatusCode::OK,
        serde_json::to_value(TranslationRun {
          text: done.text,
          provider: done.provider,
          model: done.model,
        })
        .unwrap_or_default(),
      )),
      Err(Translation::Ask(why)) => {
        Err(Refused::new(StatusCode::BAD_REQUEST, why).with_code("bad-request"))
      }
      Err(Translation::Closed(why)) => {
        Err(Refused::new(StatusCode::SERVICE_UNAVAILABLE, why).with_code("no-login-open"))
      }
      Err(Translation::TooLarge(why)) => {
        Err(Refused::new(StatusCode::PAYLOAD_TOO_LARGE, why).with_code("too-large"))
      }
      Err(Translation::NoAnswer(why)) => {
        Err(Refused::new(StatusCode::BAD_GATEWAY, why).with_code("no-answer"))
      }
    }
  }
}

crate::moved!(CreateTranslationRun);
