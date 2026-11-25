//! An account's node (docs/person-nodes.md, 「Which node is a person's」): its record, the supervisors and role runs
//! of the account placed there, the hub node it must be, a run's wait while the node's link is not heard, placed
//! again by the present records and ended where its supervisor was removed, and who reaches a node (「Who reaches
//! a node」).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::places::Ground;
use agency_server::supervisor_turn::run::placed;
use agent_session::SessionId;
use harness_providers::Machine;

use agency_server::peers::Asking;
use agency_server::peers::node::{NodeKind, NodeRead, NodeRow, NodeService};
use agency_server::person_nodes::{
  self, ActingFor, Caller, NodeWait, Placement, RunPlaced, Waited, run_placed, supervisors_placed,
  until_placed,
};
use agency_server::places::AgentRow;
use agency_server::role_agents::composed_as;
use agency_server::watches::links::{link_heard, mark_place_on, place_cwd, place_node};

use crate::support::rows;
use crate::support::scratch::Scratch;

/// A node service keeping an SSH node, a hub node reserved for a service and a person's hub node.
struct TwoNodes;

impl NodeService for TwoNodes {
  fn link_up<'a>(&'a self, _node: &'a str) -> Asking<'a, bool> {
    Box::pin(async { Ok(false) })
  }

  fn node_rows(&self) -> Asking<'_, Vec<NodeRow>> {
    let row = |node: &str, kind| NodeRow {
      node: node.into(),
      label: None,
      kind,
      state: "connected".into(),
      detail: None,
    };

    Box::pin(async move { Ok(vec![row("box", NodeKind::Ssh), row("pc-7", NodeKind::Jake)]) })
  }

  fn ssh_login<'a>(&'a self, _node: &'a str) -> Asking<'a, Option<Vec<String>>> {
    Box::pin(async { Ok(None) })
  }

  fn node_reads(&self) -> Asking<'_, Vec<NodeRead>> {
    let read = |name: &str, kind, role: Option<&str>| NodeRead {
      name: name.into(),
      label: name.into(),
      kind,
      role: role.map(str::to_owned),
      state: "connected".into(),
      checked_at: None,
    };

    Box::pin(async move {
      Ok(vec![
        read("box", NodeKind::Ssh, None),
        read("engine", NodeKind::Jake, Some("voice")),
        read("pc-7", NodeKind::Jake, None),
      ])
    })
  }
}

// "an account's node is set, replaced, listed and taken away"
#[tokio::test]
async fn an_account_s_node_is_set_replaced_listed_and_taken_away() {
  let scratch = Scratch::new().await;
  let db = scratch.db();

  person_nodes::set(db, "7", "box", false).await.unwrap().unwrap();
  person_nodes::set(db, "7", "pc-7", false).await.unwrap().unwrap();

  let every = person_nodes::every(db).await.unwrap();

  assert_eq!(every.len(), 1);
  assert_eq!((every[0].owner_account.as_str(), every[0].node.as_str()), ("7", "pc-7"));
  assert!(person_nodes::clear(db, "7").await.unwrap());
  assert!(!person_nodes::clear(db, "7").await.unwrap());
  assert_eq!(
    person_nodes::node_of(db, "7").await.unwrap().as_deref(),
    Some("account-7"),
    "an account with no record works on its isolation node"
  );
}

// "a node another account's record names is refused unless both records are shared"
#[tokio::test]
async fn a_node_another_account_s_record_names_is_refused_unless_both_records_are_shared() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let taken = |by: &str| person_nodes::Taken {
    node: "pc-7".into(),
    by: by.into(),
  };

  person_nodes::set(db, "7", "pc-7", false).await.unwrap().unwrap();
  assert_eq!(
    person_nodes::set(db, "8", "pc-7", true).await.unwrap(),
    Err(taken("7")),
    "7's is not shared"
  );

  person_nodes::set(db, "7", "pc-7", true).await.unwrap().unwrap();
  assert_eq!(
    person_nodes::set(db, "8", "pc-7", false).await.unwrap(),
    Err(taken("7")),
    "8's is not shared"
  );
  person_nodes::set(db, "8", "pc-7", true).await.unwrap().unwrap();
  assert_eq!(
    person_nodes::set(db, "7", "pc-7", false).await.unwrap(),
    Err(taken("8")),
    "an account sharing a node cannot take it back to itself while another shares it"
  );

  let every = person_nodes::every(db).await.unwrap();

  assert_eq!(
    every
      .iter()
      .map(|one| (one.owner_account.as_str(), one.node.as_str(), one.shared))
      .collect::<Vec<_>>(),
    [("7", "pc-7", true), ("8", "pc-7", true)]
  );
  assert_eq!(person_nodes::owners_of(db, "pc-7").await.unwrap(), ["7", "8"]);
}

// "only a hub node the node service keeps and reserves for no service can be an account's"
#[tokio::test]
async fn only_a_hub_node_the_node_service_keeps_and_reserves_for_no_service_can_be_an_account_s() {
  assert_eq!(person_nodes::refusal(&TwoNodes, "pc-7").await.unwrap(), None);
  assert_eq!(
    person_nodes::refusal(&TwoNodes, "box").await.unwrap().as_deref(),
    Some("box is no hub node, so it is no person's machine")
  );
  assert_eq!(
    person_nodes::refusal(&TwoNodes, "engine").await.unwrap().as_deref(),
    Some("engine is reserved for voice, so it is no person's machine")
  );
  assert_eq!(
    person_nodes::refusal(&TwoNodes, "gone").await.unwrap().as_deref(),
    Some("gone is no node the node service keeps")
  );
}

// "a supervisor of an account stands on its PC or its isolation node, and one of no account or of the
// administrator's account stands here"
#[tokio::test]
async fn a_supervisor_of_an_account_stands_on_its_node_and_one_of_no_account_or_the_administrator_s_stands_here()
 {
  let scratch = Scratch::new().await;
  let db = scratch.db();

  rows::supervisor(db, "u7", &[("owner_account", 7i64.into())]).await;
  rows::supervisor(db, "u8", &[("owner_account", 8i64.into())]).await;
  rows::supervisor(db, "u9", &[("owner_account", 9i64.into())]).await;
  rows::supervisor(db, "bmo", &[]).await;
  person_nodes::set(db, "7", "pc-7", false).await.unwrap().unwrap();
  scratch
    .run(r#"INSERT INTO "settings" ("key", "value") VALUES ('users_administrator', '9')"#, vec![])
    .await;

  let node = |name: &'static str| async move { AgentRow::named(db, name).await.unwrap().unwrap().node };

  assert_eq!(node("u7").await.as_deref(), Some("pc-7"));
  assert_eq!(
    node("u8").await.as_deref(),
    Some("account-8"),
    "an account with no PC stands on its isolation node"
  );
  assert_eq!(node("u9").await, None, "the administrator's account is the owner's own");
  assert_eq!(node("bmo").await, None);
  assert!(
    AgentRow::standing(db)
      .await
      .unwrap()
      .iter()
      .any(|row| row.name == "u7" && row.node.as_deref() == Some("pc-7"))
  );
  assert_eq!(
    supervisors_placed(db).await.unwrap(),
    [
      ("bmo".into(), None),
      ("u7".into(), Some("pc-7".into())),
      ("u8".into(), Some("account-8".into())),
      ("u9".into(), None)
    ]
  );
}

// "a role run stands on its seat's account's node, and a run on the deployment's own seat stands here"
#[tokio::test]
async fn a_role_run_stands_on_its_seat_s_account_s_node_and_one_on_the_deployment_s_own_stands_here() {
  let scratch = Scratch::new().await;
  let db = scratch.db();

  for (name, account) in [("coder-07", Some("7")), ("coder-08", Some("8")), ("coder-01", None)] {
    scratch
      .run(
        r#"INSERT INTO "role_agents" ("name", "role", "owner_account") VALUES ($1, 'coder', $2)"#,
        vec![name.into(), account.into()],
      )
      .await;
  }
  person_nodes::set(db, "7", "pc-7", false).await.unwrap().unwrap();

  let placed = |seat: &'static str| composed_as(seat.to_owned(), run_placed(db, Some("coder"), "/work"));
  let on = |account: Option<&str>, node: Option<&str>| RunPlaced {
    account: account.map(str::to_owned),
    node: node.map(str::to_owned),
  };

  assert_eq!(placed("coder-07").await.unwrap(), on(Some("7"), Some("pc-7")));
  assert_eq!(placed("coder-08").await.unwrap(), on(Some("8"), Some("account-8")));
  assert_eq!(placed("coder-01").await.unwrap(), on(None, None));
  assert_eq!(
    run_placed(db, Some("coder"), "/work").await.unwrap(),
    on(None, None),
    "a run on no seat stands here"
  );
}

// "a node's link is heard while it is connected and spoke within twenty seconds"
#[test]
fn a_node_s_link_is_heard_while_it_is_connected_and_spoke_within_twenty_seconds() {
  let data = tempfile::tempdir().unwrap();
  let state = |body: String| {
    std::fs::create_dir_all(data.path().join("nodes/pc-7")).unwrap();
    std::fs::write(data.path().join("nodes/pc-7/link-state.json"), body).unwrap();
  };
  let now = chrono::Utc::now().timestamp_millis();

  assert!(!link_heard(data.path(), "pc-7"), "a node whose link wrote nothing is not heard");
  state(format!(r#"{{"state": "connected", "heard": {now}}}"#));
  assert!(link_heard(data.path(), "pc-7"));
  state(format!(r#"{{"state": "connected", "heard": {}}}"#, now - 60_000));
  assert!(!link_heard(data.path(), "pc-7"));
  state(format!(r#"{{"state": "dropped", "heard": {now}}}"#));
  assert!(!link_heard(data.path(), "pc-7"));
}

/// A run's wait as a test drives it: the node heard where `heard` says so, the placements it reads in turn, the
/// last standing once the rest are read, a stop standing from the ask `stop_at` names on, and what it was told.
struct Driven<Heard> {
  heard: Heard,
  placements: Vec<Placement>,
  read: u32,
  stop_at: Option<u32>,
  asked: u32,
  told: Vec<Option<String>>,
}

impl<Heard: Fn(&str) -> bool + Sync> Driven<Heard> {
  fn new(heard: Heard, placements: Vec<Placement>) -> Self {
    Self {
      heard,
      placements,
      read: 0,
      stop_at: None,
      asked: 0,
      told: Vec::new(),
    }
  }
}

impl<Heard: Fn(&str) -> bool + Send + Sync> NodeWait for Driven<Heard> {
  fn heard(&self, node: &str) -> bool {
    (self.heard)(node)
  }

  async fn present(&mut self) -> Result<Placement, agency_server::Refusal> {
    self.read += 1;

    Ok(if self.placements.len() > 1 {
      self.placements.remove(0)
    } else {
      self.placements[0].clone()
    })
  }

  async fn stopped(&mut self) -> bool {
    self.asked += 1;
    self.stop_at.is_some_and(|at| self.asked >= at)
  }

  async fn told(&mut self, waiting: Option<(&str, i64)>) {
    self.told.push(waiting.map(|(node, _)| node.to_owned()));
  }
}

const EVERY: Duration = Duration::from_secs(5);
const BOUND: Duration = Duration::from_secs(3600);

// "a run waits, asking again, until its node is heard, and goes on at once where it is"
#[tokio::test(start_paused = true)]
async fn a_run_waits_asking_again_until_its_node_is_heard_and_goes_on_at_once_where_it_is() {
  let pc = || Some("pc-7".to_owned());
  let asked = AtomicU32::new(0);
  let mut waiting =
    Driven::new(|_: &str| asked.fetch_add(1, Ordering::SeqCst) >= 3, vec![Placement::On(pc())]);

  assert_eq!(until_placed("chat:1:1", pc(), &mut waiting, EVERY, BOUND).await, Waited::On(pc()));
  assert_eq!(asked.load(Ordering::SeqCst), 4);
  assert_eq!(waiting.told, [pc(), None], "the wait is told as it begins and cleared as it ends");

  let mut heard = Driven::new(|_: &str| true, vec![Placement::Gone]);

  assert_eq!(until_placed("chat:1:2", pc(), &mut heard, EVERY, BOUND).await, Waited::On(pc()));
  assert_eq!(heard.read, 0, "a run on a node heard from goes on without reading again");
  assert!(heard.told.is_empty(), "a run that never waited is told nothing");

  let mut here = Driven::new(|_: &str| false, vec![Placement::Gone]);

  assert_eq!(
    until_placed("chat:1:3", None, &mut here, EVERY, BOUND).await,
    Waited::On(None),
    "a run on this machine waits on nothing"
  );
}

// "a run waiting on a node its account's record no longer names is placed by the present record, and one whose
// supervisor was removed waits no more"
#[tokio::test(start_paused = true)]
async fn a_run_waiting_on_a_node_no_record_names_is_placed_again_and_one_whose_supervisor_went_waits_no_more()
 {
  let mut moved = Driven::new(
    |node: &str| node == "account-7",
    vec![Placement::On(Some("pc-7".to_owned())), Placement::On(Some("account-7".to_owned()))],
  );

  assert_eq!(
    until_placed("chat:1:1", Some("pc-7".to_owned()), &mut moved, EVERY, BOUND).await,
    Waited::On(Some("account-7".to_owned()))
  );

  let mut removed = Driven::new(|_: &str| false, vec![Placement::Gone]);

  assert_eq!(
    until_placed("chat:1:2", Some("pc-7".to_owned()), &mut removed, EVERY, BOUND).await,
    Waited::Gone
  );
  assert_eq!(removed.told, [Some("pc-7".to_owned()), None]);
}

// "a stop standing over a waiting run ends its wait at the next ask"
#[tokio::test(start_paused = true)]
async fn a_stop_standing_over_a_waiting_run_ends_its_wait_at_the_next_ask() {
  let mut stopped = Driven::new(|_: &str| false, vec![Placement::On(Some("pc-7".to_owned()))]);

  stopped.stop_at = Some(3);

  assert_eq!(
    until_placed("chat:1:1", Some("pc-7".to_owned()), &mut stopped, EVERY, BOUND).await,
    Waited::Stopped
  );
  assert_eq!(stopped.read, 2, "the placement is read no more once the stop is seen");
  assert_eq!(stopped.told, [Some("pc-7".to_owned()), None]);
}

// "a run waits on its node no longer than its bound, the reason naming the node and how long"
#[tokio::test(start_paused = true)]
async fn a_run_waits_on_its_node_no_longer_than_its_bound_the_reason_naming_the_node_and_how_long() {
  let mut unheard = Driven::new(|_: &str| false, vec![Placement::On(Some("pc-7".to_owned()))]);
  let began = tokio::time::Instant::now();
  let waited =
    until_placed("chat:1:1", Some("pc-7".to_owned()), &mut unheard, EVERY, Duration::from_secs(600)).await;

  assert_eq!(
    waited,
    Waited::Overdue(
      "pc-7's link was not heard for 10 minutes, so the turn ends without starting".to_owned()
    )
  );
  assert_eq!(began.elapsed(), Duration::from_secs(600));
  assert_eq!(unheard.told, [Some("pc-7".to_owned()), None]);
}

// "a waiting run's placement is read from the present records: a supervisor whose account's record went stands on
// its isolation node and a removed one nowhere, and a role run's place of the account is marked again"
#[tokio::test]
async fn a_waiting_run_s_placement_is_read_from_the_present_records_and_a_role_run_s_place_is_marked_again()
{
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let home = tempfile::tempdir().unwrap();
  let variables: HashMap<String, String> = HashMap::new();
  let service = Service::new(db.clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let machine = Machine::with_env(
    [("HOME".to_owned(), home.path().to_string_lossy().into_owned())],
    home.path().join("data"),
  );
  let ground = Ground::new(&service, &machine, None);
  let node = |agent: &'static str| {
    let ground = &ground;

    async move { placed(ground, agent).await.unwrap().map(|(_, node)| node) }
  };

  rows::supervisor(db, "u7", &[("owner_account", 7i64.into())]).await;
  person_nodes::set(db, "7", "pc-7", false).await.unwrap().unwrap();

  let session = SessionId("coder@apart:3".to_owned());
  let name = agency_server::apart_places::place_name(&session);
  let place = ground.roots.place(&name);

  agency_server::apart_places::mark(&ground, &name, &session, Some("7")).unwrap();
  mark_place_on(&place, Some("pc-7"), "/work").unwrap();

  assert_eq!(node("u7").await, Some(Some("pc-7".to_owned())));
  assert_eq!(
    placed(&ground, &name).await.unwrap().map(|(_, node)| node),
    Some(Some("pc-7".to_owned()))
  );

  person_nodes::clear(db, "7").await.unwrap();

  assert_eq!(node("u7").await, Some(Some("account-7".to_owned())));
  assert_eq!(
    placed(&ground, &name).await.unwrap().map(|(_, node)| node),
    Some(Some("account-7".to_owned()))
  );
  assert_eq!(
    (place_node(&place).as_deref(), place_cwd(&place).as_deref()),
    (Some("account-7"), Some("/work")),
    "the role run's place is marked with the node it stands on now"
  );

  scratch.run(r#"DELETE FROM "supervisors" WHERE "name" = 'u7'"#, vec![]).await;

  assert_eq!(node("u7").await, None, "a removed supervisor stands nowhere");
}

// "an account's caller reaches its own account's node alone, and a supervisor of no account reaches every node"
#[tokio::test]
async fn an_account_s_caller_reaches_its_own_account_s_node_alone_and_a_supervisor_of_no_account_every_node()
 {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let data = tempfile::tempdir().unwrap();

  rows::supervisor(db, "u7", &[("owner_account", 7i64.into())]).await;
  rows::supervisor(db, "u9", &[("owner_account", 9i64.into())]).await;
  rows::supervisor(db, "bmo", &[]).await;
  scratch
    .run(r#"INSERT INTO "settings" ("key", "value") VALUES ('users_administrator', '9')"#, vec![])
    .await;
  for (name, account) in [("coder-07", Some("7")), ("coder-01", None)] {
    scratch
      .run(
        r#"INSERT INTO "role_agents" ("name", "role", "owner_account") VALUES ($1, 'coder', $2)"#,
        vec![name.into(), account.into()],
      )
      .await;
  }
  scratch
    .run(
      r#"INSERT INTO "role_slot_requests" ("token", "request_key", "owner", "source", "role", "path", "provider",
           "model", "effort", "capacity", "state", "enqueued_at", "class", "seat")
         VALUES ('seven', 'seven', 'u7', 'job', 'coder', '/work/seven/', 'p', 'm', '', 1, 'active', 0,
           'foreground', 'coder-07')"#,
      vec![],
    )
    .await;
  person_nodes::set(db, "7", "pc-7", true).await.unwrap().unwrap();
  person_nodes::set(db, "9", "pc-7", true).await.unwrap().unwrap();
  scratch
    .run(
      r#"INSERT INTO "node_workplaces" ("node", "root", "home", "state", "created_at", "updated_at")
         VALUES ('pc-7', '~/node-root', '/home/tester', 'ready', 0, 0)"#,
      vec![],
    )
    .await;

  let access = |node: &'static str, agent: &'static str, role: Option<&'static str>, checkout| {
    let data = data.path().to_owned();

    async move {
      person_nodes::access(
        db,
        &data,
        node,
        Caller {
          agent,
          role,
          checkout,
        },
      )
      .await
      .unwrap()
    }
  };

  let own = access("pc-7", "u7", None, None).await;

  assert_eq!(own.owners, ["7", "9"]);
  assert_eq!(own.account.as_deref(), Some("7"));
  assert!(own.reaches && !own.administrator);
  // Its account shares the node, so the tool runs its command inside its account's root there.
  assert_eq!(own.root.as_deref(), Some("/home/tester/node-root/accounts/7"));
  assert_eq!(
    access("pc-8", "u7", None, None).await.root,
    None,
    "no root on a node its account does not share"
  );
  assert!(!access("pc-8", "u7", None, None).await.reaches, "another node is not its account's");

  let deployment = access("pc-8", "bmo", None, None).await;

  assert!(deployment.owners.is_empty());
  assert!(deployment.administrator && deployment.reaches && deployment.account.is_none());

  // A supervisor of the administrator's account is the owner's own and holds the grant.
  let administrator = access("pc-8", "u9", None, None).await;

  assert!(administrator.administrator && administrator.reaches);
  assert_eq!(administrator.account.as_deref(), Some("9"));

  // An isolation node is owned by the account it is named for, and reached by its callers alone.
  let isolated = access("account-7", "u7", None, None).await;

  assert_eq!(isolated.owners, ["7"]);
  assert!(isolated.reaches);
  assert_eq!(isolated.root, None, "an isolation node is its account's alone");
  assert!(!access("account-8", "u7", None, None).await.reaches, "another account's isolation node");

  // A role agent is read by its name, and a run holding no agent's name by the seat at its role and folder.
  assert_eq!(access("pc-7", "coder-07", None, None).await.account.as_deref(), Some("7"));
  // A role agent or a seat's run of no account is the deployment's own, yet holds no grant and owns no node.
  let unowned = access("pc-8", "coder-01", None, None).await;

  assert!(unowned.account.is_none() && !unowned.administrator && !unowned.reaches);
  assert_eq!(
    access("pc-8", "worker", Some("coder"), Some("/work/seven")).await.account.as_deref(),
    Some("7")
  );
  assert!(!access("pc-8", "worker", Some("coder"), Some("/work/seven")).await.reaches);
  assert!(!access("pc-8", "worker", Some("coder"), Some("/work/other")).await.reaches);

  // The grants a record holds follow the same judgement.
  let acting = |administrator| ActingFor {
    account: None,
    administrator,
  };

  assert_eq!(acting(false).grants(), Vec::<String>::new());
  assert_eq!(acting(true).grants(), ["node.admin", "platform.payment"]);
}

// "a role session's turn runs as the seat of the slot it was started on, one asked with no path among them"
#[tokio::test(flavor = "multi_thread")]
async fn a_role_session_s_turn_runs_as_the_seat_of_its_slot_one_asked_with_no_path_among_them() {
  use agency_server::order_run::dispatch::{self, Asked, Wanted};
  use agency_server::role_agents::session_seat;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = crate::support::grounds::seeded(scratch.db(), home.path()).await;
  let db = &ground.db;

  crate::support::grounds::deepseek_balance(&ground, 5.0).await;

  let asked = Asked {
    request_key: "test:pathless".into(),
    owner: "owner:pathless".into(),
    wanted: Wanted {
      role: "worker",
      path: None,
      fixed: None,
      agent: None,
    },
    ..Asked::default()
  };
  let held = dispatch::enqueue(db, &ground.events, &asked, "job", async |_, _| Ok(())).await.unwrap();
  let active = dispatch::acquire(&ground, &held).await.unwrap().expect("let in");
  let seat = active.seat.clone().expect("seated");
  let folder = "/agents/worker";

  scratch
    .run(
      r#"UPDATE "role_agents" SET "owner_account" = '7' WHERE "name" = $1"#,
      vec![seat.clone().into()],
    )
    .await;
  person_nodes::set(db, "7", "pc-7", false).await.unwrap().unwrap();
  dispatch::bind_session(db, &ground.events, &active.token, "worker@run:1").await.unwrap();

  assert_eq!(
    run_placed(db, Some("worker"), folder).await.unwrap(),
    RunPlaced::default(),
    "a slot asked with no path is found at no folder"
  );
  assert_eq!(session_seat(db, "worker@run:1").await.unwrap(), Some(seat.clone()));
  assert_eq!(
    session_seat(db, "worker@run:2").await.unwrap(),
    None,
    "a session no slot was started on"
  );
  assert_eq!(
    composed_as(seat, run_placed(db, Some("worker"), folder)).await.unwrap(),
    RunPlaced {
      account: Some("7".into()),
      node: Some("pc-7".into()),
    },
    "run as its slot's seat, the turn stands on that seat's account's node"
  );
}
