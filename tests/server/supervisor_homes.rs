//! The supervisors' homes carried into `supervisor-homes/` as the server starts (docs/supervisors.md, 「Where a
//! supervisor's turn runs」): each home moved with the session records every configuration keeps of it, a home
//! the new folder already holds left where it stands.

use agency_server::supervisor_homes::{FORMER_HOMES, carried};
use harness_providers::claude_code::ClaudeCode;

#[test]
fn a_home_moves_with_its_session_records_and_one_already_standing_stays() {
  let dir = tempfile::tempdir().unwrap();
  let data = dir.path().join("data");
  let machine_config = dir.path().join("dot-claude");
  let road_config = data.join("claude-roads").join("registered").join("member");
  let former = data.join(FORMER_HOMES);

  std::fs::create_dir_all(former.join("pal")).unwrap();
  std::fs::write(former.join("pal").join("note.txt"), "kept").unwrap();
  std::fs::create_dir_all(former.join("twin")).unwrap();
  std::fs::create_dir_all(data.join("supervisor-homes").join("twin")).unwrap();

  for config in [&machine_config, &road_config] {
    let records = ClaudeCode::project_folder(config, &former.join("pal"));

    std::fs::create_dir_all(&records).unwrap();
    std::fs::write(records.join("s1.jsonl"), "{}").unwrap();
  }

  let said = carried(&data, &machine_config);
  let home = data.join("supervisor-homes").join("pal");

  assert_eq!(std::fs::read_to_string(home.join("note.txt")).unwrap(), "kept", "{said:?}");

  for config in [&machine_config, &road_config] {
    assert!(ClaudeCode::project_folder(config, &home).join("s1.jsonl").exists(), "{said:?}");
  }

  assert!(former.join("twin").exists(), "{said:?}");
  assert!(said.iter().any(|line| line.contains("stays")), "{said:?}");
  assert!(carried(&data, &machine_config).iter().any(|line| line.contains("stays")));
}

#[test]
fn a_data_root_with_no_former_folder_is_left_as_it_is() {
  let dir = tempfile::tempdir().unwrap();

  assert!(carried(dir.path(), &dir.path().join("dot-claude")).is_empty());
  assert!(!dir.path().join("supervisor-homes").exists());
}
