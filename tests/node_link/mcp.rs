//! MCP servers as personal apps (docs/person-nodes.md, 「MCP servers」): a folder of 'my apps' holding `mcp.toml` is
//! placed with its tools listed by the runner's own binary run as a call runs, the version's manifest, schemas and
//! entries' server tools made of them; a call of an entry reaches the server's tool and answers its result, an error
//! it answers a result with `isError`; a version asking for what the place does not carry places nothing and answers
//! what it asks for; a listing that fails or lists nothing refuses the place and leaves nothing staged; a remote
//! server is reached over streamable HTTP with its secret in its header; and where bubblewrap makes namespaces here,
//! all of it runs confined, a remote server reached through the call's proxy alone.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::{Value, json};

/// A process the test started, killed and waited on as the test ends.
struct Started(Child);

impl Drop for Started {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

// A stand-in for bubblewrap: the command after `--` run unconfined in the folder bound at /tool, in the environment
// the words set alone, the runner's own binary where it is bound at /.bmo/node-link, and each secret handed on its
// descriptor written into a folder beside the stand-in, which `BMO_SECRETS` names; so the runner's keeping and calls
// are tried where no namespace can be made.
const FAKE_BWRAP: &str = r#"#!/usr/bin/env python3
import os, sys
args = sys.argv[1:]
tool = binary = None
env = {}
secrets = os.path.join(os.path.dirname(os.path.abspath(sys.argv[0])), f"secrets-{os.getpid()}")
at = 0
while at < len(args):
    word = args[at]
    if word == "--":
        command = args[at + 1:]
        break
    if word in ("--ro-bind", "--bind", "--ro-bind-try", "--symlink", "--setenv"):
        source, target = args[at + 1], args[at + 2]
        if word == "--ro-bind" and target == "/tool":
            tool = source
        if word == "--ro-bind" and target == "/.bmo/node-link":
            binary = source
        if word == "--setenv":
            env[source] = target
        at += 3
    elif word == "--ro-bind-data":
        os.makedirs(secrets, exist_ok=True)
        descriptor, target = int(args[at + 1]), args[at + 2]
        data = b""
        while True:
            part = os.read(descriptor, 65536)
            if not part:
                break
            data += part
        with open(os.path.join(secrets, os.path.basename(target)), "wb") as written:
            written.write(data)
        at += 3
    elif word in ("--perms", "--proc", "--dev", "--tmpfs", "--chdir"):
        at += 2
    else:
        at += 1
if "BMO_SECRETS" in env:
    env["BMO_SECRETS"] = secrets
if command[0] == "/.bmo/node-link":
    command[0] = binary
os.chdir(tool)
os.execvpe(command[0], command, env)
"#;

// A stand-in MCP server on its standard input and output, one JSON-RPC message a line: its tools listed over two
// pages, `sumWeek` answering the days it is given, `sum-week` a second tool taking the same entry name, `readKey`
// answering the secret its call is handed, and `lose` answering an error; with `empty` it lists no tool. It says the
// secret it starts with on its errors.
const SERVER: &str = r#"import json, os, sys

empty = sys.argv[1:] == ["empty"]
secret = open(os.path.join(os.environ.get("BMO_SECRETS", "/secrets"), "team-key")).read().strip()
print("starting with " + secret, file=sys.stderr, flush=True)

def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    if not line.strip():
        continue
    message = json.loads(line)
    if "id" not in message or "method" not in message:
        continue
    method, params = message["method"], message.get("params") or {}
    answer = {"jsonrpc": "2.0", "id": message["id"]}
    if method == "initialize":
        answer["result"] = {"protocolVersion": params["protocolVersion"], "capabilities": {"tools": {}},
                            "serverInfo": {"name": "stand-in", "version": "1"}}
    elif method == "tools/list" and empty:
        answer["result"] = {"tools": []}
    elif method == "tools/list" and not params.get("cursor"):
        answer["result"] = {"tools": [
            {"name": "sumWeek", "title": "Sum the week", "description": "Sums up the week.",
             "inputSchema": {"type": "object", "properties": {"days": {"type": "integer"}}}},
            {"name": "sum-week", "description": "Takes a name already taken.", "inputSchema": {"type": "object"}},
        ], "nextCursor": "second"}
    elif method == "tools/list":
        answer["result"] = {"tools": [
            {"name": "readKey", "annotations": {"title": "Read the key"}, "inputSchema": {"type": "object"}},
            {"name": "lose", "inputSchema": {"type": "object"}},
        ]}
    elif method == "tools/call" and params["name"] == "sumWeek":
        days = params["arguments"]["days"]
        answer["result"] = {"content": [{"type": "text", "text": f"summed {days}"}],
                            "structuredContent": {"days": days}, "isError": False}
    elif method == "tools/call" and params["name"] == "readKey":
        answer["result"] = {"content": [{"type": "text", "text": "the key is " + secret}], "isError": False}
    else:
        answer["error"] = {"code": -32603, "message": "the week is lost"}
    send(answer)
"#;

/// "stand-in value" and a line end, as a secret's file holds it, in base64.
fn team_key() -> String {
  base64::engine::general_purpose::STANDARD.encode("stand-in value\n")
}

struct Machine {
  dir: tempfile::TempDir,
  bwrap: Option<PathBuf>,
}

impl Machine {
  // A machine whose runner confines with the stand-in for bubblewrap, or with bubblewrap itself where `real`.
  fn new(real: bool) -> Self {
    let dir = tempfile::tempdir().unwrap();
    let bwrap = (!real).then(|| {
      let bwrap = dir.path().join("fake-bwrap");

      fs::write(&bwrap, FAKE_BWRAP).unwrap();
      fs::set_permissions(&bwrap, fs::Permissions::from_mode(0o755)).unwrap();
      bwrap
    });

    fs::create_dir_all(dir.path().join("tmp")).unwrap();
    Self { dir, bwrap }
  }

  fn root(&self) -> PathBuf {
    self.dir.path().join("data").join(node_link::runner::PERSONAL_TOOLS)
  }

  fn scratch(&self) -> PathBuf {
    self.dir.path().join("tmp")
  }

  // An MCP server's folder of 'my apps' holding the `mcp.toml` given and the stand-in server.
  fn server(&self, folder: &str, mcp: &str) -> PathBuf {
    let dir = self.dir.path().join("data").join(node_link::runner::MY_APPS).join(folder);

    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("mcp.toml"), mcp).unwrap();
    fs::write(dir.join("server.py"), SERVER).unwrap();
    dir
  }

  // One asking line of the runner, every line it answered, the variables given set for it.
  fn ask(&self, line: &Value, env: &[(&str, &str)]) -> Vec<Value> {
    let mut command = Command::new(crate::support::binary::path("node-link"));

    command.args(["runner", "--root"]).arg(self.root()).args(["--owner", "a1"]);

    match &self.bwrap {
      Some(bwrap) => command.env(node_link::runner::BWRAP_VARIABLE, bwrap),
      None => command.env_remove(node_link::runner::BWRAP_VARIABLE),
    };

    command.env("TMPDIR", self.scratch());

    for (name, value) in env {
      command.env(name, value);
    }

    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());

    let mut child = Started(command.spawn().expect("the runner"));
    let mut input = child.0.stdin.take().unwrap();

    input.write_all(format!("{line}\n").as_bytes()).unwrap();

    let mut answered = Vec::new();
    let mut lines = BufReader::new(child.0.stdout.take().unwrap());

    loop {
      let mut read = String::new();

      if lines.read_line(&mut read).unwrap_or(0) == 0 {
        break;
      }

      let value: Value = serde_json::from_str(&read).unwrap();
      let last = value["t"] != "said";

      answered.push(value);

      if last {
        break;
      }
    }

    drop(input);
    answered
  }

  fn place(&self, place: Value, env: &[(&str, &str)]) -> Vec<Value> {
    self.ask(&json!({ "owner": "a1", "place": place }), env)
  }

  // A call of `entry` of the version placed, its run words those its manifest names, carrying what is given.
  fn call(&self, tool: &str, version: &str, entry: &str, arguments: Value, carried: Value) -> Value {
    let manifest: toml::Table = fs::read_to_string(self.versions_dir(tool).join(version).join("tool.toml"))
      .unwrap()
      .parse()
      .unwrap();
    let mut line = json!({
      "owner": "a1",
      "tool": tool,
      "version": version,
      "run": manifest["tool"]["run"].as_array().unwrap().iter().map(|word| word.as_str().unwrap()).collect::<Vec<_>>(),
      "call": { "protocol": 1, "kind": "call", "entry": format!("my/{entry}"), "arguments": arguments },
    });

    for (name, value) in carried.as_object().unwrap().iter().filter(|(name, _)| *name != "resolve") {
      line[name] = value.clone();
    }

    let resolved = carried["resolve"].as_str().unwrap_or_default().to_owned();
    let answered = self.ask(&line, &[(node_link::proxy::RESOLVE_VARIABLE, &resolved)]);
    let end = answered.last().unwrap();

    assert_eq!(end["t"], "end", "{answered:?}");
    assert_eq!(end["code"], 0, "{answered:?}");
    serde_json::from_str(end["out"].as_str().unwrap()).unwrap()
  }

  fn versions_dir(&self, tool: &str) -> PathBuf {
    self.root().join("owners").join("a1").join("tools").join(tool)
  }

  fn standing(&self, tool: &str) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(self.versions_dir(tool))
      .into_iter()
      .flatten()
      .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
      .collect();

    found.sort();
    found
  }
}

const LOCAL: &str = r#"[server]
name = "week"
title = "Week"
description = { en = "The week's sums." }
language = "python"
transport = "local"
command = ["python3", "server.py"]

[permissions]
secrets = ["team-key"]
"#;

// The local server placed and called on the machine: its manifest made of the tools it listed, and each kind of
// call answered.
fn local_server_placed_and_called(machine: &Machine) {
  machine.server("week", LOCAL);

  let answered = machine.place(json!({ "tool": "week", "secrets": { "team-key": team_key() } }), &[]);
  let placed = answered.last().unwrap();

  assert_eq!(placed["t"], "placed", "{answered:?}");
  // What the server says as it lists is said masked.
  assert!(
    answered
      .iter()
      .any(|one| one["t"] == "said" && one["line"] == "starting with [secret team-key]"),
    "{answered:?}"
  );
  assert!(!format!("{answered:?}").contains("stand-in value"), "{answered:?}");

  let version = placed["version"].as_str().unwrap();
  let files = placed["files"].as_object().unwrap();
  let mut paths: Vec<&String> = files.keys().collect();

  paths.sort();
  assert_eq!(
    paths,
    ["schemas/lose.json", "schemas/read_key.json", "schemas/sum_week.json", "tool.toml"]
  );

  let manifest: toml::Table = files["tool.toml"].as_str().unwrap().parse().unwrap();

  assert_eq!(manifest["tool"]["name"].as_str(), Some("week"));
  assert_eq!(manifest["tool"]["protocol"].as_integer(), Some(1));
  assert_eq!(manifest["tool"]["language"].as_str(), Some("python"));
  assert_eq!(
    manifest["tool"]["run"].as_array().unwrap(),
    &[toml::Value::from("/.bmo/node-link"), toml::Value::from("mcp")]
  );

  let entries = manifest["entry"].as_array().unwrap();
  let named: Vec<&str> = entries.iter().map(|entry| entry["name"].as_str().unwrap()).collect();

  assert_eq!(named, ["sum_week", "read_key", "lose"]);
  assert_eq!(entries[0]["title"].as_str(), Some("Sum the week"));
  assert_eq!(entries[0]["description"].as_str(), Some("Sums up the week."));
  assert_eq!(entries[1]["title"].as_str(), Some("Read the key"));
  assert_eq!(entries[1]["description"].as_str(), Some("Read the key"));
  assert_eq!(entries[2]["description"].as_str(), Some("lose"));

  for entry in entries {
    assert_eq!(entry["namespace"].as_str(), Some("my"));
    assert_eq!(entry["capabilities"].as_array().unwrap(), &[toml::Value::from("my.own")]);
    assert_eq!(
      entry["input"].as_str().unwrap(),
      format!("schemas/{}.json", entry["name"].as_str().unwrap())
    );
  }

  assert_eq!(manifest["permissions"]["secrets"].as_array().unwrap(), &[toml::Value::from("team-key")]);

  let schema: Value = serde_json::from_str(files["schemas/sum_week.json"].as_str().unwrap()).unwrap();

  assert_eq!(schema["properties"]["days"]["type"], "integer");

  let placed_dir = machine.versions_dir("week").join(version);
  let names: Value =
    serde_json::from_str(&fs::read_to_string(placed_dir.join("mcp-tools.json")).unwrap()).unwrap();

  assert_eq!(names, json!({ "sum_week": "sumWeek", "read_key": "readKey", "lose": "lose" }));
  assert!(placed_dir.join("server.py").is_file() && placed_dir.join("mcp.toml").is_file());
  assert!(placed_dir.join(".bmo-source.json").is_file());

  let carried = json!({ "secrets": { "team-key": team_key() } });
  let summed = machine.call("week", version, "sum_week", json!({ "days": 7 }), carried.clone());

  assert_eq!(
    summed,
    json!({ "content": [{ "type": "text", "text": "summed 7" }], "structuredContent": { "days": 7 }, "isError": false })
  );

  let read = machine.call("week", version, "read_key", json!({}), carried.clone());

  assert_eq!(read["content"][0]["text"], "the key is [secret team-key]", "{read}");

  let lost = machine.call("week", version, "lose", json!({}), carried);

  assert_eq!(lost["isError"], true, "{lost}");
  assert_eq!(lost["content"][0]["text"], "the week is lost", "{lost}");
}

// "an MCP server's place lists its tools through the runner's own binary, run as a call runs, and makes the
// version's manifest, schemas and entries' server tools of them; a call of an entry reaches the server's tool and
// answers its result, an error the server answers a result with isError"
#[test]
fn an_mcp_server_is_placed_from_its_listing_and_its_entries_reach_its_tools() {
  local_server_placed_and_called(&Machine::new(false));
}

// "a version asking for an address or a secret the place does not carry places nothing and answers every address
// and secret it asks for, its remote server's host and secret among them"
#[test]
fn an_mcp_server_asking_for_what_the_place_does_not_carry_is_answered_wants() {
  let machine = Machine::new(false);

  machine.server(
    "files",
    r#"[server]
name = "files"
transport = "remote"
url = "https://api.example.test:8443/mcp"
auth = "header"
header = "X-Api-Key"
secret = "team-key"

[permissions]
network = ["files.example.test"]
"#,
  );

  let wants = json!({
    "t": "wants",
    "network": ["files.example.test", "api.example.test:8443"],
    "secrets": ["team-key"],
  });

  assert_eq!(machine.place(json!({ "tool": "files" }), &[]), std::slice::from_ref(&wants));
  assert_eq!(
    machine.place(
      json!({ "tool": "files", "network": ["files.example.test", "api.example.test:8443"] }),
      &[]
    ),
    [wants]
  );
  assert!(machine.standing("files").is_empty(), "{:?}", machine.standing("files"));
}

// "a listing that fails, or lists nothing, refuses the place, and the version staged is taken away"
#[test]
fn an_mcp_server_whose_listing_fails_or_is_empty_is_refused_and_nothing_is_left() {
  let machine = Machine::new(false);

  machine.server(
    "broken",
    "[server]\nname = \"broken\"\ntransport = \"local\"\ncommand = [\"python3\", \"-c\", \"import sys; sys.exit(3)\"]\n",
  );
  machine.server(
    "empty",
    "[server]\nname = \"empty\"\ntransport = \"local\"\ncommand = [\"python3\", \"server.py\", \"empty\"]\n\n[permissions]\nsecrets = [\"team-key\"]\n",
  );

  let broken = machine.place(json!({ "tool": "broken" }), &[]);
  let refused = broken.last().unwrap();

  assert_eq!(refused["t"], "refused", "{broken:?}");
  assert!(refused["why"].as_str().unwrap().contains("could not be listed"), "{refused}");
  assert!(machine.standing("broken").is_empty(), "{:?}", machine.standing("broken"));

  let empty = machine.place(json!({ "tool": "empty", "secrets": { "team-key": team_key() } }), &[]);
  let refused = empty.last().unwrap();

  assert_eq!(refused["t"], "refused", "{empty:?}");
  assert!(refused["why"].as_str().unwrap().contains("lists no tool"), "{refused}");
  assert!(machine.standing("empty").is_empty(), "{:?}", machine.standing("empty"));
}

// A stand-in remote MCP server over streamable HTTP on this machine's loopback, answering each message's POST with
// JSON and keeping the header each request carried its key in; it stands until the test ends.
fn remote_stand_in(header: &'static str) -> (u16, Arc<Mutex<Vec<String>>>) {
  use axum::http::{HeaderMap, StatusCode};
  use axum::response::IntoResponse;

  let seen = Arc::new(Mutex::new(Vec::new()));
  let keeping = seen.clone();
  let (ready, port) = std::sync::mpsc::channel();

  std::thread::spawn(move || {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

    runtime.block_on(async move {
      let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
      let answer = move |headers: HeaderMap, axum::Json(message): axum::Json<Value>| {
        let keeping = keeping.clone();

        async move {
          if let Some(key) = headers.get(header).and_then(|value| value.to_str().ok()) {
            keeping.lock().unwrap().push(key.to_owned());
          }

          let Some(id) = message.get("id").cloned() else {
            return StatusCode::ACCEPTED.into_response();
          };
          let params = &message["params"];
          let result = match message["method"].as_str().unwrap_or_default() {
            "initialize" => json!({
              "protocolVersion": params["protocolVersion"],
              "capabilities": { "tools": {} },
              "serverInfo": { "name": "remote stand-in", "version": "1" },
            }),
            "tools/list" => json!({ "tools": [{
              "name": "fetchFile",
              "description": "Fetches a file.",
              "inputSchema": { "type": "object", "properties": { "name": { "type": "string" } } },
            }] }),
            _ => json!({
              "content": [{ "type": "text", "text": format!("fetched {}", params["arguments"]["name"].as_str().unwrap_or_default()) }],
              "isError": false,
            }),
          };

          axum::Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
        }
      };
      let app = axum::Router::new().route(
        "/mcp",
        axum::routing::post(answer)
          .get(|| async { StatusCode::METHOD_NOT_ALLOWED })
          .delete(|| async { StatusCode::METHOD_NOT_ALLOWED }),
      );

      ready.send(listener.local_addr().unwrap().port()).unwrap();
      axum::serve(listener, app).await.unwrap();
    });
  });

  (port.recv_timeout(Duration::from_secs(10)).unwrap(), seen)
}

// `node-link mcp` run in `folder` as the confinement runs it, with only the secrets' folder named, the input given;
// its output and its exit code, waited on for a bounded span.
fn adapter(folder: &Path, secrets: &Path, list: bool, input: &str) -> (String, i32) {
  let mut command = Command::new(crate::support::binary::path("node-link"));

  command.arg("mcp");

  if list {
    command.arg("--list");
  }

  command
    .current_dir(folder)
    .env_clear()
    .env("PATH", "/usr/local/bin:/usr/bin:/bin")
    .env("BMO_SECRETS", secrets)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

  let mut child = Started(command.spawn().expect("node-link mcp"));

  child.0.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();

  let mut stdout = child.0.stdout.take().unwrap();
  let reading = std::thread::spawn(move || {
    let mut out = String::new();
    let _ = stdout.read_to_string(&mut out);
    out
  });
  let started = Instant::now();

  let code = loop {
    if let Some(ended) = child.0.try_wait().unwrap() {
      break ended.code().unwrap_or(-1);
    }

    assert!(started.elapsed() < Duration::from_secs(30), "node-link mcp never ended");
    std::thread::sleep(Duration::from_millis(50));
  };

  (reading.join().unwrap(), code)
}

// "a remote server is reached over streamable HTTP at its url, its secret sent as `Authorization: Bearer`, read
// from the call's secrets; its tools listed and its tool called" — outside the confinement, which a loopback stand-in
// is reached through only where bubblewrap makes namespaces.
#[test]
fn the_adapter_reaches_a_remote_server_with_its_secret_in_its_header() {
  let (port, seen) = remote_stand_in("authorization");
  let dir = tempfile::tempdir().unwrap();
  let (folder, secrets) = (dir.path().join("tool"), dir.path().join("secrets"));

  fs::create_dir_all(&folder).unwrap();
  fs::create_dir_all(&secrets).unwrap();
  fs::write(secrets.join("team-key"), "stand-in value\n").unwrap();
  fs::write(
    folder.join("mcp.toml"),
    format!(
      "[server]\nname = \"files\"\ntransport = \"remote\"\nurl = \"http://127.0.0.1:{port}/mcp\"\nauth = \"bearer\"\nsecret = \"team-key\"\n"
    ),
  )
  .unwrap();

  let (listed, code) = adapter(&folder, &secrets, true, "");

  assert_eq!(code, 0, "{listed}");

  let listed: Value = serde_json::from_str(&listed).unwrap();

  assert_eq!(listed["tools"][0]["name"], "fetchFile", "{listed}");

  fs::write(folder.join("mcp-tools.json"), r#"{ "fetch_file": "fetchFile" }"#).unwrap();

  let call =
    json!({ "protocol": 1, "kind": "call", "entry": "my/fetch_file", "arguments": { "name": "notes.md" } });
  let (out, code) = adapter(&folder, &secrets, false, &call.to_string());

  assert_eq!(code, 0, "{out}");
  assert_eq!(
    serde_json::from_str::<Value>(&out).unwrap(),
    json!({ "content": [{ "type": "text", "text": "fetched notes.md" }], "isError": false })
  );
  assert!(!seen.lock().unwrap().is_empty());
  assert!(seen.lock().unwrap().iter().all(|key| key == "Bearer stand-in value"), "{seen:?}");

  // An entry the version has no tool for is a result with `isError`, as is a server that cannot be reached.
  let unknown = json!({ "protocol": 1, "kind": "call", "entry": "my/other", "arguments": {} });
  let (out, code) = adapter(&folder, &secrets, false, &unknown.to_string());

  assert_eq!(code, 0, "{out}");
  assert_eq!(serde_json::from_str::<Value>(&out).unwrap()["isError"], true, "{out}");
}

// Why bubblewrap makes no namespace here, where it does not: the setup's own try (docs/person-nodes.md, 「What the
// PC needs」).
fn bwrap_confines() -> Result<(), String> {
  let tried = Command::new("sh")
    .args(["-c", agency_server::verbs::work_node_setup::CONFINEMENT_TRY])
    .stdin(Stdio::null())
    .output()
    .map_err(|why| why.to_string())?;

  if tried.status.success() {
    Ok(())
  } else {
    Err(String::from_utf8_lossy(&tried.stderr).trim().to_owned())
  }
}

// "where bubblewrap makes namespaces here, a local server is listed and called confined, and a remote one is reached
// through the call's proxy alone, its secret in its header"
#[test]
fn confined_mcp_servers_are_listed_and_called_local_and_remote() {
  if let Err(why) = bwrap_confines() {
    eprintln!(
      "skipped: bubblewrap makes no namespace on this machine, so no server can be confined here: {why}"
    );
    return;
  }

  let machine = Machine::new(true);

  local_server_placed_and_called(&machine);

  let (port, seen) = remote_stand_in("x-api-key");
  let address = format!("api.example.test:{port}");
  let resolve = format!("{}=127.0.0.1", "api.example.test");

  machine.server(
    "files",
    &format!(
      "[server]\nname = \"files\"\ntransport = \"remote\"\nurl = \"http://{address}/mcp\"\nauth = \"header\"\nheader = \"X-Api-Key\"\nsecret = \"team-key\"\n"
    ),
  );

  let carried = json!({ "network": [address], "secrets": { "team-key": team_key() } });
  let mut place = carried.clone();

  place["tool"] = json!("files");

  let answered = machine.place(place, &[(node_link::proxy::RESOLVE_VARIABLE, &resolve)]);
  let placed = answered.last().unwrap();

  assert_eq!(placed["t"], "placed", "{answered:?}");

  let manifest: toml::Table = placed["files"]["tool.toml"].as_str().unwrap().parse().unwrap();

  assert_eq!(
    manifest["permissions"]["network"].as_array().unwrap(),
    &[toml::Value::from(address.clone())]
  );
  assert_eq!(manifest["permissions"]["secrets"].as_array().unwrap(), &[toml::Value::from("team-key")]);

  let mut call = carried;

  call["resolve"] = json!(resolve);

  let version = placed["version"].as_str().unwrap();
  let fetched = machine.call("files", version, "fetch_file", json!({ "name": "notes.md" }), call);

  assert_eq!(
    fetched,
    json!({ "content": [{ "type": "text", "text": "fetched notes.md" }], "isError": false })
  );
  assert!(seen.lock().unwrap().iter().all(|key| key == "stand-in value"), "{seen:?}");
  assert!(!seen.lock().unwrap().is_empty());
  // The call's proxy folders went with the listing and the call.
  assert_eq!(fs::read_dir(machine.scratch()).unwrap().count(), 0);
}
