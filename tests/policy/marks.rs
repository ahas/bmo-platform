use policy::fill_marks;
use policy::job_prompt::JOB_PROMPT;

#[test]
fn every_mark_fills_at_once_and_a_mark_nothing_names_stays() {
  assert_eq!(
    fill_marks("on {branch} from {main}, {other}", &[("branch", "{main}"), ("main", "m")]),
    "on {main} from m, {other}"
  );
  assert_eq!(fill_marks("{} and {x", &[("x", "y")]), "{} and {x");
  assert_eq!(fill_marks("<{tag}>", &[("tag", "b")]), "<b>");
}

#[test]
fn the_workbench_names_its_scratch_folder() {
  assert!(!JOB_PROMPT.workbench.scratch.is_empty());
}
