//! The files a supervisor's reset takes: never one outside a bench's own data root.

use agency_server::control::supervisor_reset::files::*;
use harness_providers::Machine;

#[test]
fn a_bench_s_front_reaches_no_file_outside_its_own_data_root() {
  let home = tempfile::tempdir().unwrap();
  let bench = tempfile::tempdir().unwrap();
  let machine = Machine::with_env([("HOME".to_owned(), home.path().display().to_string())], bench.path());

  assert!(removable(&machine, &bench.path().join("brain/pal")));
  assert!(!removable(&machine, &home.path().join(".claude/projects/x/s.jsonl")));

  let own = home.path().join(".local/share/bmo/data");

  std::fs::create_dir_all(&own).unwrap();

  let live = Machine::with_env([("HOME".to_owned(), home.path().display().to_string())], &own);

  assert!(removable(&live, &home.path().join(".claude/projects/x/s.jsonl")));
}
