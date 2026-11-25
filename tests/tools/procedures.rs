//! The procedures: each document listed by its address and read whole outside any room, held in a room's
//! turn to the verbs its road performs as agency's server answers them, refused where it is no procedure,
//! and none of it for a turn a supervisor's reset overtook; then the executable, placed in a tree, reading
//! one in a room's turn through agency's own server and the consumer registered at its `road-verbs` hook.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use procedures_tool::{
  Server, as_used, filter_procedure, listed, overtaken, procedure_names, procedures_dir, read_in, written,
};
use serde_json::{Value, json};
use tool_call::{Control, Session};

use crate::support::{binary, scratch, standing};

const DOCUMENT: &str = "# Sending\n\nEvery room reads this.\n\n## Stickers [CAP:sticker]\n\n- send one\n\n## Lines\n\n- a line [CAP:text]\n- a pin [CAP:pin]\n  - its fine print\n- plain\n";

fn verbs(names: &[&str]) -> Vec<String> {
  names.iter().map(|name| (*name).to_owned()).collect()
}

// A tree whose procedures folder holds the document under the name `sending`, beside a file that is no
// procedure.
fn tree() -> tempfile::TempDir {
  let tree = tempfile::tempdir().unwrap();

  std::fs::create_dir_all(procedures_dir(tree.path())).unwrap();
  std::fs::write(procedures_dir(tree.path()).join("sending.md"), DOCUMENT).unwrap();
  std::fs::write(procedures_dir(tree.path()).join("notes.txt"), "not a procedure").unwrap();
  tree
}

// A session whose turn names the room where one is given, the agent and the moment where those are.
fn session(archive: &str, chat: Option<i64>, agent: Option<(&str, i64)>) -> Session {
  let mut variables = BTreeMap::new();

  if let Some(chat) = chat {
    variables.insert("BMO_TURN_CHAT".to_owned(), chat.to_string());
  }

  if let Some((agent, began)) = agent {
    variables.insert("BMO_TURN_AGENT".to_owned(), agent.to_owned());
    variables.insert("BMO_TURN_BEGAN".to_owned(), began.to_string());
  }

  variables.insert("BMO_DATA_DIR".to_owned(), "/nowhere".to_owned());

  Session {
    variables,
    labels: tool_call::Labels {
      archive: archive.to_owned(),
      ..tool_call::Labels::default()
    },
    ..Session::default()
  }
}

// A server no read in these tests may reach: it holds no token.
fn unreachable() -> Server {
  Server {
    control: Control {
      port: 1,
      token: None,
    },
  }
}

#[test]
fn a_room_that_performs_every_verb_reads_the_document_whole_minus_the_marks() {
  let read = filter_procedure(DOCUMENT, &verbs(&["sticker", "text", "pin"]));

  assert_eq!(
    read,
    DOCUMENT
      .replace(" [CAP:sticker]", "")
      .replace(" [CAP:text]", "")
      .replace(" [CAP:pin]", "")
  );
}

#[test]
fn a_mark_on_a_line_takes_that_line_and_what_stands_under_it_and_a_heading_its_section() {
  let read = filter_procedure(DOCUMENT, &verbs(&["text"]));

  assert_eq!(read, "# Sending\n\nEvery room reads this.\n\n## Lines\n\n- a line\n- plain\n");
  assert_eq!(
    filter_procedure("- a line [CAP:text,pin]\n- plain\n", &verbs(&["text"])),
    "- plain\n",
    "a line marked with two verbs needs both"
  );
  assert_eq!(filter_procedure("- a line [CAP:shout]\n- plain\n", &verbs(&["text"])), "- plain\n");
}

#[test]
fn the_procedures_are_the_markdown_files_of_the_folder_read_whole_outside_a_room_and_filtered_in_one() {
  let tree = tree();
  let text = written(tree.path(), "sending").unwrap();

  assert_eq!(procedure_names(tree.path()), ["sending"]);
  assert_eq!(as_used(&text, None), DOCUMENT.trim_end());
  assert_eq!(
    as_used(&text, Some(&verbs(&["text"]))),
    "# Sending\n\nEvery room reads this.\n\n## Lines\n\n- a line\n- plain"
  );
  assert_eq!(
    written(tree.path(), "notes").unwrap_err(),
    "no procedure named \"notes\"; there are: sending"
  );
}

#[tokio::test]
async fn each_procedure_is_listed_by_its_address_and_read_whole_outside_any_room() {
  let tree = tree();
  let outside = session("", None, None);

  assert_eq!(
    listed(tree.path(), &outside).await,
    [json!({ "uri": "bmo://procedure/sending", "name": "sending", "mimeType": "text/markdown" })]
  );
  assert_eq!(
    read_in(tree.path(), &outside, &unreachable(), "bmo://procedure/sending").await.unwrap(),
    json!({ "contents": [{ "uri": "bmo://procedure/sending", "mimeType": "text/markdown", "text": DOCUMENT.trim_end() }] })
  );
}

#[tokio::test]
async fn an_address_that_is_no_procedure_is_refused_before_the_server_is_asked() {
  let tree = tree();
  let room = session("", Some(-5), None);

  assert_eq!(
    read_in(tree.path(), &room, &unreachable(), "bmo://procedure/missing")
      .await
      .unwrap_err(),
    "no procedure named \"missing\"; there are: sending"
  );
  assert_eq!(
    read_in(tree.path(), &room, &unreachable(), "file:///etc/passwd").await.unwrap_err(),
    "there is no resource file:///etc/passwd"
  );
  assert!(
    read_in(tree.path(), &room, &unreachable(), "bmo://procedure/sending")
      .await
      .unwrap_err()
      .contains("AGENCY_CONTROL_TOKEN"),
    "a room's read with no server to ask is refused naming why"
  );
}

#[test]
fn the_server_is_named_by_the_environment_first_then_the_data_roots_file() {
  let data = tempfile::tempdir().unwrap();

  std::fs::create_dir_all(data.path().join("env")).unwrap();
  std::fs::write(
    data.path().join("env").join("agency.env"),
    "AGENCY_CONTROL_PORT=4100\nAGENCY_CONTROL_TOKEN=\"from-file\"\n",
  )
  .unwrap();

  assert_eq!(
    Server::read(data.path(), |_| None),
    Server {
      control: Control {
        port: 4100,
        token: Some("from-file".into())
      }
    }
  );
  assert_eq!(
    Server::read(data.path(), |name| (name == "AGENCY_CONTROL_TOKEN").then(|| "own".to_owned())),
    Server {
      control: Control {
        port: 4100,
        token: Some("own".into())
      }
    }
  );
  assert_eq!(
    Server::read(&data.path().join("none"), |_| None),
    Server {
      control: Control {
        port: 3220,
        token: None
      }
    }
  );
}

// A stand-in of agency's server answering the road's verbs: `text` for the room `-5` on the road `console`,
// and a refusal for any other room. It writes down each token it was asked under.
async fn stand_in_server() -> (u16, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
  let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
  let app = axum::Router::new()
    .route(
      "/road-verbs",
      get(
        |State(asked): State<std::sync::Arc<std::sync::Mutex<Vec<String>>>>,
         headers: HeaderMap,
         Query(query): Query<BTreeMap<String, String>>| async move {
          asked.lock().unwrap().push(
            headers
              .get("authorization")
              .and_then(|value| value.to_str().ok())
              .unwrap_or_default()
              .to_owned(),
          );

          match (query.get("chat").map(String::as_str), query.get("via").map(String::as_str)) {
            (Some("-5"), Some("console")) => (StatusCode::OK, Json(json!({ "can": ["text"] }))),
            _ => (StatusCode::BAD_REQUEST, Json(json!({ "code": "bad-request", "error": "no such room" }))),
          }
        },
      ),
    )
    .with_state(asked.clone());
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();

  tokio::spawn(async move { axum::serve(listener, app).await });

  (port, asked)
}

#[tokio::test]
async fn a_rooms_read_is_held_to_the_verbs_the_server_answers_for_its_road() {
  let tree = tree();
  let (port, asked) = stand_in_server().await;
  let server = Server {
    control: Control {
      port,
      token: Some("secret".into()),
    },
  };
  let mut room = session("", Some(-5), None);

  room.variables.insert("BMO_TURN_VIA".into(), "console".into());

  assert_eq!(
    read_in(tree.path(), &room, &server, "bmo://procedure/sending").await.unwrap()["contents"][0]["text"],
    "# Sending\n\nEvery room reads this.\n\n## Lines\n\n- a line\n- plain"
  );
  assert_eq!(*asked.lock().unwrap(), ["Bearer secret"]);
  assert_eq!(
    server.road_can(-6, None).await.unwrap_err(),
    "agency's server refused the verbs of the room's road (400 Bad Request): no such room"
  );
}

#[tokio::test]
async fn a_turn_a_supervisors_reset_overtook_is_listed_none_and_refused_its_read() {
  let scratch = scratch::Scratch::new().await;
  let tree = tree();
  let point = 1_790_000_000_000_i64;

  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ($1, $2)"#,
      vec![store::reset_point::key("supervisor").into(), point.to_string().into()],
    )
    .await;

  let overtaken_turn = session(&scratch.address(), Some(-5), Some(("supervisor", point - 7_000)));
  let at_the_point = session(&scratch.address(), Some(-5), Some(("supervisor", point)));
  let after = session(&scratch.address(), None, Some(("supervisor", point + 1)));
  let read = read_in(tree.path(), &overtaken_turn, &unreachable(), "bmo://procedure/sending").await;

  assert!(listed(tree.path(), &overtaken_turn).await.is_empty());
  assert!(
    read.as_ref().is_err_and(|why| why.contains("was reset after this turn began")),
    "{read:?}"
  );
  assert!(overtaken(&at_the_point).await.is_some());
  assert_eq!(overtaken(&after).await, None);
  assert_eq!(listed(tree.path(), &after).await.len(), 1);
}

// Agency's own server started on the store and a data root of its own, its control port named in the data
// root's `env/agency.env` as the deployment names it.
fn stand_server(home: &Path, data: &Path, store: &str, token: &str) -> standing::Standing {
  let server = standing::stand_server(home, data, store, token);

  std::fs::create_dir_all(data.join("env")).unwrap();
  std::fs::write(
    data.join("env").join("agency.env"),
    format!("AGENCY_CONTROL_PORT={}\nAGENCY_CONTROL_TOKEN={token}\n", server.port),
  )
  .unwrap();

  server
}

// The procedures executable started in the tree's placed folder with a clean environment, as the tool host
// starts it, handed `request`; what it answered.
fn ask_executable(tree: &Path, request: &Value) -> Value {
  let placed = tree.join("tools").join("procedures").join(".placed");

  std::fs::create_dir_all(&placed).unwrap();

  let mut child = Command::new(binary::path("procedures-tool"))
    .current_dir(&placed)
    .env_clear()
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();

  child.stdin.take().unwrap().write_all(request.to_string().as_bytes()).unwrap();

  let ended = child.wait_with_output().unwrap();

  assert!(ended.status.success(), "{}", String::from_utf8_lossy(&ended.stderr));
  serde_json::from_slice(&ended.stdout).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_executable_reads_a_procedure_in_a_room_through_agencys_server_and_the_registered_consumer() {
  let scratch = scratch::Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let tree = tree();
  let token = "secret";
  let server = stand_server(home.path(), &data, &scratch.address(), token);
  let request = json!({
    "protocol": 1,
    "kind": "resources_read",
    "uri": "bmo://procedure/sending",
    "session": {
      "holderName": "chat",
      "namespaces": ["bmo"],
      "capabilities": ["room.tools"],
      "grants": [],
      "variables": { "BMO_DATA_DIR": data, "BMO_TURN_CHAT": "-5", "BMO_TURN_VIA": "console" },
      "labels": { "line": "chat", "archive": scratch.address() },
    },
    "callback": "/run/c.sock",
  });
  let text = |answer: Value| answer["contents"][0]["text"].as_str().unwrap_or_default().to_owned();

  // With no consumer registered, the road performs no verb.
  assert_eq!(
    text(ask_executable(tree.path(), &request)),
    "# Sending\n\nEvery room reads this.\n\n## Lines\n\n- plain"
  );

  // A consumer that holds the rooms registers at the hook and answers the road's verbs.
  let consumer = axum::Router::new().route(
    "/",
    post(|Json(asked): Json<Value>| async move {
      let can = if asked["point"] == "road-verbs" && asked["ask"] == json!({ "chat": -5, "via": "console" })
      {
        json!(["text", "sticker"])
      } else {
        json!([])
      };

      Json(json!({ "answer": can }))
    }),
  );
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let consumer_port = listener.local_addr().unwrap().port();

  tokio::spawn(async move { axum::serve(listener, consumer).await });

  let registered = reqwest::Client::new()
    .put(format!("http://127.0.0.1:{}/hooks/road-verbs", server.port))
    .bearer_auth(token)
    .json(&json!({ "url": format!("http://127.0.0.1:{consumer_port}/") }))
    .send()
    .await
    .unwrap();

  assert!(registered.status().is_success());

  let tree_path = tree.path().to_path_buf();
  let read = tokio::task::spawn_blocking(move || ask_executable(&tree_path, &request)).await.unwrap();

  assert_eq!(
    text(read),
    "# Sending\n\nEvery room reads this.\n\n## Stickers\n\n- send one\n\n## Lines\n\n- a line\n- plain"
  );

  let listing = json!({
    "protocol": 1,
    "kind": "resources_list",
    "session": { "variables": { "BMO_DATA_DIR": data }, "labels": { "archive": scratch.address() } },
  });

  assert_eq!(
    ask_executable(tree.path(), &listing),
    json!({ "resources": [{ "uri": "bmo://procedure/sending", "name": "sending", "mimeType": "text/markdown" }] })
  );
}
