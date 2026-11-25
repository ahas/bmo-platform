//! The home bridge: which scripts it runs and which of a run's variables it carries over.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use node_link::bridge::*;

#[test]
fn only_a_script_of_the_deploy_tree_is_run() {
  let deploy = Path::new("/d");

  assert_eq!(script_in(deploy, "scripts/todo.ts"), Some(PathBuf::from("/d/scripts/todo.ts")));
  assert_eq!(script_in(deploy, "scripts/../bin/x.ts"), None);
  assert_eq!(script_in(deploy, "/etc/passwd"), None);
  assert_eq!(script_in(deploy, "packages/x.ts"), None);
  assert_eq!(script_in(deploy, "scripts/run.sh"), None);
  assert_eq!(script_in(deploy, "bin/bmo"), Some(PathBuf::from("/d/bin/bmo")));
  assert_eq!(script_in(deploy, "bin/bmo-server"), None);
}

#[test]
fn a_run_hands_over_its_own_variables_read_as_this_machines() {
  let asked: BTreeMap<String, String> = [
    ("BMO_AGENT_PLACE", "/n/w/data/agent-places/x"),
    ("BMO_DATA_DIR", "/n/w/data"),
    ("CLAUDECODE", "1"),
    ("HOME", "/n"),
  ]
  .into_iter()
  .map(|(name, value)| (name.to_owned(), value.to_owned()))
  .collect();
  let carried = carried_env(&asked, |path| path.replace("/n/w/data", "/h/data"));

  assert_eq!(carried.get("BMO_AGENT_PLACE").map(String::as_str), Some("/h/data/agent-places/x"));
  assert_eq!(carried.get("CLAUDECODE").map(String::as_str), Some("1"));
  assert!(!carried.contains_key("BMO_DATA_DIR"));
  assert!(!carried.contains_key("HOME"));
}
