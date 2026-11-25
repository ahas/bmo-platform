use policy::control_words::CONTROL_WORDS;
use policy::fill_marks;

#[test]
fn the_control_words_read_with_the_marks_they_are_filled_by() {
  let words = &CONTROL_WORDS;

  assert!(words.fallback.order.contains("{providers}"));
  assert!(
    words.fallback.models_unread.contains("{provider}") && words.fallback.models_unread.contains("{why}")
  );
  assert!(words.fallback.not_offered.contains("{key}") && words.fallback.not_offered.contains("{mode}"));
  assert!(words.accounts.unknown.contains("{id}") && words.accounts.name_long.contains("{most}"));
  assert!(words.workers.sampling.unknown.contains("{name}"));
  assert!(words.agents.gates_of.contains("{agent}") && words.agents.gates_of.contains("{gates}"));
  assert!(words.sessions.none_in_room.contains("{room}"));
  assert!(words.supervisors.own_name.contains("{pieces}"));
  assert!(
    words.tiers.answer_tokens.contains("{providers}") && words.tiers.answer_tokens.contains("{choices}")
  );
  assert!(words.registry.in_use.contains("{what}") && words.registry.in_use.contains("{names}"));
  assert!(
    words.models.engine_switching.contains("{engine}") && words.models.engine_switching.contains("{entry}")
  );
  assert!(words.supervisor_reset.not_supervisor.contains("{agent}"));
}

#[test]
fn a_generation_span_reads_as_one_sentence() {
  let generation = &CONTROL_WORDS.generation;
  let least = fill_marks(&generation.above, &[("min", "0")]);
  let span = fill_marks(&generation.most, &[("least", &least), ("max", "1")]);

  assert_eq!(
    fill_marks(&generation.range, &[("name", "top_p"), ("span", &span)]),
    "top_p is above 0 and at most 1."
  );
}
