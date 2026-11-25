//! A person's PC reached through its hub for the workplace's setup (docs/person-nodes.md, 「The workplace on a
//! PC」), against a hub standing in on the loopback: its commands run here in a scratch home, where the user
//! manager's `systemd-run` and `systemctl` are stand-ins that run a step at once. The stand-in judges what each
//! request presents as jake's hub does (jake's `docs/architecture.md`, 「Configuration and secrets」): the control
//! token reaches every agent, a credential scoped to named agents reaches those alone, and a request naming an agent
//! outside its reach is refused 403 `agent-out-of-reach`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use agency_server::peers::node::HubReach;
use agency_server::verbs::work_node_hub::{HubDoor, HubHands, wsl_said};
use agency_server::verbs::work_node_setup::{FarEnd, SetupHands, SetupShell};
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const TOKEN: &str = "stand-in-token";
const CONTROL_KEY: &str = "stand-in-control-key";
const AGENT: &str = "pc";
/// Another person's node, attached to the same hub.
const OTHER: &str = "other-pc";

// The user manager stood in for: a transient unit's command run at once to its end, and no unit active.
const SYSTEMD_RUN: &str = "#!/bin/sh\nwhile [ \"$1\" != /bin/bash ]; do shift; done\nexec \"$@\"\n";
const SYSTEMCTL: &str = "#!/bin/sh\nexit 3\n";

#[derive(Clone)]
struct Hub {
  home: PathBuf,
  shims: PathBuf,
  commands: Arc<Mutex<Vec<String>>>,
  /// Every credential presented to the hub, as it came.
  presented: Arc<Mutex<Vec<String>>>,
}

// A credential scoped to the names joined by commas, made with the hub's control key as jake makes it.
fn scoped(names: &str) -> String {
  let mut mac = Hmac::<Sha256>::new_from_slice(CONTROL_KEY.as_bytes()).unwrap();

  mac.update(names.as_bytes());
  format!("{names}.{}", hex::encode(mac.finalize().into_bytes()))
}

// Whether what a request presents reaches the agent named, else the status the hub refuses it with: 401 for a
// credential that is neither the token nor scoped with the key, 403 for one outside its scope.
fn reaches(hub: &Hub, headers: &HeaderMap, agent: &str) -> Result<(), StatusCode> {
  let presented = headers
    .get("authorization")
    .and_then(|value| value.to_str().ok())
    .and_then(|value| value.strip_prefix("Bearer "))
    .unwrap_or_default()
    .to_owned();

  hub.presented.lock().unwrap().push(presented.clone());

  if presented == TOKEN {
    return Ok(());
  }

  let Some(names) = presented
    .rsplit_once('.')
    .map(|(names, _)| names)
    .filter(|names| !names.is_empty() && scoped(names) == presented)
  else {
    return Err(StatusCode::UNAUTHORIZED);
  };

  if names.split(',').any(|name| name.trim() == agent) {
    return Ok(());
  }

  Err(StatusCode::FORBIDDEN)
}

// The hub's answer to a refused request, a 403 saying the agent is out of reach.
fn refusal(status: StatusCode) -> Response {
  match status {
    StatusCode::FORBIDDEN => (
      status,
      axum::Json(json!({ "error": "the agent is out of reach", "code": "agent-out-of-reach" })),
    )
      .into_response(),
    _ => status.into_response(),
  }
}

fn events(events: &[(&str, Value)]) -> Response {
  let body: String =
    events.iter().map(|(event, data)| format!("event: {event}\ndata: {data}\n\n")).collect();

  ([("content-type", "text/event-stream")], body).into_response()
}

// The agents attached that the credential reaches, each with what it offers.
async fn status(State(hub): State<Hub>, headers: HeaderMap) -> Response {
  let capabilities = json!(["exec:linux", "transfer", "stream:47201"]);
  let mut agents = Vec::new();

  for name in [AGENT, OTHER] {
    match reaches(&hub, &headers, name) {
      Ok(()) => agents.push(json!({ "name": name, "capabilities": capabilities })),
      Err(StatusCode::UNAUTHORIZED) => return refusal(StatusCode::UNAUTHORIZED),
      Err(_) => {}
    }
  }

  axum::Json(json!({ "agents": agents })).into_response()
}

fn asked(query: &[(String, String)], name: &str) -> Option<String> {
  query.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone())
}

// A command run here as the agent runs it on the PC, its output and its end told as the hub tells them.
async fn exec(
  State(hub): State<Hub>,
  headers: HeaderMap,
  Query(query): Query<Vec<(String, String)>>,
  axum::Json(body): axum::Json<Value>,
) -> Response {
  if let Err(refused) = reaches(&hub, &headers, &asked(&query, "agent").unwrap_or_default()) {
    return refusal(refused);
  }

  if body["shell"] != "linux" {
    return StatusCode::BAD_REQUEST.into_response();
  }

  let command = body["command"].as_str().unwrap_or_default().to_owned();
  // What the door hands the hub is one line carrying the command whole, and what was asked is what it decodes
  // to; a line handed in any other way is what was asked as it stands.
  let asked = command
    .strip_prefix("printf %s ")
    .and_then(|rest| rest.strip_suffix(" | base64 -d | /bin/bash"))
    .and_then(|coded| base64::Engine::decode(&base64::engine::general_purpose::STANDARD, coded).ok())
    .and_then(|bytes| String::from_utf8(bytes).ok())
    .unwrap_or_else(|| command.clone());

  hub.commands.lock().unwrap().push(asked);

  let path = format!("{}:{}", hub.shims.display(), std::env::var("PATH").unwrap_or_default());
  let done = tokio::process::Command::new("bash")
    .arg("-c")
    .arg(&command)
    .env("HOME", &hub.home)
    .env("PATH", path)
    .env_remove("XDG_CACHE_HOME")
    .stdin(std::process::Stdio::null())
    .kill_on_drop(true)
    .output()
    .await
    .unwrap();
  let chunk =
    |bytes: &[u8]| json!({ "seq": 0, "chunk": { "bytes": bytes.len(), "base64": STANDARD.encode(bytes) } });

  events(&[
    ("started", json!({ "id": "c1" })),
    ("stdout", chunk(&done.stdout)),
    ("stderr", chunk(&done.stderr)),
    ("exit", json!({ "code": done.status.code(), "signal": null, "canceled": false })),
  ])
}

// An upload written whole where it is named, once its length and checksum match what was declared.
async fn upload(
  State(hub): State<Hub>,
  headers: HeaderMap,
  Query(query): Query<Vec<(String, String)>>,
  body: Bytes,
) -> Response {
  let asked = |name: &str| asked(&query, name);

  if let Err(refused) = reaches(&hub, &headers, &asked("agent").unwrap_or_default()) {
    return refusal(refused);
  }

  if asked("total") != Some(body.len().to_string())
    || asked("checksum") != Some(format!("{:x}", Sha256::digest(&body)))
  {
    return events(&[(
      "error",
      json!({ "code": "integrity", "message": "the declared upload did not match" }),
    )]);
  }

  std::fs::write(asked("path").unwrap(), &body).unwrap();
  events(&[("completed", json!({ "total": body.len() }))])
}

struct Stood {
  reach: HubReach,
  home: PathBuf,
  commands: Arc<Mutex<Vec<String>>>,
  presented: Arc<Mutex<Vec<String>>>,
  _scratch: tempfile::TempDir,
}

const SETTINGS: &str =
  "JAKE_HUB_PORT=8790\nJAKE_CONTROL_TOKEN=stand-in-token\nJAKE_CONTROL_KEY=stand-in-control-key\n";

async fn stand_hub() -> Stood {
  stand_hub_with(SETTINGS).await
}

async fn stand_hub_with(settings: &str) -> Stood {
  let scratch = tempfile::tempdir().unwrap();
  let home = scratch.path().join("home");
  let shims = scratch.path().join("shims");

  std::fs::create_dir_all(&home).unwrap();
  std::fs::create_dir_all(&shims).unwrap();

  for (name, body) in [("systemd-run", SYSTEMD_RUN), ("systemctl", SYSTEMCTL)] {
    let file = shims.join(name);

    std::fs::write(&file, body).unwrap();
    std::fs::set_permissions(&file, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
  }

  let token_file = scratch.path().join("hub.env");

  std::fs::write(&token_file, settings).unwrap();

  let commands = Arc::new(Mutex::new(Vec::new()));
  let presented = Arc::new(Mutex::new(Vec::new()));
  let hub = Hub {
    home: home.clone(),
    shims,
    commands: commands.clone(),
    presented: presented.clone(),
  };
  let app = Router::new()
    .route("/status", get(status))
    .route("/exec", post(exec))
    .route("/upload", post(upload))
    .with_state(hub);
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());

  tokio::spawn(async move { axum::serve(listener, app).await });

  Stood {
    reach: HubReach {
      address,
      token_file: token_file.to_string_lossy().into_owned(),
      agent: AGENT.into(),
    },
    home,
    commands,
    presented,
    _scratch: scratch,
  }
}

async fn door(stood: &Stood) -> HubDoor {
  HubDoor::open(&stood.reach, Path::new("/")).await.unwrap()
}

// "a step runs as a transient unit of the person's own and comes back with its output and its code"
#[tokio::test]
async fn a_step_runs_as_a_transient_unit_and_comes_back_with_its_output_and_its_code() {
  let stood = stand_hub().await;
  let door = door(&stood).await;
  let ran = door.run(r#"echo "out $HOME"; echo err >&2; exit 3"#).await;

  assert_eq!(
    (ran.code, ran.stdout.as_str(), ran.stderr.as_str()),
    (3, format!("out {}\n", stood.home.display()).as_str(), "err\n")
  );
  assert!(
    stood.commands.lock().unwrap()[0]
      .contains("systemd-run --user --collect --quiet --unit=bmo-node-step-")
  );
  assert!(stood.commands.lock().unwrap()[0].contains("-p KillMode=process"));
  // The step's files are gone once it is read back.
  assert_eq!(std::fs::read_dir(stood.home.join(".cache/bmo-node-setup")).unwrap().count(), 0);
}

// "an agent the hub does not have attached is refused, and what an attached one does not offer is said"
#[tokio::test]
async fn an_absent_agent_is_refused_and_what_an_attached_one_does_not_offer_is_said() {
  let stood = stand_hub().await;
  let absent = HubReach {
    agent: "elsewhere".into(),
    ..stood.reach.clone()
  };
  let refused = HubDoor::open(&absent, Path::new("/")).await.err().unwrap();
  let ports = FarEnd {
    mirror_port: 47201,
    herdr_port: 47202,
  };

  assert!(refused.contains("no agent elsewhere attached"));
  assert_eq!(
    door(&stood).await.wanting(&ports),
    [
      "jake: the agent pc does not offer port 47202 for herdr to stream to; its JAKE_STREAM_PORTS must list it"
    ]
  );
}

// "WSL's mirrored networking is said with the range Windows keeps for the Linux side and each far end port in it;
// another mode is said alone"
#[test]
fn wsl_s_mirrored_range_is_said_with_each_far_end_port_inside_it() {
  let ports = [(47201, "the mirror"), (13123, "the tool host")];

  assert_eq!(
    wsl_said("mirrored\n44620\t48715\n", &ports),
    [
      "wsl: networking mode mirrored; Windows keeps ports 44620-48715 for the Linux side's own connections",
      "wsl: port 47201 for the mirror lies in that range, where a Linux connection's own port may hold it when the far end starts again",
    ]
  );
  assert_eq!(wsl_said("nat\n32768 60999\n", &ports), ["wsl: networking mode nat"]);
}

// "a tree goes up through one upload and is laid over the node's copy, leaving what was built there"
#[tokio::test]
async fn a_tree_goes_up_through_one_upload_and_is_laid_over_the_node_s_copy_leaving_what_was_built_there() {
  let stood = stand_hub().await;
  let door = door(&stood).await;
  let here = tempfile::tempdir().unwrap();
  let from = here.path().join("deploy");
  let to = stood.home.join("w/deploy");

  for (file, body) in [("docs/a.md", "a"), ("node_modules/x/i.js", "x"), (".env", "secret")] {
    std::fs::create_dir_all(from.join(file).parent().unwrap()).unwrap();
    std::fs::write(from.join(file), body).unwrap();
  }

  for (file, body) in [("bin/agent-host", "built"), ("stale.md", "old")] {
    std::fs::create_dir_all(to.join(file).parent().unwrap()).unwrap();
    std::fs::write(to.join(file), body).unwrap();
  }

  let hands = HubHands {
    install: false,
    copy_code: true,
    door: &door,
    staging: here.path().join("staging"),
  };
  let (copied, said) = hands.copy(&format!("{}/", from.display()), &format!("{}/", to.display())).await;

  assert!(copied, "{said}");
  assert_eq!(std::fs::read_to_string(to.join("docs/a.md")).unwrap(), "a");
  assert_eq!(std::fs::read_to_string(to.join("bin/agent-host")).unwrap(), "built");
  assert!(!to.join("stale.md").exists() && !to.join("node_modules").exists() && !to.join(".env").exists());
  assert!(
    !stood.home.join("w/deploy.incoming").exists() && !stood.home.join("w/deploy.incoming.tar.gz").exists()
  );
}

// "a node is reached with a credential scoped to its agent alone, which the hub refuses on another person's node"
#[tokio::test]
async fn a_node_is_reached_with_a_credential_scoped_to_its_agent_which_the_hub_refuses_on_another_node() {
  let stood = stand_hub().await;
  let door = door(&stood).await;
  let ran = door.run("true").await;
  let presented = stood.presented.lock().unwrap().clone();

  assert_eq!(ran.code, 0, "{}", ran.stderr);
  // The status read, the step's start and its read and removal each carried the one scoped credential.
  assert!(presented.len() >= 4);
  assert!(presented.iter().all(|credential| *credential == scoped(AGENT)), "{presented:?}");
  // A door to the other person's node is opened with that node's own credential, never one reaching both.
  let elsewhere = HubReach {
    agent: OTHER.into(),
    ..stood.reach.clone()
  };

  HubDoor::open(&elsewhere, Path::new("/")).await.unwrap();
  assert_eq!(stood.presented.lock().unwrap().last(), Some(&scoped(OTHER)));

  // The credential the door holds, sent naming another person's node, is refused before anything runs there.
  let refused = reqwest::Client::new()
    .post(format!("{}/exec", stood.reach.address))
    .query(&[("agent", OTHER)])
    .bearer_auth(scoped(AGENT))
    .json(&json!({ "shell": "linux", "command": "touch reached" }))
    .send()
    .await
    .unwrap();

  assert_eq!(refused.status(), 403);
  assert_eq!(refused.json::<Value>().await.unwrap()["code"], "agent-out-of-reach");
  assert!(!stood.commands.lock().unwrap().iter().any(|command| command.contains("reached")));
}

// "a hub whose settings hold no control key is not reached, and the control token is never sent in its place"
#[tokio::test]
async fn a_hub_whose_settings_hold_no_control_key_is_not_reached() {
  let stood = stand_hub_with("JAKE_CONTROL_TOKEN=stand-in-token\n").await;
  let refused = HubDoor::open(&stood.reach, Path::new("/")).await.err().unwrap();

  assert!(refused.contains("holds no JAKE_CONTROL_KEY"), "{refused}");
  assert!(stood.presented.lock().unwrap().is_empty());
}

#[test]
fn a_file_s_place_on_a_windows_agent_is_the_folder_s_windows_view_and_its_name() {
  use agency_server::verbs::work_node_hub::windows_view;

  assert_eq!(
    windows_view("\\\\wsl.localhost\\distro\\home\\person\r\n", "tree.tar.gz"),
    "\\\\wsl.localhost\\distro\\home\\person\\tree.tar.gz"
  );
  assert_eq!(windows_view("C:\\folder\\", "a"), "C:\\folder\\a");
}
