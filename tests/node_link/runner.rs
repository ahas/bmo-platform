//! Personal tools on a PC (docs/person-nodes.md, 「Personal tools on a PC」): the runner confines a tool to its own
//! version and data folders where bubblewrap makes namespaces here, and refuses what it does not run; and a call asked
//! on the near end's runner socket is carried over a stand-in hub to the far end's runner and answered, its caller
//! going away ending the tool's process on the PC; and a person's local AI on the PC is reached from here through the
//! link (「A local AI on a PC」).

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

use serde_json::{Value, json};

/// Waits, bounded, until the condition holds.
fn wait_for(what: &str, mut holds: impl FnMut() -> bool) {
  for _ in 0..400 {
    if holds() {
      return;
    }

    sleep(Duration::from_millis(50));
  }

  panic!("{what} never came");
}

/// A process the test started, killed and waited on as the test ends.
struct Started(Child);

impl Drop for Started {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

/// A runnable file written whole.
fn runnable(path: &Path, text: &str) {
  fs::create_dir_all(path.parent().unwrap()).unwrap();
  fs::write(path, text).unwrap();
  fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// A version of an owner's tool placed under the runner's root, the files given in it.
fn placed_version(root: &Path, owner: &str, tool: &str, version: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = node_link::runner::version_dir(root, owner, tool, version);

  fs::create_dir_all(&dir).unwrap();

  for (name, text) in files {
    runnable(&dir.join(name), text);
  }

  dir
}

/// The passage's asking line for a call of `tool` at `version` of `owner`, under `run`, with the arguments given.
fn asking(owner: &str, tool: &str, version: &str, run: &[&str], arguments: Value) -> String {
  let call =
    json!({ "protocol": 1, "kind": "call", "entry": format!("my/{tool}"), "arguments": arguments });

  format!(
    "{}\n",
    json!({ "owner": owner, "tool": tool, "version": version, "run": run, "call": call })
  )
}

/// The runner run once with the asking line on its input, its environment's bubblewrap the one given, and every line
/// it answered.
fn run_once(
  root: &Path,
  owner: &str,
  bwrap: Option<&str>,
  asked: &str,
  extra_env: &[(&str, &str)],
) -> Vec<Value> {
  let mut command = Command::new(crate::support::binary::path("node-link"));

  command.args(["runner", "--root"]).arg(root).args(["--owner", owner]);
  command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());

  match bwrap {
    Some(bwrap) => command.env(node_link::runner::BWRAP_VARIABLE, bwrap),
    None => command.env_remove(node_link::runner::BWRAP_VARIABLE),
  };

  for (name, value) in extra_env {
    command.env(name, value);
  }

  let mut child = Started(command.spawn().expect("the runner"));
  let mut input = child.0.stdin.take().unwrap();

  input.write_all(asked.as_bytes()).unwrap();

  // The input stays open until the answer, as a caller's passage does.
  let mut answered = Vec::new();
  let mut lines = BufReader::new(child.0.stdout.take().unwrap());

  loop {
    let mut line = String::new();

    if lines.read_line(&mut line).unwrap_or(0) == 0 {
      break;
    }

    let value: Value = serde_json::from_str(&line).unwrap();
    let last = value["t"] != "said";

    answered.push(value);

    if last {
      break;
    }
  }

  drop(input);
  answered
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

const PROBE: &str = r#"import json, os, sys

call = json.load(sys.stdin)
looked = {}
for path in call["arguments"]["forbidden"]:
    try:
        open(path).read()
        looked[path] = "read"
    except OSError:
        looked[path] = "unread"
open("/data/kept.txt", "w").write("kept")
try:
    open("/tool/written.txt", "w").write("x")
    tool = "written"
except OSError:
    tool = "read-only"
said = {"looked": looked, "tool": tool, "root": sorted(os.listdir("/")), "env": sorted(os.environ)}
print(json.dumps({"content": [{"type": "text", "text": json.dumps(said)}], "isError": False}))
"#;

// "a personal tool reads its own version and data folders alone, writes its data folder alone, and runs in a clean
// environment, where bubblewrap makes namespaces here"
#[test]
fn a_confined_tool_reaches_its_own_folders_alone() {
  if let Err(why) = bwrap_confines() {
    eprintln!(
      "skipped: bubblewrap makes no namespace on this machine, so no tool can be confined here: {why}"
    );
    return;
  }

  let dir = tempfile::tempdir().unwrap();
  let root = dir.path().join("data").join(node_link::runner::PERSONAL_TOOLS);
  let version = placed_version(&root, "a1", "probe", "v1", &[("probe.py", PROBE)]);
  let other = placed_version(&root, "a1", "other", "v1", &[("secret.txt", "other tool's")]);
  let token = dir.path().join("data").join("env").join("tool-token");
  let home_file = dir.path().join("home").join(".claude").join("credentials.json");

  for (file, text) in [(&token, "token"), (&home_file, "credentials")] {
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, text).unwrap();
  }

  let forbidden = [token.clone(), home_file.clone(), other.join("secret.txt")];
  let answered = run_once(
    &root,
    "a1",
    None,
    &asking("a1", "probe", "v1", &["python3", "probe.py"], json!({ "forbidden": forbidden })),
    &[("BMO_TOOL_TOKEN", "never-handed")],
  );
  let end = answered.last().unwrap();

  assert_eq!(end["t"], "end", "{answered:?}");
  assert_eq!(end["code"], 0, "{answered:?}");

  let result: Value = serde_json::from_str(end["out"].as_str().unwrap()).unwrap();
  let said: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();

  for path in &forbidden {
    assert_eq!(said["looked"][path.to_string_lossy().as_ref()], "unread", "{said}");
  }

  assert_eq!(said["tool"], "read-only", "{said}");
  assert!(!version.join("written.txt").exists());
  assert_eq!(
    fs::read_to_string(node_link::runner::data_dir(&root, "a1", "probe").join("kept.txt")).unwrap(),
    "kept"
  );
  assert!(!said["root"].as_array().unwrap().iter().any(|name| name == "home"), "{said}");
  assert!(!said["env"].as_array().unwrap().iter().any(|name| name == "BMO_TOOL_TOKEN"), "{said}");
}

// "the confined command reads the version folder alone at /tool and writes the data folder at /data, with the
// network its own and a clean environment"
#[test]
fn the_confined_command_binds_the_two_folders_alone() {
  let version = Path::new("/r/owners/a1/tools/probe/v1");
  let data = Path::new("/r/owners/a1/data/probe");
  let words = node_link::runner::confined(version, data, &["python3".into(), "probe.py".into()]);
  let joined = words.join(" ");

  assert!(
    joined.starts_with("--unshare-all --die-with-parent --new-session --clearenv"),
    "{joined}"
  );
  assert!(joined.contains("--ro-bind /r/owners/a1/tools/probe/v1 /tool"), "{joined}");
  assert!(joined.contains("--bind /r/owners/a1/data/probe /data"), "{joined}");
  assert!(joined.ends_with("-- python3 probe.py"), "{joined}");
  assert_eq!(words.iter().filter(|word| *word == "--bind").count(), 1, "{joined}");
}

// "the runner refuses a call of another account, a version not placed here and a machine with no bubblewrap"
#[test]
fn the_runner_refuses_what_it_does_not_run() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path().join("tools");

  placed_version(&root, "a1", "weekly", "v1", &[]);

  let refused = |owner: &str, version: &str, bwrap: Option<&str>| {
    let answered =
      run_once(&root, "a1", bwrap, &asking(owner, "weekly", version, &["true"], json!({})), &[]);

    assert_eq!(answered.len(), 1, "{answered:?}");
    assert_eq!(answered[0]["t"], "refused", "{answered:?}");
    answered[0]["why"].as_str().unwrap().to_owned()
  };

  assert!(refused("b2", "v1", Some("true")).contains("no personal tool of the account b2"));
  assert!(refused("a1", "v9", Some("true")).contains("version v9 of the tool weekly is not placed"));
  assert!(refused("a1", "../v1", Some("true")).contains("is no version's name"));
  assert!(refused("a1", "v1", Some("/nonexistent/bwrap")).contains("runs confined or not at all"));
}

// A stand-in for bubblewrap: the command after `--` run unconfined, in the folder bound at /tool and with /tool read as
// that folder, so the link's plumbing is tried where no namespace can be made.
const FAKE_BWRAP: &str = r#"#!/usr/bin/env python3
import os, sys
args = sys.argv[1:]
tool = None
for at, word in enumerate(args):
    if word == "--ro-bind" and args[at + 2] == "/tool":
        tool = args[at + 1]
    if word == "--":
        command = args[at + 1:]
        break
command = [word.replace("/tool/", tool + "/", 1) if word.startswith("/tool/") else word for word in command]
os.chdir(tool)
os.execvp(command[0], command)
"#;

// A stand-in for the hub's connect program: its agent and port, one process joining its input and output to that port
// on the PC's loopback, so killing it closes the stream.
const FAKE_CONNECT: &str = r#"#!/usr/bin/env python3
import os, socket, sys, threading
if sys.argv[1] != "connect":
    sys.exit(2)
stream = socket.create_connection(("127.0.0.2", int(sys.argv[3])))
def back():
    while True:
        data = stream.recv(65536)
        if not data:
            os._exit(0)
        os.write(1, data)
threading.Thread(target=back, daemon=True).start()
while True:
    data = os.read(0, 65536)
    if not data:
        os._exit(0)
    stream.sendall(data)
"#;

/// A free port on the PC's stand-in loopback address.
fn free_port() -> u16 {
  let listener = std::net::TcpListener::bind("127.0.0.2:0").unwrap();

  listener.local_addr().unwrap().port()
}

// "a call asked on the near end's runner socket is carried through the hub to the far end's runner and answered; one
// for another account is refused at the near end; and a call whose caller goes away ends the tool's process on the PC"
#[test]
fn a_call_is_carried_to_the_pc_s_runner_and_ends_with_its_caller() {
  let binary = crate::support::binary::path("node-link");
  let dir = tempfile::tempdir().unwrap();
  let there = dir.path().join("there");
  let tools = there.join("data").join(node_link::runner::PERSONAL_TOOLS);
  let fake_bwrap = dir.path().join("fake-bwrap");
  let connect = dir.path().join("jake-hub");
  let hub_env = dir.path().join("hub.env");
  let pid_file = dir.path().join("tool.pid");
  let mirror_port = free_port();

  runnable(&fake_bwrap, FAKE_BWRAP);
  runnable(&connect, FAKE_CONNECT);
  fs::write(&hub_env, "JAKE_CONTROL_PORT=1\nJAKE_CONTROL_TOKEN=t\nJAKE_CONTROL_KEY=k\n").unwrap();
  placed_version(
    &tools,
    "a1",
    "weekly",
    "v1",
    &[(
      "answer.sh",
      "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"content\":[{\"type\":\"text\",\"text\":\"from the pc\"}],\"isError\":false}'\necho said on the pc >&2\n",
    )],
  );
  placed_version(&tools, "a1", "sleepy", "v1", &[]);
  fs::create_dir_all(there.join("data").join("run")).unwrap();

  let _far = Started(
    Command::new(&binary)
      .args(["serve", "--root"])
      .arg(&there)
      .args(["--listen", &format!("127.0.0.2:{mirror_port}")])
      .env(node_link::runner::BWRAP_VARIABLE, &fake_bwrap)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .expect("serve"),
  );
  let spec = dir.path().join("link.json");
  let runner_socket = dir.path().join("runner.sock");

  fs::write(
    &spec,
    json!({
      "node": "u7",
      "ssh": [],
      "root": there,
      "binary": binary,
      "herdr": { "local": dir.path().join("h.sock"), "remote": dir.path().join("rh.sock") },
      "bridge": dir.path().join("b.sock"),
      "deploy": dir.path(),
      "control": dir.path().join("c.sock"),
      "proc": dir.path().join("proc"),
      "state": dir.path().join("state.json"),
      "jake": {
        "agent": "u7",
        "connect": [connect, "connect"],
        "hub_env": hub_env,
        "mirror_port": mirror_port,
        "herdr_port": 1
      },
      "runner": runner_socket,
      "runner_owner": "a1"
    })
    .to_string(),
  )
  .unwrap();

  let _near = Started(
    Command::new(&binary)
      .arg("connect")
      .arg(&spec)
      .env("NODE_LINK_FIRST_WAIT_MS", "50")
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .expect("connect"),
  );

  wait_for("the runner socket", || runner_socket.exists());

  let ask = |line: &str| {
    let mut stream = std::os::unix::net::UnixStream::connect(&runner_socket).unwrap();

    stream.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    stream.write_all(line.as_bytes()).unwrap();
    stream
  };
  let answers = |stream: std::os::unix::net::UnixStream| -> Vec<Value> {
    let mut answered = Vec::new();

    for line in BufReader::new(stream).lines() {
      let Ok(line) = line else { break };
      let value: Value = serde_json::from_str(&line).unwrap();
      let last = value["t"] != "said";

      answered.push(value);

      if last {
        break;
      }
    }

    answered
  };

  // Answered from the PC, what the tool said on its errors with it.
  let answered = answers(ask(&asking("a1", "weekly", "v1", &["./answer.sh"], json!({}))));
  let end = answered.last().unwrap();

  assert_eq!(end["t"], "end", "{answered:?}");
  assert_eq!(end["code"], 0, "{answered:?}");
  assert!(end["out"].as_str().unwrap().contains("from the pc"), "{answered:?}");
  assert!(answered.iter().any(|one| one["line"] == "said on the pc"), "{answered:?}");

  // Another account's call is refused before it leaves here.
  let refused = answers(ask(&asking("b2", "weekly", "v1", &["./answer.sh"], json!({}))));

  assert_eq!(refused[0]["t"], "refused", "{refused:?}");
  assert!(refused[0]["why"].as_str().unwrap().contains("account b2"), "{refused:?}");

  // A call that would run for long, left by its caller: the tool's process on the PC ends.
  let script = format!("echo $$ > {}; exec sleep 300", pid_file.display());
  let caller = ask(&asking("a1", "sleepy", "v1", &["sh", "-c", &script], json!({})));

  wait_for("the tool's process on the PC", || {
    fs::read_to_string(&pid_file).is_ok_and(|pid| pid.ends_with('\n'))
  });

  let pid: i32 = fs::read_to_string(&pid_file).unwrap().trim().parse().unwrap();

  assert!(Path::new(&format!("/proc/{pid}")).exists());
  drop(caller);
  wait_for("the tool's process ended with its caller", || {
    fs::read_to_string(format!("/proc/{pid}/stat")).map_or(true, |stat| stat.contains(") Z "))
  });
}

// A stand-in for an address a tool may reach: a server on this machine's loopback answering every request
// `reached <path>`, standing until the test ends.
fn stand_in_server() -> u16 {
  let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
  let port = listener.local_addr().unwrap().port();

  std::thread::spawn(move || {
    for stream in listener.incoming().flatten() {
      std::thread::spawn(move || {
        let mut reading = BufReader::new(stream.try_clone().unwrap());
        let mut first = String::new();

        let _ = reading.read_line(&mut first);

        loop {
          let mut header = String::new();

          if reading.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
            break;
          }
        }

        let path = first.split_whitespace().nth(1).unwrap_or("/").to_owned();
        let body = format!("reached {path}");
        let mut stream = stream;
        let _ = write!(
          stream,
          "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
          body.len()
        );
      });
    }
  });

  port
}

const REACHING: &str = r#"import json, os, socket, sys, urllib.request

call = json.load(sys.stdin)
port = call["arguments"]["port"]
said = {}

def tunnel(host):
    try:
        with socket.create_connection(("127.0.0.1", 3128), timeout=10) as stream:
            stream.sendall(f"CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n\r\n".encode())
            answer = stream.recv(4096).decode(errors="replace")
            if " 200 " not in answer.split("\r\n")[0] + " ":
                return answer.split("\r\n")[0]
            stream.sendall(f"GET /tunnelled HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n".encode())
            got = b""
            while True:
                part = stream.recv(4096)
                if not part:
                    break
                got += part
            return got.decode(errors="replace").split("\r\n\r\n", 1)[-1]
    except OSError as why:
        return f"failed: {why}"

def direct(host, to):
    try:
        with socket.create_connection((host, to), timeout=5):
            return "connected"
    except OSError as why:
        return f"failed: {why}"

try:
    said["plain"] = urllib.request.urlopen(f"http://api.example.test:{port}/plain", timeout=10).read().decode()
except Exception as why:
    said["plain"] = f"failed: {why}"
said["tunnel"] = tunnel("api.example.test")
said["other"] = tunnel("other.example.test")
said["address"] = tunnel("127.0.0.1")
said["direct"] = direct("127.0.0.1", port)
said["outside"] = direct("192.0.2.1", 80)
try:
    said["resolved"] = socket.gethostbyname("api.example.test")
except OSError as why:
    said["resolved"] = f"failed: {why}"
said["secret"] = open(os.path.join(os.environ["BMO_SECRETS"], "team-key")).read()
said["secrets"] = sorted(os.listdir("/secrets"))
print("the key is " + said["secret"].strip(), file=sys.stderr)
print(json.dumps({"content": [{"type": "text", "text": json.dumps(said)}], "isError": False}))
"#;

// "a call naming addresses reaches those names through its own proxy and nothing else — another name, an address
// written whole, the PC's loopback and the world beyond all refused or unreached — and a call carrying a secret reads
// it in the confinement alone, its value masked in what the tool says, nothing of the call left on the PC"
#[test]
fn a_call_reaches_its_listed_names_alone_and_reads_its_secrets_for_the_call() {
  if let Err(why) = bwrap_confines() {
    eprintln!(
      "skipped: bubblewrap makes no namespace on this machine, so no tool can be confined here: {why}"
    );
    return;
  }

  let dir = tempfile::tempdir().unwrap();
  let root = dir.path().join("data").join(node_link::runner::PERSONAL_TOOLS);
  let scratch = dir.path().join("tmp");
  let port = stand_in_server();

  fs::create_dir_all(&scratch).unwrap();
  placed_version(&root, "a1", "reaching", "v1", &[("reaching.py", REACHING)]);

  let mut asked: Value = serde_json::from_str(&asking(
    "a1",
    "reaching",
    "v1",
    &["python3", "reaching.py"],
    json!({ "port": port }),
  ))
  .unwrap();

  asked["network"] = json!([format!("api.example.test:{port}")]);
  // "stand-in value" in base64.
  asked["secrets"] = json!({ "team-key": "c3RhbmQtaW4gdmFsdWUK" });

  let answered = run_once(
    &root,
    "a1",
    None,
    &format!("{asked}\n"),
    &[
      (
        node_link::proxy::RESOLVE_VARIABLE,
        "api.example.test=127.0.0.1,other.example.test=127.0.0.1",
      ),
      ("TMPDIR", scratch.to_str().unwrap()),
    ],
  );
  let end = answered.last().unwrap();

  assert_eq!(end["t"], "end", "{answered:?}");
  assert_eq!(end["code"], 0, "{answered:?}");

  let result: Value = serde_json::from_str(end["out"].as_str().unwrap()).unwrap();
  let text = result["content"][0]["text"].as_str().unwrap();
  let said: Value = serde_json::from_str(text).unwrap();

  assert_eq!(said["plain"], "reached /plain", "{said}");
  assert_eq!(said["tunnel"], "reached /tunnelled", "{said}");
  assert!(said["other"].as_str().unwrap().contains("403"), "{said}");
  assert!(said["address"].as_str().unwrap().contains("403"), "{said}");
  assert!(said["direct"].as_str().unwrap().starts_with("failed"), "{said}");
  assert!(said["outside"].as_str().unwrap().starts_with("failed"), "{said}");
  assert!(said["resolved"].as_str().unwrap().starts_with("failed"), "{said}");
  assert_eq!(said["secrets"], json!(["team-key"]), "{said}");
  // What the tool wrote is masked, its output and its errors alike.
  assert_eq!(said["secret"], "[secret team-key]\n", "{said}");
  assert!(answered.iter().any(|one| one["line"] == "the key is [secret team-key]"), "{answered:?}");
  assert!(!format!("{answered:?}").contains("stand-in value"), "{answered:?}");
  // The call's proxy folder went with the call, before its answer.
  assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0);
}

// "a call's confinement binds its proxy and the forwarder where it names addresses, the runner's own binary where its
// run words open on it, and each secret from its own descriptor, read alone"
#[test]
fn a_call_s_confinement_carries_its_proxy_and_secrets() {
  let version = Path::new("/r/owners/a1/tools/probe/v1");
  let data = Path::new("/r/owners/a1/data/probe");
  let opened = node_link::runner::Opened {
    binary: Some("/proc/7/exe".into()),
    proxy: Some("/tmp/bmo-call-1/proxy.sock".into()),
    secrets: vec![("team-key".to_owned(), 9)],
  };
  let words =
    node_link::runner::confined_call(version, data, &["python3".into(), "probe.py".into()], &opened);
  let joined = words.join(" ");

  assert!(joined.contains("--ro-bind /proc/7/exe /.bmo/node-link"), "{joined}");
  assert!(joined.contains("--bind /tmp/bmo-call-1/proxy.sock /.bmo/proxy.sock"), "{joined}");
  assert!(joined.contains("--setenv HTTPS_PROXY http://127.0.0.1:3128"), "{joined}");
  assert!(joined.contains("--perms 0400 --ro-bind-data 9 /secrets/team-key"), "{joined}");
  assert!(
    joined.ends_with(
      "-- /.bmo/node-link forward --listen 127.0.0.1:3128 --to /.bmo/proxy.sock -- python3 probe.py"
    ),
    "{joined}"
  );
  assert_eq!(words.iter().filter(|word| *word == "--unshare-all").count(), 1, "{joined}");

  let plain = node_link::runner::confined_call(version, data, &["true".into()], &Default::default());

  assert_eq!(plain, node_link::runner::confined(version, data, &["true".into()]));

  // A run opening on the runner's own binary, as an MCP server's does, has it bound with no proxy.
  let own = node_link::runner::Opened {
    binary: Some("/proc/7/exe".into()),
    ..Default::default()
  };
  let mcp = ["/.bmo/node-link".to_owned(), "mcp".to_owned()];
  let joined = node_link::runner::confined_call(version, data, &mcp, &own).join(" ");

  assert!(joined.contains("--ro-bind /proc/7/exe /.bmo/node-link"), "{joined}");
  assert!(!joined.contains("proxy"), "{joined}");
  assert!(joined.ends_with("-- /.bmo/node-link mcp"), "{joined}");
}

// "an address a manifest lists is a name, a domain's subdomains or either on a port; an address written whole, a
// name not listed and a name standing for this machine are refused"
#[test]
fn the_proxy_reaches_listed_names_alone() {
  let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
  let reach = node_link::proxy::Reach::of(
    &["api.example.test:8080".to_owned(), "*.example.org".to_owned()],
    node_link::proxy::stand_ins("api.example.test=192.0.2.7"),
  )
  .unwrap();

  runtime.block_on(async {
    assert_eq!(
      reach.reach("API.example.test.", 8080).await.unwrap(),
      ["192.0.2.7:8080".parse().unwrap()]
    );

    for (host, port, why) in [
      ("api.example.test", 443, "not among the addresses"),
      ("example.org", 443, "not among the addresses"),
      ("192.0.2.7", 8080, "is an address"),
      ("[::1]", 443, "is an address"),
    ] {
      let refused = reach.reach(host, port).await.unwrap_err();

      assert!(refused.contains(why), "{host}:{port}: {refused}");
    }
  });

  assert!(node_link::proxy::Reach::of(&["192.0.2.7".to_owned()], Default::default()).is_err());
  assert!(node_link::proxy::this_machine("127.0.0.1".parse().unwrap()));
  assert!(node_link::proxy::this_machine("::ffff:127.0.0.1".parse().unwrap()));
  assert!(node_link::proxy::this_machine(
    std::net::Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1).into()
  ));
  assert!(!node_link::proxy::this_machine("192.0.2.7".parse().unwrap()));
}

// A local AI standing in on the PC's loopback: every connection answered with one HTTP response naming what it was
// asked, standing until the test ends.
fn local_ai_on_the_pc() -> u16 {
  let listener = std::net::TcpListener::bind("127.0.0.2:0").unwrap();
  let port = listener.local_addr().unwrap().port();

  std::thread::spawn(move || {
    for stream in listener.incoming().flatten() {
      let mut reading = BufReader::new(stream.try_clone().unwrap());
      let mut first = String::new();

      let _ = reading.read_line(&mut first);

      let body = format!("asked {}", first.trim());
      let mut writing = stream;
      let _ = write!(
        writing,
        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
      );
    }
  });

  port
}

// "a person's local AI on the PC is reached from here at the port the spec names, each connection carried over the hub
// to the far end, which connects from the PC; a test asked once on the runner socket is told whether the PC reached
// it, and one for another account, or for an address the PC does not reach, is refused"
#[test]
fn a_local_ai_on_the_pc_is_reached_from_here_through_the_link() {
  let binary = crate::support::binary::path("node-link");
  let dir = tempfile::tempdir().unwrap();
  let there = dir.path().join("there");
  let connect = dir.path().join("jake-hub");
  let hub_env = dir.path().join("hub.env");
  let mirror_port = free_port();
  let served = local_ai_on_the_pc();
  let here = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();

  runnable(&connect, FAKE_CONNECT);
  fs::write(&hub_env, "JAKE_CONTROL_PORT=1\nJAKE_CONTROL_TOKEN=t\nJAKE_CONTROL_KEY=k\n").unwrap();
  fs::create_dir_all(there.join("data").join("run")).unwrap();

  let _far = Started(
    Command::new(&binary)
      .args(["serve", "--root"])
      .arg(&there)
      .args(["--listen", &format!("127.0.0.2:{mirror_port}")])
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .expect("serve"),
  );
  let spec = dir.path().join("link.json");
  let runner_socket = dir.path().join("runner.sock");

  fs::write(
    &spec,
    json!({
      "node": "u7",
      "ssh": [],
      "root": there,
      "binary": binary,
      "herdr": { "local": dir.path().join("h.sock"), "remote": dir.path().join("rh.sock") },
      "bridge": dir.path().join("b.sock"),
      "deploy": dir.path(),
      "control": dir.path().join("c.sock"),
      "proc": dir.path().join("proc"),
      "state": dir.path().join("state.json"),
      "jake": {
        "agent": "u7",
        "connect": [connect, "connect"],
        "hub_env": hub_env,
        "mirror_port": mirror_port,
        "herdr_port": 1
      },
      "runner": runner_socket,
      "runner_owner": "a1",
      "reach": [{ "port": here, "to": format!("127.0.0.2:{served}") }]
    })
    .to_string(),
  )
  .unwrap();

  let _near = Started(
    Command::new(&binary)
      .arg("connect")
      .arg(&spec)
      .env("NODE_LINK_FIRST_WAIT_MS", "50")
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .expect("connect"),
  );

  // A request at the port here is answered by the server on the PC.
  let mut answered = String::new();

  wait_for("the local AI reached from here", || {
    let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", here)) else {
      return false;
    };

    stream.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let _ = stream.write_all(b"POST /v1/chat/completions HTTP/1.1\r\nhost: x\r\ncontent-length: 0\r\n\r\n");
    answered.clear();
    let _ = std::io::Read::read_to_string(&mut stream, &mut answered);
    answered.contains("asked POST /v1/chat/completions")
  });

  // A test asked once on the runner socket: told it was reached, then spoken to as the address.
  let ask = |line: Value| {
    let mut stream = std::os::unix::net::UnixStream::connect(&runner_socket).unwrap();

    stream.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    stream.write_all(format!("{line}\n").as_bytes()).unwrap();
    stream
  };
  let first_line = |stream: &std::os::unix::net::UnixStream| -> Value {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    let mut reading = stream;

    while std::io::Read::read(&mut reading, &mut byte).unwrap_or(0) == 1 && byte[0] != b'\n' {
      line.push(byte[0]);
    }

    serde_json::from_slice(&line).unwrap()
  };

  let mut reached = ask(json!({ "owner": "a1", "reach": format!("127.0.0.2:{served}") }));

  assert_eq!(first_line(&reached), json!({ "t": "reached" }));
  reached.write_all(b"GET /v1/models HTTP/1.1\r\nhost: x\r\n\r\n").unwrap();

  let mut said = String::new();
  let _ = std::io::Read::read_to_string(&mut reached, &mut said);

  assert!(said.contains("asked GET /v1/models"), "{said}");

  // Another account's ask is refused here, and an address the PC does not reach is refused with why.
  let other = ask(json!({ "owner": "b2", "reach": format!("127.0.0.2:{served}") }));

  assert_eq!(first_line(&other)["t"], "refused");

  let closed = free_port();
  let unreached = ask(json!({ "owner": "a1", "reach": format!("127.0.0.2:{closed}") }));
  let refusal = first_line(&unreached);

  assert_eq!(refusal["t"], "refused", "{refusal}");
  assert!(refusal["why"].as_str().unwrap().contains(&closed.to_string()), "{refusal}");
}
