use policy::background_work::BACKGROUND_WORK;
use policy::door_words::DOOR_WORDS;

#[test]
fn reads_its_clocks_and_each_line_names_the_marks_it_is_filled_by() {
  let words = &BACKGROUND_WORK.words;

  assert!(BACKGROUND_WORK.look_seconds > 0 && BACKGROUND_WORK.result_chars > 0);
  assert!(BACKGROUND_WORK.stop_bound_seconds * 1_000 > BACKGROUND_WORK.stop_grace_ms);

  for mark in ["{launched}", "{kind}", "{description}", "{status}", "{cuts}", "{result}", "{output}"] {
    assert!(words.ended.contains(mark), "{mark}");
  }

  for mark in ["{launched}", "{kind}", "{description}", "{why}", "{cuts}"] {
    assert!(words.lost.contains(mark), "{mark}");
  }

  assert!(words.cut.contains("{call}") && words.cut.contains("{minutes}") && words.cut.contains("{why}"));
  assert!(words.summary_ended.contains("{description}") && words.summary_lost.contains("{description}"));
  assert!(DOOR_WORDS.background_stop.ended_program.contains("{tasks}"));
  assert!(DOOR_WORDS.background_stop.left.contains("{pids}"));
  assert!(DOOR_WORDS.agent_view.steps > 0 && !DOOR_WORDS.agent_view.addressed.is_empty());
}
