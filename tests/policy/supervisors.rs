use policy::supervisors::*;

#[test]
fn the_role_reads_in_its_fixed_order_and_the_persona_is_no_part_of_it() {
  assert_eq!(SUPERVISORS.pieces_role, ["absolute-rules", "frame", "turn", "agent-words"]);
}

#[test]
fn only_the_answer_ceiling_is_held_by_the_window() {
  let open: Vec<&str> = GENERATION
    .settings
    .iter()
    .filter(|one| one.bounds.as_ref().is_some_and(|bounds| bounds.max.is_none()))
    .map(|one| one.name.as_str())
    .collect();

  assert_eq!(open, ["max_tokens"]);
  assert!(
    GENERATION
      .setting("top_p")
      .is_some_and(|one| one.bounds.as_ref().is_some_and(|bounds| bounds.above_min))
  );
}

#[test]
fn a_switch_has_no_bounds_and_every_other_setting_has_them() {
  for one in &GENERATION.settings {
    assert_eq!(one.switch, one.bounds.is_none(), "{}", one.name);
  }

  assert!(GENERATION.setting("preserve_thinking").is_some_and(|one| one.switch && one.template));
}
