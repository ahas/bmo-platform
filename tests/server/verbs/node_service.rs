//! The node service a node verb asks (docs/work-nodes.md, 「The workplace on a node」): the one the deployment's
//! `env/agency.env` names where the verb's own environment names none.

use std::collections::HashMap;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::verbs::node_service;

use crate::support::scratch::Scratch;

// "a node verb whose environment names no node service asks the one the deployment's environment file names"
#[tokio::test]
async fn a_node_verb_whose_environment_names_no_node_service_asks_the_one_the_deployment_s_file_names() {
  // The verb asks the process environment first, and a deployed machine hands its own node service's
  // name to every process its service starts, a test run among them. A blank value is the verb's own
  // word for no name, so the test blanks it rather than trusting the runner's environment to be clean.
  // SAFETY: the standard library locks the environment around a write, and nothing in this binary
  // reads it past the library, through a C `environ`.
  unsafe { std::env::set_var(agency_server::peers::node::ADDRESS.url, "") };

  let scratch = Scratch::new().await;
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let data = tempfile::tempdir().unwrap();

  assert!(
    node_service(data.path(), &service).node_rows().await.unwrap().is_empty(),
    "with no file, no node service is asked and none is listed"
  );

  // A port nothing listens on: the service the file names is asked, and its silence is a fault, never an
  // empty list read as no node kept.
  std::fs::create_dir_all(data.path().join("env")).unwrap();
  std::fs::write(
    data.path().join("env/agency.env"),
    "AGENCY_NODE_URL=http://127.0.0.1:9\nAGENCY_NODE_TOKEN=test-token\n",
  )
  .unwrap();

  assert!(node_service(data.path(), &service).node_rows().await.is_err());
}
