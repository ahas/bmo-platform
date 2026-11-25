#![allow(
  dead_code,
  unused_imports,
  unused_variables,
  clippy::needless_lifetimes,
  clippy::let_and_return,
  clippy::unnecessary_to_owned
)]
//! A supervisor's or a repository's own sampling (docs/api.md, 「supervisors」, 「repositories」): the settings
//! it takes with their bounds, the values it holds, and a save of them, kept in the store's settings under its
//! name; a repository's is what a run on its pinned registration is sent.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use policy::control_words::CONTROL_WORDS;
use policy::fill_marks;
use policy::supervisors::GENERATION;
use sea_orm::DatabaseConnection;
use serde_json::{Map, Value, json};

use super::fault::{Fault, refused};
use super::supervisor_settings::{generation_refusal, generation_settings};
use super::{Call, Refused, Route, answer};
use crate::queue::settings;
use store::one;

// The setting a run's answer is held under, which a worker's own sampling leaves to its line.
const ANSWER_CEILING: &str = "max_tokens";

// Every refusal of a sampling is about the sampling as a whole, but for a value out of its setting's
// bounds.
const WHOLE: &str = "sampling";

pub fn key(agent: &str) -> String {
  format!("agent_sampling:{agent}")
}

/// The settings a worker samples with, with their bounds: every generation setting but the answer's
/// ceiling.
fn sampling_settings() -> Vec<Value> {
  generation_settings(None)
    .into_iter()
    .filter(|setting| setting["name"] != ANSWER_CEILING)
    .collect()
}

// What a stored sampling holds: every entry whose value is a finite number or a switch.
pub fn held_values(text: Option<String>) -> Map<String, Value> {
  let Some(Value::Object(read)) = text.and_then(|text| serde_json::from_str(&text).ok()) else {
    return Map::new();
  };

  read
    .into_iter()
    .filter(|(_, value)| value.is_boolean() || value.as_f64().is_some_and(f64::is_finite))
    .collect()
}

// The supervisor or repository a name stands for, which keeps a sampling of its own: its name as the store
// keeps it.
pub async fn sampling_owner(db: &DatabaseConnection, name: &str) -> Result<String, Fault> {
  let words = &CONTROL_WORDS.workers.sampling;
  let held = one(
    db,
    r#"SELECT "name" FROM "supervisors" WHERE "name" = $1 UNION SELECT "name" FROM "repositories" WHERE "name" = $1"#,
    vec![name.into()],
  )
  .await?;

  match held {
    Some(row) => Ok(row.try_get("", "name")?),
    None => refused(WHOLE, fill_marks(&words.no_agent, &[("agent", name)])),
  }
}

pub async fn view(db: &DatabaseConnection, agent: &str) -> Result<Value, Fault> {
  let values = held_values(settings::get(db, &key(agent)).await?);

  Ok(json!({ "agent": agent, "settings": sampling_settings(), "values": values }))
}

// The settings a save names laid over the held ones: each a value its setting takes, or null to leave it to
// the engine. One out of its bounds refuses the whole save.
pub fn changed(asked: &Value, mut held: Map<String, Value>) -> Result<Map<String, Value>, Fault> {
  let words = &CONTROL_WORDS.workers.sampling;
  let Value::Object(asked) = asked else {
    return refused(WHOLE, words.shape.clone());
  };

  for (name, value) in asked {
    let Some(setting) = GENERATION.setting(name).filter(|setting| setting.name != ANSWER_CEILING) else {
      return refused(WHOLE, fill_marks(&words.unknown, &[("name", name)]));
    };

    if value.is_null() {
      held.remove(name);
      continue;
    }

    if let Some(refusal) = generation_refusal(setting, value, None) {
      return refused(name, refusal);
    }

    held.insert(name.clone(), value.clone());
  }

  Ok(held)
}
