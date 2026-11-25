//! GitHub's deliveries of pull request events (docs/pull-request-reviews.md, 「The delivery」; docs/api.md,
//! 「pull request reviews」): checked against the webhook secret, answered at once, and acted on after.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use serde_json::{Value, json};

use super::{Call, Refused, Route, answer};
use crate::watches::pull_requests::{act, holders, read_delivery, secret, signed};

/// One delivery of GitHub's.
pub struct GithubDelivery;

impl Route for GithubDelivery {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/github/deliveries";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let ground = call.shared_ground();
    let Some(secret) = secret(ground.vars()) else {
      return Err(
        Refused::new(StatusCode::SERVICE_UNAVAILABLE, "this deployment names no webhook secret")
          .with_code("no-webhook-secret"),
      );
    };
    let signature = call.header("x-hub-signature-256").unwrap_or_default();
    let event = call.header("x-github-event").unwrap_or_default();
    let body = call.raw_body().await?;

    if !signed(&secret, &body, &signature) {
      return Err(
        Refused::new(StatusCode::UNAUTHORIZED, "the delivery is not signed by the webhook secret")
          .with_code("bad-signature"),
      );
    }

    let read: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);

    if let Some(delivered) = read_delivery(&event, &read) {
      tokio::spawn(async move {
        let held = holders(&ground.db).await;

        if let Err(why) = act(ground.vars(), &held, &delivered).await {
          eprintln!("pull requests: a delivery was not acted on: {why}");
        }
      });
    }

    Ok(answer(StatusCode::ACCEPTED, json!({ "taken": true })))
  }
}

crate::moved!(GithubDelivery);
