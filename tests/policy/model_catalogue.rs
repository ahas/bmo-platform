use policy::model_catalogue::*;

#[test]
fn the_rules_read_and_every_default_has_a_ceiling() {
  let defaults = &MODEL_CATALOGUE.defaults;

  assert!(MODEL_CATALOGUE.file_pattern.ends_with("gguf$"));
  assert!(
    [&defaults.temperature, &defaults.top_p, &defaults.presence_penalty, &defaults.max_tokens]
      .iter()
      .all(|bounds| bounds.max.is_some_and(|max| max > bounds.min))
  );
  assert!(defaults.top_p.above_min && defaults.max_tokens.whole);
}
