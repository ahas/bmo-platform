use agency_server::control::fault::Fault;
use agency_server::control::models::*;
use harness_machine::engine_health::{self, EngineHealth};
use policy::model_catalogue::MODEL_CATALOGUE;
use serde_json::{Value, json};

fn refusal(made: Result<Value, Fault>) -> Option<(String, String)> {
  match made {
    Err(Fault::Field(field, error)) => Some((field, error)),
    _ => None,
  }
}

#[test]
fn a_default_is_held_to_its_bounds_and_said_with_them() {
  let defaults = &MODEL_CATALOGUE.defaults;

  assert!(checked_default("temperature", &json!(0.7), &defaults.temperature, "temperature").is_ok());
  assert_eq!(
    refusal(checked_default("temperature", &json!(3), &defaults.temperature, "temperature")),
    Some(("temperature".into(), "temperature is a number from 0 to 2.".into()))
  );
  assert_eq!(
    refusal(checked_default("topP", &json!(0), &defaults.top_p, "top_p")),
    Some(("topP".into(), "top_p is above 0 and at most 1.".into()))
  );
  assert_eq!(
    refusal(checked_default("maxTokens", &json!(1.5), &defaults.max_tokens, "max_tokens")),
    Some(("maxTokens".into(), "max_tokens is a whole number from 1 to 1048576.".into()))
  );
}

#[test]
fn an_engine_set_aside_or_answering_is_busy() {
  let health = |in_hand: Option<i64>, aside: Option<&str>| EngineHealth {
    context: None,
    model: None,
    in_hand,
    aside: aside.map(|reason| engine_health::Aside {
      reason: reason.into(),
      until: json!(99),
    }),
  };

  assert_eq!(engine_busy(None), None);
  assert_eq!(engine_busy(Some(&health(Some(0), None))), None);
  assert!(engine_busy(Some(&health(Some(2), None))).is_some_and(|busy| busy.contains("2 requests")));
  assert!(engine_busy(Some(&health(None, Some("training")))).is_some_and(|busy| busy.contains("training")));
}

#[test]
fn a_source_is_an_https_address_or_a_path_under_the_data_root() {
  assert!(checked_source(&json!("https://example.com/rvn.gguf")).is_ok());
  assert!(checked_source(&json!("runs/tuned/out.gguf")).is_ok());
  assert!(checked_source(&json!("../out.gguf")).is_err());
  assert!(checked_source(&json!("  ")).is_err());
}
