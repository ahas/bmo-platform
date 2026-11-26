//! The queue's minute pass (docs/agent-control.md, 「An idle agent is told to the orderers of the items it could
//! take」 and 「An item's change is told to its orderer」): an agent switched on, free and able to take an open
//! item told to the orderer of each item it could take, one instruction a room, once its launch seconds pass,
//! again where a room's items change, and withdrawn once it is busy; an open item whose `agent` or `role` label no
//! launch takes told once for each label, at once as the owner changes an agent; a draft awaiting approval told
//! once to the supervisor that ordered it, in its room or its parent's up the `raised-by` chain; a held item every
//! item it waits on has ended told once to its orderer while it stays held; and an item whose notices are
//! switched off told of by none of these.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use agency_server::agent_roles::job_role;
use agency_server::queue::orderer::Orderer;
use agency_server::queue::settings;
use agency_server::role_agents::RoleAgent;
use agency_server::watches::machine::{LaunchHeld, tell_launch_held};
use agency_server::watches::queue_idle::{
  DRAFT_TOLD_PREFIX, Draft, IDLE_OWED, IDLE_SEEN_PREFIX, Open, Standing, approval_instruction, brief_head,
  draft_of, drafts_due, held, idle_agents, idle_instruction, room_of, unlaunchable,
};
use axum::body::Bytes;
use axum::extract::State;
use axum::http::Uri;
use axum::response::IntoResponse;
use policy::queue_idle::QUEUE_IDLE;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::support::rows::{repository, seats};
use crate::support::scratch::Scratch;
use crate::support::wording::assert_no_instruction;

// The pass with no gate deployed, so a word for a job goes as written.
async fn pass(
  db: &sea_orm::DatabaseConnection,
  vars: &std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
  agency_server::watches::queue_idle::pass(db, vars, &crate::support::gates::none()).await
}

fn open(id: i64, assignee: Option<&str>) -> Open {
  Open {
    id,
    title: format!("item {id}"),
    assignee: assignee.map(str::to_owned),
    queue: "todo".to_owned(),
    ..Open::default()
  }
}

fn room(room: Option<i64>) -> Orderer {
  Orderer {
    room,
    requester: None,
  }
}

fn standing(open: Vec<Open>, free: &[&str]) -> Standing {
  Standing {
    open,
    free: free.iter().map(|name| (*name).to_owned()).collect(),
    known: ["w1", "w2", "worker"].map(str::to_owned).to_vec(),
    ..Standing::default()
  }
}

// An agent of a role on a road of Claude Code's, switched on.
fn agent(name: &str, role: &str, model: &str) -> RoleAgent {
  RoleAgent {
    name: name.to_owned(),
    role: role.to_owned(),
    owner_account: None,
    owner_unit: None,
    home_node: None,
    provider: Some("claude-code".to_owned()),
    model: Some(model.to_owned()),
    effort: None,
    thinking: None,
    answer_tokens: None,
    advisor: None,
    enabled: true,
    keeps_cache: false,
    sessions: Default::default(),
    fold_ratio: None,
    prompt: None,
    bundles: None,
    skills: None,
    added_at: 0,
    follows_asker: None,
  }
}

fn coder(name: &str, model: &str) -> RoleAgent {
  agent(name, "coder", model)
}

// An open item of the coder role on the checkout w1.
fn for_coder(id: i64) -> Open {
  Open {
    role: Some("coder".to_owned()),
    ..open(id, Some("w1"))
  }
}

// Each idle agent by name with the items it could take, in the order they are told.
fn idle(standing: &Standing) -> Vec<(String, Vec<i64>)> {
  idle_agents(standing)
    .iter()
    .map(|idle| (idle.agent.name.clone(), idle.items.iter().map(|item| item.id).collect()))
    .collect()
}

#[test]
fn a_checkout_is_free_for_an_item_naming_it_its_repository_or_none_as_the_worker_checkout() {
  let mut support_free = standing(vec![], &["w1-w2"]);

  assert!(!support_free.free_for(Some("w1")), "a base of another repository, whatever its name");
  support_free.repositories = BTreeMap::from([("w1-w2".to_owned(), "w1".to_owned())]);
  assert!(support_free.free_for(Some("w1")), "its support worktree free");

  assert!(!standing(vec![], &["w1"]).free_for(Some("w2")), "another repository's free checkout");
  assert!(
    !standing(vec![], &["w1"]).free_for(None),
    "naming none while the worker checkout is held"
  );
  assert!(standing(vec![], &["w1", "worker"]).free_for(None));
}

// A checkout is free as an assignment reads it: a hold stands on it as much as a job, the hold its item was
// assigned again after let go first.
#[tokio::test]
async fn a_checkout_a_hold_stands_on_is_not_free() {
  let scratch = Scratch::new().await;
  let db = scratch.db();

  repository(db, "w1", "/checkouts/w1", &[("support_count", 1i64.into())]).await;
  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at", "changed_at")
         VALUES (1, 30, 'left', 'w1', 'held', 0, 0), (2, 31, 'left', 'w1-w2', 'held', 0, 0),
                (3, 31, 'harness', 'w1', 'released', 1, 1)"#,
      vec![],
    )
    .await;

  let standing = held(db, vec![open(1, Some("w1"))]).await.unwrap();

  assert_eq!(standing.free, ["w1-w2"], "the base held, the support worktree's hold let go");
  assert!(standing.free_for(Some("w1")));
}

// A repository set to `auto` counts as free with every checkout held, until the launcher holds launches for the
// machine's room; one set to a count waits for a checkout.
#[tokio::test]
async fn a_repository_that_grows_counts_as_free_until_launches_are_held_for_the_machine() {
  let scratch = Scratch::new().await;
  let db = scratch.db();

  repository(db, "grown", "/checkouts/grown", &[]).await;
  repository(db, "fixed", "/checkouts/fixed", &[("support_count", 0i64.into())]).await;
  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at", "changed_at")
         VALUES (1, 30, 'left', 'grown', 'held', 0, 0), (2, 31, 'left', 'fixed', 'held', 0, 0)"#,
      vec![],
    )
    .await;

  let standing = held(db, vec![]).await.unwrap();

  assert!(standing.free_for(Some("grown")) && !standing.free_for(Some("fixed")));

  tell_launch_held(
    db,
    &LaunchHeld {
      at: 1,
      held: vec!["available memory 3GB < 4GB".to_owned()],
    },
  )
  .await
  .unwrap();

  let standing = held(db, vec![]).await.unwrap();

  assert!(!standing.free_for(Some("grown")), "a launch held for the machine grows nothing");
}

#[test]
fn an_agent_switched_on_on_a_road_carrying_no_run_and_able_to_take_an_open_item_is_idle_with_it() {
  let base = Standing {
    agents: vec![coder("coder-a", "opus")],
    ..standing(vec![for_coder(1)], &["w1"])
  };

  assert_eq!(idle(&base), [("coder-a".to_owned(), vec![1])]);

  let not_idle = |change: &dyn Fn(&mut Standing), why: &str| {
    let mut changed = base.clone();

    change(&mut changed);
    assert!(idle(&changed).is_empty(), "{why}");
  };

  not_idle(&|held| held.carrying = vec!["coder-a".to_owned()], "it carries a run");
  not_idle(&|held| held.agents[0].enabled = false, "switched off");
  not_idle(&|held| held.agents[0].owner_account = Some(7), "a login account's seat");
  not_idle(&|held| held.agents[0].provider = None, "its road names no provider");
  not_idle(&|held| held.agents[0].model = None, "its road names no model");
  not_idle(&|held| held.open[0].role = Some("worker".to_owned()), "the item is for another role");
  not_idle(&|held| held.open[0].agent = Some("coder-b".to_owned()), "the item names another agent");
  not_idle(&|held| held.open[0].model = Some("gpt".to_owned()), "a model its road does not run");
  not_idle(&|held| held.free.clear(), "no checkout the item may go to is free");

  let mut on_its_model = base.clone();

  on_its_model.open[0].model = Some("claude-code/opus".to_owned());
  assert_eq!(idle(&on_its_model).len(), 1, "its model named with its provider");

  // An item naming the agent is its whatever its role label says: the job runs as its agent's role.
  let mut named = base.clone();

  named.open[0].role = Some("worker".to_owned());
  named.open[0].agent = Some("coder-a".to_owned());
  named.open[0].model = Some("gpt".to_owned());
  assert_eq!(idle(&named), [("coder-a".to_owned(), vec![1])]);
}

#[test]
fn an_idle_agent_s_items_stand_in_the_queue_s_order_and_an_item_naming_no_agent_or_model_goes_to_any() {
  let placed = |id: i64, priority: i64, position: i64| Open {
    priority,
    position,
    ..for_coder(id)
  };
  let held = Standing {
    agents: vec![
      coder("coder-a", "opus"),
      coder("coder-b", "local"),
      agent("worker-a", job_role(), "opus"),
    ],
    ..standing(
      vec![placed(7, 0, 2), placed(5, 0, 2), placed(6, 0, 1), placed(9, 1, 5), open(4, Some("w1"))],
      &["w1"],
    )
  };

  assert_eq!(
    idle(&held),
    [
      ("coder-a".to_owned(), vec![9, 6, 5, 7]),
      ("coder-b".to_owned(), vec![9, 6, 5, 7]),
      ("worker-a".to_owned(), vec![4]),
    ],
    "the highest priority, then the earliest place, then the lowest number; an item naming no role is the job \
     role's"
  );
}

#[test]
fn an_idle_agent_is_told_in_its_orderer_s_room_with_its_road_and_the_room_s_items() {
  let coder_a = coder("coder-a", "opus");
  let (first, second) = (for_coder(2265), for_coder(2268));
  let told = idle_instruction(&coder_a, &[&first, &second], &room(Some(-9_000_000_000_003)));
  let order = told["said"]["order"].as_str().unwrap();

  assert_eq!(
    (&told["sort"], &told["kind"], &told["room"]),
    (
      &json!("job"),
      &json!("agent-idle:coder-a@-9000000000003"),
      &json!(-9_000_000_000_003_i64)
    )
  );

  // A room naming none goes to the owner's private room under a kind of its own, so the receiver, which hands it
  // and a room naming the owner's to one chat, never lets one replace the other.
  let owner = idle_instruction(&coder_a, &[&first], &room(None));

  assert_eq!((&owner["room"], &owner["kind"]), (&Value::Null, &json!("agent-idle:coder-a@owner")));

  for word in ["coder-a", "coder", "claude-code/opus", "#2265 (item 2265)", "#2268 (item 2268)"] {
    assert!(order.contains(word), "{word} in {order}");
  }

  assert!(order.find("#2265").unwrap() < order.find("#2268").unwrap(), "{order}");
  assert_no_instruction(order);

  // An item asked in a room names it: its roomId, else the room its scope names.
  assert_eq!(room_of(&json!({ "scope": "-9000000000003" })), Some(-9_000_000_000_003));
  assert_eq!(room_of(&json!({ "roomId": 5, "scope": "-7" })), Some(5));
  assert_eq!(room_of(&json!({ "scope": "" })), None);
}

#[test]
fn an_item_whose_assignee_is_no_checkout_is_unlaunchable_and_no_agent_could_take_it() {
  let stuck = Standing {
    agents: vec![agent("worker-a", job_role(), "opus")],
    ..standing(vec![open(1, Some("gone")), open(2, Some("w1")), open(3, None)], &["w1"])
  };

  assert_eq!(unlaunchable(&stuck), vec![&open(1, Some("gone"))]);
  assert_eq!(idle(&stuck), [("worker-a".to_owned(), vec![2])]);
}

#[test]
fn a_draft_names_who_raised_it_its_parent_and_its_own_room() {
  let derived = draft_of(&json!({
    "id": 2433, "title": "a follow-up", "state": "draft", "brief": "do it",
    "labels": { "raised-by": "2432" }, "requesterName": "chat", "assigneeName": "w1",
  }))
  .unwrap();

  assert_eq!(
    (
      derived.parent,
      derived.raised_by.as_deref(),
      derived.room,
      derived.repository.as_deref()
    ),
    (Some(2432), Some("item #2432"), None, Some("w1"))
  );

  let asked =
    draft_of(&json!({ "id": 7, "title": "asked", "requesterName": "member", "roomId": 5 })).unwrap();

  assert_eq!((asked.parent, asked.raised_by.as_deref(), asked.room), (None, Some("member"), Some(5)));
  assert_eq!(draft_of(&json!({ "labels": { "raised-by": 12 } })), None, "a member with no number");
  assert_eq!(
    drafts_due(&[derived.clone(), asked], &BTreeSet::from([7])),
    vec![&derived],
    "a draft already told is not due"
  );
}

#[test]
fn a_draft_is_told_as_a_job_order_with_its_kind_event_and_the_start_of_its_brief() {
  let brief = (1..=40).map(|line| format!("line {line}\n\n")).collect::<String>();
  let draft = Draft {
    id: 2434,
    title: "a follow-up".to_owned(),
    raised_by: Some("item #2432".to_owned()),
    parent: Some(2432),
    repository: Some("w1".to_owned()),
    brief,
    room: None,
  };
  let told = approval_instruction(&draft, &room(Some(-9_000_000_000_003)));
  let order = told["said"]["order"].as_str().unwrap();

  assert_eq!(told["sort"], "job");
  assert_eq!(told["kind"], "item-awaits-approval:2434");
  assert_eq!(told["events"], json!(["item-awaits-approval:#2434"]));
  assert_eq!(told["room"], json!(-9_000_000_000_003_i64));
  assert_eq!(
    approval_instruction(&draft, &room(None))["room"],
    Value::Null,
    "the owner's private room"
  );

  for word in ["#2434", "a follow-up", "item #2432", "w1", "line 1"] {
    assert!(order.contains(word), "{order}");
  }

  assert_no_instruction(order);

  let head = brief_head(&draft.brief);

  assert_eq!(head.lines().count(), QUEUE_IDLE.draft_brief_lines, "{head}");
  assert!(!head.contains("\n\n") && !order.contains("line 40"), "{order}");

  let long = brief_head(&"x".repeat(QUEUE_IDLE.draft_brief_chars + 10));

  assert_eq!(long.chars().count(), QUEUE_IDLE.draft_brief_chars + 1, "cut, with its mark");
}

type Calls = Arc<Mutex<Vec<(String, Value)>>>;
type Drafts = Arc<Mutex<Vec<Value>>>;

#[derive(Clone, Default)]
struct Peers {
  calls: Calls,
  drafts: Drafts,
}

// The job service and the receiver in one: two queues, one paused, the open items across them, the drafts
// a test sets and the parents the drafts name; every call kept.
async fn stand_in(State(peers): State<Peers>, uri: Uri, body: Bytes) -> axum::response::Response {
  peers
    .calls
    .lock()
    .unwrap()
    .push((uri.path().to_owned(), serde_json::from_slice(&body).unwrap_or(Value::Null)));

  if uri.path() == "/items" && uri.query().is_some_and(|query| query.contains("state=draft")) {
    return axum::Json(json!({ "items": *peers.drafts.lock().unwrap(), "more": false })).into_response();
  }

  // The parents: 30 asked in a room, 31 raised by 30 and naming none, 33 and 34 raising each other.
  match uri.path() {
    "/items/30" => {
      return axum::Json(json!({ "id": 30, "state": "done", "scope": "-9000000000007" })).into_response();
    }
    "/items/31" => {
      return axum::Json(json!({ "id": 31, "state": "done", "labels": { "raised-by": "30" } }))
        .into_response();
    }
    "/items/33" => {
      return axum::Json(json!({ "id": 33, "state": "done", "labels": { "raised-by": "34" } }))
        .into_response();
    }
    "/items/34" => {
      return axum::Json(json!({ "id": 34, "state": "done", "labels": { "raised-by": "33" } }))
        .into_response();
    }
    path if path.starts_with("/items/") => {
      return (axum::http::StatusCode::NOT_FOUND, axum::Json(json!({ "code": "not-found" })))
        .into_response();
    }
    _ => {}
  }

  match uri.path() {
    "/queues" => axum::Json(json!({
      "items": [
        { "name": "todo", "state": "open", "holds": [] },
        { "name": "later", "state": "paused", "holds": [] },
      ],
      "more": false,
    })),
    "/items" => axum::Json(json!({
      "items": [
        { "id": 11, "title": "for the free worker", "state": "open", "queueName": "todo", "assigneeName": "w1",
          "noticesOff": null },
        { "id": 12, "title": "for the busy worker", "state": "open", "queueName": "todo", "assigneeName": "w2",
          "noticesOff": null },
        { "id": 13, "title": "in a paused queue", "state": "open", "queueName": "later", "assigneeName": "w1",
          "noticesOff": null },
        { "id": 14, "title": "opened by its lane", "state": "open", "queueName": "todo", "assigneeName": "w1",
          "labels": { "lane": "pr-review" }, "scope": "-9000000000003", "noticesOff": null },
        { "id": 15, "title": "the owner's own ask", "state": "open", "queueName": "later", "assigneeName": "w1",
          "labels": { "asked-by": "owner" }, "scope": "-9000000000003", "noticesOff": null },
      ],
      "more": false,
    })),
    _ => axum::Json(json!({ "id": 1 })),
  }
  .into_response()
}

async fn stand(peers: &Peers) -> BTreeMap<String, String> {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());
  let app = axum::Router::new().fallback(stand_in).with_state(peers.clone());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  [
    ("BMO_JOB_URL".to_owned(), address.clone()),
    ("BMO_JOB_TOKEN".to_owned(), "job-token".to_owned()),
    ("BMO_RECEIVER_URL".to_owned(), address.clone()),
    ("BMO_RECEIVER_TOKEN".to_owned(), "receiver-token".to_owned()),
  ]
  .into()
}

// The approval instructions the stand-in was handed, in the order they came.
fn approvals(calls: &Calls) -> Vec<Value> {
  calls
    .lock()
    .unwrap()
    .iter()
    .filter(|(path, body)| {
      path == "/instructions"
        && body["kind"].as_str().is_some_and(|kind| kind.starts_with("item-awaits-approval:"))
    })
    .map(|(_, body)| body.clone())
    .collect()
}

#[tokio::test]
async fn a_draft_is_told_once_to_the_supervisor_that_ordered_it_and_never_after_it_leaves_draft() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let peers = Peers::default();
  let vars = stand(&peers).await;

  *peers.drafts.lock().unwrap() = vec![
    json!({ "id": 21, "title": "asked in a room", "state": "draft", "scope": "-9000000000005",
            "requesterName": "member", "brief": "fix the thing" }),
    json!({ "id": 22, "title": "a worker's follow-up", "state": "draft", "labels": { "raised-by": "30" },
            "assigneeName": "w1", "brief": "the follow-up" }),
    json!({ "id": 23, "title": "a follow-up of a follow-up", "state": "draft",
            "labels": { "raised-by": "31" } }),
    json!({ "id": 24, "title": "naming nothing", "state": "draft" }),
    json!({ "id": 25, "title": "its parent gone", "state": "draft", "labels": { "raised-by": "32" } }),
    json!({ "id": 26, "title": "a chain going round", "state": "draft", "labels": { "raised-by": "33" } }),
  ];

  pass(db, &vars).await.unwrap();

  let told = approvals(&peers.calls);
  let by_item: BTreeMap<String, (Value, Value)> = told
    .iter()
    .map(|body| (body["kind"].as_str().unwrap().to_owned(), (body["room"].clone(), body["sort"].clone())))
    .collect();

  assert_eq!(told.len(), 6, "each draft told once: {told:?}");
  assert_eq!(
    by_item,
    BTreeMap::from([
      ("item-awaits-approval:21".to_owned(), (json!(-9_000_000_000_005_i64), json!("job"))),
      ("item-awaits-approval:22".to_owned(), (json!(-9_000_000_000_007_i64), json!("job"))),
      ("item-awaits-approval:23".to_owned(), (json!(-9_000_000_000_007_i64), json!("job"))),
      ("item-awaits-approval:24".to_owned(), (Value::Null, json!("job"))),
      ("item-awaits-approval:25".to_owned(), (Value::Null, json!("job"))),
      ("item-awaits-approval:26".to_owned(), (Value::Null, json!("job"))),
    ]),
    "its own room, else its parent's up the chain, else the owner's private room"
  );
  assert!(
    told
      .iter()
      .all(|body| body["events"][0].as_str().unwrap().starts_with("item-awaits-approval:#"))
  );

  // The next pass, as after a restart, tells none of them again while they stay drafts.
  pass(db, &vars).await.unwrap();
  assert_eq!(approvals(&peers.calls).len(), 6, "told once");

  // An approved draft leaves `draft`: its row let go, and it is told nothing more.
  peers.drafts.lock().unwrap().retain(|draft| draft["id"] != 21);
  pass(db, &vars).await.unwrap();
  assert_eq!(approvals(&peers.calls).len(), 6, "an item that left draft is told nothing");

  let rows = |key: String| async move { agency_server::queue::settings::get(db, &key).await.unwrap() };

  assert_eq!(rows(format!("{DRAFT_TOLD_PREFIX}21")).await, None, "its row let go");
  assert!(rows(format!("{DRAFT_TOLD_PREFIX}22")).await.is_some(), "a standing draft's row kept");

  // A new draft is told on the next pass.
  peers
    .drafts
    .lock()
    .unwrap()
    .push(json!({ "id": 27, "title": "a new one", "state": "draft", "roomId": 9 }));
  pass(db, &vars).await.unwrap();

  let told = approvals(&peers.calls);

  assert_eq!(told.len(), 7);
  assert_eq!((&told[6]["kind"], &told[6]["room"]), (&json!("item-awaits-approval:27"), &json!(9)));
}

#[tokio::test]
async fn an_item_the_owner_asked_for_opens_at_once_and_is_never_told_for_approval() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let peers = Peers::default();
  let vars = stand(&peers).await;

  // The open items the stand-in serves carry one the owner's ask opened; no draft stands.
  pass(db, &vars).await.unwrap();
  pass(db, &vars).await.unwrap();

  assert!(approvals(&peers.calls).is_empty(), "{:?}", approvals(&peers.calls));
  assert!(
    peers.calls.lock().unwrap().iter().any(|(path, _)| path == "/items"),
    "the pass read the job service"
  );
}

// The kinds of the instructions the stand-in was handed, each with its room, in the order they came.
fn kinds(peers: &crate::support::peers::Peers) -> Vec<(String, Value)> {
  peers
    .lock()
    .unwrap()
    .instructed()
    .iter()
    .map(|body| (body["kind"].as_str().unwrap_or_default().to_owned(), body["room"].clone()))
    .collect()
}

// The order of the last instruction of a kind the stand-in was handed.
fn order_of(peers: &crate::support::peers::Peers, kind: &str) -> String {
  peers
    .lock()
    .unwrap()
    .instructed()
    .iter()
    .rfind(|body| body["kind"] == kind)
    .map(|body| body["said"]["order"].as_str().unwrap_or_default().to_owned())
    .unwrap_or_default()
}

// The moment an agent was first seen idle set past the launch seconds, as if it had stayed idle that long.
async fn idle_since(db: &sea_orm::DatabaseConnection, agent: &str) {
  let key = format!("{IDLE_SEEN_PREFIX}{agent}");
  let since = agency_server::now_ms() - QUEUE_IDLE.launch_seconds * 1000 - 1;

  assert!(settings::get(db, &key).await.unwrap().is_some(), "{agent} was seen idle");
  settings::set(db, &key, &since.to_string()).await.unwrap();
}

#[tokio::test]
async fn open_items_that_cannot_go_are_told_once_to_their_orderer_and_a_merged_one_is_no_agent_s() {
  use crate::support::peers::{item, stand};

  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  repository(db, "w1", "/checkouts/w1", &[]).await;
  repository(db, "worker", "/checkouts/worker", &[]).await;
  seats(db, job_role(), &[("claude-code", "opus")]).await;

  {
    let mut held = peers.lock().unwrap();

    held.queues = vec![
      json!({ "name": "todo", "state": "open", "holds": [] }),
      json!({ "name": "later", "state": "paused", "holds": [] }),
      json!({ "name": "aside", "state": "open", "holds": [{ "reason": "deploy" }] }),
    ];
    // 90 holds the room; 91 is raised by it and names no checkout, so it goes to `worker`; 92 waits on a paused queue, 93 on one
    // under a hold; 94 names a repository no checkout is of.
    held.items.insert(90, item(90, "done", json!({ "scope": "-9000" })));
    held.items.insert(91, item(91, "open", json!({ "raisedBy": 90 })));
    held
      .items
      .insert(92, item(92, "open", json!({ "queueName": "later", "scope": "-9200" })));
    held
      .items
      .insert(93, item(93, "open", json!({ "queueName": "aside", "labels": { "raised-by": "90" } })));
    held
      .items
      .insert(94, item(94, "open", json!({ "assigneeName": "nowhere", "scope": "-9400" })));
    // 95 and 96 merged their commits and wait for their repository's deployment, one on the free checkout's
    // queue and one on the paused queue: neither is work to take.
    held.items.insert(
      95,
      item(
        95,
        "open",
        json!({ "labels": { "close-phase": "waiting-batch", "close-commit": "abc" }, "scope": "-9500" }),
      ),
    );
    held.items.insert(
      96,
      item(
        96,
        "open",
        json!({ "queueName": "later", "labels": { "close-phase": "waiting-live" }, "scope": "-9600" }),
      ),
    );
  }

  pass(db, &vars).await.unwrap();
  idle_since(db, &format!("{}-01", job_role())).await;
  pass(db, &vars).await.unwrap();
  pass(db, &vars).await.unwrap();

  let told: BTreeMap<String, Value> = kinds(&peers).into_iter().collect();
  let agent_idle = format!("{IDLE_OWED}{}-01@-9000", job_role());

  assert_eq!(kinds(&peers).len(), 4, "each told once: {:?}", kinds(&peers));
  assert_eq!(
    told,
    BTreeMap::from([
      ("item-queue-paused:92".to_owned(), json!(-9200)),
      ("item-queue-paused:93".to_owned(), json!(-9000)),
      ("item-unlaunchable:94".to_owned(), json!(-9400)),
      (agent_idle.clone(), json!(-9000)),
    ]),
    "each in its orderer's room, a derived item's in its parent's"
  );

  let order = order_of(&peers, &agent_idle);

  assert!(
    order.contains("#91") && !order.contains("#95"),
    "a merged item is no work to take: {order}"
  );

  // The paused queue opens: the item's mark let go; the queue paused again tells it again.
  peers.lock().unwrap().queues[1]["state"] = json!("open");
  pass(db, &vars).await.unwrap();
  assert!(agency_server::queue::settings::get(db, "queue_paused_told:92").await.unwrap().is_none());
  peers.lock().unwrap().queues[1]["state"] = json!("paused");
  pass(db, &vars).await.unwrap();

  let again = kinds(&peers).into_iter().filter(|(kind, _)| kind == "item-queue-paused:92").count();

  assert_eq!(again, 2, "a transition that comes again is told again");
}

#[tokio::test]
async fn an_item_whose_agent_label_was_refused_is_told_once_for_that_label() {
  use agency_server::watches::queue_idle::{AGENT_TOLD_PREFIX, agent_told_let_go, tell_agent_refused};

  use crate::support::peers::stand;

  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;
  let item = json!({ "title": "on a named agent", "scope": "-9500", "labels": { "agent": "nobody-07" } });
  let why = "Item #95 names agent=nobody-07, which is no agent the agency registers";

  assert!(tell_agent_refused(db, &vars, 95, &item, "nobody-07", why).await.unwrap());
  assert!(!tell_agent_refused(db, &vars, 95, &item, "nobody-07", why).await.unwrap(), "told once");
  assert_eq!(kinds(&peers), vec![("item-agent-refused:95".to_owned(), json!(-9500))]);

  let said = peers.lock().unwrap().instructed()[0]["said"]["order"]
    .as_str()
    .unwrap_or_default()
    .to_owned();

  assert!(said.contains("agent=nobody-07"), "{said}");

  // Another label refused is told again; a launch of the item lets the mark go.
  assert!(tell_agent_refused(db, &vars, 95, &item, "auto", why).await.unwrap());
  agent_told_let_go(db, 95).await;
  assert!(
    agency_server::queue::settings::get(db, &format!("{AGENT_TOLD_PREFIX}95"))
      .await
      .unwrap()
      .is_none()
  );
  assert_eq!(kinds(&peers).len(), 2);
}

#[test]
fn a_held_item_names_what_it_waits_on_and_an_item_waited_on_ends_once_done_and_not_waiting_live() {
  use agency_server::watches::queue_idle::{follower_of, wait_over};

  let one = follower_of(&json!({ "id": 2650, "title": "after", "state": "held",
    "labels": { "held_kind": "pause", "follows": "2649" }, "scope": "-9100" }))
  .unwrap();

  assert_eq!(
    (one.label.as_str(), one.waits_on.as_slice(), one.room),
    ("follows=2649", &[2649][..], Some(-9100))
  );

  let several =
    follower_of(&json!({ "id": 2693, "labels": { "waiting-on": "#2664, 2665" }, "raisedBy": 2600 }))
      .unwrap();

  assert_eq!((several.waits_on.as_slice(), several.parent), (&[2664, 2665][..], Some(2600)));
  assert_eq!(
    follower_of(&json!({ "id": 1, "labels": { "held_kind": "pause" } })),
    None,
    "no wait label"
  );
  assert_eq!(
    follower_of(&json!({ "id": 1, "labels": { "follows": "2649, soon" } })),
    None,
    "a part naming no item"
  );
  assert_eq!(
    follower_of(&json!({ "labels": { "follows": "2649" } })),
    None,
    "a member with no number"
  );

  assert!(wait_over(&json!({ "state": "done" })));
  assert!(wait_over(
    &json!({ "state": "done", "labels": { "close-phase": "live", "close-commit": "abc" } })
  ));
  assert!(
    wait_over(&json!({ "state": "done", "labels": { "close-phase": "testing" } })),
    "no commit waits"
  );
  assert!(
    wait_over(
      &json!({ "state": "done", "labels": { "close-phase": "waiting-live", "close-commit": "abc" } })
    ),
    "done is past its deployment, whatever the close label still reads"
  );
  assert!(!wait_over(
    &json!({ "state": "leased", "labels": { "close-phase": "waiting-live", "close-commit": "abc" } })
  ));
  assert!(!wait_over(&json!({ "state": "leased" })));
  assert!(!wait_over(&json!({ "state": "open" })));
}

#[test]
fn a_held_item_whose_wait_ended_is_told_with_what_it_waited_on_and_no_instruction() {
  let waited = [
    json!({ "id": 2649, "title": "the first", "state": "done" }),
    json!({ "id": 2651, "title": "the second", "state": "done",
            "labels": { "close-phase": "live", "close-commit": "abc" } }),
  ];
  let told =
    agency_server::job_notices::wait_ended(2650, "after", "follows=2649,2651", &waited, &room(Some(-9100)));
  let order = told["said"]["order"].as_str().unwrap();

  assert_eq!(
    (&told["sort"], &told["kind"], &told["room"]),
    (&json!("job"), &json!("item-wait-ended:2650"), &json!(-9100))
  );
  assert_eq!(told["events"], json!(["item-wait-ended:#2650"]));

  for word in ["#2650", "after", "follows=2649,2651", "#2649", "the first", "#2651", "close-phase live"] {
    assert!(order.contains(word), "{word}: {order}");
  }

  assert_no_instruction(order);
}

#[tokio::test]
async fn a_held_item_is_told_once_its_wait_ends_and_let_go_once_it_is_no_longer_held() {
  use agency_server::watches::queue_idle::WAIT_TOLD_PREFIX;

  use crate::support::peers::{item, stand};

  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  {
    let mut held = peers.lock().unwrap();

    held.queues = vec![json!({ "name": "todo", "state": "open", "holds": [] })];
    // 100 is done; 101 is leased; 102 is done with its commit waiting to go live; 103 is done and live.
    held.items.insert(100, item(100, "done", json!({})));
    held.items.insert(101, item(101, "leased", json!({})));
    held.items.insert(
      102,
      item(
        102,
        "leased",
        json!({ "labels": { "close-phase": "waiting-live", "close-commit": "abc" } }),
      ),
    );
    held.items.insert(
      103,
      item(103, "done", json!({ "labels": { "close-phase": "live", "close-commit": "def" } })),
    );
    // The held items waiting on them, and one open item naming a done one.
    let follows =
      |on: &str, room: &str| json!({ "labels": { "held_kind": "pause", "follows": on }, "scope": room });

    held.items.insert(110, item(110, "held", follows("100", "-1100")));
    held
      .items
      .insert(111, item(111, "held", json!({ "labels": { "waiting-on": "101" }, "raisedBy": 110 })));
    held.items.insert(112, item(112, "held", follows("102", "-1120")));
    held.items.insert(113, item(113, "held", follows("100, #103", "-1130")));
    held.items.insert(114, item(114, "held", follows("100,101", "-1140")));
    held.items.insert(115, item(115, "open", follows("100", "-1150")));
    held.items.insert(116, item(116, "held", follows("999", "-1160")));
  }

  let ended = |peers: &crate::support::peers::Peers| -> Vec<(String, Value)> {
    kinds(peers)
      .into_iter()
      .filter(|(kind, _)| kind.starts_with("item-wait-ended:"))
      .collect()
  };

  pass(db, &vars).await.unwrap();
  pass(db, &vars).await.unwrap();

  assert_eq!(
    ended(&peers),
    vec![
      ("item-wait-ended:110".to_owned(), json!(-1100)),
      ("item-wait-ended:113".to_owned(), json!(-1130))
    ],
    "a held item following done items is told once; following a leased one, one waiting to go live, an item \
     the job service does not answer for, or not held, it is not"
  );

  // The leased item is done: every item that waited on it alone or with others is told, in its orderer's room.
  peers.lock().unwrap().items.get_mut(&101).unwrap()["state"] = json!("done");
  pass(db, &vars).await.unwrap();
  pass(db, &vars).await.unwrap();

  assert_eq!(
    ended(&peers)[2..],
    [
      ("item-wait-ended:111".to_owned(), json!(-1100)),
      ("item-wait-ended:114".to_owned(), json!(-1140))
    ]
  );

  // Lifted, an item's mark is let go; one still held keeps its mark and is not told again.
  peers.lock().unwrap().items.get_mut(&110).unwrap()["state"] = json!("open");
  pass(db, &vars).await.unwrap();

  let mark = |id: i64| async move {
    agency_server::queue::settings::get(db, &format!("{WAIT_TOLD_PREFIX}{id}"))
      .await
      .unwrap()
  };

  assert_eq!(mark(110).await, None, "no longer held, its mark let go");
  assert!(mark(113).await.is_some(), "still held, its mark kept");
  assert_eq!(ended(&peers).len(), 4, "told once while it stays held");
}

fn on_model(id: i64, model: &str) -> Open {
  Open {
    model: Some(model.to_owned()),
    role: Some("coder".to_owned()),
    ..open(id, Some("w1"))
  }
}

// Two coders on one model and one on another, those named carrying a run.
fn coders(open: Vec<Open>, carrying: &[&str]) -> Standing {
  Standing {
    agents: vec![coder("coder-a", "opus"), coder("coder-b", "opus"), coder("coder-c", "local")],
    carrying: carrying.iter().map(|name| (*name).to_owned()).collect(),
    ..standing(open, &["w1", "w2"])
  }
}

#[test]
fn an_item_naming_a_model_no_agent_of_its_role_runs_has_no_candidate() {
  use agency_server::role_agents::roads_held;
  use agency_server::watches::queue_idle::without_candidate;

  let named = Open {
    agent: Some("coder-a".to_owned()),
    ..on_model(3, "gpt")
  };
  let mut held = coders(vec![on_model(1, "gpt"), on_model(2, "opus"), named, on_model(4, "local")], &[]);

  held.agents[2].enabled = false;

  assert_eq!(
    without_candidate(&held),
    vec![&on_model(1, "gpt"), &on_model(4, "local")],
    "none, or switched off"
  );
  assert!(
    idle_agents(&held).iter().all(|idle| !idle.items.contains(&&on_model(1, "gpt"))),
    "no agent could take it"
  );

  let roads = roads_held("coder", &held.agents);

  assert!(roads.contains("- coder-a: claude-code/opus\n"), "{roads}");
  assert!(roads.contains("- coder-c: claude-code/local, switched off"), "{roads}");
}

#[tokio::test]
async fn an_item_whose_model_no_agent_runs_is_told_once_and_let_go_once_an_agent_runs_it() {
  use agency_server::watches::queue_idle::NO_CANDIDATE_TOLD_PREFIX;

  use crate::support::peers::{item, stand};

  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;

  crate::support::rows::seats(db, "worker", &[("deepseek", "deepseek-flash")]).await;
  peers.lock().unwrap().queues = vec![json!({ "name": "todo", "state": "open", "holds": [] })];
  peers.lock().unwrap().items.insert(
    96,
    item(
      96,
      "open",
      json!({ "scope": "-9600", "labels": { "model": "rare-model", "role": "worker" } }),
    ),
  );

  pass(db, &vars).await.unwrap();
  pass(db, &vars).await.unwrap();

  let told: Vec<(String, Value)> = kinds(&peers)
    .into_iter()
    .filter(|(kind, _)| kind.starts_with("item-no-candidate"))
    .collect();

  assert_eq!(told, vec![("item-no-candidate:96".to_owned(), json!(-9600))], "told once, in its room");

  let order = peers
    .lock()
    .unwrap()
    .instructed()
    .iter()
    .find(|body| body["kind"] == "item-no-candidate:96")
    .unwrap()["said"]["order"]
    .as_str()
    .unwrap_or_default()
    .to_owned();

  assert!(
    order.contains("model=rare-model") && order.contains("the worker role:\n- worker-01"),
    "{order}"
  );
  assert_no_instruction(&order);

  // An agent of the role moved onto the model: the mark is let go.
  scratch
    .run(r#"UPDATE "role_agents" SET "provider" = 'claude-code', "model" = 'rare-model' WHERE "name" = 'worker-01'"#, vec![])
    .await;
  pass(db, &vars).await.unwrap();
  assert!(
    agency_server::queue::settings::get(db, &format!("{NO_CANDIDATE_TOLD_PREFIX}96"))
      .await
      .unwrap()
      .is_none()
  );
}

// An item's notices switched off, as the job service answers the switch.
fn switched_off() -> Value {
  json!({ "reason": "the owner sees to it", "by": "owner", "at": 1 })
}

// The agent-idle instructions the stand-in was handed, in order.
fn idle_told(peers: &crate::support::peers::Peers) -> Vec<Value> {
  peers
    .lock()
    .unwrap()
    .instructed()
    .into_iter()
    .filter(|body| body["kind"].as_str().is_some_and(|kind| kind.starts_with(IDLE_OWED)))
    .collect()
}

#[tokio::test]
async fn an_idle_agent_is_told_to_each_room_after_its_launch_seconds_again_for_a_new_item_and_withdrawn_once_busy()
 {
  use crate::support::peers::{item, stand};

  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;
  let role = job_role();
  let agent = format!("{role}-01");

  repository(db, "worker", "/checkouts/worker", &[]).await;
  seats(db, role, &[("claude-code", "opus")]).await;

  {
    let mut held = peers.lock().unwrap();

    held.queues = vec![json!({ "name": "todo", "state": "open", "holds": [] })];
    held.items.insert(91, item(91, "open", json!({ "scope": "-9100" })));
    held.items.insert(92, item(92, "open", json!({ "scope": "-9200" })));
    held
      .items
      .insert(93, item(93, "open", json!({ "scope": "-9100", "noticesOff": switched_off() })));
    held.items.insert(94, item(94, "open", json!({ "scope": "-9100", "noticesOff": null })));
  }

  pass(db, &vars).await.unwrap();
  assert!(idle_told(&peers).is_empty(), "first seen idle, nothing owed yet");

  idle_since(db, &agent).await;
  pass(db, &vars).await.unwrap();

  let told = idle_told(&peers);
  let by_room: BTreeMap<String, String> = told
    .iter()
    .map(|body| (body["room"].to_string(), body["said"]["order"].as_str().unwrap().to_owned()))
    .collect();

  assert_eq!(told.len(), 2, "one instruction a room: {told:?}");
  assert!(
    told
      .iter()
      .all(|body| body["kind"] == format!("{IDLE_OWED}{agent}@{}", body["room"]) && body["sort"] == "job"),
    "{told:?}"
  );
  assert_eq!(by_room.keys().collect::<Vec<_>>(), ["-9100", "-9200"]);

  let first = &by_room["-9100"];

  for word in [agent.as_str(), role, "claude-code/opus", "#91 (item 91)", "#94 (item 94)"] {
    assert!(first.contains(word), "{word} in {first}");
  }

  assert!(!first.contains("#93"), "an item whose notices are switched off is left out: {first}");
  assert!(by_room["-9200"].contains("#92") && !by_room["-9200"].contains("#91"));

  pass(db, &vars).await.unwrap();
  assert_eq!(idle_told(&peers).len(), 2, "nothing changed, nothing told");

  // A new item it could take in a room owes that room's notice again.
  peers.lock().unwrap().items.insert(95, item(95, "open", json!({ "scope": "-9100" })));
  pass(db, &vars).await.unwrap();

  let told = idle_told(&peers);

  assert_eq!(told.len(), 3, "{told:?}");
  assert_eq!(told[2]["room"], json!(-9100));
  assert!(told[2]["said"]["order"].as_str().unwrap().contains("#95"), "{told:?}");

  // It carries a run: the notices kept are withdrawn and its mark let go; busy, it is told nothing.
  let kept: Vec<String> = agency_server::outbox::owed(db, IDLE_OWED).await.unwrap().into_keys().collect();

  assert_eq!(kept, [format!("{agent}@-9100"), format!("{agent}@-9200")]);
  scratch
    .run(
      r#"INSERT INTO "jobs" ("item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "started_at", "prompt", "item_agent")
         VALUES (91, 1, 'runner', 1, 'worker', 'running', 1000, 'the work', $1)"#,
      vec![agent.clone().into()],
    )
    .await;
  pass(db, &vars).await.unwrap();

  let withdrawn: BTreeSet<String> = peers.lock().unwrap().withdrawn().into_iter().collect();

  assert!(
    withdrawn.is_superset(&BTreeSet::from(["2".to_owned(), "3".to_owned()])),
    "{withdrawn:?}"
  );
  assert!(agency_server::outbox::owed(db, IDLE_OWED).await.unwrap().is_empty());
  assert_eq!(settings::get(db, &format!("{IDLE_SEEN_PREFIX}{agent}")).await.unwrap(), None);

  pass(db, &vars).await.unwrap();
  assert_eq!(idle_told(&peers).len(), 3, "a working agent is told nothing");
  assert_eq!(settings::get(db, &format!("{IDLE_SEEN_PREFIX}{agent}")).await.unwrap(), None);
}

#[tokio::test]
async fn an_item_whose_notices_are_switched_off_is_owed_no_notice_of_its_standing() {
  use crate::support::peers::{item, stand};

  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;
  let role = job_role();

  repository(db, "worker", "/checkouts/worker", &[]).await;
  seats(db, role, &[("claude-code", "opus")]).await;

  {
    let mut held = peers.lock().unwrap();
    let off = |extra: Value, switch: Value| {
      let mut extra = extra;

      extra["noticesOff"] = switch;
      extra
    };

    held.queues = vec![
      json!({ "name": "todo", "state": "open", "holds": [] }),
      json!({ "name": "later", "state": "paused", "holds": [] }),
    ];
    // Each pair alike but for the switch: off first, then null.
    for (id, state, extra) in [
      (120, "open", json!({ "queueName": "later" })),
      (122, "open", json!({ "labels": { "model": "rare-model" } })),
      (124, "draft", json!({})),
      (126, "held", json!({ "labels": { "follows": "100" } })),
      (128, "open", json!({ "labels": { "agent": "nobody-07" } })),
      (130, "open", json!({ "labels": { "role": "ghost" } })),
    ] {
      held.items.insert(id, item(id, state, off(extra.clone(), switched_off())));
      held.items.insert(id + 1, item(id + 1, state, off(extra, Value::Null)));
    }

    held.items.insert(100, item(100, "done", json!({})));
  }

  pass(db, &vars).await.unwrap();

  let told: BTreeSet<String> = kinds(&peers).into_iter().map(|(kind, _)| kind).collect();

  assert_eq!(
    told,
    BTreeSet::from(
      [
        "item-queue-paused:121",
        "item-no-candidate:123",
        "item-awaits-approval:125",
        "item-wait-ended:127",
        "item-agent-refused:129",
        "item-role-unknown:131",
      ]
      .map(str::to_owned)
    ),
    "only the items whose notices are on"
  );
}

#[tokio::test]
async fn an_item_naming_an_agent_or_a_role_no_launch_takes_is_told_once_and_at_once_as_an_agent_changes() {
  use agency_server::watches::queue_idle::agents_changed;

  use crate::support::peers::{item, stand};

  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;
  let role = job_role();
  let names = |at: usize| format!("{role}-{at:02}");

  repository(db, "worker", "/checkouts/worker", &[]).await;
  scratch
    .run(
      r#"INSERT INTO "roles" ("name", "label", "max_concurrent", "prompt", "position", "created_at", "updated_at")
         VALUES ($1, 'Worker', 1, 'Work.', 0, 0, 0)"#,
      vec![role.into()],
    )
    .await;
  seats(db, role, &[("claude-code", "opus"), ("claude-code", "opus")]).await;
  scratch
    .run(r#"UPDATE "role_agents" SET "enabled" = 0 WHERE "name" = $1"#, vec![names(2).into()])
    .await;
  scratch
    .run(
      r#"INSERT INTO "role_agents" ("name", "role") VALUES ($1, $2)"#,
      vec![names(3).into(), role.into()],
    )
    .await;

  {
    let mut held = peers.lock().unwrap();
    let named = |label: &str, value: &str, room: &str| json!({ "labels": { label: value }, "scope": room });

    held.queues = vec![json!({ "name": "todo", "state": "open", "holds": [] })];
    held.items.insert(140, item(140, "open", named("agent", &names(2), "-1400")));
    held.items.insert(141, item(141, "open", named("agent", "nobody-07", "-1410")));
    held.items.insert(142, item(142, "open", named("agent", &names(3), "-1420")));
    held.items.insert(143, item(143, "open", named("role", "ghost", "-1430")));
    held.items.insert(144, item(144, "open", named("agent", &names(1), "-1440")));
  }

  pass(db, &vars).await.unwrap();
  pass(db, &vars).await.unwrap();

  let refused = |peers: &crate::support::peers::Peers| -> Vec<(String, Value)> {
    kinds(peers)
      .into_iter()
      .filter(|(kind, _)| kind.starts_with("item-agent-refused:") || kind.starts_with("item-role-unknown:"))
      .collect()
  };

  assert_eq!(
    refused(&peers),
    [
      ("item-agent-refused:140".to_owned(), json!(-1400)),
      ("item-agent-refused:141".to_owned(), json!(-1410)),
      ("item-agent-refused:142".to_owned(), json!(-1420)),
      ("item-role-unknown:143".to_owned(), json!(-1430)),
    ],
    "each once, in its orderer's room"
  );

  let roles_held = format!("The roles it holds: {role}.");

  for (kind, why) in [
    ("item-agent-refused:140", "switched off"),
    ("item-agent-refused:141", "registers no agent nobody-07"),
    ("item-agent-refused:142", "follows its supervisor's road"),
    ("item-role-unknown:143", roles_held.as_str()),
  ] {
    let order = order_of(&peers, kind);

    assert!(order.contains(why), "{why} in {order}");
  }

  // Another role label is told again.
  peers.lock().unwrap().items.get_mut(&143).unwrap()["labels"]["role"] = json!("phantom");
  pass(db, &vars).await.unwrap();
  assert_eq!(refused(&peers).len(), 5);
  assert!(order_of(&peers, "item-role-unknown:143").contains("role=phantom"));

  // The agent item 144 names is switched off: told as the change is made, with no pass.
  scratch
    .run(r#"UPDATE "role_agents" SET "enabled" = 0 WHERE "name" = $1"#, vec![names(1).into()])
    .await;
  agents_changed(db, &vars).await;

  assert_eq!(refused(&peers)[5], ("item-agent-refused:144".to_owned(), json!(-1440)));
}
