//! Agency's own switches (docs/api.md, 「settings」): the research supervisor, the downgrade return, the
//! console secrets the DeepSeek top-up reads its card from and signs in with, and the minutes between two flushes
//! of the deploy queue, read and changed. Each of the four stands on the delegation chain's platform link, which
//! the administrator's account writes; where no account service is named, the settings row it stood under before
//! the value moved is its ground. No switch warms
//! or stops everything at once: a supervisor's warming is its own row's (docs/keepalive-schedule.md).

use std::path::{Component, Path};

use axum::http::{Method, StatusCode};
use axum::response::Response;
use policy::organisation::ORGANISATION;
use serde_json::{Value, json};

use super::{Call, Refused, Route, answer};
use crate::chain_settings::{self, Standing};
use crate::lines::token::research_supervisor;
use crate::places::Ground;
use crate::watches::deploy_flush;

/// A switch naming a console secret by its file name: the field a caller reads and writes it by, the chain kind
/// it stands under and the refusal of a value that names no secret.
struct SecretSetting {
  field: &'static str,
  kind: fn() -> String,
  refusal: &'static str,
}

// The switch of the research supervisor, as the chain kind names it.
fn research_standing() -> Standing {
  chain_settings::of(&ORGANISATION.settings.research_supervisor.key)
    .expect("the research supervisor stands on the chain")
}

/// The console secret the DeepSeek top-up reads its card from.
const DEEPSEEK_CARD_SECRET: SecretSetting = SecretSetting {
  field: "deepseekCardSecret",
  kind: || ORGANISATION.settings.deepseek_card_secret.key.clone(),
  refusal: "The DeepSeek card secret is a console secret's name, one plain file name, or null.",
};

/// The console secret the DeepSeek top-up signs in to DeepSeek's platform with.
const DEEPSEEK_LOGIN_SECRET: SecretSetting = SecretSetting {
  field: "deepseekLoginSecret",
  kind: || ORGANISATION.settings.deepseek_login_secret.key.clone(),
  refusal: "The DeepSeek login secret is a console secret's name, one plain file name, or null.",
};

const SECRET_SETTINGS: [SecretSetting; 2] = [DEEPSEEK_CARD_SECRET, DEEPSEEK_LOGIN_SECRET];

// The switches as a caller reads them: each the platform's word on the chain, its row where no chain speaks.
async fn view(ground: &Ground) -> Result<Value, Refused> {
  let research = research_supervisor(ground, None).await.map_err(Refused::failed)?;
  let mut switches = json!({ "researchSupervisor": research, "downgradeReturn": Value::Null });

  for setting in &SECRET_SETTINGS {
    let standing = chain_settings::of(&(setting.kind)()).ok_or_else(|| {
      Refused::failed(format!("the switch {} does not stand on the chain", setting.field))
    })?;
    let named = chain_settings::text(ground, &standing).await.map_err(Refused::failed)?;

    switches[setting.field] = json!(named);
  }

  switches["deployFlushMinutes"] = json!(deploy_flush::minutes(ground).await);

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

// The platform's word written, the refusal of a chain that cannot be reached kept in the route's words.
async fn write(ground: &Ground, standing: &Standing, value: Value, field: &str) -> Result<(), Refused> {
  chain_settings::write(ground, standing, value)
    .await
    .map_err(|error| Refused::failed(error).with_field(field))
}

/// A secret's name as the console's store keeps it: one plain file name of the administrator's space,
/// never a path reaching past it.
fn is_secret_name(name: &str) -> bool {
  let parts: Vec<Component> = Path::new(name).components().collect();

  matches!(parts.as_slice(), [Component::Normal(part)] if *part == name)
}

async fn change_research_supervisor(ground: &Ground, asked: &Value) -> Result<(), Refused> {
  let standing = research_standing();

  match asked {
    Value::Null => write(ground, &standing, Value::Null, "researchSupervisor").await,
    Value::String(name) if crate::supervisors::named(&ground.db, name).await?.is_some() => {
      write(ground, &standing, Value::String(name.clone()), "researchSupervisor").await
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
  ground: &Ground,
  setting: &SecretSetting,
  asked: &Value,
) -> Result<(), Refused> {
  let standing = chain_settings::of(&(setting.kind)())
    .ok_or_else(|| Refused::failed(format!("the switch {} does not stand on the chain", setting.field)))?;

  match asked {
    Value::Null => write(ground, &standing, Value::Null, setting.field).await,
    Value::String(name) if is_secret_name(name) => {
      write(ground, &standing, Value::String(name.clone()), setting.field).await
    }
    _ => Err(Refused::new(StatusCode::BAD_REQUEST, setting.refusal).with_field(setting.field)),
  }
}

async fn change_flush_minutes(ground: &Ground, asked: &Value) -> Result<(), Refused> {
  let standing = chain_settings::of(&ORGANISATION.settings.deploy_flush_minutes.key)
    .expect("the deploy flush minutes stand on the chain");

  match asked {
    Value::Null => write(ground, &standing, Value::Null, "deployFlushMinutes").await,
    Value::Number(number) if number.as_i64().is_some_and(|minutes| minutes > 0) => {
      write(ground, &standing, number.clone().into(), "deployFlushMinutes").await
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
/// policy's. Each is written on the chain's platform link in the administrator's name, or into the settings row
/// where no account service is named. A switch the body leaves out stands as it is.
pub struct SettingsWrite;

impl Route for SettingsWrite {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/settings";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let ground = call.shared_ground();

    if let Some(asked) = body.get("researchSupervisor") {
      change_research_supervisor(&ground, asked).await?;
    }

    for setting in &SECRET_SETTINGS {
      if let Some(asked) = body.get(setting.field) {
        change_secret_name(&ground, setting, asked).await?;
      }
    }

    if let Some(asked) = body.get("deployFlushMinutes") {
      change_flush_minutes(&ground, asked).await?;
    }

    Ok(answer(StatusCode::OK, view(&call.shared_ground()).await?))
  }
}

crate::moved!(SettingsWrite);
