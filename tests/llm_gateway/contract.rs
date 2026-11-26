//! The model gateway's wire contract (docs/own-llm.md, 「The gateway」), asked of the command under test
//! as a run asks it: the gateway started as a process on a free port over a scratch data root, its
//! keys and a scratch store holding the registry, with a stand-in for LiteLLM first on its `PATH` that
//! answers as a child does and writes down how it was started, what it was asked and when it was
//! stopped; and once with LiteLLM itself, as installed for the account, in front of a backend of the
//! test's own. The command is the crate's binary, or the one `LLM_GATEWAY_CONTRACT_COMMAND` names.

use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgConnection;
use sqlx::{Connection, Executor, Row};
use tokio::net::TcpStream;

use crate::support::scratch::Scratch;

const GATEWAY_KEY: &str = "gateway-key-0123456789";

// LiteLLM, standing in: started as the gateway starts it, it writes down its table, port, arguments
// and environment; answers its liveliness path; answers a request for chat or vectors naming a model its
// table holds no entry for 400, as LiteLLM does; streams a chat request carrying `x-standin-stream` back as
// events, `x-standin-count` of them `x-standin-gap` milliseconds apart, ending the stream where it names
// `whole` and holding it open with nothing more where it names `stall`, and writes down its caller going;
// echoes every other request, the model list's and the model information routes' with each name its table
// holds as a model of the list's `data`, named as LiteLLM names it there; holds `/held` open until the test lets it go; ends at `/die`; and writes down its stop. A
// `fail-start` file beside it makes it end before it answers.
const STANDIN: &str = r#"
import { appendFileSync, existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
const dir = dirname(import.meta.dir);
const args = process.argv.slice(2);
const at = (flag) => args[args.indexOf(flag) + 1];
const config = at("--config");
const table = /config-(\w+)\.yaml$/.exec(config)[1];
const models = JSON.parse(readFileSync(config, "utf8")).model_list.map((entry) => entry.model_name);
const log = (entry) => appendFileSync(join(dir, "children.log"), JSON.stringify({ table, ...entry }) + "\n");
if (existsSync(join(dir, "fail-start"))) {
  log({ event: "failed" });
  process.exit(3);
}
log({ event: "started", args, cwd: process.cwd(), env: process.env });
process.on("SIGTERM", () => {
  log({ event: "stopped" });
  process.exit(0);
});
Bun.serve({
  hostname: "127.0.0.1",
  port: Number(at("--port")),
  idleTimeout: 0,
  async fetch(request) {
    const url = new URL(request.url);
    if (url.pathname === "/health/liveliness") return new Response("I'm alive!");
    if (url.pathname === "/die") process.exit(1);
    if (url.pathname === "/held") {
      log({ event: "holding" });
      return new Response(new ReadableStream({
        async start(controller) {
          controller.enqueue(new TextEncoder().encode(`${table}-first;`));
          while (!existsSync(join(dir, `release-${table}`))) await Bun.sleep(20);
          controller.enqueue(new TextEncoder().encode(`${table}-second`));
          controller.close();
        },
      }));
    }
    const body = await request.text();
    if (request.method === "POST" && ["/v1/chat/completions", "/chat/completions", "/v1/embeddings", "/embeddings", "/v1/messages"].includes(url.pathname)) {
      let named;
      try { named = JSON.parse(body).model; } catch {}
      if (typeof named === "string" && !models.includes(named)) {
        return Response.json(
          { error: { message: `Invalid model name passed in model=${named}` } },
          { status: 400, headers: { "x-standin": table } },
        );
      }
    }
    // A Messages answer whose one text block writes four bytes at a time, held open after its first delta until
    // the release file stands.
    if (request.headers.get("x-standin-messages")) {
      const event = (data) => new TextEncoder().encode(`event: ${data.type}\ndata: ${JSON.stringify(data)}\n\n`);
      return new Response(new ReadableStream({
        async start(controller) {
          controller.enqueue(event({ type: "message_start", message: { id: "m-1" } }));
          controller.enqueue(event({ type: "content_block_start", index: 0, content_block: { type: "text", text: "" } }));
          controller.enqueue(event({ type: "content_block_delta", index: 0, delta: { type: "text_delta", text: "abcd" } }));
          controller.enqueue(event({ type: "content_block_delta", index: 0, delta: { type: "text_delta", text: "efgh" } }));
          while (!existsSync(join(dir, "release-messages"))) await Bun.sleep(20);
          controller.enqueue(event({ type: "content_block_stop", index: 0 }));
          controller.enqueue(event({ type: "message_stop" }));
          controller.close();
        },
      }), { headers: { "content-type": "text/event-stream; charset=utf-8", "x-standin": table } });
    }
    const stream = request.headers.get("x-standin-stream");
    if (stream) {
      const gap = Number(request.headers.get("x-standin-gap") ?? 0);
      const count = Number(request.headers.get("x-standin-count") ?? 0);
      let ended = false;
      const gone = () => {
        if (!ended) {
          ended = true;
          log({ event: "cancelled", stream });
        }
      };
      request.signal.addEventListener("abort", gone);
      return new Response(new ReadableStream({
        async start(controller) {
          for (let at = 0; at < count; at++) {
            if (at > 0) await Bun.sleep(gap);
            controller.enqueue(new TextEncoder().encode(`data: ${at}\n\n`));
          }
          if (stream === "whole") {
            ended = true;
            controller.close();
          }
        },
        cancel: gone,
      }), { headers: { "content-type": "text/event-stream; charset=utf-8", "x-standin": table } });
    }
    const named = { "/v1/models": "id", "/model/info": "model_name", "/model_group/info": "model_group" }[url.pathname];
    const listed = named ? { data: [...new Set(models)].map((name) => ({ [named]: name })) } : {};
    return Response.json(
      { table, method: request.method, path: url.pathname, search: url.search, headers: Object.fromEntries(request.headers), body, ...listed },
      { status: Number(request.headers.get("x-standin-status") ?? 200), headers: { "x-standin": table, "content-encoding": "identity" } },
    );
  },
});
"#;

// A child, standing in, whose every chat request is answered `200` with a stream's headers and then
// nothing, its connection held open; it answers its liveliness path, and writes down its caller going.
const SILENT: &str = r#"
import json, os, select, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

args = sys.argv[1:]
port = int(args[args.index("--port") + 1])
log_path = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "children.log")

def log(entry):
    with open(log_path, "a") as log_file:
        log_file.write(json.dumps(entry) + "\n")

class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):
        pass

    def do_GET(self):
        body = b"I'm alive!"
        self.send_response(200)
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        self.rfile.read(int(self.headers.get("content-length", 0)))
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.send_header("transfer-encoding", "chunked")
        self.end_headers()
        self.wfile.flush()
        while True:
            readable, _, _ = select.select([self.connection], [], [], 60)
            if readable and not self.connection.recv(1):
                log({"event": "cancelled", "stream": "silent"})
                return

log({"event": "started"})
ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
"#;

// A registration served by two nodes.
const PAIR: &str = "
  INSERT INTO registered_models VALUES ('pair-0-1', 'Pair 0.1', 'pair-model', 32768, 4096, 0, 0, 1, 0);
  INSERT INTO registered_model_addresses VALUES ('pair-0-1', 'alpha', 1, 'http://127.0.0.1:18021/v1/chat/completions', 'alpha-llm.token', 0, NULL);
  INSERT INTO registered_model_addresses VALUES ('pair-0-1', 'bravo', 2, 'http://127.0.0.1:18023/v1/chat/completions', NULL, 0, NULL);
";

// A registration served by one address.
const SINGLE: &str = "
  INSERT INTO registered_models VALUES ('single', 'Single', 'single-model', 65536, 8192, 0, 0, 2, 0);
  INSERT INTO registered_model_addresses VALUES ('single', 'd-vex', 1, 'https://llm.example.com/v1/chat/completions', NULL, 0, NULL);
";

fn private(path: &Path, text: &str, mode: u32) {
  std::fs::create_dir_all(path.parent().unwrap()).unwrap();
  std::fs::write(path, text).unwrap();
  std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

fn command() -> Command {
  match std::env::var("LLM_GATEWAY_CONTRACT_COMMAND") {
    Ok(named) if !named.trim().is_empty() => {
      let mut words = named.split_whitespace();
      let mut command = Command::new(words.next().unwrap());
      command.args(words);
      command
    }
    _ => Command::new(crate::support::binary::path("llm-gateway")),
  }
}

struct World {
  dir: tempfile::TempDir,
  store: Scratch,
  gateway: Option<Child>,
  port: u16,
  /// The arbiter's master the gateway is pointed at, where the test stands one.
  master: Option<String>,
  /// The stall span the gateway is told in place of its policy's, in milliseconds, where the test names one.
  stall_ms: Option<u64>,
  /// Whether the gateway is started through the world's `live` link, as its unit starts it through the
  /// deploy link, and told no code root.
  linked: bool,
  /// How often the gateway reads the store, in milliseconds.
  beat_ms: u64,
}

impl World {
  /// A scratch world: the gateway's key and a node's key in place, the stand-in first on the `PATH`,
  /// and a migrated store holding the registrations given.
  async fn new(registry: &str) -> World {
    World::over(Scratch::new().await, registry).await
  }

  async fn over(store: Scratch, registry: &str) -> World {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    private(&root.join("data/env/llm-gateway.key"), &format!("{GATEWAY_KEY}\n"), 0o600);
    private(&root.join("data/env/alpha-llm.token"), "alpha-key\n", 0o600);
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::write(root.join("bin/standin.ts"), STANDIN).unwrap();
    let wrapper = format!("#!/bin/sh\nexec bun {} \"$@\"\n", root.join("bin/standin.ts").display());
    private(&root.join("bin/litellm"), &wrapper, 0o755);
    std::fs::write(root.join("children.log"), "").unwrap();
    let world = World {
      dir,
      store,
      gateway: None,
      port: 0,
      master: None,
      stall_ms: None,
      linked: false,
      beat_ms: 100,
    };
    world.write(registry).await;
    world
  }

  /// The world with LiteLLM itself in place of the stand-in, found as the gateway's unit finds it.
  fn on_litellm(self) -> World {
    std::fs::remove_file(self.root().join("bin/litellm")).unwrap();
    self
  }

  /// The world with a child that answers every chat request with a stream's headers and nothing after
  /// them, which Bun cannot stand in for: it holds a stream's headers back until its first byte.
  fn with_silent_child(self) -> World {
    std::fs::write(self.root().join("bin/silent.py"), SILENT).unwrap();
    let wrapper =
      format!("#!/bin/sh\nexec python3 -I {} \"$@\"\n", self.root().join("bin/silent.py").display());
    private(&self.root().join("bin/litellm"), &wrapper, 0o755);
    self
  }

  /// The world with a gateway that cuts a streamed answer silent for `ms` milliseconds.
  fn stalling_after(mut self, ms: u64) -> World {
    self.stall_ms = Some(ms);
    self
  }

  /// The world whose gateway reads the store once as it starts and not again within any test's span, so
  /// what is written after that reading reaches it only by a lookup of its own.
  fn reading_the_store_once(mut self) -> World {
    self.beat_ms = 3_600_000;
    self
  }

  /// The world whose gateway serves the members given, as the deployment's members file names them.
  fn with_members(self, members: Value) -> World {
    private(&self.folder().join("members.json"), &members.to_string(), 0o600);
    self
  }

  /// The world whose gateway is started through its `live` link, which stands on the tree named.
  fn started_through_the_link_at(mut self, tree: &str) -> World {
    self.linked = true;
    self.point_live_at(tree);
    self
  }

  /// A tree as a release lays it: the launcher under `bin/` and the repository's hooks, with the role
  /// hook ending in the line given, so two trees' hooks differ.
  fn lay_tree(&self, tree: &str, last_line: &str) {
    let at = self.root().join("trees").join(tree);
    let hooks = at.join("crates/llm-gateway/hooks");
    std::fs::create_dir_all(at.join("bin")).unwrap();
    std::fs::create_dir_all(&hooks).unwrap();
    std::os::unix::fs::symlink(crate::support::binary::path("llm-gateway"), at.join("bin/llm-gateway"))
      .unwrap();
    let own = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/llm-gateway/hooks");
    for hook in std::fs::read_dir(own).unwrap() {
      let hook = hook.unwrap();
      if hook.path().extension().is_some_and(|extension| extension == "py") {
        std::fs::copy(hook.path(), hooks.join(hook.file_name())).unwrap();
      }
    }
    let role = hooks.join("roles.py");
    let text = std::fs::read_to_string(&role).unwrap();
    std::fs::write(role, format!("{text}\n{last_line}\n")).unwrap();
  }

  /// The `live` link put on the tree named, in one step as a deployment flips its link.
  fn point_live_at(&self, tree: &str) {
    let next = self.root().join("live.next");
    std::os::unix::fs::symlink(self.root().join("trees").join(tree), &next).unwrap();
    std::fs::rename(next, self.root().join("live")).unwrap();
  }

  fn root(&self) -> &Path {
    self.dir.path()
  }

  fn folder(&self) -> PathBuf {
    self.root().join("data/llm-gateway")
  }

  async fn write(&self, statements: &str) {
    if statements.trim().is_empty() {
      return;
    }
    let mut store = PgConnection::connect(&self.store.address()).await.unwrap();
    store.execute(statements).await.unwrap();
    store.close().await.unwrap();
  }

  fn spawn(&self) -> Child {
    // The account's own programs come after the stand-in, as on the unit's path.
    let path = format!(
      "{}:{}/.local/bin:{}",
      self.root().join("bin").display(),
      std::env::var("HOME").unwrap_or_default(),
      std::env::var("PATH").unwrap_or_default()
    );
    let log = std::fs::File::create(self.root().join("gateway.log")).unwrap();
    let mut command = if self.linked {
      Command::new(self.root().join("live/bin/llm-gateway"))
    } else {
      let mut command = command();
      command.env("BMO_CODE_ROOT", env!("CARGO_MANIFEST_DIR"));
      command
    };
    // No master but the test's own: the worker's environment names none to the gateway.
    command.env_remove("AGENCY_ARBITER_URL").env_remove("AGENCY_ARBITER_TOKEN_FILE");
    if let Some(master) = &self.master {
      command.env("AGENCY_ARBITER_URL", master);
    }
    command.env_remove("LLM_GATEWAY_STALL_MS").env_remove("BMO_LLM_GATEWAY_MEMBERS");
    if let Some(ms) = self.stall_ms {
      command.env("LLM_GATEWAY_STALL_MS", ms.to_string());
    }
    command
      .args(["--port", &self.port.to_string()])
      .env("PATH", path)
      .env("BMO_DATA_DIR", self.root().join("data"))
      .env("BMO_ARCHIVE_URL", self.store.address())
      .env("LLM_GATEWAY_REFRESH_MS", self.beat_ms.to_string())
      .stdin(Stdio::null())
      .stdout(log.try_clone().unwrap())
      .stderr(log)
      .process_group(0)
      .spawn()
      .expect("the gateway starts")
  }

  /// The gateway started on a port of its own choosing, read off the line it names it in, so no
  /// other process can take the port between its choosing and its binding.
  async fn start(mut self) -> World {
    self.gateway = Some(self.spawn());
    let ready = Instant::now() + Duration::from_secs(20);
    self.port = loop {
      let heard = self.said().lines().find_map(|line| {
        line
          .strip_prefix("llm-gateway: serving on http://127.0.0.1:")?
          .trim()
          .parse::<u16>()
          .ok()
      });
      if let Some(port) = heard {
        break port;
      }
      assert!(Instant::now() < ready, "the gateway never listened; it said {}", self.said());
      tokio::time::sleep(Duration::from_millis(20)).await;
    };
    self
  }

  fn said(&self) -> String {
    std::fs::read_to_string(self.root().join("gateway.log")).unwrap_or_default()
  }

  /// What the stand-in children wrote down, in order.
  fn children(&self) -> Vec<Value> {
    std::fs::read_to_string(self.root().join("children.log"))
      .unwrap_or_default()
      .lines()
      .map(|line| serde_json::from_str(line).unwrap())
      .collect()
  }

  fn events(&self, event: &str) -> Vec<Value> {
    self.children().into_iter().filter(|entry| entry["event"] == event).collect()
  }

  fn status(&self) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(self.folder().join("status.json")).ok()?).ok()
  }

  /// The table the gateway serves once it serves one other than `not`.
  async fn serving(&self, not: Option<&str>) -> String {
    self.serving_within(not, Duration::from_secs(30)).await
  }

  async fn serving_within(&self, not: Option<&str>, bound: Duration) -> String {
    let mut seen = None;
    until_within(&format!("a table other than {not:?} to be served"), bound, || {
      seen = self.status().and_then(|status| status["serving"].as_str().map(str::to_owned));
      seen.is_some() && seen.as_deref() != not
    })
    .await;
    seen.unwrap()
  }

  /// The table the gateway serves once it serves one holding an entry under `name`. A store migrated
  /// while the gateway runs is read as an empty registry first, so the table holding the registrations
  /// written after it follows that one.
  async fn serving_holding(&self, name: &str) -> String {
    let mut seen = None;
    until_within(&format!("a table holding {name} to be served"), Duration::from_secs(30), || {
      seen = self.status().and_then(|status| status["serving"].as_str().map(str::to_owned));
      seen
        .as_deref()
        .is_some_and(|table| self.table_names(table).iter().any(|held| held == name))
    })
    .await;
    seen.unwrap()
  }

  /// The model names `table` holds entries under, in its order; none where its file is not written.
  fn table_names(&self, table: &str) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(self.folder().join(format!("config-{table}.yaml"))) else {
      return Vec::new();
    };
    let config: Value = serde_json::from_str(&text).unwrap();
    config["model_list"]
      .as_array()
      .unwrap()
      .iter()
      .map(|entry| entry["model_name"].as_str().unwrap().to_owned())
      .collect()
  }

  /// A request as a caller of the gateway sends it: with the gateway's key as its Bearer unless the
  /// headers name an authorization of their own.
  async fn open(
    &self,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
  ) -> hyper::Response<Incoming> {
    let keyed = format!("Bearer {GATEWAY_KEY}");
    let mut headers = headers.to_vec();
    if !headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("authorization")) {
      headers.push(("authorization", &keyed));
    }
    self.open_bare(method, path, &headers, body).await
  }

  /// A request carrying the headers named and nothing more.
  async fn open_bare(
    &self,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
  ) -> hyper::Response<Incoming> {
    let stream = TcpStream::connect(("127.0.0.1", self.port)).await.unwrap();
    let (mut sender, connection) =
      hyper::client::conn::http1::handshake(TokioIo::new(stream)).await.unwrap();
    tokio::spawn(connection);
    let mut request = hyper::Request::builder().method(method).uri(path).header("host", "127.0.0.1:18030");
    for (name, value) in headers {
      request = request.header(*name, *value);
    }
    sender
      .send_request(request.body(Full::new(Bytes::from(body.to_owned()))).unwrap())
      .await
      .unwrap()
  }

  async fn ask(
    &self,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
  ) -> (u16, hyper::HeaderMap, String) {
    read_whole(self.open(method, path, headers, body).await).await
  }

  /// Which child answered a plain request.
  async fn answered_by(&self) -> String {
    let (status, headers, body) = self.ask("GET", "/v1/models", &[], "").await;
    assert_eq!(status, 200, "{body}");
    headers["x-standin"].to_str().unwrap().to_owned()
  }

  /// The request records the store holds, in the order they were written.
  async fn recorded(&self) -> Vec<Value> {
    let mut store = PgConnection::connect(&self.store.address()).await.unwrap();
    let rows = sqlx::query(
      "SELECT account, model, run, session, input_tokens, output_tokens, cost_micros FROM gateway_requests \
       ORDER BY id",
    )
    .fetch_all(&mut store)
    .await
    .unwrap();
    store.close().await.unwrap();
    rows
      .iter()
      .map(|row| {
        json!({
          "account": row.get::<Option<i64>, _>("account"),
          "model": row.get::<String, _>("model"),
          "run": row.get::<Option<String>, _>("run"),
          "session": row.get::<Option<String>, _>("session"),
          "inputTokens": row.get::<Option<i64>, _>("input_tokens"),
          "outputTokens": row.get::<Option<i64>, _>("output_tokens"),
          "costMicros": row.get::<i64, _>("cost_micros"),
        })
      })
      .collect()
  }

  /// The request records once the store holds `count` of them.
  async fn recorded_once(&self, count: usize) -> Vec<Value> {
    let bound = Instant::now() + Duration::from_secs(30);
    loop {
      let recorded = self.recorded().await;
      if recorded.len() >= count {
        return recorded;
      }
      assert!(
        Instant::now() < bound,
        "waited in vain for {count} request records: {recorded:?}\n{}",
        self.said()
      );
      tokio::time::sleep(Duration::from_millis(20)).await;
    }
  }

  fn terminate(&self) {
    unsafe {
      libc::kill(self.gateway.as_ref().unwrap().id() as i32, libc::SIGTERM);
    }
  }
}

async fn read_whole(answer: hyper::Response<Incoming>) -> (u16, hyper::HeaderMap, String) {
  let (status, headers) = (answer.status().as_u16(), answer.headers().clone());
  let body = answer.into_body().collect().await.unwrap().to_bytes();
  (status, headers, String::from_utf8_lossy(&body).into_owned())
}

impl Drop for World {
  fn drop(&mut self) {
    // The gateway and every child it started, as one process group.
    if let Some(gateway) = &mut self.gateway {
      unsafe {
        libc::kill(-(gateway.id() as i32), libc::SIGKILL);
      }
      let _ = gateway.wait();
    }
  }
}

async fn until(what: &str, holds: impl FnMut() -> bool) {
  until_within(what, Duration::from_secs(30), holds).await
}

async fn until_within(what: &str, bound: Duration, mut holds: impl FnMut() -> bool) {
  let bound = Instant::now() + bound;
  while !holds() {
    assert!(Instant::now() < bound, "waited in vain for {what}");
    tokio::time::sleep(Duration::from_millis(20)).await;
  }
}

#[tokio::test]
async fn without_its_own_key_the_gateway_does_not_stand() {
  for mode in [None, Some(0o644)] {
    let world = World::new(PAIR).await;
    let key = world.root().join("data/env/llm-gateway.key");
    match mode {
      None => std::fs::remove_file(&key).unwrap(),
      Some(mode) => std::fs::set_permissions(&key, std::fs::Permissions::from_mode(mode)).unwrap(),
    }
    let mut gateway = world.spawn();
    let bound = Instant::now() + Duration::from_secs(20);
    let ended = loop {
      if let Some(ended) = gateway.try_wait().unwrap() {
        break ended;
      }
      assert!(Instant::now() < bound, "the gateway stood without its key");
      tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(ended.code(), Some(1));
    assert!(world.said().contains("llm-gateway.key"), "{}", world.said());
    assert!(world.children().is_empty());
  }
}

// "the open block of a streamed answer whose request names its program's session is read behind the key while it
// is written, and let go of once it ends"
#[tokio::test]
async fn a_streamed_answer_s_open_block_is_read_while_it_is_written() {
  let world = World::new(PAIR).await.start().await;
  world.serving(None).await;
  let headers = [("x-standin-messages", "1"), ("x-claude-code-session-id", "session-a")];
  let answer = world.open("POST", "/v1/messages", &headers, "{}").await;
  let reading = tokio::spawn(read_whole(answer));
  let open = read_until(&world, "/gateway/streams/session-a", |(status, body)| {
    *status == 200 && body.contains(r#""bytes":8"#)
  })
  .await;

  assert_eq!(
    serde_json::from_str::<Value>(&open.1).unwrap(),
    json!({ "index": 0, "kind": "text", "bytes": 8 })
  );

  let unkeyed = world.open_bare("GET", "/gateway/streams/session-a", &[], "").await;
  assert_eq!(unkeyed.status().as_u16(), 401, "the reading stands behind the key");

  std::fs::write(world.root().join("release-messages"), "").unwrap();
  let (status, _, body) = reading.await.unwrap();
  assert_eq!(status, 200);
  assert!(body.contains("message_stop"), "the answer passed whole: {body}");

  read_until(&world, "/gateway/streams/session-a", |(status, _)| *status == 404).await;
}

// A keyed GET of the gateway asked again until its answer holds, within the wait every check here keeps.
async fn read_until(world: &World, path: &str, holds: impl Fn(&(u16, String)) -> bool) -> (u16, String) {
  let bound = Instant::now() + Duration::from_secs(30);

  loop {
    let (status, _, body) = world.ask("GET", path, &[], "").await;
    let read = (status, body);

    if holds(&read) {
      return read;
    }

    assert!(Instant::now() < bound, "waited in vain on {path}: {read:?}");
    tokio::time::sleep(Duration::from_millis(20)).await;
  }
}

#[tokio::test]
async fn before_any_table_it_answers_503_and_serves_the_registry_once_it_stands() {
  let world = World::over(Scratch::blank().await, "").await.start().await;

  let (status, _, body) = world.ask("GET", "/v1/models", &[], "").await;
  assert_eq!((status, body.as_str()), (503, "the gateway has no table to serve yet"));

  store::migration::up(world.store.db()).await.unwrap();
  world.write(PAIR).await;
  let table = world.serving_holding("pair-0-1").await;
  assert_eq!(world.answered_by().await, table);
}

#[tokio::test]
async fn a_request_and_its_answer_pass_as_they_stand() {
  let world = World::new(PAIR).await.start().await;
  let table = world.serving(None).await;

  let (status, headers, body) = world
    .ask(
      "POST",
      "/v1/messages?beta=true",
      &[
        ("authorization", &format!("Bearer {GATEWAY_KEY}")),
        ("x-litellm-timeout", "600"),
        ("accept-encoding", "gzip"),
        ("keep-alive", "timeout=5"),
        ("x-standin-status", "201"),
      ],
      r#"{"model":"pair-0-1"}"#,
    )
    .await;
  assert_eq!(status, 201);
  assert_eq!(headers["x-standin"], table.as_str());
  assert!(headers.get("content-encoding").is_none());
  let asked: Value = serde_json::from_str(&body).unwrap();
  assert_eq!(
    (&asked["method"], &asked["path"], &asked["search"], &asked["body"]),
    (
      &json!("POST"),
      &json!("/v1/messages"),
      &json!("?beta=true"),
      &json!(r#"{"model":"pair-0-1"}"#)
    )
  );
  assert_eq!(asked["headers"]["authorization"], format!("Bearer {GATEWAY_KEY}"));
  assert_eq!(asked["headers"]["x-litellm-timeout"], "600");
  assert_eq!(asked["headers"]["accept-encoding"], "identity");
  assert!(asked["headers"].get("keep-alive").is_none());

  // A child's own failure comes back as it came.
  let (status, _, _) = world.ask("GET", "/v1/models", &[("x-standin-status", "500")], "").await;
  assert_eq!(status, 500);
}

#[tokio::test]
async fn the_table_is_written_private_beside_its_hooks_and_the_keys_reach_the_child_alone() {
  let world = World::new(PAIR).await;
  private(&world.folder().join("config-0000000000000000.yaml"), "left over", 0o600);
  let world = world.start().await;
  let table = world.serving(None).await;

  assert!(!world.folder().join("config-0000000000000000.yaml").exists());
  let written = world.folder().join(format!("config-{table}.yaml"));
  assert_eq!(std::fs::metadata(&written).unwrap().permissions().mode() & 0o777, 0o600);
  assert_eq!(world.table_names(&table), ["pair-0-1", "pair-0-1@alpha", "pair-0-1", "pair-0-1@bravo"]);
  let config: Value = serde_json::from_str(&std::fs::read_to_string(&written).unwrap()).unwrap();
  assert_eq!(
    config["model_list"][0]["litellm_params"]["api_key"],
    "os.environ/BMO_KEY_ALPHA_LLM_TOKEN"
  );
  for hook in ["thinking.py", "sampling.py", "reminders.py", "long_tools.py", "roles.py", "costs.py"] {
    assert!(world.folder().join(hook).exists(), "{hook} was not copied beside the table");
  }

  let started = world.events("started");
  assert_eq!(started.len(), 1);
  let child = &started[0];
  let port = world.status().unwrap();
  assert_eq!(port["starting"], Value::Null);
  assert_eq!(port["draining"], 0);
  assert_eq!(
    std::fs::metadata(world.folder().join("status.json")).unwrap().permissions().mode() & 0o777,
    0o600
  );
  let args: Vec<&str> = child["args"].as_array().unwrap().iter().map(|arg| arg.as_str().unwrap()).collect();
  assert_eq!(args[..2], ["--config", written.to_str().unwrap()]);
  assert_eq!(args[2..4], ["--host", "127.0.0.1"]);
  assert_eq!(args[6..], ["--num_workers", "1"]);
  assert_eq!(child["cwd"], world.folder().to_str().unwrap());
  assert_eq!(child["env"]["LLM_GATEWAY_KEY"], GATEWAY_KEY);
  assert_eq!(child["env"]["BMO_KEY_ALPHA_LLM_TOKEN"], "alpha-key");
  assert_eq!(child["env"]["LLM_GATEWAY_FRONT"], format!("http://127.0.0.1:{}", world.port));
  for kept in ["BMO_DATA_DIR", "BMO_ARCHIVE_URL", "LLM_GATEWAY_REFRESH_MS"] {
    assert!(child["env"].get(kept).is_none(), "the child was handed {kept}");
  }

  // Readings that change nothing start nothing.
  let read = Instant::now();
  while read.elapsed() < Duration::from_secs(1) {
    tokio::time::sleep(Duration::from_millis(100)).await;
  }
  assert_eq!(world.events("started").len(), 1);
}

#[tokio::test]
async fn a_launcher_left_running_serves_the_hooks_of_the_tree_its_link_is_moved_to() {
  let world = World::new(PAIR).await;
  world.lay_tree("first", "# the first tree's");
  world.lay_tree("second", "# the second tree's");
  let world = world.started_through_the_link_at("first").start().await;
  let first = world.serving(None).await;
  let laid = |world: &World| std::fs::read_to_string(world.folder().join("roles.py")).unwrap();
  assert!(laid(&world).ends_with("# the first tree's\n"));

  world.point_live_at("second");

  let second = world.serving(Some(&first)).await;
  assert_ne!(second, first, "the moved link's hooks are a new table");
  assert!(laid(&world).ends_with("# the second tree's\n"));
}

#[tokio::test]
async fn a_new_table_takes_over_and_the_old_child_is_stopped_only_once_its_request_ends() {
  let world = World::new(PAIR).await.start().await;
  let first = world.serving(None).await;

  let held = world.open("GET", "/held", &[], "").await;
  let mut held = held.into_body();
  let part = held.frame().await.unwrap().unwrap().into_data().unwrap();
  assert_eq!(part, format!("{first}-first;").as_bytes());

  world.write(SINGLE).await;
  let second = world.serving(Some(&first)).await;
  assert_eq!(world.answered_by().await, second);
  assert!(world.events("stopped").is_empty(), "the old child was stopped while it held a request");
  assert_eq!(world.status().unwrap()["draining"], 1);

  std::fs::write(world.root().join(format!("release-{first}")), "").unwrap();
  let mut rest = Vec::new();
  while let Some(frame) = held.frame().await {
    if let Ok(data) = frame.unwrap().into_data() {
      rest.extend_from_slice(&data);
    }
  }
  assert_eq!(rest, format!("{first}-second").as_bytes());
  until("the old child to be stopped", || {
    world.events("stopped").iter().any(|entry| entry["table"] == first.as_str())
  })
  .await;
  until("nothing to be draining", || {
    world.status().is_some_and(|status| status["draining"] == 0)
  })
  .await;
  assert!(!world.events("stopped").iter().any(|entry| entry["table"] == second.as_str()));
  assert!(!world.folder().join(format!("config-{first}.yaml")).exists());
}

#[tokio::test]
async fn a_key_given_again_is_a_new_child_on_it() {
  let world = World::new(PAIR).await.start().await;
  let first = world.serving(None).await;

  private(&world.root().join("data/env/alpha-llm.token"), "alpha-key-given-again\n", 0o600);
  let second = world.serving(Some(&first)).await;

  let started = world.events("started");
  let child = started.iter().find(|entry| entry["table"] == second.as_str()).unwrap();
  assert_eq!(child["env"]["BMO_KEY_ALPHA_LLM_TOKEN"], "alpha-key-given-again");
  assert_eq!(world.answered_by().await, second);
  // The old child held nothing, so it is stopped at once.
  until("the old child to be stopped", || {
    world.events("stopped").iter().any(|entry| entry["table"] == first.as_str())
  })
  .await;
}

// A refusal for want of the key, as the front answers it: 401 in the OpenAI error shape.
fn assert_refused((status, headers, body): (u16, hyper::HeaderMap, String)) {
  assert_eq!(status, 401, "{body}");
  assert_eq!(headers["www-authenticate"], "Bearer");
  assert_eq!(headers["content-type"], "application/json");
  let error: Value = serde_json::from_str(&body).unwrap();
  assert_eq!(error["error"]["type"], "authentication_error", "{body}");
  assert_eq!(error["error"]["code"], "401", "{body}");
  assert!(error["error"]["message"].as_str().is_some_and(|message| !message.is_empty()), "{body}");
}

#[tokio::test]
async fn a_request_without_the_gateways_key_is_refused_401_at_the_front() {
  let world = World::over(Scratch::blank().await, "").await.start().await;

  // Before any table stands the key is still asked for first.
  assert_refused(read_whole(world.open_bare("GET", "/v1/models", &[], "").await).await);

  store::migration::up(world.store.db()).await.unwrap();
  world.write(PAIR).await;
  let table = world.serving_holding("pair-0-1").await;

  let wrong = format!("Bearer {GATEWAY_KEY}-wrong");
  let basic = format!("Basic {GATEWAY_KEY}");
  let refusals: [&[(&str, &str)]; 5] = [
    &[],
    &[("authorization", &wrong)],
    &[("authorization", "Bearer ")],
    &[("authorization", &basic)],
    &[("x-api-key", GATEWAY_KEY)],
  ];
  for headers in refusals {
    for path in ["/v1/models", "/v1/chat/completions", "/v1/messages", "/health/liveliness", "/held"] {
      assert_refused(read_whole(world.open_bare("POST", path, headers, "{}").await).await);
    }
  }
  // Nothing refused reached the child.
  assert!(world.events("holding").is_empty());

  let keyed = format!("bearer {GATEWAY_KEY}");
  let (status, headers, body) = read_whole(
    world
      .open_bare("POST", "/v1/chat/completions", &[("authorization", &keyed)], "{}")
      .await,
  )
  .await;
  assert_eq!(status, 200, "{body}");
  assert_eq!(headers["x-standin"], table.as_str());
  let asked: Value = serde_json::from_str(&body).unwrap();
  assert_eq!(asked["path"], "/v1/chat/completions");
  assert_eq!(asked["headers"]["authorization"], keyed);
}

#[tokio::test]
async fn a_table_whose_child_does_not_start_leaves_the_one_serving() {
  let world = World::new(PAIR).await.start().await;
  let first = world.serving(None).await;

  std::fs::write(world.root().join("fail-start"), "").unwrap();
  world.write(SINGLE).await;
  until("the new child to fail", || !world.events("failed").is_empty()).await;
  until("the gateway to say so", || world.said().contains("did not start")).await;

  assert_eq!(world.answered_by().await, first);
  let status = world.status().unwrap();
  assert_eq!(status["serving"], first.as_str());
  assert!(world.events("stopped").is_empty());
}

#[tokio::test]
async fn a_child_that_fails_is_answered_502() {
  let world = World::new(PAIR).await.start().await;
  world.serving(None).await;

  let (status, _, _) = world.ask("GET", "/die", &[], "").await;
  assert_eq!(status, 502);
  let (status, _, body) = world.ask("GET", "/v1/models", &[], "").await;
  assert_eq!(status, 502);
  assert!(body.starts_with("the gateway could not reach its table"), "{body}");
}

#[tokio::test]
async fn a_stopped_gateway_stops_its_children_and_takes_its_status_away() {
  let mut world = World::new(PAIR).await.start().await;
  let table = world.serving(None).await;

  world.terminate();
  let gateway = world.gateway.as_mut().unwrap();
  let bound = Instant::now() + Duration::from_secs(20);
  let ended = loop {
    if let Some(ended) = gateway.try_wait().unwrap() {
      break ended;
    }
    assert!(Instant::now() < bound, "the gateway did not end on SIGTERM");
    tokio::time::sleep(Duration::from_millis(20)).await;
  };
  assert_eq!(ended.code(), Some(0));
  assert!(world.events("stopped").iter().any(|entry| entry["table"] == table.as_str()));
  assert!(!world.folder().join("status.json").exists());
  assert!(!world.folder().join(format!("config-{table}.yaml")).exists());
}

#[tokio::test]
async fn a_stopped_gateway_takes_no_new_request_and_lets_the_one_in_hand_end_before_it_ends() {
  let mut world = World::new(PAIR).await.start().await;
  let table = world.serving(None).await;
  let held = world.open("GET", "/held", &[], "").await;
  let mut held = held.into_body();
  let part = held.frame().await.unwrap().unwrap().into_data().unwrap();

  assert_eq!(part, format!("{table}-first;").as_bytes());

  world.terminate();

  // The front takes no new connection from the stop on, and the gateway stands while its request is in hand.
  let port = world.port;
  let bound = Instant::now() + Duration::from_secs(20);

  while TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
    assert!(Instant::now() < bound, "the gateway took connections after its stop");
    tokio::time::sleep(Duration::from_millis(20)).await;
  }

  assert!(
    world.gateway.as_mut().unwrap().try_wait().unwrap().is_none(),
    "the gateway ended with a request in hand"
  );
  assert!(world.events("stopped").is_empty(), "the child was stopped while it held a request");

  std::fs::write(world.root().join(format!("release-{table}")), "").unwrap();

  let mut rest = Vec::new();

  while let Some(frame) = held.frame().await {
    if let Ok(data) = frame.unwrap().into_data() {
      rest.extend_from_slice(&data);
    }
  }

  assert_eq!(rest, format!("{table}-second").as_bytes(), "the request in hand ended whole");

  let gateway = world.gateway.as_mut().unwrap();
  let bound = Instant::now() + Duration::from_secs(20);
  let ended = loop {
    if let Some(ended) = gateway.try_wait().unwrap() {
      break ended;
    }
    assert!(Instant::now() < bound, "the gateway did not end once its request had");
    tokio::time::sleep(Duration::from_millis(20)).await;
  };

  assert_eq!(ended.code(), Some(0));
  assert!(world.events("stopped").iter().any(|entry| entry["table"] == table.as_str()));
}

// A backend of the test's own: it keeps every chat request it is sent and answers each with one word.
async fn echo_backend() -> (u16, std::sync::Arc<std::sync::Mutex<Vec<Value>>>) {
  let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
  let kept = seen.clone();
  let app = axum::Router::new().route(
    "/v1/chat/completions",
    axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
      let kept = kept.clone();
      async move {
        kept.lock().unwrap().push(body);
        axum::Json(json!({
          "id": "chatcmpl-echo",
          "object": "chat.completion",
          "created": 1,
          "model": "echo-model",
          "choices": [{ "index": 0, "message": { "role": "assistant", "content": "done" }, "finish_reason": "stop" }],
          "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2, "cost": 0.0123 },
        }))
      }
    }),
  );
  let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
  let port = listener.local_addr().unwrap().port();
  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  (port, seen)
}

#[tokio::test(flavor = "multi_thread")]
async fn through_litellm_a_runs_long_tools_reach_the_backend_and_each_requests_cost_reaches_the_store() {
  let (port, seen) = echo_backend().await;
  let echo = format!(
    "INSERT INTO registered_models VALUES ('echo', 'Echo', 'echo-model', 32768, 4096, 0, 0, 3, 0);
     INSERT INTO registered_model_addresses VALUES ('echo', 'local', 1, 'http://127.0.0.1:{port}/v1/chat/completions', NULL, 0, NULL);"
  );
  let world = World::new(&echo).await.on_litellm().start().await;
  world.serving_within(None, Duration::from_secs(180)).await;

  // A turn that called the shell and carries its result back, as Claude Code sends one.
  let turn = json!({
    "model": "echo",
    "max_tokens": 64,
    "tools": [{ "name": "Bash", "description": "Runs a command.", "input_schema": { "type": "object", "properties": { "command": { "type": "string" } } } }],
    "messages": [
      { "role": "user", "content": "List the folder." },
      { "role": "assistant", "content": [{ "type": "tool_use", "id": "toolu_01", "name": "Bash", "input": { "command": "ls" } }] },
      { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": "toolu_01", "content": "a.txt" }] },
    ],
  })
  .to_string();
  let key = format!("Bearer {GATEWAY_KEY}");
  let asked = |long_tools: Option<&'static str>| {
    let mut headers = vec![("authorization", key.as_str()), ("content-type", "application/json")];
    headers.extend(long_tools.map(|names| ("x-bmo-long-tools", names)));
    headers
  };

  let mut named = asked(Some("Bash,Agent,Task,WebFetch,WebSearch"));
  named.extend([("x-bmo-run", "run-a"), ("x-claude-code-session-id", "session-a")]);
  let (status, _, body) = world.ask("POST", "/v1/messages", &named, &turn).await;
  assert_eq!(status, 200, "{body}\n{}", world.said());
  let (status, _, body) = world.ask("POST", "/v1/messages", &asked(None), &turn).await;
  assert_eq!(status, 200, "{body}");

  let seen = seen.lock().unwrap().clone();
  assert_eq!(seen.len(), 2, "{seen:?}");
  assert_eq!(seen[0]["bmo_long_tools"], "Bash,Agent,Task,WebFetch,WebSearch", "{}", seen[0]);
  let last = seen[0]["messages"].as_array().unwrap().last().unwrap();
  assert_eq!((&last["role"], &last["tool_call_id"]), (&json!("tool"), &json!("toolu_01")), "{last}");
  assert!(seen[1].get("bmo_long_tools").is_none(), "{}", seen[1]);

  // The costs hook hands each request's cost to the front, which writes it down: the platform's, on the
  // gateway's own key, under the run and the session the first request named, at the cost the provider's own
  // answer named.
  let recorded = world.recorded_once(2).await;
  let named = recorded
    .iter()
    .find(|row| row["run"] == "run-a")
    .unwrap_or_else(|| panic!("{recorded:?}"));
  assert_eq!(
    named,
    &json!({
      "account": null, "model": "echo", "run": "run-a", "session": "session-a",
      "inputTokens": 1, "outputTokens": 1, "costMicros": 12_300,
    })
  );
  assert!(
    recorded.iter().any(|row| row["run"].is_null() && row["model"] == "echo"),
    "{recorded:?}"
  );
}

// A registration naming a card service, whose engine the master stands, beside the pair.
const CARDED: &str = "
  INSERT INTO registered_models (name, label, model, context_tokens, answer_tokens, thinking, walked, added_at, card_service)
    VALUES ('flash', 'Flash', 'flash-model', 32768, 4096, 0, 0, 3, 'arena');
";

// The arbiter's master, standing in: it keeps every demand asked and every one given back, answers a
// demand waiting until the test grants it, holds a read open while it waits, shows the service's
// instance standing in its picture once the test stands it, and refuses every ask with `no-place` where
// the test says so. The demands it holds as it starts are the test's.
#[derive(Default)]
struct Master {
  asked: Vec<Value>,
  given_back: Vec<i64>,
  held: Vec<Value>,
  granted: bool,
  standing: bool,
  refusing: bool,
}

type Shared = std::sync::Arc<std::sync::Mutex<Master>>;

fn demand_of(master: &Master, id: i64) -> Option<Value> {
  let asked = master.held.iter().find(|demand| demand["id"] == id)?;
  let granted = master.granted;
  Some(json!({
    "id": id,
    "serviceName": asked["serviceName"],
    "count": 1,
    "cards": if granted { json!([{ "nodeName": "n3", "address": "http://[::1]:9/v1/chat/completions" }]) } else { json!([]) },
    "state": if granted { "granted" } else { "waiting" },
    "reason": if granted { Value::Null } else { json!({ "code": "starting", "text": "The engine is starting." }) },
    "waitedMs": 0,
    "ref": asked["ref"],
  }))
}

async fn master_standin(held: Vec<Value>) -> (String, Shared) {
  use axum::extract::{Path as Key, Query, State};
  use axum::http::StatusCode;
  use axum::response::IntoResponse;
  use axum::routing::{get, post};

  let shared: Shared = std::sync::Arc::new(std::sync::Mutex::new(Master {
    held,
    ..Master::default()
  }));
  let app = axum::Router::new()
    .route(
      "/demands",
      post(|State(shared): State<Shared>, axum::Json(ask): axum::Json<Value>| async move {
        let mut master = shared.lock().unwrap();
        master.asked.push(ask.clone());
        if master.refusing {
          let said = json!({ "code": "no-place", "error": "The service arena is placed on no node." });
          return (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(said)).into_response();
        }
        let id = 1_000 + master.asked.len() as i64;
        master
          .held
          .push(json!({ "id": id, "serviceName": ask["serviceName"], "ref": ask["ref"] }));
        let demand = demand_of(&master, id).unwrap();
        (StatusCode::ACCEPTED, axum::Json(demand)).into_response()
      })
      .get(|State(shared): State<Shared>| async move {
        let master = shared.lock().unwrap();
        let items: Vec<Value> = master
          .held
          .iter()
          .filter_map(|one| demand_of(&master, one["id"].as_i64().unwrap()))
          .collect();
        axum::Json(json!({ "items": items, "more": false }))
      }),
    )
    .route(
      "/demands/{id}",
      get(
        |State(shared): State<Shared>,
         Key(id): Key<i64>,
         Query(query): Query<std::collections::HashMap<String, u64>>| async move {
          let bound = Instant::now() + Duration::from_millis(query.get("waitMs").copied().unwrap_or(0));
          loop {
            let demand = demand_of(&shared.lock().unwrap(), id);
            match demand {
              None => {
                return (
                  StatusCode::NOT_FOUND,
                  axum::Json(json!({ "code": "not-found", "error": "No such demand." })),
                )
                  .into_response();
              }
              Some(demand) if demand["state"] == "granted" || Instant::now() >= bound => {
                return axum::Json(demand).into_response();
              }
              Some(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
          }
        },
      )
      .delete(|State(shared): State<Shared>, Key(id): Key<i64>| async move {
        let mut master = shared.lock().unwrap();
        master.given_back.push(id);
        let before = master.held.len();
        master.held.retain(|demand| demand["id"] != id);
        if master.held.len() < before { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND }
      }),
    )
    .route(
      "/cluster",
      get(|State(shared): State<Shared>| async move {
        let standing = shared.lock().unwrap().standing;
        let instances = if standing {
          json!([{ "nodeName": "n3", "address": "http://[::1]:9/v1/chat/completions" }])
        } else {
          json!([])
        };
        axum::Json(json!({
          "services": [{ "serviceName": "arena", "tier": 2, "nodeNames": ["n3"], "instances": instances }],
          "nodes": [{ "name": "n3", "link": { "state": "connected" }, "cards": [{ "queue": [] }] }],
        }))
      }),
    )
    .with_state(shared.clone());
  let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
  let address = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  (address, shared)
}

async fn carded_world(held: Vec<Value>) -> (World, Shared) {
  let (address, master) = master_standin(held).await;
  let mut world = World::new(&format!("{PAIR}{CARDED}")).await;
  world.master = Some(address);
  (world.start().await, master)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_on_an_engine_standing_nowhere_waits_for_one_shared_card_and_gives_it_back_once_answered()
{
  let (world, master) = carded_world(vec![]).await;
  let first = world.serving(None).await;
  let answered = std::sync::atomic::AtomicUsize::new(0);
  let asking = |path: &'static str, body: &'static str| {
    let answered = &answered;
    let world = &world;
    async move {
      let asked = world.ask("POST", path, &[], body).await;
      answered.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
      asked
    }
  };

  let plain = asking("/v1/chat/completions", r#"{"model":"flash","messages":[]}"#);
  // Pinned to a node the master does not grant: sent under the registration's name alone.
  let pinned = asking("/v1/messages", r#"{"model":"flash@n4","max_tokens":1}"#);
  let steer = async {
    until("the door to ask its demand", || !master.lock().unwrap().asked.is_empty()).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(answered.load(std::sync::atomic::Ordering::SeqCst), 0, "a request went before its card");

    // Granted, but the instance not yet in the table: still held.
    master.lock().unwrap().granted = true;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(answered.load(std::sync::atomic::Ordering::SeqCst), 0, "a request went before its entry");
    assert!(master.lock().unwrap().given_back.is_empty(), "the card was given back while waited on");

    master.lock().unwrap().standing = true;
  };
  let ((plain_status, plain_headers, plain_body), (pinned_status, _, pinned_body), ()) =
    tokio::join!(plain, pinned, steer);

  assert_eq!(plain_status, 200, "{plain_body}\n{}", world.said());
  assert_ne!(plain_headers["x-standin"], first.as_str(), "sent before the instance's table took over");
  let sent: Value = serde_json::from_str(&plain_body).unwrap();
  assert_eq!(serde_json::from_str::<Value>(sent["body"].as_str().unwrap()).unwrap()["model"], "flash");
  assert_eq!(pinned_status, 200, "{pinned_body}");
  let sent: Value = serde_json::from_str(&pinned_body).unwrap();
  assert_eq!(serde_json::from_str::<Value>(sent["body"].as_str().unwrap()).unwrap()["model"], "flash");

  let asked = master.lock().unwrap().asked.clone();
  assert_eq!(asked.len(), 1, "the two requests did not share one demand: {asked:?}");
  assert_eq!(asked[0], json!({ "serviceName": "arena", "count": 1, "ref": "agency gateway door" }));
  until("the demand to be given back", || master.lock().unwrap().given_back == [1_001]).await;

  // The engine stays on its card, so the next request finds it served and asks nothing.
  let (status, _, body) = world.ask("POST", "/v1/chat/completions", &[], r#"{"model":"flash"}"#).await;
  assert_eq!(status, 200, "{body}");
  assert_eq!(master.lock().unwrap().asked.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_for_vectors_on_an_engine_standing_nowhere_asks_its_card_as_a_chat_request_does() {
  let (world, master) = carded_world(vec![]).await;
  let first = world.serving(None).await;
  let asked = world.ask("POST", "/v1/embeddings", &[], r#"{"model":"flash","input":["a text"]}"#);
  let steer = async {
    until("the door to ask its demand", || !master.lock().unwrap().asked.is_empty()).await;
    master.lock().unwrap().granted = true;
    master.lock().unwrap().standing = true;
  };
  let ((status, headers, body), ()) = tokio::join!(asked, steer);

  assert_eq!(status, 200, "{body}\n{}", world.said());
  assert_ne!(headers["x-standin"], first.as_str(), "sent before the instance's table took over");
  let sent: Value = serde_json::from_str(&body).unwrap();
  assert_eq!(sent["path"], "/v1/embeddings");
  let asked = master.lock().unwrap().asked.clone();
  assert_eq!(asked, [json!({ "serviceName": "arena", "count": 1, "ref": "agency gateway door" })]);
  until("the demand to be given back", || master.lock().unwrap().given_back == [1_001]).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_model_is_400_without_a_demand_and_a_refused_demand_is_503_with_the_masters_sentence() {
  let (world, master) = carded_world(vec![]).await;
  world.serving(None).await;

  let (status, _, body) = world.ask("POST", "/v1/chat/completions", &[], r#"{"model":"nowhere"}"#).await;
  assert_eq!(status, 400, "{body}");
  assert!(body.contains("Invalid model name"), "{body}");
  let (status, _, body) = world.ask("POST", "/v1/messages", &[], r#"{"model":"pair-0-1@nowhere"}"#).await;
  assert_eq!(status, 400, "{body}");
  let (status, _, body) = world.ask("POST", "/v1/embeddings", &[], r#"{"model":"nowhere"}"#).await;
  assert_eq!(status, 400, "{body}");
  assert!(
    master.lock().unwrap().asked.is_empty(),
    "a demand was asked for a model no card service serves"
  );

  master.lock().unwrap().refusing = true;
  let (status, _, body) = world.ask("POST", "/v1/chat/completions", &[], r#"{"model":"flash"}"#).await;
  assert_eq!((status, body.as_str()), (503, "The service arena is placed on no node."));
  assert_eq!(master.lock().unwrap().asked.len(), 1);

  // The refusal ends the line: the next request asks anew.
  let (status, _, _) = world.ask("POST", "/v1/chat/completions", &[], r#"{"model":"flash"}"#).await;
  assert_eq!(status, 503);
  assert_eq!(master.lock().unwrap().asked.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_whose_caller_leaves_while_its_card_waits_gives_the_demand_back() {
  let (world, master) = carded_world(vec![]).await;
  world.serving(None).await;

  let left = tokio::time::timeout(
    Duration::from_millis(800),
    world.ask("POST", "/v1/chat/completions", &[], r#"{"model":"flash"}"#),
  )
  .await;
  assert!(left.is_err(), "the request was answered with no card granted");
  until("the waiting demand to be given back", || master.lock().unwrap().given_back == [1_001]).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_demands_a_launcher_left_under_the_doors_reference_are_given_back_as_it_starts() {
  let left = vec![
    json!({ "id": 7, "serviceName": "arena", "ref": "agency gateway door" }),
    json!({ "id": 8, "serviceName": "arena", "ref": "agency translator" }),
  ];
  let (_world, master) = carded_world(left).await;

  until("the door's left demand to be given back", || master.lock().unwrap().given_back == [7]).await;
  tokio::time::sleep(Duration::from_millis(300)).await;
  assert_eq!(master.lock().unwrap().given_back, [7], "a demand of another holder was given back");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_naming_its_rank_and_pool_has_the_door_ask_them_on_a_line_of_their_own() {
  let (world, master) = carded_world(vec![]).await;
  world.serving(None).await;
  let plain = world.ask("POST", "/v1/chat/completions", &[], r#"{"model":"flash","messages":[]}"#);
  let named = world.ask(
    "POST",
    "/v1/chat/completions",
    &[("x-bmo-card-rank", "0"), ("x-bmo-card-pool", "1 mine")],
    r#"{"model":"flash","messages":[]}"#,
  );
  let steer = async {
    until("the door to ask both lines' demands", || master.lock().unwrap().asked.len() == 2).await;
    let asked = master.lock().unwrap().asked.clone();
    let ranked = asked.iter().find(|one| one.get("rank").is_some()).expect("a demand at the rank named");
    assert_eq!((&ranked["rank"], &ranked["pool"]), (&json!(0), &json!({ "key": "mine", "cards": 1 })));
    let unnamed = asked.iter().find(|one| one.get("rank").is_none()).expect("a demand naming none");
    assert!(unnamed.get("pool").is_none(), "{unnamed:?}");
    master.lock().unwrap().granted = true;
    master.lock().unwrap().standing = true;
  };
  tokio::join!(plain, named, steer);
}

// The span a stall test's gateway is given.
const STALL_MS: u64 = 600;

// A streamed chat request to the stand-in: `count` events `gap` milliseconds apart, the stream ended or held.
async fn stream_of(world: &World, stream: &str, count: u32, gap: u64) -> hyper::Response<Incoming> {
  let (count, gap) = (count.to_string(), gap.to_string());
  world
    .open(
      "POST",
      "/v1/chat/completions",
      &[
        ("content-type", "application/json"),
        ("x-standin-stream", stream),
        ("x-standin-count", &count),
        ("x-standin-gap", &gap),
      ],
      r#"{"model":"pair-0-1","stream":true}"#,
    )
    .await
}

// The lines the gateway wrote of a stream it cut.
fn cut_lines(world: &World) -> Vec<String> {
  world
    .said()
    .lines()
    .filter(|line| line.contains("was cut"))
    .map(str::to_owned)
    .collect()
}

#[tokio::test]
async fn the_stall_span_is_read_from_the_policy() {
  let world = World::new(PAIR).await.start().await;
  let span = policy::gateway::GATEWAY.stall_seconds * 1000;
  assert!(
    world
      .said()
      .contains(&format!("llm-gateway: a streamed answer silent for {span}ms is cut")),
    "{}",
    world.said()
  );
}

#[tokio::test]
async fn a_stream_that_keeps_sending_is_relayed_whole_however_long_it_takes() {
  let world = World::new(PAIR).await.stalling_after(STALL_MS).start().await;
  world.serving(None).await;

  // Six events half a span apart: the stream outlasts the span many times and is never silent for it.
  let started = Instant::now();
  let (status, headers, body) = read_whole(stream_of(&world, "whole", 6, STALL_MS / 2).await).await;
  assert_eq!(status, 200);
  assert!(started.elapsed() > Duration::from_millis(STALL_MS * 2));
  assert!(headers["content-type"].to_str().unwrap().starts_with("text/event-stream"));
  assert_eq!(body, (0..6).map(|at| format!("data: {at}\n\n")).collect::<String>());
  assert!(cut_lines(&world).is_empty(), "{}", world.said());
}

#[tokio::test]
async fn a_stream_silent_for_the_span_is_cut_and_its_child_let_go() {
  let world = World::new(PAIR).await.stalling_after(STALL_MS).start().await;
  let table = world.serving(None).await;

  let answer = stream_of(&world, "stall", 1, 0).await;
  assert_eq!(answer.status(), 200);
  let mut body = answer.into_body();
  let first = body.frame().await.unwrap().unwrap().into_data().unwrap();
  assert_eq!(first, "data: 0\n\n".as_bytes());

  // The caller's stream ends with an error, as a broken connection does.
  let cut = tokio::time::timeout(Duration::from_secs(10), async {
    loop {
      match body.frame().await {
        Some(Ok(_)) => continue,
        Some(Err(_)) => return true,
        None => return false,
      }
    }
  })
  .await
  .expect("the stalled stream was never cut");
  assert!(cut, "the stalled stream ended as if whole");

  until("the child to see its caller go", || {
    world.events("cancelled").iter().any(|entry| entry["stream"] == "stall")
  })
  .await;
  let lines = cut_lines(&world);
  assert_eq!(lines.len(), 1, "{}", world.said());
  assert!(lines[0].contains("model pair-0-1"), "{}", lines[0]);
  assert!(lines[0].contains(&format!("table {table}")), "{}", lines[0]);
  assert!(lines[0].contains("sent no byte for 0.6s"), "{}", lines[0]);

  // The request is no longer in hand: a new table stops the old child at once.
  world.write(SINGLE).await;
  world.serving(Some(&table)).await;
  until("the old child to be stopped", || {
    world.events("stopped").iter().any(|entry| entry["table"] == table.as_str())
  })
  .await;
}

#[tokio::test]
async fn an_answer_of_headers_alone_is_cut_by_the_same_span() {
  let world = World::new(PAIR).await.with_silent_child().stalling_after(STALL_MS).start().await;
  world.serving(None).await;

  let answer = stream_of(&world, "silent", 0, 0).await;
  assert_eq!(answer.status(), 200);
  assert_eq!(answer.headers()["content-type"], "text/event-stream");
  let read = tokio::time::timeout(Duration::from_secs(10), answer.into_body().collect())
    .await
    .expect("the silent stream was never cut");
  assert!(read.is_err(), "the silent stream ended as if whole");
  until("the child to see its caller go", || !world.events("cancelled").is_empty()).await;
  assert_eq!(cut_lines(&world).len(), 1, "{}", world.said());
}

#[tokio::test]
async fn an_answer_not_streamed_keeps_no_clock() {
  let world = World::new(PAIR).await.stalling_after(STALL_MS).start().await;
  let table = world.serving(None).await;

  let held = world.open("GET", "/held", &[], "").await;
  let mut held = held.into_body();
  let part = held.frame().await.unwrap().unwrap().into_data().unwrap();
  assert_eq!(part, format!("{table}-first;").as_bytes());
  tokio::time::sleep(Duration::from_millis(STALL_MS * 3)).await;

  std::fs::write(world.root().join(format!("release-{table}")), "").unwrap();
  let rest = held.collect().await.expect("the held answer is carried whole").to_bytes();
  assert_eq!(rest, format!("{table}-second").as_bytes());
  assert!(cut_lines(&world).is_empty(), "{}", world.said());
}

// A backend of the test's own that streams one chunk of an answer and then nothing, its connection held
// open, and says when its caller has let the stream go.
async fn stalling_backend() -> (u16, std::sync::Arc<std::sync::atomic::AtomicBool>) {
  struct Gone(std::sync::Arc<std::sync::atomic::AtomicBool>);
  impl Drop for Gone {
    fn drop(&mut self) {
      self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
  }
  let gone = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
  let told = gone.clone();
  let app = axum::Router::new().route(
    "/v1/chat/completions",
    axum::routing::post(move || {
      let guard = Gone(told.clone());
      async move {
        let chunk = json!({
          "id": "chatcmpl-stall",
          "object": "chat.completion.chunk",
          "created": 1,
          "model": "stall-model",
          "choices": [{ "index": 0, "delta": { "role": "assistant", "content": "thinking" }, "finish_reason": null }],
        });
        let first = futures_util::stream::once(async move {
          Ok::<_, std::convert::Infallible>(Bytes::from(format!("data: {chunk}\n\n")))
        });
        let held = futures_util::StreamExt::map(
          futures_util::StreamExt::chain(first, futures_util::stream::pending()),
          move |chunk| {
            let _ = &guard;
            chunk
          },
        );
        axum::response::Response::builder()
          .header("content-type", "text/event-stream")
          .body(axum::body::Body::from_stream(held))
          .unwrap()
      }
    }),
  );
  let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
  let port = listener.local_addr().unwrap().port();
  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  (port, gone)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stalled_stream_cut_at_the_front_lets_go_of_the_backend_through_litellm() {
  let (port, gone) = stalling_backend().await;
  let stall = format!(
    "INSERT INTO registered_models VALUES ('stall', 'Stall', 'stall-model', 32768, 4096, 0, 0, 3, 0);
     INSERT INTO registered_model_addresses VALUES ('stall', 'local', 1, 'http://127.0.0.1:{port}/v1/chat/completions', NULL, 0, NULL);"
  );
  let world = World::new(&stall).await.on_litellm().stalling_after(3_000).start().await;
  world.serving_within(None, Duration::from_secs(180)).await;

  let answer = world
    .open(
      "POST",
      "/v1/chat/completions",
      &[("content-type", "application/json")],
      r#"{"model":"stall","stream":true,"messages":[{"role":"user","content":"Say something."}]}"#,
    )
    .await;
  assert_eq!(answer.status(), 200, "{}", world.said());
  let read = tokio::time::timeout(Duration::from_secs(60), answer.into_body().collect())
    .await
    .expect("the stalled stream was never cut");
  assert!(read.is_err(), "the stalled stream ended as if whole");
  assert_eq!(cut_lines(&world).len(), 1, "{}", world.said());
  until("the backend's stream to be let go", || gone.load(std::sync::atomic::Ordering::SeqCst)).await;
}

// The digest the store keeps of a call key, as agency's server writes it: SHA-256 of the key, in lowercase hex.
fn call_key_digest(key: &str) -> String {
  hex::encode(Sha256::digest(key.trim().as_bytes()))
}

// A call key of the account's as the server issues one, revoked where `revoked` says.
fn call_key(account: i64, key: &str, revoked: bool) -> String {
  format!(
    "INSERT INTO gateway_call_keys (account, key_hash, issued_at, revoked_at) VALUES ({account}, '{}', 1, {});",
    call_key_digest(key),
    if revoked { "2" } else { "NULL" }
  )
}

// A registration of the supply hook, as the server keeps one.
const SUPPLY_HOOK: &str =
  r#"INSERT INTO settings (key, value) VALUES ('hook_kept:supply', '{"url":"http://127.0.0.1:9/hooks"}');"#;

// A platform reservation of the account's, in the state given, expiring at the moment named, or never.
fn reservation(account: i64, state: &str, expires_at: Option<i64>) -> String {
  format!(
    "INSERT INTO supply_reservations (run, opened_step, last_step, account, source, reservation, amount_micros, \
     unit_micros, expires_at, state) VALUES ('run-{account}', 1, 1, {account}, 'plan', 'r-{account}-{state}', \
     1000000, 1000000, {}, '{state}');",
    expires_at.map_or("NULL".to_owned(), |at| at.to_string())
  )
}

fn bearer(key: &str) -> String {
  format!("Bearer {key}")
}

// A refusal in the OpenAI error shape, with its status and its code.
fn assert_openai_error((status, _, body): (u16, hyper::HeaderMap, String), expected: u16, code: &str) {
  assert_eq!(status, expected, "{body}");
  let error: Value = serde_json::from_str(&body).unwrap();
  assert_eq!(error["error"]["code"], code, "{body}");
  assert!(error["error"]["type"].as_str().is_some_and(|kind| !kind.is_empty()), "{body}");
  assert!(error["error"]["message"].as_str().is_some_and(|message| !message.is_empty()), "{body}");
}

#[tokio::test]
async fn a_call_key_reaches_the_child_under_the_gateways_key_and_the_front_names_the_payer() {
  let world = World::new(&format!(
    "{PAIR}{}{}",
    call_key(7, "call-key-seven", false),
    reservation(7, "open", None)
  ))
  .await
  .start()
  .await;
  world.serving(None).await;

  let seven = bearer("call-key-seven");
  let (status, _, body) = world
    .ask(
      "POST",
      "/v1/chat/completions",
      &[("authorization", &seven), ("x-bmo-payer", "platform")],
      r#"{"model":"pair-0-1"}"#,
    )
    .await;
  assert_eq!(status, 200, "{body}");
  let asked: Value = serde_json::from_str(&body).unwrap();
  assert_eq!(asked["headers"]["authorization"], bearer(GATEWAY_KEY), "the call key goes no further");
  assert_eq!(asked["headers"]["x-bmo-payer"], "7", "the payer a caller writes is the front's to name");

  let (status, _, body) = world.ask("GET", "/v1/models", &[("x-bmo-payer", "7")], "").await;
  assert_eq!(status, 200, "{body}");
  let asked: Value = serde_json::from_str(&body).unwrap();
  assert_eq!(asked["headers"]["x-bmo-payer"], "platform");
}

#[tokio::test]
async fn a_revoked_or_unknown_call_key_is_refused_401() {
  let world = World::new(&format!("{PAIR}{}", call_key(7, "call-key-revoked", true)))
    .await
    .start()
    .await;
  world.serving(None).await;

  for key in ["call-key-revoked", "call-key-unknown"] {
    assert_refused(world.ask("GET", "/v1/models", &[("authorization", &bearer(key))], "").await);
  }
}

#[tokio::test]
async fn a_call_key_issued_after_the_last_reading_is_looked_up_and_taken() {
  let world = World::new(PAIR).await.reading_the_store_once().start().await;
  world.serving(None).await;
  world.write(&call_key(7, "call-key-fresh", false)).await;

  let (status, _, body) = world
    .ask("GET", "/v1/models", &[("authorization", &bearer("call-key-fresh"))], "")
    .await;
  assert_eq!(status, 200, "{body}");
}

#[tokio::test]
async fn a_personal_member_answers_its_own_persons_call_key_alone() {
  let person = "INSERT INTO personal_supply (owner_account, road, secret, place, enabled, added_at, refused_at) \
                VALUES (7, 'openrouter', 'router-own', 1, true, 1, NULL);";
  let world = World::new(&format!(
    "{PAIR}{}{}{person}{SUPPLY_HOOK}",
    call_key(7, "call-key-seven", false),
    call_key(8, "call-key-eight", false)
  ))
  .await
  .with_members(json!([{
    "name": "router-flash", "road": "openrouter", "label": "Router Flash", "provider": "openrouter",
    "model": "vendor/flash-1", "api_base": "https://openrouter.ai/api/v1", "key_file": "openrouter.key",
    "context_tokens": 131072, "answer_tokens": 8192, "thinking": false,
  }]));
  private(&world.root().join("data/env/openrouter.key"), "platform-key\n", 0o600);
  private(&world.root().join("data/env/users/7/router-own"), "person-key\n", 0o600);
  let world = world.start().await;
  let table = world.serving_holding("router-flash@u7").await;

  let started = world.events("started");
  let child = started.iter().find(|entry| entry["table"] == table.as_str()).unwrap();
  assert_eq!(
    child["env"]["BMO_KEY_USERS_7_ROUTER_OWN"], "person-key",
    "the person's key reaches the child"
  );

  let body = r#"{"model":"router-flash@u7"}"#;
  for key in ["call-key-eight", GATEWAY_KEY] {
    let refused = world
      .ask("POST", "/v1/chat/completions", &[("authorization", &bearer(key))], body)
      .await;
    assert_openai_error(refused, 403, "not-your-member");
  }

  // The person's own member is their own supply, so no reservation is asked of it while the hook stands.
  let (status, _, answer) = world
    .ask("POST", "/v1/chat/completions", &[("authorization", &bearer("call-key-seven"))], body)
    .await;
  assert_eq!(status, 200, "{answer}");
  let asked: Value = serde_json::from_str(&answer).unwrap();
  assert_eq!(asked["headers"]["x-bmo-payer"], "7");
}

#[tokio::test]
async fn the_administrators_personal_member_answers_the_administrators_call_key_and_not_the_gateways() {
  let person = "INSERT INTO personal_supply (owner_account, road, secret, place, enabled, added_at, refused_at) \
                VALUES (1, 'openrouter', 'router-own', 1, true, 1, NULL);";
  let world = World::new(&format!(
    "{PAIR}{ADMINISTRATOR}{}{person}{SUPPLY_HOOK}",
    call_key(1, "call-key-one", false)
  ))
  .await
  .with_members(json!([{
    "name": "router-flash", "road": "openrouter", "label": "Router Flash", "provider": "openrouter",
    "model": "vendor/flash-1", "api_base": "https://openrouter.ai/api/v1", "key_file": "openrouter.key",
    "context_tokens": 131072, "answer_tokens": 8192, "thinking": false,
  }]));
  private(&world.root().join("data/env/openrouter.key"), "platform-key\n", 0o600);
  private(&world.root().join("data/env/users/1/router-own"), "person-key\n", 0o600);
  let world = world.start().await;
  world.serving_holding("router-flash@u1").await;

  let (status, _, answer) = chat_on(&world, "call-key-one", "router-flash@u1").await;
  assert_eq!(status, 200, "{answer}");
  assert_openai_error(chat_on(&world, GATEWAY_KEY, "router-flash@u1").await, 403, "not-your-member");
  assert_eq!(
    listed_models(&world, "/v1/models", "call-key-one").await,
    ["pair-0-1", "pair-0-1@alpha", "pair-0-1@bravo", "router-flash", "router-flash@u1"]
  );
}

// The members of OpenRouter's road the owned-supply tests serve: `router-flash` owned by account 7,
// `router-own` by the administrator, account 1, and `router-free` by nobody.
fn owned_members() -> Value {
  let member = |name: &str, account: Option<i64>| {
    let mut member = json!({
      "name": name, "road": "openrouter", "provider": "openrouter", "model": format!("vendor/{name}"),
      "key_file": "openrouter.key", "context_tokens": 131072, "answer_tokens": 8192, "thinking": false,
    });
    if let Some(account) = account {
      member["account"] = account.into();
    }
    member
  };
  json!([
    member("router-flash", Some(7)),
    member("router-own", Some(1)),
    member("router-free", None)
  ])
}

// The administrator named in the settings, as the console writes it.
const ADMINISTRATOR: &str = "INSERT INTO settings (key, value) VALUES ('users_administrator', '1');";

// A chat request naming the model under the key given.
async fn chat_on(world: &World, key: &str, model: &str) -> (u16, hyper::HeaderMap, String) {
  let body = json!({ "model": model }).to_string();
  world
    .ask("POST", "/v1/chat/completions", &[("authorization", &bearer(key))], &body)
    .await
}

#[tokio::test]
async fn an_owned_member_or_registration_answers_its_owner_alone_and_the_gateways_key_the_administrators() {
  let owned = "UPDATE registered_models SET owner_account = 7 WHERE name = 'pair-0-1';";
  let world = World::new(&format!(
    "{PAIR}{owned}{ADMINISTRATOR}{}{}{}",
    call_key(1, "call-key-one", false),
    call_key(7, "call-key-seven", false),
    call_key(8, "call-key-eight", false)
  ))
  .await
  .with_members(owned_members());
  private(&world.root().join("data/env/openrouter.key"), "platform-key\n", 0o600);
  let world = world.start().await;
  world.serving_holding("router-flash").await;

  for model in ["pair-0-1", "pair-0-1@alpha", "router-flash"] {
    for key in ["call-key-eight", GATEWAY_KEY, "call-key-one"] {
      assert_openai_error(chat_on(&world, key, model).await, 403, "not-your-member");
    }
    let (status, _, answer) = chat_on(&world, "call-key-seven", model).await;
    assert_eq!(status, 200, "{model} under its owner's key: {answer}");
  }
  // The administrator's member answers the gateway's own key, and the administrator's call key as it, which
  // reaches the platform's member with no reservation asked.
  for (key, model) in [
    (GATEWAY_KEY, "router-own"),
    ("call-key-one", "router-own"),
    ("call-key-one", "router-free"),
  ] {
    let (status, _, answer) = chat_on(&world, key, model).await;
    assert_eq!(status, 200, "{model} under {key}: {answer}");
  }
  for key in ["call-key-seven", "call-key-eight"] {
    assert_openai_error(chat_on(&world, key, "router-own").await, 403, "not-your-member");
  }
  // The platform's member is no person's own, so a call key reaches it only on a reservation.
  assert_openai_error(chat_on(&world, "call-key-eight", "router-free").await, 402, "no-supply");

  // Every listing and description of models leaves out what a call key does not reach; the gateway's own key
  // lists the whole table, which the deployment's reads of what stands go by, a person's own registration
  // among it.
  for path in ["/v1/models", "/model/info", "/model_group/info"] {
    assert_eq!(listed_models(&world, path, "call-key-eight").await, ["router-free"], "{path}");
    assert_eq!(
      listed_models(&world, path, "call-key-seven").await,
      ["pair-0-1", "pair-0-1@alpha", "pair-0-1@bravo", "router-flash", "router-free"],
      "{path}"
    );
    assert_eq!(
      listed_models(&world, path, "call-key-one").await,
      ["router-free", "router-own"],
      "{path}"
    );
    assert_eq!(
      listed_models(&world, path, GATEWAY_KEY).await,
      [
        "pair-0-1",
        "pair-0-1@alpha",
        "pair-0-1@bravo",
        "router-flash",
        "router-free",
        "router-own"
      ],
      "{path}"
    );
  }
  for (key, status) in [("call-key-eight", 403), ("call-key-seven", 200), (GATEWAY_KEY, 200)] {
    for path in ["/v1/models/router-flash", "/v1/models/pair-0-1%40alpha"] {
      let (answered, _, body) = world.ask("GET", path, &[("authorization", &bearer(key))], "").await;
      assert_eq!(answered, status, "{path} under {key}: {body}");
    }
  }

  // A request whose model cannot be read is refused to a call key, LiteLLM never asked which one it reads.
  let doubled = r#"{"model":"router-free","model":"router-flash"}"#;
  let refused = world
    .ask("POST", "/v1/chat/completions", &[("authorization", &bearer("call-key-eight"))], doubled)
    .await;
  assert_openai_error(refused, 400, "model-unread");
}

// A reservation naming an organisation unit opens that unit's names and none other: the unit's own member and
// the unit's own registration answer the key that holds it, the platform's names stay behind a platform
// reservation the unit's does not grant, and a platform reservation opens none of the unit's
// (docs/own-llm.md, 「Each reservation opens what it names and nothing else」).
#[tokio::test]
async fn each_reservation_opens_what_it_names_and_nothing_else() {
  let unit_of_four = "INSERT INTO supply_reservations (run, opened_step, last_step, account, source, \
     reservation, amount_micros, unit_micros, expires_at, state, unit_id) \
     VALUES ('run-org-7', 1, 1, 7, 'organisation', 'r-org-7', 1000000, 1000000, NULL, 'open', 4);";
  let team = "
  INSERT INTO registered_models (name, label, model, context_tokens, answer_tokens, thinking, walked, \
     added_at, owner_unit) VALUES ('team-router', 'Team Router', 'served', 32768, 4096, 0, 1, 1, 4);
  INSERT INTO registered_model_addresses (registration, node, position, url, key_file, key_owned) \
     VALUES ('team-router', 'n1', 1, 'http://127.0.0.1:18021/v1/chat/completions', NULL, 0);";
  let world = World::new(&format!(
    "{PAIR}{team}{}{}{}{}",
    call_key(7, "call-key-seven", false),
    call_key(8, "call-key-eight", false),
    unit_of_four,
    reservation(8, "open", None),
  ))
  .await
  .with_members(json!([
    { "name": "router-team", "road": "openrouter", "provider": "openrouter", "model": "vendor/team",
      "key_file": "openrouter.key", "context_tokens": 131072, "answer_tokens": 8192, "thinking": false,
      "unit": 4 },
    { "name": "router-above", "road": "openrouter", "provider": "openrouter", "model": "vendor/above",
      "key_file": "openrouter.key", "context_tokens": 131072, "answer_tokens": 8192, "thinking": false,
      "unit": 5 },
    { "name": "router-free", "road": "openrouter", "provider": "openrouter", "model": "vendor/free",
      "key_file": "openrouter.key", "context_tokens": 131072, "answer_tokens": 8192, "thinking": false },
  ]));
  private(&world.root().join("data/env/openrouter.key"), "platform-key\n", 0o600);
  let world = world.start().await;
  world.serving_holding("router-team").await;

  // The unit's people reach the unit's names: its own member under its own name, and its registration.
  assert_eq!(chat_on(&world, "call-key-seven", "router-team").await.0, 200);
  assert_eq!(chat_on(&world, "call-key-seven", "team-router").await.0, 200);
  // The platform's door stays shut to it: a run of a platform name asks a reservation of the platform's own.
  assert_openai_error(chat_on(&world, "call-key-seven", "router-free").await, 402, "no-supply");
  // Another unit's member stands behind that unit's reservation, which this key does not hold.
  assert_openai_error(chat_on(&world, "call-key-seven", "router-above").await, 403, "not-your-member");

  // And the platform's reservation runs the platform's names and none of the unit's.
  assert_openai_error(chat_on(&world, "call-key-eight", "team-router").await, 403, "not-your-member");
  assert_openai_error(chat_on(&world, "call-key-eight", "router-team").await, 403, "not-your-member");
  assert_eq!(chat_on(&world, "call-key-eight", "router-free").await.0, 200);

  // The listings name each key what it reaches: the unit's people see their unit's names among the standing
  // ones, and an account holding only the platform's reservation is handed none of the unit's.
  assert_eq!(
    listed_models(&world, "/v1/models", "call-key-seven").await,
    [
      "pair-0-1",
      "pair-0-1@alpha",
      "pair-0-1@bravo",
      "router-free",
      "router-team",
      "team-router"
    ]
  );
  assert_eq!(
    listed_models(&world, "/v1/models", "call-key-eight").await,
    ["pair-0-1", "pair-0-1@alpha", "pair-0-1@bravo", "router-free"]
  );

  // The description asks the same reach as the request.
  let (answered, _, body) = world
    .ask("GET", "/v1/models/team-router", &[("authorization", &bearer("call-key-seven"))], "")
    .await;
  assert_eq!(answered, 200, "{body}");
  assert_openai_error(
    world
      .ask("GET", "/v1/models/team-router", &[("authorization", &bearer("call-key-eight"))], "")
      .await,
    403,
    "not-your-member",
  );
}

#[tokio::test]
async fn a_call_key_reaches_the_request_routes_and_the_model_lists_alone() {
  let world = World::new(&format!(
    "{PAIR}{}{}",
    call_key(7, "call-key-seven", false),
    call_key(8, "call-key-eight", false)
  ))
  .await
  .with_members(owned_members());
  private(&world.root().join("data/env/openrouter.key"), "platform-key\n", 0o600);
  let world = world.start().await;
  world.serving_holding("router-flash").await;

  // LiteLLM's writes and its spend and health reads are the gateway's own key's.
  let asks = [
    ("POST", "/model/new"),
    ("POST", "/key/generate"),
    ("POST", "/team/new"),
    ("GET", "/spend/logs"),
    ("GET", "/health"),
    ("DELETE", "/v1/models/router-flash"),
  ];
  for (method, path) in asks {
    let refused = world.ask(method, path, &[("authorization", &bearer("call-key-seven"))], "{}").await;
    assert_openai_error(refused, 403, "gateway-key-only");
    let (status, _, body) = world.ask(method, path, &[("authorization", &bearer(GATEWAY_KEY))], "{}").await;
    assert_eq!(status, 200, "{method} {path} under the gateway's key: {body}");
  }

  // The request routes and the model lists answer a call key, a token count held to the model's owner as a
  // request is.
  let count = r#"{"model":"router-flash"}"#;
  let counted = |key: &str| {
    let key = bearer(key);
    let world = &world;
    async move { world.ask("POST", "/v1/messages/count_tokens", &[("authorization", &key)], count).await }
  };
  let (status, _, body) = counted("call-key-seven").await;
  assert_eq!(status, 200, "{body}");
  assert_openai_error(counted("call-key-eight").await, 403, "not-your-member");
  let (status, _, body) = chat_on(&world, "call-key-seven", "router-flash").await;
  assert_eq!(status, 200, "{body}");
  assert!(
    listed_models(&world, "/v1/models", "call-key-seven")
      .await
      .contains(&"router-flash".to_owned())
  );
}

#[tokio::test]
async fn an_owner_set_while_the_gateway_runs_holds_from_the_next_beat() {
  let world = World::new(&format!(
    "{PAIR}{}{}",
    call_key(8, "call-key-eight", false),
    reservation(8, "open", None)
  ))
  .await
  .with_members(owned_members());
  private(&world.root().join("data/env/openrouter.key"), "platform-key\n", 0o600);
  let world = world.start().await;
  world.serving_holding("router-free").await;
  let (status, _, answer) = chat_on(&world, "call-key-eight", "router-free").await;
  assert_eq!(status, 200, "{answer}");

  // The owner is set in a file whose members the table refuses, one of them named as the registration is; the
  // last good table stands, and the owner holds all the same.
  let mut members = owned_members();
  members[2]["account"] = json!(7);
  members.as_array_mut().unwrap().push(json!({
    "name": "pair-0-1", "provider": "openai", "model": "taken", "key_file": "openrouter.key",
  }));
  let world = world.with_members(members);

  // The beat, every 100 milliseconds, reads the file again; the owner holds once one has read it.
  let bound = Instant::now() + Duration::from_secs(30);
  while chat_on(&world, "call-key-eight", "router-free").await.0 != 403 {
    assert!(Instant::now() < bound, "the owner set never held");
    tokio::time::sleep(Duration::from_millis(20)).await;
  }
  assert_openai_error(chat_on(&world, "call-key-eight", "router-free").await, 403, "not-your-member");
}

// The names a listing or description of models answers under a key, in order.
async fn listed_models(world: &World, path: &str, key: &str) -> Vec<String> {
  let (status, _, body) = world.ask("GET", path, &[("authorization", &bearer(key))], "").await;
  assert_eq!(status, 200, "{body}");
  let listed: Value = serde_json::from_str(&body).unwrap();
  let mut ids: Vec<String> = listed["data"]
    .as_array()
    .unwrap()
    .iter()
    .map(|model| {
      let named = ["id", "model_name", "model_group"].iter().find_map(|field| model[*field].as_str());
      named.unwrap().to_owned()
    })
    .collect();
  ids.sort();
  ids
}

// A chat request on the pair under the authorization given.
async fn chat_on_the_pair(world: &World, authorization: String) -> (u16, hyper::HeaderMap, String) {
  world
    .ask(
      "POST",
      "/v1/chat/completions",
      &[("authorization", &authorization)],
      r#"{"model":"pair-0-1"}"#,
    )
    .await
}

#[tokio::test]
async fn a_person_reaches_platform_supply_only_on_an_open_reservation_hook_or_none() {
  let world = World::new(&format!(
    "{PAIR}{}{}{SUPPLY_HOOK}{}{}",
    call_key(7, "call-key-seven", false),
    call_key(8, "call-key-eight", false),
    reservation(7, "open", Some(1)),
    reservation(7, "captured", None),
  ))
  .await
  .reading_the_store_once()
  .start()
  .await;
  world.serving(None).await;
  let (seven, eight) = (bearer("call-key-seven"), bearer("call-key-eight"));
  let chat = |key: &String| chat_on_the_pair(&world, key.clone());

  // An expired reservation and a captured one open nothing.
  assert_openai_error(chat(&seven).await, 402, "no-supply");
  // A request naming no model, and the platform's own, are not the person's supply to refuse.
  let (status, _, body) = world.ask("GET", "/v1/models", &[("authorization", &seven)], "").await;
  assert_eq!(status, 200, "{body}");
  let (status, _, body) = chat(&bearer(GATEWAY_KEY)).await;
  assert_eq!(status, 200, "{body}");

  // A reservation opened after the last reading is looked up before the refusal.
  world.write(&reservation(7, "open", Some(i64::MAX))).await;
  let (status, _, body) = chat(&seven).await;
  assert_eq!(status, 200, "{body}");

  // With no hook registered no reservation is opened, so the platform's supply stays out of reach.
  assert_openai_error(chat(&eight).await, 402, "no-supply");
  world.write("DELETE FROM settings WHERE key = 'hook_kept:supply';").await;
  assert_openai_error(chat(&eight).await, 402, "no-supply");
}

#[tokio::test]
async fn the_costs_route_writes_one_request_record_each_and_takes_the_gateways_key_alone() {
  let world = World::new(&call_key(7, "call-key-seven", false)).await.start().await;
  let named = json!({
    "payer": "7", "run": "run-a", "session": "session-a", "model": "pair-0-1",
    "costUsd": 0.0123456, "inputTokens": 10, "outputTokens": 20,
  })
  .to_string();
  let platform = json!({ "payer": "platform", "model": "pair-0-1", "costUsd": 0 }).to_string();

  assert_refused(
    world
      .ask("POST", "/gateway/costs", &[("authorization", &bearer("call-key-seven"))], &named)
      .await,
  );
  for cost in [&named, &platform] {
    let (status, _, body) =
      world.ask("POST", "/gateway/costs", &[("content-type", "application/json")], cost).await;
    assert_eq!(status, 204, "{body}");
  }

  assert_eq!(
    world.recorded().await,
    [
      json!({
        "account": 7, "model": "pair-0-1", "run": "run-a", "session": "session-a",
        "inputTokens": 10, "outputTokens": 20, "costMicros": 12346,
      }),
      json!({
        "account": null, "model": "pair-0-1", "run": null, "session": null,
        "inputTokens": null, "outputTokens": null, "costMicros": 0,
      }),
    ]
  );
}
