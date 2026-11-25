//! The services agency calls, asked through the clients their repositories publish: over HTTP of a
//! stand-in of the test's own, which writes down each call and answers by its path, and by the defaults
//! where the deployment names no address; and the supervisor each of agency's reference calls is read for.

use std::sync::{Arc, Mutex};

use agency_server::config::{Config, PeerAddresses};
use agency_server::peers::brain::{BrainReset, Made};
use agency_server::peers::gate::Gate;
use agency_server::peers::node::NodeKind;
use agency_server::peers::{Asking, PeerFault, Peers};
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::net::TcpListener;

/// One call as the stand-in took it: its method, its path and query, its authorization, and its body.
#[derive(Clone, Debug, PartialEq)]
struct Call {
  method: String,
  path: String,
  query: Option<String>,
  authorization: Option<String>,
  body: Value,
}

type Calls = Arc<Mutex<Vec<Call>>>;

fn node(name: &str, label: &str, kind: &str, role: Option<&str>, state: &str, up: bool) -> Value {
  json!({
    "name": name,
    "label": label,
    "kind": kind,
    "role": role,
    "ownership": "shared",
    "account": null,
    "monitored": true,
    "callers": [],
    "address": format!("worker@{name}.lan"),
    "state": state,
    "tone": if up { "good" } else { "bad" },
    "checkedAt": 1_700_000_000_000_i64,
    "up": up,
    "thisMachine": false
  })
}

fn machine() -> Value {
  json!({
    "name": "local",
    "label": "This machine",
    "kind": "local",
    "tracking": true,
    "readAt": 1_700_000_000_000_i64,
    "standing": {
      "reading": {
        "at": 1_700_000_000_000_i64,
        "load1": 3.0,
        "threads": 24,
        "memTotalGb": 32.0,
        "memAvailableGb": 8.0,
        "swapTotalGb": 16.0,
        "swapFreeGb": 8.0,
        "diskFreeGb": 100.0,
        "memory": null,
        "cpu": null
      },
      "alerts": [],
      "browsers": 2,
      "scopes": [{ "scope": "worker", "megabytes": 900, "browsers": 2 }],
      "largest": []
    }
  })
}

const CHANGES: &str = "event: machine.changed\ndata: not-json\n\n\
event: machine.changed\ndata: {\"id\":2,\"kind\":\"machine.changed\",\"at\":1700000000000,\"data\":{\"name\":\"local\",\"label\":\"This machine\",\"kind\":\"local\",\"tracking\":true,\"standing\":{\"reading\":null,\"alerts\":[{\"cause\":\"memory\",\"fullPct\":12.5,\"alertPct\":10,\"minutes\":2}],\"browsers\":2,\"scopes\":[],\"largest\":[]}}}\n\n";

fn brain(agent: &str, id: i64) -> Value {
  json!({ "id": id, "accountKey": "1", "agentKey": agent, "name": agent, "createdAt": 1 })
}

fn brains(members: &[(&str, i64)]) -> Value {
  json!({ "items": members.iter().map(|(agent, id)| brain(agent, *id)).collect::<Vec<_>>(), "more": false })
}

async fn answer(
  State(calls): State<Calls>,
  method: Method,
  uri: Uri,
  headers: HeaderMap,
  body: axum::body::Bytes,
) -> Response {
  let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
  let path = uri.path().to_owned();
  let query = uri.query().map(str::to_owned);
  let authorization = headers.get("authorization").map(|value| value.to_str().unwrap().to_owned());

  calls.lock().unwrap().push(Call {
    method: method.to_string(),
    path: path.clone(),
    query: query.clone(),
    authorization,
    body: body.clone(),
  });

  let answer = match (method.as_str(), path.strip_prefix("/base/").unwrap_or(&path)) {
    ("GET", "nodes") => json!({
      "items": [
        node("near", "Near", "ssh", Some("jobs"), "connected", true),
        node("far", "Far", "hub", None, "unreachable", false)
      ],
      "more": false
    }),
    ("GET", "nodes/near") => node("near", "Near", "ssh", Some("jobs"), "connected", true),
    ("GET", "nodes/far") => node("far", "Far", "hub", None, "unreachable", false),
    ("GET", "nodes/near/target") => json!({
      "nodeName": "near",
      "kind": "ssh",
      "role": "jobs",
      "host": "near.example",
      "port": 2222,
      "user": "worker",
      "keyPath": "/keys/near",
      "shell": "posix",
      "sshOptions": ["-o", "BatchMode=yes"],
      "sshArgs": ["-o", "BatchMode=yes", "worker@near.example"]
    }),
    ("GET", "nodes/far/target") => json!({
      "nodeName": "far",
      "kind": "hub",
      "sshOptions": [],
      "sshArgs": []
    }),
    ("GET", "nodes/gone/target") => {
      return (
        StatusCode::NOT_FOUND,
        Json(json!({ "code": "not-found", "error": "No node of that name." })),
      )
        .into_response();
    }
    ("GET", "nodes/odd/target") => {
      return (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "code": "failed", "error": "The check failed." })),
      )
        .into_response();
    }
    ("GET", "machines/local") => machine(),
    ("GET", "events") if query.as_deref().is_some_and(|query| query.contains("kinds=machine.changed")) => {
      return ([("content-type", "text/event-stream")], CHANGES).into_response();
    }
    ("GET", "brains") if query.as_deref() == Some("kind=agent&agentKey=kept&limit=1") => {
      brains(&[("kept", 5)])
    }
    ("GET", "brains") if query.as_deref() == Some("kind=agent&agentKey=gone&limit=1") => brains(&[]),
    ("GET", "brains") if query.as_deref() == Some("kind=agent&agentKey=busy&limit=1") => {
      brains(&[("busy", 6)])
    }
    ("GET", "brains") if query.as_deref() == Some("kind=agent&agentKey=fresh&limit=1") => brains(&[]),
    ("POST", "brains") => json!({
      "id": 7,
      "accountKey": body["accountKey"],
      "agentKey": body["agentKey"],
      "name": body["name"],
      "createdAt": 1
    }),
    ("POST", "translations") => json!({
      "readings": body["originals"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|original| json!({ "text": format!("read: {}", original.as_str().unwrap_or_default()), "whole": true }))
        .collect::<Vec<_>>()
    }),
    ("POST", "reference/selected") => json!({
      "entries": [{ "path": "knowledge/index.md", "text": "a known fact", "source": "knowledge" }]
    }),
    ("DELETE", "brains/5") => brain("kept", 5),
    ("DELETE", "brains/6") => {
      return (
        StatusCode::CONFLICT,
        Json(json!({ "code": "conflict", "error": "The brain is being written." })),
      )
        .into_response();
    }
    _ => return (StatusCode::NOT_FOUND, "nothing here").into_response(),
  };

  Json(answer).into_response()
}

/// The stand-in served on a free port; its address and the calls it took.
async fn stand_in() -> (String, Calls) {
  let calls = Calls::default();
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}/base/", listener.local_addr().unwrap());
  let routes = axum::Router::new().fallback(answer).with_state(calls.clone());

  tokio::spawn(async move { axum::serve(listener, routes).await.unwrap() });

  (address, calls)
}

fn peers_at(address: &str, token: Option<&str>) -> Peers {
  let config = Config::read(|name| match name {
    "AGENCY_NODE_URL" | "AGENCY_BRAIN_URL" | "BMO_GATE_URL" => Some(address.to_owned()),
    "AGENCY_NODE_TOKEN" | "AGENCY_BRAIN_TOKEN" => token.map(str::to_owned),
    _ => None,
  })
  .unwrap();

  Peers::of(&config.peers)
}

// The brain at the stand-in, a brain it makes filed under the account named.
fn brain_at(address: &str, account: Option<&str>) -> Peers {
  let config = Config::read(|name| match name {
    "AGENCY_BRAIN_URL" => Some(address.to_owned()),
    "AGENCY_BRAIN_ACCOUNT" => account.map(str::to_owned),
    _ => None,
  })
  .unwrap();

  Peers::of(&config.peers)
}

fn taken(calls: &Calls) -> Vec<Call> {
  std::mem::take(&mut *calls.lock().unwrap())
}

#[tokio::test]
async fn the_node_service_is_asked_for_its_links_nodes_and_logins() {
  let (address, calls) = stand_in().await;
  let peers = peers_at(&address, Some("node-token"));

  assert!(peers.node.link_up("near").await.unwrap());
  assert!(!peers.node.link_up("far").await.unwrap());

  let rows = peers.node.node_rows().await.unwrap();

  assert_eq!(rows.len(), 2);
  assert_eq!(rows[0].node, "near");
  assert_eq!(rows[0].label.as_deref(), Some("Near"));
  assert_eq!(rows[0].kind, NodeKind::Ssh);
  assert_eq!(rows[0].state, "connected");
  assert_eq!(rows[1].node, "far");
  assert_eq!(rows[1].kind, NodeKind::Jake);

  assert_eq!(
    peers.node.ssh_login("near").await.unwrap(),
    Some(vec!["-o".to_owned(), "BatchMode=yes".to_owned(), "worker@near.example".to_owned()])
  );
  assert_eq!(peers.node.ssh_login("far").await.unwrap(), None);
  assert_eq!(
    peers.node.ssh_login("gone").await.unwrap(),
    None,
    "a node that is not listed has no route"
  );

  let calls = taken(&calls);
  let paths: Vec<&str> = calls.iter().map(|call| call.path.as_str()).collect();

  assert_eq!(
    paths,
    [
      "/base/nodes/near",
      "/base/nodes/far",
      "/base/nodes",
      "/base/nodes/near/target",
      "/base/nodes/far/target",
      "/base/nodes/gone/target"
    ]
  );
  assert!(calls.iter().all(|call| call.authorization.as_deref() == Some("Bearer node-token")));
}

#[tokio::test]
async fn the_node_service_is_asked_for_its_standing_and_heard_telling_changes() {
  let (address, calls) = stand_in().await;
  let peers = peers_at(&address, Some("node-token"));

  let standing = peers.node.machine_standing().await.unwrap();

  assert_eq!(standing.browsers, 2);
  assert_eq!(standing.scopes.len(), 1);
  assert_eq!(standing.scopes[0].scope, "worker");
  assert_eq!(standing.scopes[0].megabytes, 900);

  let told: Vec<_> = peers.node.pressures().await.unwrap().collect().await;

  assert_eq!(told.len(), 2, "the event of another shape is not passed over: {told:?}");
  assert!(matches!(told[0], Err(PeerFault::Malformed { .. })), "{:?}", told[0]);

  let machine = told[1].as_ref().unwrap();
  let alerts = &machine.standing.as_ref().unwrap().alerts;

  assert_eq!(alerts.len(), 1);
  assert!(matches!(alerts[0], agency_server::peers::node::Alert::Memory { full_pct: 12.5, .. }));

  let calls = taken(&calls);

  assert_eq!(
    calls.iter().map(|call| call.path.as_str()).collect::<Vec<_>>(),
    ["/base/machines/local", "/base/events"]
  );
  assert_eq!(
    calls[1].query.as_deref(),
    Some("kinds=machine.changed&machineName=local"),
    "only this machine's changes are read"
  );
  assert!(calls.iter().all(|call| call.authorization.as_deref() == Some("Bearer node-token")));
}

#[tokio::test]
async fn the_brain_is_asked_by_the_agent_and_its_refusals_come_back_as_faults() {
  let (address, calls) = stand_in().await;
  let peers = peers_at(&address, Some("brain-token"));

  let preview = peers.brain.reset("kept", true).await.unwrap();

  assert_eq!(preview.took["brains"], json!(1));
  assert_eq!(preview.could_not, Vec::<String>::new());

  let took = peers.brain.reset("kept", false).await.unwrap();

  assert_eq!(took.took["brains"], json!(1));

  peers.brain.forget("kept").await.unwrap();
  peers.brain.forget("gone").await.unwrap();
  assert_eq!(peers.brain.reset("gone", false).await.unwrap(), BrainReset::default());

  let refused = peers.brain.forget("busy").await.unwrap_err();

  assert_eq!(
    refused,
    PeerFault::Failed {
      peer: "brain",
      status: 409,
      code: "conflict".into(),
      error: "The brain is being written.".into(),
    }
  );

  let calls = taken(&calls);

  assert_eq!(
    calls
      .iter()
      .map(|call| (call.method.as_str(), call.path.as_str(), call.query.as_deref()))
      .collect::<Vec<_>>(),
    [
      ("GET", "/base/brains", Some("kind=agent&agentKey=kept&limit=1")),
      ("GET", "/base/brains", Some("kind=agent&agentKey=kept&limit=1")),
      ("DELETE", "/base/brains/5", None),
      ("GET", "/base/brains", Some("kind=agent&agentKey=kept&limit=1")),
      ("DELETE", "/base/brains/5", None),
      ("GET", "/base/brains", Some("kind=agent&agentKey=gone&limit=1")),
      ("GET", "/base/brains", Some("kind=agent&agentKey=gone&limit=1")),
      ("GET", "/base/brains", Some("kind=agent&agentKey=busy&limit=1")),
      ("DELETE", "/base/brains/6", None)
    ]
  );
  assert!(calls.iter().all(|call| call.authorization.as_deref() == Some("Bearer brain-token")));
}

#[tokio::test]
async fn a_brain_is_made_under_the_account_named_where_the_agent_keeps_none() {
  let (address, calls) = stand_in().await;
  let peers = brain_at(&address, Some("1"));

  assert_eq!(peers.brain.make("kept", "Kept", None).await.unwrap(), Made::Kept);
  assert_eq!(peers.brain.make("fresh", "Fresh one", None).await.unwrap(), Made::Made);

  let made = taken(&calls);

  assert_eq!(
    made
      .iter()
      .map(|call| (call.method.as_str(), call.path.as_str(), call.query.as_deref()))
      .collect::<Vec<_>>(),
    [
      ("GET", "/base/brains", Some("kind=agent&agentKey=kept&limit=1")),
      ("GET", "/base/brains", Some("kind=agent&agentKey=fresh&limit=1")),
      ("POST", "/base/brains", None)
    ]
  );
  assert_eq!(
    made[2].body,
    json!({ "accountKey": "1", "agentKey": "fresh", "kind": "agent", "name": "Fresh one" })
  );

  let refused = brain_at(&address, Some("  ")).brain.make("fresh", "Fresh one", None).await.unwrap_err();

  assert!(
    matches!(&refused, PeerFault::Refused { peer: "brain", code, .. } if code == "no-account"),
    "{refused:?}"
  );
  assert_eq!(
    taken(&calls).iter().map(|call| call.method.as_str()).collect::<Vec<_>>(),
    ["GET"],
    "with no account named the brain is only looked for"
  );
}

#[tokio::test]
async fn a_brain_is_filed_under_its_owner_s_account_where_the_agent_names_one() {
  let (address, calls) = stand_in().await;

  for account in [Some("1"), None] {
    let made = brain_at(&address, account).brain.make("fresh", "Fresh one", Some(42)).await.unwrap();

    assert_eq!(made, Made::Made);
    assert_eq!(
      taken(&calls).last().map(|call| call.body.clone()),
      Some(json!({ "accountKey": "42", "agentKey": "fresh", "kind": "agent", "name": "Fresh one" }))
    );
  }
}

#[tokio::test]
async fn a_service_with_no_listener_is_unreachable() {
  let listener = TcpListener::bind("[::1]:0").await.unwrap();
  let address = format!("http://{}/", listener.local_addr().unwrap());

  drop(listener);

  let fault = peers_at(&address, None).node.machine_standing().await.unwrap_err();

  assert!(
    matches!(
      fault,
      PeerFault::Unreachable {
        peer: "node service",
        ..
      }
    ),
    "{fault:?}"
  );

  let fault = peers_at(&address, None).brain.forget("kept").await.unwrap_err();

  assert!(matches!(fault, PeerFault::Unreachable { peer: "brain", .. }), "{fault:?}");

  let fault = brain_at(&address, Some("1")).brain.make("kept", "Kept", None).await.unwrap_err();

  assert!(matches!(fault, PeerFault::Unreachable { peer: "brain", .. }), "{fault:?}");
}

#[tokio::test]
async fn the_node_service_s_reads_are_asked_and_a_node_with_no_reach_is_none() {
  let (address, calls) = stand_in().await;
  let peers = peers_at(&address, None);

  let reads = peers.node.node_reads().await.unwrap();

  assert_eq!(reads.len(), 2);
  assert_eq!(reads[0].name, "near");
  assert_eq!(reads[0].kind, NodeKind::Ssh);
  assert_eq!(reads[0].role.as_deref(), Some("jobs"));
  assert_eq!(reads[0].checked_at, Some(1_700_000_000_000));
  assert_eq!(reads[1].name, "far");
  assert_eq!(reads[1].kind, NodeKind::Jake);

  let near = peers.node.ssh_reach("near").await.unwrap().unwrap();

  assert_eq!(near.host, "near.example");
  assert_eq!(near.port, 2222);
  assert_eq!(near.user, "worker");
  assert_eq!(near.key_path.as_deref(), Some("/keys/near"));
  assert_eq!(near.shell.as_deref(), Some("posix"));
  assert_eq!(peers.node.ssh_reach("far").await.unwrap(), None, "reached through a hub");
  assert_eq!(peers.node.ssh_reach("gone").await.unwrap(), None, "not listed");
  assert!(matches!(peers.node.ssh_reach("odd").await, Err(PeerFault::Failed { status: 500, .. })));

  let paths: Vec<String> = taken(&calls).into_iter().map(|call| call.path).collect();

  assert_eq!(
    paths,
    [
      "/base/nodes",
      "/base/nodes/near/target",
      "/base/nodes/far/target",
      "/base/nodes/gone/target",
      "/base/nodes/odd/target"
    ]
  );
}

#[tokio::test]
async fn a_service_with_no_address_answers_its_default() {
  let peers = Peers::of(&PeerAddresses::default());

  assert!(!peers.node.link_up("near").await.unwrap());
  assert!(peers.node.node_rows().await.unwrap().is_empty());
  assert_eq!(peers.node.ssh_login("near").await.unwrap(), None);
  assert_eq!(peers.node.machine_standing().await.unwrap().browsers, 0);
  assert!(peers.node.pressures().await.is_ok(), "a stream that tells none is opened");
  peers.brain.forget("kept").await.unwrap();
  assert_eq!(peers.brain.reset("kept", false).await.unwrap(), BrainReset::default());
  assert_eq!(peers.brain.make("kept", "Kept", None).await.unwrap(), Made::Undeployed);
  assert!(
    peers
      .gate
      .reference("bmo", vec!["the instruction".to_owned()])
      .await
      .unwrap()
      .is_empty()
  );
}

#[test]
fn a_picked_reference_stands_under_a_heading_named_by_path_and_source() {
  use agency_server::peers::gate::attach;

  assert_eq!(attach("the prompt", &[]), "the prompt");
  assert_eq!(
    attach(
      "the prompt",
      &[bmo_gate_client::Entry {
        path: "knowledge/index.md".to_owned(),
        text: "a known fact".to_owned(),
        source: "knowledge".to_owned(),
      }]
    ),
    "the prompt\n\n## Reference\n\n`knowledge/index.md` (knowledge)\n\na known fact\n\n"
  );
}

#[tokio::test]
async fn the_gate_is_asked_for_a_roomless_reference_read_for_the_supervisor_named() {
  let (address, calls) = stand_in().await;
  let peers = peers_at(&address, None);
  let entries = peers.gate.reference("u7", vec!["the instruction".to_owned()]).await.unwrap();

  assert_eq!(entries.len(), 1);
  assert_eq!(entries[0].path, "knowledge/index.md");

  let asked = taken(&calls).pop().expect("the gate was asked");

  assert_eq!((asked.method.as_str(), asked.path.as_str()), ("POST", "/base/reference/selected"));
  assert_eq!(asked.body["agent"], json!("u7"), "{}", asked.body);
  assert_eq!(asked.body["roomId"], json!(0));
  assert_eq!(asked.body["lines"], json!(["the instruction"]));
  assert!(asked.body.get("situation").is_none(), "{}", asked.body);
}

#[tokio::test]
async fn the_gate_is_asked_to_read_originals_into_a_language_and_answers_each_reading_in_order() {
  let (address, calls) = stand_in().await;
  let peers = peers_at(&address, None);
  let readings = peers.gate.translate("en", vec!["first".to_owned(), "second".to_owned()]).await.unwrap();

  assert_eq!(
    readings
      .iter()
      .map(|reading| (reading.text.as_str(), reading.whole))
      .collect::<Vec<_>>(),
    [("read: first", true), ("read: second", true)]
  );

  let asked = taken(&calls).pop().expect("the gate was asked");

  assert_eq!((asked.method.as_str(), asked.path.as_str()), ("POST", "/base/translations"));
  assert_eq!(asked.body, json!({ "language": "en", "originals": ["first", "second"] }));
}

/// A gate that writes down the supervisor each reference is asked for, and picks nothing.
#[derive(Default)]
struct Recording(std::sync::Mutex<Vec<String>>);

impl Gate for Recording {
  fn reference<'a>(
    &'a self,
    supervisor: &'a str,
    _lines: Vec<String>,
  ) -> Asking<'a, Vec<bmo_gate_client::Entry>> {
    self.0.lock().unwrap().push(supervisor.to_owned());
    Box::pin(async { Ok(Vec::new()) })
  }
}

impl Recording {
  fn asked(&self) -> Vec<String> {
    std::mem::take(&mut *self.0.lock().unwrap())
  }
}

// A job's reference is read for its holder, the supervisor that launched it: an account's own `u<user>`, or
// the owner's supervisor for the owner's jobs; a job naming no holder asks nothing.
#[tokio::test]
async fn a_job_s_reference_is_read_for_the_supervisor_holding_it() {
  use agency_server::control::work::referenced;

  let gate = Recording::default();
  let instruction = json!({ "repository": "agency", "sentences": ["Name the supervisor."] });

  for holder in ["u7", "bmo"] {
    let job = json!({ "holderName": holder, "instruction": instruction });

    assert_eq!(referenced(&gate, &job, "the prompt").await, "the prompt");
    assert_eq!(gate.asked(), [holder]);
  }

  referenced(&gate, &json!({ "holderName": "", "instruction": instruction }), "the prompt").await;
  assert!(gate.asked().is_empty(), "no holder, no supervisor to read for");
}

/// A gate whose every reading comes back as the holders' sheet, writing down the lines each was asked over.
#[derive(Default)]
struct Sheet(std::sync::Mutex<Vec<Vec<String>>>);

impl Gate for Sheet {
  fn reference<'a>(
    &'a self,
    _supervisor: &'a str,
    lines: Vec<String>,
  ) -> Asking<'a, Vec<bmo_gate_client::Entry>> {
    self.0.lock().unwrap().push(lines);

    Box::pin(async move {
      Ok(vec![bmo_gate_client::Entry {
        path: "holders/sheet.md".to_owned(),
        text: "- [unit#3] the unit stands for order".to_owned(),
        source: "memory".to_owned(),
      }])
    })
  }
}

impl Sheet {
  fn asked(&self) -> Vec<Vec<String>> {
    std::mem::take(&mut *self.0.lock().unwrap())
  }
}

// The sheet a holder left is one of the entries the reading attaches with no pick, so a job takes it onto
// its first prompt even with no instruction to say lines: what attaches goes to a lineless turn too.
#[tokio::test]
async fn a_job_takes_the_holders_sheet_even_with_no_instruction() {
  use agency_server::control::work::referenced;

  let gate = Sheet::default();
  let prompt = referenced(&gate, &json!({ "holderName": "bmo" }), "the prompt").await;

  assert!(prompt.starts_with("the prompt\n\n## Reference\n"), "{prompt}");
  assert!(prompt.contains("`holders/sheet.md` (memory)"), "{prompt}");
  assert!(prompt.contains("- [unit#3] the unit stands for order"), "{prompt}");
  let asked = gate.asked();

  assert_eq!(asked, [Vec::<String>::new()], "asked with no line, and asked at all");
}

// The job's reading follows the two switches a chat round follows: off either, the launch asks the gate
// nothing; a holder the store holds no settings for stands reading, and a job naming none asks nothing.
#[tokio::test]
async fn the_job_reading_follows_the_holders_reference_and_brain_switches() {
  use crate::support::{grounds::ground_on, scratch::Scratch};
  use agency_server::control::work::reading_reaches;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let job = json!({ "holderName": "u9" });

  assert!(reading_reaches(&ground, &job).await, "settings the store holds none of stand reading");

  for (reference, brain) in [(0, 1), (1, 0)] {
    scratch
      .run(
        r#"INSERT INTO supervisor_settings (agent, reference_on, brain_on) VALUES ($1, $2, $3)
           ON CONFLICT (agent) DO UPDATE SET reference_on = $2, brain_on = $3"#,
        vec!["u9".into(), reference.into(), brain.into()],
      )
      .await;

    assert!(
      !reading_reaches(&ground, &job).await,
      "reference {reference}, brain {brain}: the reading is closed"
    );
  }

  scratch
    .run(
      "UPDATE supervisor_settings SET reference_on = 1, brain_on = 1 WHERE agent = $1",
      vec!["u9".into()],
    )
    .await;

  assert!(reading_reaches(&ground, &job).await, "both switches on open the reading again");
  assert!(
    !reading_reaches(&ground, &json!({ "holderName": "" })).await,
    "no holder, no reading asked"
  );
}
