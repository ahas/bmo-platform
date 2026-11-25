//! An account's personal apps across the boundary (docs/api.md, 「personal-apps」): listed, put, sent back and
//! removed, each through `crate::personal_apps`. A caller naming a person acts on that person's account alone.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use serde_json::Value;

use super::{Call, Refused, Route, answer};
use crate::access::Acting;
use crate::personal_apps;

// The account the path names, refused where a caller naming a person names another.
fn account_of(call: &Call) -> Result<String, Refused> {
  let account = call
    .param("ownerAccount")
    .map(str::trim)
    .filter(|account| !account.is_empty())
    .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "an account is required"))?
    .to_owned();

  match Acting::of(call)? {
    Acting::Person(person) if person.to_string() != account => Err(
      Refused::new(StatusCode::FORBIDDEN, "The signed-in person acts on their own personal apps alone")
        .with_code("forbidden"),
    ),
    _ => Ok(account),
  }
}

fn name_of(call: &Call) -> String {
  call.param("name").unwrap_or_default().trim().to_owned()
}

/// `GET /personal-apps/{ownerAccount}`: the account's apps as the tool host holds them, and the versions the PC keeps.
pub struct ListPersonalApps;

impl Route for ListPersonalApps {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/personal-apps/{ownerAccount}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let account = account_of(&call)?;

    Ok(answer(StatusCode::OK, personal_apps::list(call.ground(), &account).await?))
  }
}

crate::moved!(ListPersonalApps);

/// `POST /personal-apps/{ownerAccount}/{name}/put`: the app's folder on the PC built into a new version and put.
pub struct PutPersonalApp;

impl Route for PutPersonalApp {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/personal-apps/{ownerAccount}/{name}/put";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let account = account_of(&call)?;
    let done = personal_apps::put(call.ground(), &account, &name_of(&call)).await?;

    Ok(answer(StatusCode::OK, done.answer()))
  }
}

crate::moved!(PutPersonalApp);

/// `POST /personal-apps/{ownerAccount}/{name}/back`: the app sent back to a version its PC keeps.
pub struct BackPersonalApp;

impl Route for BackPersonalApp {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/personal-apps/{ownerAccount}/{name}/back";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let account = account_of(&call)?;
    let body = call.body().await.unwrap_or(Value::Null);
    let version = match body.get("version") {
      None | Some(Value::Null) => None,
      Some(Value::String(version)) if !version.trim().is_empty() => Some(version.trim().to_owned()),
      Some(_) => {
        return Err(
          Refused::new(StatusCode::BAD_REQUEST, "version is a version's name").with_field("version"),
        );
      }
    };
    let done = personal_apps::back(call.ground(), &account, &name_of(&call), version.as_deref()).await?;

    Ok(answer(StatusCode::OK, done.answer()))
  }
}

crate::moved!(BackPersonalApp);

/// `POST /personal-apps/{ownerAccount}/screen-session`: a short-lived tool session a console reads and calls the
/// account's personal screens under.
pub struct ScreenSession;

impl Route for ScreenSession {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/personal-apps/{ownerAccount}/screen-session";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let account = account_of(&call)?;

    Ok(answer(
      StatusCode::CREATED,
      personal_apps::screen_session(call.ground(), &account).await?,
    ))
  }
}

crate::moved!(ScreenSession);

/// `DELETE /personal-apps/{ownerAccount}/{name}`: the app taken out at home and on the PC.
pub struct RemovePersonalApp;

impl Route for RemovePersonalApp {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/personal-apps/{ownerAccount}/{name}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let account = account_of(&call)?;
    let done = personal_apps::remove(call.ground(), &account, &name_of(&call)).await?;

    Ok(answer(StatusCode::OK, done.answer()))
  }
}

crate::moved!(RemovePersonalApp);
