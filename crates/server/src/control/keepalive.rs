//! Agency's own switches (docs/api.md, 「settings」): the research supervisor, the downgrade return, the
//! console secrets the DeepSeek top-up reads its card from and signs in with, and the minutes between two flushes
//! of the deploy queue, read and changed. No switch warms
//! or stops everything at once: a supervisor's warming is its own row's (docs/keepalive-schedule.md).

use std::path::{Component, Path};

use axum::http::{Method, StatusCode};
use axum::response::Response;
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};

use super::{Call, Refused, Route, answer};
use crate::lines::token::{RESEARCH_SUPERVISOR, research_supervisor};
use crate::queue::settings;
use crate::watches::deploy_flush;

/// A switch naming a console secret by its file name: the field a caller reads and writes it by, the settings
/// key it stands under and the refusal of a value that names no secret.
struct SecretSetting {
  field: &'static str,
  key: &'static str,
  refusal: &'static str,
}

/// The console secret the DeepSeek top-up reads its card from.
const DEEPSEEK_CARD_SECRET: SecretSetting = SecretSetting {
  field: "deepseekCardSecret",
  key: "deepseek_card_secret",
  refusal: "The DeepSeek card secret is a console secret's name, one plain file name, or null.",
};

/// The console secret the DeepSeek top-up signs in to DeepSeek's platform with.
const DEEPSEEK_LOGIN_SECRET: SecretSetting = SecretSetting {
  field: "deepseekLoginSecret",
  key: "deepseek_login_secret",
  refusal: "The DeepSeek login secret is a console secret's name, one plain file name, or null.",
};

const SECRET_SETTINGS: [SecretSetting; 2] = [DEEPSEEK_CARD_SECRET, DEEPSEEK_LOGIN_SECRET];

// The switches as a caller reads them.
async fn view(ground: &crate::places::Ground) -> Result<Value, Refused> {
  let research = research_supervisor(ground).await.map_err(Refused::failed)?;
  let mut switches = json!({ "researchSupervisor": research, "downgradeReturn": Value::Null });

  for setting in &SECRET_SETTINGS {
    let named = settings::get(&ground.db, setting.key).await.map_err(Refused::failed)?;

    switches[setting.field] = json!(named);
  }

  switches["deployFlushMinutes"] = json!(deploy_flush::minutes(&ground.db).await.map_err(Refused::failed)?);

  Ok(switches)
}

/// Agency's own switches.
pub struct SettingsRead;

impl Route for SettingsRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/settings";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    Ok(answer(StatusCode::OK, view(&call.shared_ground()).await?))
  }
}

crate::moved!(SettingsRead);

/// A secret's name as the console's store keeps it: one plain file name of the administrator's space,
/// never a path reaching past it.
fn is_secret_name(name: &str) -> bool {
  let parts: Vec<Component> = Path::new(name).components().collect();

  matches!(parts.as_slice(), [Component::Normal(part)] if *part == name)
}

async fn change_research_supervisor(store: &DatabaseConnection, asked: &Value) -> Result<(), Refused> {
  match asked {
    Value::Null => Ok(settings::forget(store, RESEARCH_SUPERVISOR).await?),
    Value::String(name) if crate::supervisors::named(store, name).await?.is_some() => {
      super::set_setting(store, RESEARCH_SUPERVISOR, name).await
    }
    _ => Err(
      Refused::new(
        StatusCode::BAD_REQUEST,
        "The research supervisor is a supervisor the store holds, or null.",
      )
      .with_field("researchSupervisor"),
    ),
  }
}

async fn change_secret_name(
  store: &DatabaseConnection,
  setting: &SecretSetting,
  asked: &Value,
) -> Result<(), Refused> {
  match asked {
    Value::Null => Ok(settings::forget(store, setting.key).await?),
    Value::String(name) if is_secret_name(name) => super::set_setting(store, setting.key, name).await,
    _ => Err(Refused::new(StatusCode::BAD_REQUEST, setting.refusal).with_field(setting.field)),
  }
}

async fn change_flush_minutes(store: &DatabaseConnection, asked: &Value) -> Result<(), Refused> {
  match asked {
    Value::Null => Ok(settings::forget(store, deploy_flush::MINUTES_KEY).await?),
    Value::Number(number) if number.as_i64().is_some_and(|minutes| minutes > 0) => {
      super::set_setting(store, deploy_flush::MINUTES_KEY, &number.to_string()).await
    }
    _ => Err(
      Refused::new(
        StatusCode::BAD_REQUEST,
        "The deploy flush minutes are a positive whole number, or null for the policy's.",
      )
      .with_field("deployFlushMinutes"),
    ),
  }
}

/// Agency's own switches changed as the body names them: the research supervisor, a supervisor the store
/// holds, or null for none; the DeepSeek top-up's card and login secrets, each a console secret's name, or null
/// for none; the minutes between two flushes of the deploy queue, a positive whole number, or null for the
/// policy's. A switch the body leaves out stands as it is.
pub struct SettingsWrite;

impl Route for SettingsWrite {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/settings";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;

    if let Some(asked) = body.get("researchSupervisor") {
      change_research_supervisor(call.store(), asked).await?;
    }

    for setting in &SECRET_SETTINGS {
      if let Some(asked) = body.get(setting.field) {
        change_secret_name(call.store(), setting, asked).await?;
      }
    }

    if let Some(asked) = body.get("deployFlushMinutes") {
      change_flush_minutes(call.store(), asked).await?;
    }

    Ok(answer(StatusCode::OK, view(&call.shared_ground()).await?))
  }
}

crate::moved!(SettingsWrite);
