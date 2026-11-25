//! A supervisor's sleep setting on its own screen (docs/sleep.md, 「Its setting」; docs/api.md, 「supervisors」):
//! whether it sleeps and at what hour, read with the policy's hour it falls back on and its record.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use policy::sleep::SLEEP;
use sea_orm::ConnectionTrait;
use serde_json::{Value, json};

use super::{Call, Refused, Route, answer};
use crate::sleep::{Setting, set, setting};

/// The setting as the route answers it.
fn shown(setting: &Setting) -> Value {
  json!({
    "on": setting.on,
    "hour": setting.hour,
    "defaultHour": SLEEP.hour,
    "sleptOn": setting.slept_on,
    "triedAt": setting.tried_at,
  })
}

/// The supervisor the path names, refused 404 where none is held.
async fn held<C: ConnectionTrait>(db: &C, agent: String) -> Result<String, Refused> {
  match crate::supervisors::named(db, &agent).await? {
    Some(_) => Ok(agent),
    None => Err(Refused::new(StatusCode::NOT_FOUND, format!("No supervisor named {agent} is held."))),
  }
}

/// `GET /supervisors/{supervisorName}/sleep`: whether the supervisor sleeps, at what hour, and its record.
pub struct SupervisorSleepRead;

impl Route for SupervisorSleepRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/supervisors/{supervisorName}/sleep";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let named = call.param("supervisorName").unwrap_or_default().to_owned();
    let agent = held(call.store(), named).await?;

    Ok(answer(StatusCode::OK, shown(&setting(call.store(), &agent).await?)))
  }
}

crate::moved!(SupervisorSleepRead);

/// `PUT /supervisors/{supervisorName}/sleep`: whether the supervisor sleeps and its own hour, none for the
/// policy's; answered with the setting as it then stands.
pub struct SupervisorSleepChange;

impl Route for SupervisorSleepChange {
  const METHOD: Method = Method::PUT;
  const PATH: &'static str = "/supervisors/{supervisorName}/sleep";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let named = call.param("supervisorName").unwrap_or_default().to_owned();
    let body = call.body().await?;
    let agent = held(call.store(), named).await?;
    let Some(on) = body.get("on").and_then(Value::as_bool) else {
      return Err(Refused::new(StatusCode::BAD_REQUEST, "on (true or false) is required."));
    };
    let hour = match body.get("hour") {
      None | Some(Value::Null) => None,
      Some(hour) => match hour.as_u64().filter(|hour| *hour < 24) {
        Some(hour) => Some(hour as u32),
        None => {
          return Err(Refused::new(StatusCode::BAD_REQUEST, "hour is a whole hour from 0 to 23, or null."));
        }
      },
    };

    set(call.store(), &agent, on, hour).await?;

    Ok(answer(StatusCode::OK, shown(&setting(call.store(), &agent).await?)))
  }
}

crate::moved!(SupervisorSleepChange);
