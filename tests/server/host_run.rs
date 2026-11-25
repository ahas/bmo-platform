use agency_server::host_run::*;

#[test]
fn a_view_line_is_one_line_cut_with_a_mark() {
  assert_eq!(flat_line("one\n  two\tthree"), "one two three");

  // A character wider than one byte, so the cut is counted in characters.
  let long = "é".repeat(200);
  let cut = flat_line(&long);

  assert_eq!(cut.chars().count(), PROGRESS_WIDTH);
  assert!(cut.ends_with('…'));
}

#[test]
fn what_is_unread_stops_at_the_last_ended_line_unless_the_whole_is_asked() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("stdout");

  std::fs::write(&path, "one\ntwo\nthr").unwrap();

  assert_eq!(unread(&path, 0, false), ("one\ntwo\n".to_owned(), 8));
  assert_eq!(unread(&path, 8, true), ("thr".to_owned(), 11));
  assert_eq!(unread(&path, 11, true), (String::new(), 11));
}
