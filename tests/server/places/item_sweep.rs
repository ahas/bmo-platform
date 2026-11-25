//! What finished items left in the workbenches, swept: with nobody answering which items are open, only an
//! empty workbench no agent's row names goes; with the open items answered, every scratch folder and database
//! of an item with no open job goes, while an open item's, a folder named for no item and what stands beside
//! the items stay; and a look with nothing changed acts on nothing. The items' databases are a stand-in's,
//! since the suite's cluster may be the machine's own; the store's cluster is asked only of a database of the
//! test's own.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agency_server::config::Config;
use agency_server::places::Ground;
use agency_server::places::item_sweep::{ItemDatabases, OpenItems, OpenItemsHook, StoreCluster, Sweep};
use agency_server::seeds::deployment;
use agency_server::{Refusal, Service};
use harness_providers::Machine;
use policy::job_prompt::JOB_PROMPT;

use crate::support::scratch::Scratch;

// Databases standing in for the cluster's: the names held, and those dropped.
struct Databases {
  held: Mutex<Vec<String>>,
  dropped: Mutex<Vec<String>>,
}

impl ItemDatabases for Databases {
  async fn list<'a>(&'a self, prefix: &'a str) -> Result<Vec<String>, Refusal> {
    Ok(
      self
        .held
        .lock()
        .unwrap()
        .iter()
        .filter(|name| name.starts_with(prefix))
        .cloned()
        .collect(),
    )
  }

  async fn drop<'a>(&'a self, name: &'a str) -> Result<(), String> {
    self.held.lock().unwrap().retain(|held| held != name);
    self.dropped.lock().unwrap().push(name.to_owned());

    Ok(())
  }
}

fn database(item: &str) -> String {
  format!("{}{item}_check", JOB_PROMPT.workbench.item_database)
}

async fn ground(scratch: &Scratch, home: &std::path::Path) -> Arc<Ground> {
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), home.to_string_lossy().into_owned()),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
    ],
    home.join("data"),
  );

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());

  Arc::new(Ground::new(&service, &machine, None))
}

#[tokio::test]
async fn a_finished_item_s_scratch_goes_once_its_open_items_are_answered() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path()).await;
  let benches = home.path().join("data").join("workbench");
  let items = benches.join("helper").join(&JOB_PROMPT.workbench.scratch);

  crate::support::rows::repository(scratch.db(), "helper", "/checkouts/helper", &[]).await;

  for folder in [
    items.join("5"),
    items.join("6"),
    items.join("notes"),
    benches.join("helper").join("dev-data"),
  ] {
    std::fs::create_dir_all(folder).unwrap();
  }

  std::fs::create_dir_all(benches.join("gone")).unwrap();
  std::fs::create_dir_all(benches.join("left")).unwrap();
  std::fs::write(benches.join("left").join("kept"), "").unwrap();

  let databases = Databases {
    held: Mutex::new(vec![database("5"), database("6"), "other_database".to_owned()]),
    dropped: Mutex::default(),
  };
  let sweep = Sweep::default();

  // Nobody answers which items are open: only the empty workbench no row names goes.
  assert_eq!(sweep.sweep(&ground, &databases).await.unwrap(), ["gone/"]);
  assert!(items.join("6").is_dir() && databases.dropped.lock().unwrap().is_empty());

  // The open items answered: the closed item's folder and database go, the rest stays.
  ground
    .hooks
    .register::<OpenItemsHook, _, _>(|()| async { Some(OpenItems { items: vec![5] }) });

  let removed = sweep.sweep(&ground, &databases).await.unwrap();

  assert_eq!(removed, [format!("helper/{}/6", JOB_PROMPT.workbench.scratch), database("6")]);
  assert_eq!(*databases.dropped.lock().unwrap(), [database("6")]);
  assert!(!items.join("6").exists() && items.join("5").is_dir() && items.join("notes").is_dir());
  assert!(benches.join("helper").join("dev-data").is_dir() && benches.join("left").join("kept").exists());

  // Nothing changed since: nothing is acted on.
  std::fs::create_dir_all(items.join("7")).unwrap();
  assert!(sweep.sweep(&ground, &databases).await.unwrap().is_empty());
  assert!(items.join("7").is_dir());
}

#[tokio::test]
async fn the_store_s_cluster_lists_an_item_s_database_and_drops_it() {
  let scratch = Scratch::new().await;
  // A database of the test's own, numbered apart from every item's.
  let name = database(&format!("{}", 900_000_000 + std::process::id()));
  let cluster = StoreCluster(scratch.db());

  scratch.run(&format!(r#"CREATE DATABASE "{name}""#), vec![]).await;
  assert!(cluster.list(&JOB_PROMPT.workbench.item_database).await.unwrap().contains(&name));
  cluster.drop(&name).await.unwrap();
  assert!(!cluster.list(&JOB_PROMPT.workbench.item_database).await.unwrap().contains(&name));
}
