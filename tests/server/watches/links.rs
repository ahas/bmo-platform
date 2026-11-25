//! The links to the work nodes: paths as a node reads them, a place marked with its node, and a pass that
//! writes each ready workplace's spec over the SSH login the node service answers and stands its link.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use agency_server::peers::Asking;
use agency_server::peers::node::{HubReach, NodeRow, NodeService};
use agency_server::watches::links::{
  LinkUnits, Roots, Skipped, deployed_commit, link_unit, loopback_port, mark_place_node, mark_place_on,
  node_path, place_node, program_records, project_dir, settle, withdraw_lent,
};
use node_link::spec::{Spec, rewrite};

use crate::support::scratch::Scratch;

#[test]
fn a_path_from_the_home_is_read_against_the_nodes_home() {
  assert_eq!(node_path("~/node-root", Some("/home/tester")), "/home/tester/node-root");
  assert_eq!(node_path("~", Some("/home/tester")), "/home/tester");
  assert_eq!(node_path("/srv/node", Some("/home/tester")), "/srv/node");
  assert_eq!(node_path("~/node-root", None), "~/node-root");
}

#[test]
fn the_gateway_is_carried_to_a_node_where_its_address_is_on_this_machine_s_loopback() {
  assert_eq!(loopback_port("http://127.0.0.1:18030"), Some(18030));
  assert_eq!(loopback_port("http://localhost/llm"), Some(80));
  assert_eq!(loopback_port("http://[::1]:4000"), Some(4000));
  assert_eq!(loopback_port("https://gateway.example/llm"), None);
  assert_eq!(loopback_port("not an address"), None);
}

#[test]
fn the_deployed_commit_is_read_off_the_record_beside_the_deploy_tree_s_link() {
  let root = tempfile::tempdir().unwrap();

  std::fs::write(root.path().join("deployed.json"), r#"{"commit": "abc123", "subject": "s"}"#).unwrap();

  assert_eq!(deployed_commit(&root.path().join("deploy")).as_deref(), Some("abc123"));
  assert_eq!(deployed_commit(&root.path().join("deploy").join("deploy")), None);
}

#[test]
fn a_working_directory_names_its_records_with_every_other_character_a_hyphen() {
  assert_eq!(project_dir("/home/tester/work-n1", "/r/projects"), "/r/projects/-home-tester-work-n1");
}

#[test]
fn a_place_names_the_home_its_program_keeps_its_records_under() {
  let scratch = tempfile::tempdir().unwrap();
  let place = scratch.path().join("agent-a");

  std::fs::create_dir_all(&place).unwrap();
  assert_eq!(program_records(&place), None);

  std::fs::write(
    place.join("program.json"),
    r#"{"command": {"args": [], "env": {"CLAUDE_CONFIG_DIR": "/data/claude-roads/road-a/model-a"}}}"#,
  )
  .unwrap();
  assert_eq!(program_records(&place).as_deref(), Some("/data/claude-roads/road-a/model-a"));

  std::fs::write(place.join("program.json"), r#"{"command": {"args": [], "env": {}}}"#).unwrap();
  assert_eq!(program_records(&place), None);
}

#[test]
fn a_place_is_marked_with_its_node_and_unmarked_when_it_comes_back() {
  let scratch = tempfile::tempdir().unwrap();
  let place = scratch.path().join("agent-places").join("worker");

  assert_eq!(place_node(&place), None);
  mark_place_node(&place, Some("n1")).unwrap();
  assert_eq!(place_node(&place), Some("n1".into()));
  mark_place_node(&place, None).unwrap();
  mark_place_node(&place, None).unwrap();
  assert_eq!(place_node(&place), None);
}

/// A node service that logs in to `n1` alone and reaches `u7` and `u8` through a hub.
struct LogsInToOne;

const LOGIN: [&str; 4] = ["ssh", "-o", "BatchMode=yes", "tester@n1.example"];

impl NodeService for LogsInToOne {
  fn link_up<'a>(&'a self, _node: &'a str) -> Asking<'a, bool> {
    Box::pin(async { Ok(false) })
  }

  fn node_rows(&self) -> Asking<'_, Vec<NodeRow>> {
    Box::pin(async { Ok(vec![]) })
  }

  fn ssh_login<'a>(&'a self, node: &'a str) -> Asking<'a, Option<Vec<String>>> {
    Box::pin(async move { Ok((node == "n1").then(|| LOGIN.map(str::to_owned).to_vec())) })
  }

  fn hub_reach<'a>(&'a self, node: &'a str) -> Asking<'a, Option<HubReach>> {
    Box::pin(async move {
      Ok(["u7", "u8"].contains(&node).then(|| HubReach {
        address: "http://127.0.0.1:8791".into(),
        token_file: "~/hub.env".into(),
        agent: format!("{node}-agent"),
      }))
    })
  }
}

/// Units that start every link, setup and check asked of them and run none before.
#[derive(Default)]
struct Units {
  started: Mutex<Vec<(String, PathBuf)>>,
  set_up: Mutex<Vec<String>>,
  checked: Mutex<Vec<String>>,
}

impl LinkUnits for Units {
  async fn running(&self, _unit: &str) -> bool {
    false
  }

  async fn start(&self, unit: &str, spec: &Path) -> bool {
    self.started.lock().unwrap().push((unit.to_owned(), spec.to_path_buf()));
    true
  }

  async fn stop(&self, _unit: &str) {}

  async fn sync(&self, _node: &str) -> bool {
    false
  }

  async fn set_up(&self, node: &str) -> bool {
    self.set_up.lock().unwrap().push(node.to_owned());
    true
  }

  async fn check(&self, node: &str) -> bool {
    self.checked.lock().unwrap().push(node.to_owned());
    true
  }
}

// "a workplace not ready is checked again once its while has passed, and stands no link meanwhile"
#[tokio::test]
async fn a_workplace_not_ready_is_checked_again_once_its_while_has_passed_and_stands_no_link_meanwhile() {
  let scratch = Scratch::new().await;
  let trees = tempfile::tempdir().unwrap();
  let roots = Roots {
    data: trees.path().join("data"),
    deploy: trees.path().join("agency/deploy"),
    cli_deploy: trees.path().join("cli/deploy"),
    agents: trees.path().join("agents"),
    home: trees.path().to_path_buf(),
    mcp_port: 3290,
    gateway_port: Some(18030),
    gateway_key: trees.path().join("data/env/llm-gateway.key"),
    jake_hub: Some("/opt/jake/jake-hub".into()),
    config_dir: trees.path().join(".claude"),
    path: "/usr/bin:/bin".into(),
    bun: "bun".into(),
  };
  let now = chrono::Utc::now().timestamp();
  let again = policy::work_nodes::WORK_NODES.check_again_seconds;

  // u7 was checked longer ago than the while, u8 just now, and account-9 is an isolation node, stood up apart.
  for (node, checked_at) in [("u7", now - again), ("u8", now - 1), ("account-9", now - again)] {
    scratch
      .run(
        r#"INSERT INTO "node_workplaces" ("node", "root", "home", "state", "checked_at", "created_at", "updated_at")
           VALUES ($1, '~/node-root', '/home/tester', 'missing', $2, 0, 0)"#,
        vec![node.into(), checked_at.into()],
      )
      .await;
  }

  let units = Units::default();
  let settled = settle(scratch.db(), &LogsInToOne, &units, &roots).await.unwrap();

  assert_eq!(settled.checking, ["u7"]);
  assert_eq!(*units.checked.lock().unwrap(), ["u7"]);
  assert!(units.started.lock().unwrap().is_empty(), "no link stands on a workplace not ready");
}

/// A node service whose hub holds the PCs `u7`, `u8` and `u9` now, and not `u6`.
struct HoldsPcs;

impl NodeService for HoldsPcs {
  fn link_up<'a>(&'a self, node: &'a str) -> Asking<'a, bool> {
    Box::pin(async move { Ok(["u7", "u8", "u9"].contains(&node)) })
  }

  fn node_rows(&self) -> Asking<'_, Vec<NodeRow>> {
    Box::pin(async { Ok(vec![]) })
  }

  fn ssh_login<'a>(&'a self, _node: &'a str) -> Asking<'a, Option<Vec<String>>> {
    Box::pin(async { Ok(None) })
  }
}

// "a person's PC is stood up by itself once its hub holds it, and again past the while where it is left not ready"
#[tokio::test]
async fn a_persons_pc_is_stood_up_by_itself_once_its_hub_holds_it_and_again_past_the_while_where_it_is_not_ready()
 {
  let scratch = Scratch::new().await;
  let trees = tempfile::tempdir().unwrap();
  let roots = Roots {
    data: trees.path().join("data"),
    deploy: trees.path().join("agency/deploy"),
    cli_deploy: trees.path().join("cli/deploy"),
    agents: trees.path().join("agents"),
    home: trees.path().to_path_buf(),
    mcp_port: 3290,
    gateway_port: Some(18030),
    gateway_key: trees.path().join("data/env/llm-gateway.key"),
    jake_hub: Some("/opt/jake/jake-hub".into()),
    config_dir: trees.path().join(".claude"),
    path: "/usr/bin:/bin".into(),
    bun: "bun".into(),
  };
  let db = scratch.db();
  let now = chrono::Utc::now().timestamp();
  let again = policy::person_nodes::PERSON_NODES.setup_again_seconds;

  // u6 is off, u7 newly joined with no workplace, u8 left missing longer ago than the while, u9 just now; u5 is a
  // hub node no account's record names, left missing, which is checked again as any node is.
  for (account, node) in [("6", "u6"), ("7", "u7"), ("8", "u8"), ("9", "u9")] {
    agency_server::person_nodes::set(db, account, node, false).await.unwrap().unwrap();
  }
  for (node, checked_at) in [("u5", now - 3600 * 24), ("u8", now - again), ("u9", now - 1)] {
    scratch
      .run(
        r#"INSERT INTO "node_workplaces" ("node", "root", "home", "state", "checked_at", "created_at", "updated_at")
           VALUES ($1, '~/node-root', '/home/tester', 'missing', $2, 0, 0)"#,
        vec![node.into(), checked_at.into()],
      )
      .await;
  }

  let units = Units::default();
  let settled = settle(db, &HoldsPcs, &units, &roots).await.unwrap();

  assert_eq!(settled.setting_up, ["u7", "u8"], "the PCs held now, new or left missing past the while");
  assert_eq!(*units.set_up.lock().unwrap(), ["u7", "u8"]);
  assert_eq!(settled.checking, ["u5"], "a PC is stood up again rather than checked");
  assert!(units.started.lock().unwrap().is_empty(), "no link stands on a workplace not ready");
}

#[tokio::test]
async fn a_ready_workplace_gets_its_spec_over_the_node_services_login_or_its_hub_and_one_with_no_route_is_skipped()
 {
  let scratch = Scratch::new().await;
  let trees = tempfile::tempdir().unwrap();
  let roots = Roots {
    data: trees.path().join("data"),
    deploy: trees.path().join("agency/deploy"),
    cli_deploy: trees.path().join("cli/deploy"),
    agents: trees.path().join("agents"),
    home: trees.path().to_path_buf(),
    mcp_port: 3290,
    gateway_port: Some(18030),
    gateway_key: trees.path().join("data/env/llm-gateway.key"),
    jake_hub: Some("/opt/jake/jake-hub".into()),
    config_dir: trees.path().join(".claude"),
    path: "/usr/bin:/bin".into(),
    bun: "bun".into(),
  };

  for node in ["n1", "n2", "u7", "u8"] {
    scratch
      .run(
        r#"INSERT INTO "node_workplaces" ("node", "root", "home", "state", "created_at", "updated_at")
           VALUES ($1, '~/node-root', '/home/tester', 'ready', 0, 0)"#,
        vec![node.into()],
      )
      .await;
  }
  crate::support::rows::repository(scratch.db(), "worker", "~/work", &[("node", "n1".into())]).await;
  // A supervisor of an account whose node is the PC u7.
  crate::support::rows::supervisor(scratch.db(), "u7-bmo", &[("owner_account", 7i64.into())]).await;
  agency_server::person_nodes::set(scratch.db(), "7", "u7", false).await.unwrap().unwrap();
  std::fs::create_dir_all(trees.path().join("data/nodes/n1")).unwrap();
  std::fs::write(
    trees.path().join("data/nodes/n1/setup.json"),
    r#"{ "herdrSession": "node-session", "herdrSocket": "/home/tester/.config/herdr/herdr.sock" }"#,
  )
  .unwrap();

  for (node, setup) in [
    ("u7", r#"{ "herdrSession": "bmo", "herdrSocket": "/home/tester/h.sock" }"#),
    ("u8", r#"{ "herdrSession": "bmo", "herdrSocket": "/home/tester/h.sock" }"#),
  ] {
    std::fs::create_dir_all(trees.path().join("data/nodes").join(node)).unwrap();
    std::fs::write(trees.path().join("data/nodes").join(node).join("setup.json"), setup).unwrap();
  }

  let units = Units::default();
  let settled = settle(scratch.db(), &LogsInToOne, &units, &roots).await.unwrap();

  assert_eq!(settled.started, ["n1", "u7", "u8"]);
  assert_eq!(
    settled.skipped,
    [Skipped {
      node: "n2".into(),
      why: "n2 has no SSH route and no hub".into(),
    }]
  );
  assert_eq!(units.started.lock().unwrap()[0], (link_unit("n1"), roots.spec_path("n1")));

  // A person's PC: no login, its hub's connect program and settings, and the far end's ports.
  let pc = Spec::read(&roots.spec_path("u7")).unwrap();
  let jake = pc.jake.unwrap();

  assert!(pc.ssh.is_empty());
  assert_eq!(jake.agent, "u7-agent");
  assert_eq!(jake.connect, ["/opt/jake/jake-hub", "connect"]);
  assert_eq!(jake.hub_env, trees.path().join("hub.env"));
  assert_eq!((jake.mirror_port, jake.herdr_port), (47201, 47202));
  // The tool host and the model gateway here are both reached from the PC's loopback, and a program on a gateway
  // road takes its road's variables and its account's own call key there, never the gateway's own key and never
  // a login's token; an API key goes only as a person's own `api-key` account hands it to their own run
  // (docs/work-nodes.md, 「The model gateway on a node」).
  assert_eq!(pc.ports, [3290, 18030]);
  assert_eq!(pc.handed, [agency_server::call_keys::path(&roots.data, 7)]);
  assert!(pc.env_keep.iter().any(|kept| kept == "ANTHROPIC_BASE_URL"));
  assert!(pc.env_keep.iter().any(|kept| kept == "ANTHROPIC_API_KEY"));
  assert!(!pc.env_keep.iter().any(|kept| kept.starts_with("ANTHROPIC_AUTH")));
  // An account's node asks its bridge for the policy's verbs alone (docs/person-nodes.md, 「Who reaches a node」).
  assert_eq!(pc.bridge_verbs.as_ref(), Some(&policy::person_nodes::PERSON_NODES.bridge_verbs));
  // The supervisor's place goes there: its program sent, its running record brought back.
  assert_eq!(place_node(&roots.place("u7-bmo")).as_deref(), Some("u7"));
  assert!(pc.push.iter().any(|push| push.local == roots.place("u7-bmo")));
  assert_eq!(
    pc.folders,
    ["/home/tester/node-root/data/supervisor-homes/u7-bmo"],
    "its folder is made there, under the node's data root on a node its account alone owns"
  );
  assert!(pc.pull.iter().any(|landing| landing.local == roots.place("u7-bmo")
    && landing.pull.remote == "/home/tester/node-root/data/agent-places/u7-bmo"));
  assert!(pc.path_head[0].ends_with("node-root/bun/bin"));
  assert_eq!(place_node(&roots.place("worker")).as_deref(), Some("n1"));

  let spec = Spec::read(&roots.spec_path("n1")).unwrap();

  assert_eq!(spec.bridge_verbs, None, "a node of the deployment's own asks for any verb");

  assert_eq!(spec.ssh, LOGIN);
  assert_eq!(spec.root, "/home/tester/node-root");
  assert_eq!(spec.herdr.session.as_deref(), Some("node-session"));
  assert_eq!(spec.push.len(), 1);
  assert!(
    !spec.pull.iter().any(|landing| landing.pull.remote == "/home/tester/work/graph"),
    "no checkout's graph is pulled"
  );
  assert_eq!(spec.deploy, roots.cli_deploy, "the home bridge runs the CLI's tree, not agency's");

  let agency_bin = roots.deploy.join("bin").display().to_string();
  let on_node = rewrite(&agency_bin, spec.rewrite.iter().map(|(from, to)| (from.as_str(), to.as_str())));

  assert_eq!(
    on_node, "/home/tester/node-root/deploy/bin",
    "agency's own tree reads as the node's copy"
  );
}

#[tokio::test]
async fn an_account_with_no_pc_is_stood_up_on_its_isolation_node_and_linked_with_no_ssh_and_no_port() {
  let scratch = Scratch::new().await;
  let trees = tempfile::tempdir().unwrap();
  let roots = Roots {
    data: trees.path().join("data"),
    deploy: trees.path().join("agency/deploy"),
    cli_deploy: trees.path().join("cli/deploy"),
    agents: trees.path().join("agents"),
    home: trees.path().to_path_buf(),
    mcp_port: 3290,
    gateway_port: Some(18030),
    gateway_key: trees.path().join("data/env/llm-gateway.key"),
    jake_hub: None,
    config_dir: trees.path().join(".claude"),
    path: "/usr/bin:/bin".into(),
    bun: "bun".into(),
  };
  let db = scratch.db();
  let home = Path::new(&policy::account_isolation::ACCOUNT_ISOLATION.home_base).join("bmo-account-8");
  let at = |part: &str| home.join(part).display().to_string();

  // Account 8's supervisor has no PC; account 9 is the administrator's, whose runs are the owner's own.
  crate::support::rows::supervisor(db, "u8-bmo", &[("owner_account", 8i64.into())]).await;
  crate::support::rows::supervisor(db, "u9-bmo", &[("owner_account", 9i64.into())]).await;
  scratch
    .run(r#"INSERT INTO "settings" ("key", "value") VALUES ('users_administrator', '9')"#, vec![])
    .await;

  let units = Units::default();
  let settled = settle(db, &LogsInToOne, &units, &roots).await.unwrap();

  assert_eq!(
    settled.setting_up,
    ["account-8"],
    "its node is stood up, and none for the administrator's"
  );
  assert_eq!(place_node(&roots.place("u8-bmo")).as_deref(), Some("account-8"));
  assert_eq!(place_node(&roots.place("u9-bmo")), None);

  // Once its setup stands it, its link starts the far end in the sandbox, as its own user.
  scratch
    .run(
      r#"INSERT INTO "node_workplaces" ("node", "root", "home", "state", "checked_at", "created_at", "updated_at")
         VALUES ('account-8', '~/.local/share/bmo-node', $1, 'ready', 0, 0, 0)"#,
      vec![home.display().to_string().into()],
    )
    .await;
  std::fs::create_dir_all(trees.path().join("data/nodes/account-8")).unwrap();
  std::fs::write(
    trees.path().join("data/nodes/account-8/setup.json"),
    serde_json::json!({ "herdrSession": "bmo", "herdrSocket": at(".config/herdr/herdr.sock") }).to_string(),
  )
  .unwrap();

  let units = Units::default();
  let settled = settle(db, &LogsInToOne, &units, &roots).await.unwrap();

  assert!(settled.setting_up.is_empty(), "a ready node is not stood up again");
  assert_eq!(settled.started, ["account-8"]);

  let spec = Spec::read(&roots.spec_path("account-8")).unwrap();
  let local = spec.local.expect("the local transport");

  assert!(spec.ssh.is_empty() && spec.jake.is_none());
  assert!(spec.ports.is_empty(), "the node reaches the tool host over the loopback it shares");
  assert_eq!(spec.bridge, home.join("link/bridge.sock"));
  assert_eq!(local.herdr, home.join("link/herdr.sock"));
  assert_eq!(&local.run_as[..3], ["sudo", "-n", "systemd-run"]);
  assert!(local.run_as.contains(&"--uid=bmo-account-8".to_owned()));
  assert!(local.run_as.contains(&format!("--property=BindPaths={}", home.display())));
  assert_eq!(local.run_as.last().map(String::as_str), Some("--"));
  assert_eq!(spec.bridge_verbs.as_ref(), Some(&policy::person_nodes::PERSON_NODES.bridge_verbs));
  assert!(spec.push.iter().any(|push| push.local == roots.place("u8-bmo")));
  assert_eq!(spec.root, at(".local/share/bmo-node"));
}

#[tokio::test]
async fn each_account_sharing_a_node_keeps_its_supervisors_folders_under_a_root_of_its_own_there() {
  let scratch = Scratch::new().await;
  let trees = tempfile::tempdir().unwrap();
  let roots = Roots {
    data: trees.path().join("data"),
    deploy: trees.path().join("agency/deploy"),
    cli_deploy: trees.path().join("cli/deploy"),
    agents: trees.path().join("agents"),
    home: trees.path().to_path_buf(),
    mcp_port: 3290,
    gateway_port: Some(18030),
    gateway_key: trees.path().join("data/env/llm-gateway.key"),
    jake_hub: Some("/opt/jake/jake-hub".into()),
    config_dir: trees.path().join(".claude"),
    path: "/usr/bin:/bin".into(),
    bun: "bun".into(),
  };
  let db = scratch.db();

  scratch
    .run(
      r#"INSERT INTO "node_workplaces" ("node", "root", "home", "state", "created_at", "updated_at")
         VALUES ('u7', '~/node-root', '/home/tester', 'ready', 0, 0)"#,
      vec![],
    )
    .await;
  std::fs::create_dir_all(trees.path().join("data/nodes/u7")).unwrap();
  std::fs::write(
    trees.path().join("data/nodes/u7/setup.json"),
    r#"{ "herdrSession": "bmo", "herdrSocket": "/home/tester/h.sock" }"#,
  )
  .unwrap();

  for (supervisor, account) in [("u7-bmo", 7i64), ("u8-bmo", 8i64)] {
    crate::support::rows::supervisor(db, supervisor, &[("owner_account", account.into())]).await;
    agency_server::person_nodes::set(db, &account.to_string(), "u7", true)
      .await
      .unwrap()
      .unwrap();
  }

  let settled = settle(db, &LogsInToOne, &Units::default(), &roots).await.unwrap();

  assert_eq!(settled.started, ["u7"]);

  let pc = Spec::read(&roots.spec_path("u7")).unwrap();
  let own = |supervisor: &str| {
    pc.to_node(
      &harness_machine::supervisor_prompt::homes(&roots.data)
        .join(supervisor)
        .display()
        .to_string(),
    )
  };

  assert_eq!(own("u7-bmo"), "/home/tester/node-root/accounts/7/supervisor-homes/u7-bmo");
  assert_eq!(own("u8-bmo"), "/home/tester/node-root/accounts/8/supervisor-homes/u8-bmo");
  let mut folders = pc.folders.clone();

  folders.sort();
  assert_eq!(folders, [own("u7-bmo"), own("u8-bmo")], "each folder is made under its account's root");
  // The places stay in the node's data root, which every account on it shares.
  assert!(pc.pull.iter().any(|landing| landing.local == roots.place("u8-bmo")
    && landing.pull.remote == "/home/tester/node-root/data/agent-places/u8-bmo"));
}

// A login lent to an account never stays on a PC other accounts share: the program standing there on it is let go
// between its turns, its program file removed so the link says it gone on the node, while a program on the account's
// own login, one on a PC of its own and one with a turn going are left as they are.
#[tokio::test]
async fn a_program_on_a_lent_login_is_let_go_from_a_node_other_accounts_share_between_its_turns() {
  use agency_server::door::place_run::{begin_run, end_run};
  use agency_server::{person_nodes, turn_pickup};

  let scratch = Scratch::new().await;
  let trees = tempfile::tempdir().unwrap();
  let roots = Roots {
    data: trees.path().join("data"),
    deploy: trees.path().join("agency/deploy"),
    cli_deploy: trees.path().join("cli/deploy"),
    agents: trees.path().join("agents"),
    home: trees.path().to_path_buf(),
    mcp_port: 3290,
    gateway_port: Some(18030),
    gateway_key: trees.path().join("data/env/llm-gateway.key"),
    jake_hub: None,
    config_dir: trees.path().join(".claude"),
    path: "/usr/bin:/bin".into(),
    bun: "bun".into(),
  };
  let lent = roots.data.join("env/lent-login");
  let own = roots.data.join("env/own-login");
  let db = scratch.db();

  scratch.run(r#"DELETE FROM "claude_accounts""#, vec![]).await;
  scratch
    .run(
      r#"INSERT INTO "claude_accounts"
           ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at", "user_id", "lendable")
         VALUES ('lent', 'lent', 1, 1, 'directory', $1, 0, 1, true),
                ('seven', 'seven', 2, 1, 'directory', $2, 0, 7, false)"#,
      vec![lent.to_string_lossy().into_owned().into(), own.to_string_lossy().into_owned().into()],
    )
    .await;

  for (account, node, shared) in
    [("7", "pc-shared", true), ("8", "pc-shared", true), ("9", "pc-nine", false)]
  {
    person_nodes::set(db, account, node, shared).await.unwrap().unwrap();
  }

  for (name, owner) in [("u7", 7i64), ("u7-own", 7), ("u8", 8), ("u9", 9)] {
    crate::support::rows::supervisor(db, name, &[("owner_account", owner.into())]).await;
  }

  let placed = |name: &str, node: &str, login: &Path| {
    let place = roots.data.join("agent-places").join(name);

    std::fs::create_dir_all(&place).unwrap();
    mark_place_on(&place, Some(node), "/folder").unwrap();
    std::fs::write(
      place.join("program.json"),
      serde_json::json!({ "identity": name, "restart": null, "command": {
        "args": [], "cwd": "/folder", "env": { "CLAUDE_CONFIG_DIR": login } } })
      .to_string(),
    )
    .unwrap();
    place
  };
  let u7 = placed("u7", "pc-shared", &lent);
  let u7_own = placed("u7-own", "pc-shared", &own);
  let u8 = placed("u8", "pc-shared", &lent);
  let u9 = placed("u9", "pc-nine", &lent);

  // u8's program is in a turn, and u7's last turn ended in its program but its end has not reached its caller.
  begin_run(&u8, "chat:-8:1", "a turn", 1, Default::default()).unwrap();
  begin_run(&u7, "chat:-7:1", "a turn", 1, Default::default()).unwrap();
  end_run(&u7, 2).unwrap();

  let kept = turn_pickup::InFlight {
    session: "u7@-7".into(),
    turn: 1,
    run: "chat:-7:1".into(),
    key: None,
    agent: "u7".into(),
    chat_id: Some(-7),
    ledger: None,
    job: None,
    started_at: 1,
  };

  turn_pickup::keep(db, &kept).await;

  assert!(
    withdraw_lent(db, &roots).await.unwrap().is_empty(),
    "every turn is left to reach its caller"
  );
  assert!(u7.join("program.json").exists());

  turn_pickup::forget(db, "chat:-7:1", "its end reached its caller").await;

  assert_eq!(withdraw_lent(db, &roots).await.unwrap(), ["u7"]);
  assert!(!u7.join("program.json").exists(), "the lent login's program is let go");
  assert!(u7_own.join("program.json").exists(), "the account's own login stays");
  assert!(u8.join("program.json").exists(), "a program with a turn going is left to its turn");
  assert!(u9.join("program.json").exists(), "a PC of the account's own keeps what it was lent");

  end_run(&u8, 2).unwrap();

  assert_eq!(withdraw_lent(db, &roots).await.unwrap(), ["u8"]);
  assert!(!u8.join("program.json").exists());
  assert!(withdraw_lent(db, &roots).await.unwrap().is_empty());
}
