//! The checkouts jobs work in, as the repositories hold them (docs/worker-slots.md): the built-in standing
//! first where no repository's base is there, each repository's base followed by the support worktrees its
//! count asks for and the checkouts it keeps on the nodes, and a checkout read back to its repository.

use std::path::Path;

use agency_server::control::loans::candidates;
use agency_server::queue::workers::{BUILTIN_WORKER, Worker, repository_of, support_name, workers};

use crate::support::scratch::Scratch;

#[tokio::test]
async fn the_built_in_stands_first_where_no_repository_is_there() {
  let scratch = Scratch::new().await;
  let listed = workers(scratch.db(), Path::new("/source")).await.unwrap();

  assert_eq!(listed[0].name, BUILTIN_WORKER);
  assert_eq!(listed[0].cwd, "/source");
}

#[tokio::test]
async fn a_repositorys_support_worktrees_follow_its_count_and_its_node_checkouts_follow_them() {
  let scratch = Scratch::new().await;

  scratch
    .run(
      r#"INSERT INTO "repositories" ("name", "label", "path", "support_count", "created_at", "updated_at")
         VALUES ('infra', 'Infra', '/repos/infra', 2, 0, 0)"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "repository_node_checkouts" ("repository", "node", "path", "created_at")
         VALUES ('infra', 'n1', '/remote/infra', 0)"#,
      vec![],
    )
    .await;

  let listed = workers(scratch.db(), Path::new("/source")).await.unwrap();
  let names: Vec<&str> = listed.iter().map(|worker| worker.name.as_str()).collect();

  assert_eq!(
    names,
    [
      BUILTIN_WORKER,
      "infra",
      &support_name("infra", 2),
      &support_name("infra", 3),
      "infra@n1"
    ]
  );
  assert_eq!(listed[2].cwd, "/repos/infra-w2");
  assert_eq!(listed[2].support_of.as_deref(), Some("infra"));
  assert_eq!(listed[4].node.as_deref(), Some("n1"));
  assert!(listed[4].is_node_checkout());

  for checkout in ["infra", "infra-w3", "infra@n1"] {
    let repository = repository_of(scratch.db(), checkout).await.unwrap().unwrap();

    assert_eq!(repository.name, "infra", "{checkout} is a checkout of infra");
  }
}

fn checkout(name: &str, support_of: Option<&str>, node: Option<&str>) -> Worker {
  Worker {
    name: name.to_owned(),
    label: name.to_owned(),
    cwd: format!("/work/{name}"),
    builtin: false,
    support_of: support_of.map(str::to_owned),
    support_slot: support_of.map(|_| 2),
    node: node.map(str::to_owned),
    umbrella: false,
  }
}

#[test]
fn a_borrow_takes_a_support_worktree_first_the_base_last_and_only_on_its_own_machine() {
  let known = [
    checkout("other", None, None),
    checkout("other-w2", Some("other"), None),
    checkout("other-n1", Some("other"), Some("n1")),
    checkout("elsewhere", None, None),
  ];
  let names = |node: Option<&str>| -> Vec<String> {
    candidates(&known, "other", node).iter().map(|checkout| checkout.name.clone()).collect()
  };

  assert_eq!(names(None), ["other-w2", "other"]);
  assert_eq!(names(Some("n1")), ["other-n1"]);
  assert!(candidates(&known, "nobody", None).is_empty());
}
