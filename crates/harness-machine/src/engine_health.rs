//! What an engine behind a registration says of itself (docs/harness-machine.md, 「An engine's
//! health」): asked at each of the registration's addresses in order, with the key its file holds,
//! the first that answers with an object.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::keys;

// The largest integer a double holds exactly, which JavaScript calls a safe integer.
const SAFE_MOST: i64 = (1 << 53) - 1;

/// One address of a registration, as its row names it.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Address {
  pub url: String,
  pub key_file: Option<String>,
}

/// Why the engine stands aside and until when, in seconds since the epoch.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Aside {
  pub reason: String,
  pub until: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineHealth {
  /// The context of the cards it holds now, or none while it holds none.
  pub context: Option<i64>,
  /// The model file it serves.
  pub model: Option<String>,
  /// The requests it has in hand, answered or waiting their turn.
  pub in_hand: Option<i64>,
  pub aside: Option<Aside>,
}

fn whole(value: Option<&Value>, least: i64) -> Option<i64> {
  let number = value?.as_number()?;
  let held = number.as_i64().or_else(|| {
    number
      .as_f64()
      .filter(|float| float.fract() == 0.0 && float.abs() <= SAFE_MOST as f64)
      .map(|float| float as i64)
  })?;

  (least..=SAFE_MOST).contains(&held).then_some(held)
}

/// The health an engine's answer says: a value of the wrong shape is none.
pub fn read_health(said: &Value) -> Option<EngineHealth> {
  if !said.is_object() && !said.is_array() {
    return None;
  }

  let aside = said.get("aside").and_then(|aside| {
    Some(Aside {
      reason: aside.get("reason")?.as_str()?.to_owned(),
      until: aside.get("until").filter(|until| until.is_number())?.clone(),
    })
  });

  Some(EngineHealth {
    context: whole(said.get("context"), 1),
    model: said
      .get("model")
      .and_then(Value::as_str)
      .filter(|model| !model.is_empty())
      .map(str::to_owned),
    in_hand: whole(said.get("in_hand"), 0),
    aside,
  })
}

// Where an address's engine answers for its health: the address's origin.
fn health_url(address: &str) -> Option<String> {
  let parsed = url::Url::parse(address).ok()?;

  Some(format!("{}/health", parsed.origin().ascii_serialization()))
}

/// The health of the engine behind a registration's addresses, or none where no address with a
/// readable key answers with one — a registration served by another operator's front gives none.
pub async fn health(data_root: &Path, addresses: &[Address]) -> Option<EngineHealth> {
  let client = reqwest::Client::new();

  for address in addresses {
    let Some(key) = address.key_file.as_deref().and_then(|file| keys::read(data_root, file)) else {
      continue;
    };
    let Some(url) = health_url(&address.url) else {
      continue;
    };
    let Ok(answer) = client.get(url).bearer_auth(key).send().await else {
      continue;
    };

    if !answer.status().is_success() {
      continue;
    }

    if let Some(health) = answer.json::<Value>().await.ok().as_ref().and_then(read_health) {
      return Some(health);
    }
  }

  None
}
