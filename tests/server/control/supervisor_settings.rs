use agency_server::control::supervisor_settings::*;
use policy::supervisors::GENERATION;
use serde_json::json;

#[test]
fn a_generation_value_is_refused_with_its_bounds() {
  let setting = |name: &str| GENERATION.setting(name).unwrap();

  assert_eq!(generation_refusal(setting("temperature"), &json!(0.9), None), None);
  assert_eq!(
    generation_refusal(setting("top_p"), &json!(0), None).as_deref(),
    Some("top_p is above 0 and at most 1.")
  );
  assert_eq!(
    generation_refusal(setting("top_k"), &json!(2.5), None).as_deref(),
    Some("top_k is a whole number.")
  );
  assert_eq!(
    generation_refusal(setting("seed"), &json!(-1), None).as_deref(),
    Some("seed is at least 0 and at most 4,294,967,295.")
  );
  assert_eq!(
    generation_refusal(setting("max_tokens"), &json!(4_096), Some(4_096)).as_deref(),
    Some("max_tokens is at least 1 and at most 4,095.")
  );
  assert_eq!(
    generation_refusal(setting("repeat_last_n"), &json!("many"), None).as_deref(),
    Some("repeat_last_n is a number.")
  );
  assert_eq!(generation_refusal(setting("preserve_thinking"), &json!(false), None), None);
  assert_eq!(
    generation_refusal(setting("preserve_thinking"), &json!(0), None).as_deref(),
    Some("preserve_thinking is on or off.")
  );
  assert_eq!(
    generation_refusal(setting("temperature"), &json!(true), None).as_deref(),
    Some("temperature is a number.")
  );
}

#[test]
fn a_switch_is_shown_with_no_bounds() {
  let shown = generation_settings(None);
  let switch = shown.iter().find(|one| one["name"] == "preserve_thinking").unwrap();

  assert_eq!(switch, &json!({ "name": "preserve_thinking", "switch": true, "template": true }));
}

#[test]
fn the_answer_ceiling_s_bound_is_the_window_s() {
  let shown = generation_settings(Some(8_192));
  let ceiling = shown.iter().find(|one| one["name"] == "max_tokens").unwrap();

  assert_eq!(ceiling["max"], json!(8_191.0));
  assert_eq!(ceiling["whole"], json!(true));
  assert!(shown.iter().find(|one| one["name"] == "temperature").unwrap().get("whole").is_none());
}
