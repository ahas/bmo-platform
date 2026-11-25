//! The skills and MCP servers an account attaches to its agents, across the boundary (docs/api.md,
//! 「attachments」): listed and set one holder at a time, through `crate::attachments`. A caller naming a person acts
//! on that person's account alone.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use bmo_agency_client::AttachmentSet;

use super::personal_skill_routes::account_of;
use super::{Call, Refused, Route, answer};
use crate::attachments;

fn json<T: serde::Serialize>(value: &T) -> serde_json::Value {
  serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
}

/// `GET /attachments/{ownerAccount}`: the account's holders and what is attached to each.
pub struct ListAttachments;

impl Route for ListAttachments {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/attachments/{ownerAccount}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let account = account_of(&call)?;
    let ground = call.shared_ground();

    Ok(answer(StatusCode::OK, json(&attachments::listing(&ground, &account).await?)))
  }
}

crate::moved!(ListAttachments);

/// `PUT /attachments/{ownerAccount}/{holderKind}/{holder}`: one holder's attached skills and MCP servers set whole.
pub struct SetAttachments;

impl Route for SetAttachments {
  const METHOD: Method = Method::PUT;
  const PATH: &'static str = "/attachments/{ownerAccount}/{holderKind}/{holder}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let account = account_of(&call)?;
    let kind = call.param("holderKind").unwrap_or_default().trim().to_owned();
    let holder = call.param("holder").unwrap_or_default().trim().to_owned();
    let ground = call.shared_ground();
    let asked: AttachmentSet = serde_json::from_value(call.body().await?)
      .map_err(|why| Refused::new(StatusCode::BAD_REQUEST, format!("the body does not read: {why}")))?;
    let set = attachments::set(&ground, &account, (&kind, &holder), &asked.skills, &asked.mcp).await?;

    Ok(answer(StatusCode::OK, json(&set)))
  }
}

crate::moved!(SetAttachments);
