use policy::agent_renames::*;

#[test]
fn the_renames_read_each_with_its_old_and_new_name_apart() {
  assert!(AGENT_RENAMES.renames.iter().all(|rename| rename.from != rename.to));
}
