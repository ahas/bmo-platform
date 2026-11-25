//! What every place is stood on: what a run apart is handed, the code tree the configuration names, and
//! the label of a home on a copy of the data root.

use std::collections::HashMap;
use std::path::Path;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::places::{DEPLOY_VARIABLE, Ground};
use harness_providers::Machine;
use sea_orm::DatabaseConnection;

fn service(set: &[(&str, &str)]) -> Service {
  let variables: HashMap<String, String> =
    set.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect();

  Service::new(
    DatabaseConnection::Disconnected,
    Config::read(|name| variables.get(name).cloned()).unwrap(),
  )
}

fn machine(vars: &[(&str, &str)], data: &str) -> Machine {
  Machine::with_env(vars.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())), data)
}

fn code_tree(tree: &Path) {
  std::fs::create_dir_all(tree.join("prompts").join("rules")).unwrap();
}

// The host starts a run on an emptied environment, so whatever a runner of this binary needs to find its
// data root has to be named in what it is handed.
#[test]
fn a_run_apart_is_handed_the_home_and_the_bmo_variables_and_nothing_else() {
  let machine = machine(
    &[("HOME", "/home/tester"), ("BMO_OWNER_ID", "7"), ("SSH_AUTH_SOCK", "/run/keyring")],
    "/data",
  );
  let ground = Ground::new(&service(&[]), &machine, None);
  let env = ground.run_env("search-9");

  assert_eq!(env.get("HOME").map(String::as_str), Some("/home/tester"));
  assert_eq!(env.get("BMO_OWNER_ID").map(String::as_str), Some("7"));
  assert_eq!(env.get("BMO_RUN_NAME").map(String::as_str), Some("search-9"));
  assert!(!env.contains_key("SSH_AUTH_SOCK"));
}

#[test]
fn a_server_started_on_a_release_reads_its_code_there_names_the_link_and_hands_the_release_on() {
  let home = tempfile::tempdir().unwrap();
  let release = home.path().join("deploy-releases").join("0123456789ab");
  let newer = home.path().join("deploy-releases").join("ba9876543210");
  let link = home.path().join("deploy");

  code_tree(&release);
  code_tree(&newer);
  std::os::unix::fs::symlink("deploy-releases/ba9876543210", &link).unwrap();

  let at = |path: &Path| path.to_string_lossy().into_owned();
  let service = service(&[("AGENCY_CODE_DIR", &at(&release)), ("AGENCY_DEPLOY_DIR", &at(&link))]);
  let ground = Ground::new(&service, &machine(&[("HOME", &at(home.path()))], "/data"), None);

  assert_eq!(ground.roots.deploy, link);
  assert_eq!(ground.code_root(), release);
  assert_eq!(ground.shown_code_root(), link);
  assert_eq!(ground.env().get(DEPLOY_VARIABLE), Some(&at(&release)));
}

#[test]
fn a_named_tree_holding_no_code_gives_way_to_the_link_and_a_release_a_launcher_named_wins() {
  let home = tempfile::tempdir().unwrap();
  let empty = home.path().join("empty");
  let link = home.path().join(".local/share/bmo-agency/deploy");
  let pinned = home.path().join("pinned");

  std::fs::create_dir_all(&empty).unwrap();
  code_tree(&link);
  code_tree(&pinned);

  let at = |path: &Path| path.to_string_lossy().into_owned();
  let service = service(&[("AGENCY_CODE_DIR", &at(&empty))]);
  let unnamed = Ground::new(&service, &machine(&[("HOME", &at(home.path()))], "/data"), None);

  assert_eq!(unnamed.roots.deploy, link, "the link stands under the home where none is named");
  assert_eq!(unnamed.code_root(), link);

  let launched = Ground::new(
    &service,
    &machine(&[("HOME", &at(home.path())), (DEPLOY_VARIABLE, &at(&pinned))], "/data"),
    None,
  );

  assert_eq!(launched.code_root(), pinned);
  assert_eq!(launched.env().get(DEPLOY_VARIABLE), Some(&at(&pinned)));
}

#[test]
fn a_copy_of_the_data_root_is_told_apart_by_its_name() {
  let own = Ground::new(
    &service(&[]),
    &machine(&[("HOME", "/home/tester")], "/home/tester/.local/share/bmo/data"),
    None,
  );
  let copy = Ground::new(&service(&[]), &machine(&[("HOME", "/home/tester")], "/scratch/trial/data"), None);

  assert_eq!(own.copy(), None);
  assert_eq!(copy.copy().as_deref(), Some("trial"));
}
