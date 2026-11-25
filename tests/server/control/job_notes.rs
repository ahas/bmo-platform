//! A job's prompt carries its item's newest notes, as many as fit the policy's bytes, after a line saying
//! how many older ones were left out (docs/api.md, 「assignments and jobs」).

use agency_server::control::work::notes_within;
use policy::job_prompt::JOB_PROMPT;

#[test]
fn notes_that_fit_are_carried_whole_in_their_order() {
  assert_eq!(notes_within(&["first", "second"], 100), "- first\n- second\n");
}

#[test]
fn only_the_newest_notes_that_fit_are_carried_and_the_older_ones_are_counted() {
  let notes = ["oldest note", "older note", "newer note", "newest note"];
  let carried = notes_within(&notes, 30);
  let left = policy::fill_marks(&JOB_PROMPT.notes.left_out, &[("n", "2")]);

  assert_eq!(carried, format!("{left}\n- newer note\n- newest note\n"));
}

#[test]
fn a_newest_note_past_the_bytes_alone_is_cut_at_them_on_a_character_boundary() {
  let long = "é".repeat(100);
  let carried = notes_within(&["old", &long], 40);
  let kept = carried.lines().last().unwrap();

  assert!(kept.starts_with("- é") && kept.ends_with(" …"), "{carried}");
  assert!(kept.len() < 40, "{carried}");
  assert!(carried.contains(&policy::fill_marks(&JOB_PROMPT.notes.left_out, &[("n", "1")])));
}

#[test]
fn the_policy_bounds_the_notes_and_says_how_many_are_left_out() {
  assert!(JOB_PROMPT.notes.most_bytes > 0);
  assert!(JOB_PROMPT.notes.left_out.contains("{n}"));
}
