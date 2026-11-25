use policy::machine_room::MACHINE_ROOM;

#[test]
fn no_fixed_job_count_holds_a_launch_and_each_count_still_says_why_where_the_owner_sets_one() {
  assert_eq!(MACHINE_ROOM.launch_jobs.default, None);
  assert_eq!(MACHINE_ROOM.launch_browser_jobs.default, None);
  assert!(MACHINE_ROOM.launch_jobs.held.contains("{limit}"));
  assert!(MACHINE_ROOM.launch_browser_jobs.held.contains("{limit}"));
}
