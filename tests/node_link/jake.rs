//! A link to a person's PC (docs/person-nodes.md, 「The link over jake」): the hub let go of the listens it keeps for
//! the agent, and the folders and the bridge the far end serves.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Query, State};
use serde_json::{Value, json};

type Calls = Arc<Mutex<Vec<String>>>;

// The scope a credential names, which stands ahead of its mac.
fn scope(headers: &axum::http::HeaderMap) -> String {
  headers
    .get("authorization")
    .and_then(|value| value.to_str().ok())
    .and_then(|value| value.strip_prefix("Bearer "))
    .and_then(|credential| credential.rsplit_once('.'))
    .map_or(String::new(), |(scope, _)| scope.to_owned())
}

async fn listens(State(calls): State<Calls>, headers: axum::http::HeaderMap) -> axum::Json<Value> {
  calls.lock().unwrap().push(format!("GET as {}", scope(&headers)));

  axum::Json(json!({ "listens": [
    { "port": 13123, "target": "127.0.0.1:13123", "listening": false },
    { "port": 18030, "target": "127.0.0.1:18030", "listening": true },
  ] }))
}

async fn unlisten(
  State(calls): State<Calls>,
  headers: axum::http::HeaderMap,
  Query(query): Query<Vec<(String, String)>>,
) -> axum::Json<Value> {
  let port = query
    .iter()
    .find(|(key, _)| key == "port")
    .map(|(_, port)| port.clone())
    .unwrap_or_default();

  calls.lock().unwrap().push(format!("DELETE {port} as {}", scope(&headers)));

  axum::Json(json!({ "ok": true }))
}

// "every listen the hub keeps for the agent is let go of, with a credential scoped to that agent alone"
#[tokio::test]
async fn every_listen_the_hub_keeps_for_the_agent_is_let_go_of() {
  let calls: Calls = Arc::default();
  let app = Router::new()
    .route("/listens", axum::routing::get(listens))
    .route("/listen", axum::routing::delete(unlisten))
    .with_state(calls.clone());
  let hub = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = hub.local_addr().unwrap().port();
  let settings = tempfile::NamedTempFile::new().unwrap();

  tokio::spawn(async move { axum::serve(hub, app).await });
  std::fs::write(
    settings.path(),
    format!("JAKE_CONTROL_PORT={port}\nJAKE_CONTROL_TOKEN=t\nJAKE_CONTROL_KEY=k\n"),
  )
  .unwrap();

  assert_eq!(node_link::hub::drop_listens(settings.path(), "pc").await.unwrap(), [13123, 18030]);
  assert_eq!(*calls.lock().unwrap(), ["GET as pc", "DELETE 13123 as pc", "DELETE 18030 as pc"]);
}

// "the folders the placed programs work in are made on the node, and one that stands is left as it is"
#[test]
fn the_folders_the_placed_programs_work_in_are_made_on_the_node() {
  let node = tempfile::tempdir().unwrap();
  let standing = node.path().join("data/supervisor-homes/kept");
  let wanted = node.path().join("data/supervisor-homes/new");

  std::fs::create_dir_all(&standing).unwrap();
  std::fs::write(standing.join("notes.md"), "kept").unwrap();

  node_link::serve::make_folders(&[standing.display().to_string(), wanted.display().to_string()]).unwrap();

  assert!(wanted.is_dir());
  assert_eq!(std::fs::read_to_string(standing.join("notes.md")).unwrap(), "kept");
}

// "a run on the node asks the home bridge and ends as the run at home ended, its output and errors its own"
#[tokio::test]
async fn a_run_on_the_node_asks_the_home_bridge_and_ends_as_the_run_at_home_ended() {
  use std::os::unix::fs::PermissionsExt;

  let dir = tempfile::tempdir().unwrap();
  let deploy = dir.path().join("deploy");
  let verbs = deploy.join("bin/bmo");

  std::fs::create_dir_all(verbs.parent().unwrap()).unwrap();
  std::fs::write(&verbs, "#!/bin/sh\necho \"$@\"\ncat\necho said >&2\nexit 5\n").unwrap();
  std::fs::set_permissions(&verbs, std::fs::Permissions::from_mode(0o755)).unwrap();

  let socket = dir.path().join("bridge.sock");
  let listener = tokio::net::UnixListener::bind(&socket).unwrap();

  tokio::spawn(async move {
    let (stream, _) = listener.accept().await.unwrap();

    node_link::bridge::answer(stream, deploy, "pc".into(), None, |path| path.replace("/n/data", "/h/data"))
      .await
      .unwrap();
  });

  let (mut out, mut err) = (Vec::new(), Vec::new());
  let args = ["agent-status", "status", "--place", "/n/data/agent-places/w"].map(str::to_owned);
  let code = node_link::bridge::ask(&socket, "bin/bmo", &args, &b"payload"[..], &mut out, &mut err).await;

  assert_eq!(code, 5);
  assert_eq!(
    String::from_utf8_lossy(&out),
    "agent-status status --place /h/data/agent-places/w\npayload"
  );
  assert_eq!(String::from_utf8_lossy(&err), "said\n");
}
