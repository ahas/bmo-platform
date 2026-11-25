use policy::fill_marks;
use policy::queue_words::QUEUE_WORDS;

#[test]
fn the_queue_words_read_with_the_marks_they_are_filled_by() {
  let workers = &QUEUE_WORDS.workers;

  assert!(workers.name_taken.contains("{name}"));
  assert!(workers.support_change.contains("{name}") && workers.support_change.contains("{base}"));
  assert!(workers.keepalive_over.contains("{keepalive}") && workers.keepalive_over.contains("{count}"));
  assert!(QUEUE_WORDS.pins.unknown.contains("{providers}"));
  assert!(QUEUE_WORDS.routes.model_wrong.contains("{selection}"));
  assert!(QUEUE_WORDS.stops.not_ended.contains("{units}"));
  assert!(!QUEUE_WORDS.pool.no_place.is_empty());
}

#[test]
fn a_leftover_workspace_is_named_by_its_label_and_its_panes_and_said_unmarked_where_it_is() {
  let words = &QUEUE_WORDS.leftovers;
  let named = fill_marks(&words.workspace, &[("label", "scratch"), ("panes", "2")]);

  assert_eq!(fill_marks(&words.unmarked, &[("workspace", &named)]), "scratch (2 pane(s)), unmarked");
  assert!(
    fill_marks(&words.account, &[("panes", "2"), ("named", &named)]).ends_with("scratch (2 pane(s)).")
  );
}
