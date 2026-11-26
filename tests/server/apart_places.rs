//! A thrown-away session's place: the label a job's run is shown under, and the parts a job cannot name.

use agency_server::apart_places::job_label;

#[test]
fn a_job_names_its_place_by_its_run_its_item_and_its_agent() {
  assert_eq!(job_label("job-814206", Some(721), Some("coder-01")), "job-814206 · #721 · coder-01");
}

#[test]
fn a_part_the_job_does_not_name_is_left_out_of_its_place_label() {
  assert_eq!(job_label("job-814206", Some(721), None), "job-814206 · #721");
  assert_eq!(job_label("job-814206", None, Some("coder-01")), "job-814206 · coder-01");
  assert_eq!(job_label("job-814206", None, None), "job-814206");
}
