use policy::door_words::DOOR_WORDS;
use policy::fill_marks;

#[test]
fn reads_its_lines_and_each_names_the_marks_it_is_filled_by() {
  assert!(!DOOR_WORDS.provider_off.is_empty());
  assert!(!DOOR_WORDS.fold_title.is_empty());
  assert!(DOOR_WORDS.notes.title.contains("{attempt}"));
  assert!(DOOR_WORDS.notes.herdr_back.contains("{state}"));
  assert!(DOOR_WORDS.screens.not_taken.contains("{message}"));
  assert!(DOOR_WORDS.screens.unknown_window.contains("{screen}"));
  assert!(DOOR_WORDS.confirm.tries.contains("{tries}"));
  assert!(DOOR_WORDS.status_line.running.contains("{title}"));
}

#[test]
fn a_status_line_fills_into_one_line() {
  let line = fill_marks(&DOOR_WORDS.status_line.rested_after_run, &[("at", "09:30"), ("name", "run-1")]);

  assert_eq!(line, "resting · since 09:30 · last run run-1");
}
