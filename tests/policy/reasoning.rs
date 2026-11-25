use policy::reasoning::*;

#[test]
fn the_scale_runs_from_low_to_max() {
  assert_eq!(REASONING.levels.first().map(String::as_str), Some("low"));
  assert_eq!(REASONING.levels.last().map(String::as_str), Some("max"));
}
