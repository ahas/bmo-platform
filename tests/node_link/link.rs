//! The link run whole on this machine (docs/work-nodes.md, 「The link」): `connect` with an OpenSSH
//! stand-in that runs the far end here, so the mirror is read both ways through the real frames, and a
//! dropped connection is made again.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

use serde_json::{Value, json};

/// Waits, bounded, until the condition holds.
fn wait_for(what: &str, mut holds: impl FnMut() -> bool) {
  for _ in 0..200 {
    if holds() {
      return;
    }

    sleep(Duration::from_millis(50));
  }

  panic!("{what} never came");
}

fn read_json(path: &Path) -> Option<Value> {
  serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// This process's start time, as a running record names a process.
fn own_birth() -> String {
  let stat = fs::read_to_string("/proc/self/stat").expect("stat");
  let (_, rest) = stat.rsplit_once(") ").expect("fields");

  rest.split(' ').nth(19).expect("start time").to_owned()
}

struct Link(Child);

impl Drop for Link {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

#[test]
fn the_link_mirrors_both_ways_and_is_made_again_after_its_connection_drops() {
  let binary = crate::support::binary::path("node-link");
  let dir = tempfile::tempdir().expect("dir");
  let here = dir.path().join("here");
  let there = dir.path().join("there");
  let place_here = here.join("agent-places").join("w");
  let place_there = there.join("data").join("agent-places").join("w");

  for made in [&place_here, &place_there, &there.join("data").join("run")] {
    fs::create_dir_all(made).expect("made");
  }

  // OpenSSH standing in: the options and the destination passed over, the far end run here, and the
  // process it runs as written down so the test can cut the connection.
  let ssh = dir.path().join("ssh");

  fs::write(
    &ssh,
    "#!/bin/sh\necho $$ > \"$LINK_PID_FILE\"\nwhile [ \"$#\" -gt 0 ] && [ \"$1\" != serve ]; do binary=\"$1\"; shift; done\nshift\nexec \"$binary\" serve \"$@\"\n",
  )
  .expect("ssh");
  fs::set_permissions(&ssh, fs::Permissions::from_mode(0o755)).expect("runnable");

  let spec = dir.path().join("link.json");
  let state = dir.path().join("state.json");

  fs::write(
    &spec,
    json!({
      "node": "t",
      "ssh": ["-o", "BatchMode=yes", "tester@node"],
      "root": there,
      "binary": binary,
      "herdr": { "local": dir.path().join("h.sock"), "remote": dir.path().join("rh.sock") },
      "ports": [],
      "bridge": dir.path().join("b.sock"),
      "deploy": dir.path(),
      "control": dir.path().join("c.sock"),
      "rewrite": [[here.to_string_lossy(), there.join("data").to_string_lossy()]],
      "push": [{ "local": place_here, "files": ["program.json"], "rewrite": true }],
      "pull": [
        { "remote": place_there, "local": place_here, "files": ["running.json"], "alive": true },
        { "remote": there.join("records"), "local": here.join("records"), "tree": true, "append": true }
      ],
      "proc": dir.path().join("proc"),
      "state": state,
      "follow": [here],
      "withheld": [here.join("env")],
      "handed": [here.join("env/llm-gateway.key")],
      "path_head": ["/node/bin"],
      "env_keep": ["BMO_AGENT_PLACE"],
      "env_add": [["BMO_NODE", "t"]]
    })
    .to_string(),
  )
  .expect("spec");

  let pid_file = dir.path().join("far.pid");
  let _link = Link(
    Command::new(&binary)
      .arg("connect")
      .arg(&spec)
      .env("NODE_LINK_SSH", &ssh)
      .env("NODE_LINK_FIRST_WAIT_MS", "50")
      .env("LINK_PID_FILE", &pid_file)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .expect("connect"),
  );
  let connected = || read_json(&state).is_some_and(|read| read["state"] == "connected");

  wait_for("the connection", connected);

  // Here to there: the program, its paths read as the node's, the bot's token left here. Its settings name the
  // gateway's key for their key helper, and another file of the credential folder beside it.
  let key = here.join("env/llm-gateway.key");

  fs::create_dir_all(here.join("env")).expect("the credential folder");
  fs::write(&key, "gateway-key").expect("the gateway's key");
  fs::write(here.join("env/other.key"), "other").expect("another credential");
  fs::write(
    here.join("settings.json"),
    json!({ "apiKeyHelper": format!("cat '{}'", key.display()), "other": here.join("env/other.key") })
      .to_string(),
  )
  .expect("settings");

  let program = json!({ "command": { "args": ["x", "--settings", here.join("settings.json")], "cwd": "/w", "env": { "BMO_AGENT_PLACE": place_here, "BMO_CONTROL_TOKEN": "secret" } } });

  fs::write(place_here.join("program.json"), program.to_string()).expect("program");
  wait_for("the program on the node", || place_there.join("program.json").exists());

  let pushed = read_json(&place_there.join("program.json")).expect("pushed");
  let key_there = there.join("data/env/llm-gateway.key");

  wait_for("the gateway's key on the node", || key_there.exists());
  assert_eq!(fs::read_to_string(&key_there).expect("the key there"), "gateway-key");
  assert_eq!(fs::metadata(&key_there).expect("its mode").permissions().mode() & 0o777, 0o600);
  assert_eq!(
    read_json(&there.join("data/settings.json")).expect("the settings there")["apiKeyHelper"],
    format!("cat '{}'", key_there.display())
  );
  assert!(!there.join("data/env/other.key").exists(), "no other credential goes");

  assert_eq!(pushed["command"]["env"]["BMO_AGENT_PLACE"], json!(place_there));
  assert_eq!(pushed["command"]["env"]["BMO_NODE"], "t");
  assert!(pushed["command"]["env"]["BMO_CONTROL_TOKEN"].is_null());
  assert!(
    pushed["command"]["env"]["PATH"]
      .as_str()
      .is_some_and(|path| path.starts_with("/node/bin:"))
  );

  // There to here: the running record and which of its processes live, a record as it grows, and
  // the node's load and memory.
  let own = std::process::id();

  fs::write(
    place_there.join("running.json"),
    json!({ "pid": own, "birth": own_birth(), "loop": { "pid": 1, "birth": "0" } }).to_string(),
  )
  .expect("running");
  wait_for("the running record here", || place_here.join("running.json").exists());
  wait_for("this process told alive", || {
    read_json(&place_here.join("node-alive.json")).is_some_and(|alive| {
      alive["alive"].as_array().is_some_and(|all| all.iter().any(|one| one["pid"] == own))
    })
  });

  fs::create_dir_all(there.join("records").join("s1")).expect("records");
  fs::write(there.join("records").join("s1").join("a.jsonl"), "one\n").expect("record");
  wait_for("the record here", || {
    fs::read_to_string(here.join("records/s1/a.jsonl")).is_ok_and(|text| text == "one\n")
  });
  fs::OpenOptions::new()
    .append(true)
    .open(there.join("records/s1/a.jsonl"))
    .and_then(|mut file| std::io::Write::write_all(&mut file, b"two\n"))
    .expect("grown");
  wait_for("the grown record here", || {
    fs::read_to_string(here.join("records/s1/a.jsonl")).is_ok_and(|text| text == "one\ntwo\n")
  });
  wait_for("the node's memory", || dir.path().join("proc").join("meminfo").exists());

  // The connection cut: the link says it dropped, makes it again, and carries on where it was.
  let first = fs::read_to_string(&pid_file).expect("pid").trim().to_owned();

  Command::new("kill").args(["-KILL", &first]).status().expect("kill");
  wait_for("a new connection", || {
    fs::read_to_string(&pid_file).is_ok_and(|pid| pid.trim() != first) && connected()
  });

  fs::write(
    place_here.join("program.json"),
    json!({ "command": { "args": ["y"], "cwd": "/w", "env": {} } }).to_string(),
  )
  .expect("program again");
  wait_for("the new program on the node", || {
    read_json(&place_there.join("program.json")).is_some_and(|read| read["command"]["args"][0] == "y")
  });
}

// "a link to a person's PC mirrors both ways over the hub's connect program with no SSH, and carries home each
// connection taken on a port it names or on the node's bridge socket, saying which port the far end could not
// listen on"
#[test]
fn a_link_to_a_persons_pc_mirrors_and_carries_home_over_the_hubs_connect_program_with_no_ssh() {
  let binary = crate::support::binary::path("node-link");
  let dir = tempfile::tempdir().expect("dir");
  let here = dir.path().join("here");
  let there = dir.path().join("there");
  let place_here = here.join("agent-places").join("w");
  let place_there = there.join("data").join("agent-places").join("w");

  for made in [&place_here, &place_there, &there.join("data").join("run")] {
    fs::create_dir_all(made).expect("made");
  }

  // The far end stands on the PC as its own service, listening at the loopback for the mirror. The PC's loopback is
  // another address of this machine's, so a port it carries home stands apart from the same port here.
  let mirror = std::net::TcpListener::bind("127.0.0.2:0").expect("a port");
  let mirror_port = mirror.local_addr().expect("its address").port();
  drop(mirror);

  // Here, a stand-in for the tool host, answering a line with its name; and a port the PC already holds, which its
  // far end cannot listen on. The home bridge is the near end's own.
  let tool_host = std::net::TcpListener::bind("127.0.0.1:0").expect("a port here");
  let tool_port = tool_host.local_addr().expect("its address").port();
  let held = std::net::TcpListener::bind("127.0.0.2:0").expect("a port held on the PC");
  let held_port = held.local_addr().expect("its address").port();

  std::thread::spawn(move || {
    for stream in tool_host.incoming().flatten() {
      answer_line("tool", Box::new(stream));
    }
  });

  let _far = Link(
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

  // The hub's connect program standing in: its agent and port, joined to that port here, and only for a
  // control credential scoped to that agent alone.
  let connect = dir.path().join("jake-hub");
  let hub_env = dir.path().join("hub.env");

  fs::write(&hub_env, "JAKE_CONTROL_PORT=1\nJAKE_CONTROL_TOKEN=t\nJAKE_CONTROL_KEY=k\n")
    .expect("hub settings");
  fs::write(
    &connect,
    "#!/bin/bash\n[ \"$1\" = connect ] || exit 2\n[ \"${JAKE_CONTROL_CREDENTIAL%%.*}\" = \"$2\" ] || exit 3\nexec 3<>/dev/tcp/127.0.0.2/$3\ncat <&3 &\ncat >&3\n",
  )
  .expect("connect");
  fs::set_permissions(&connect, fs::Permissions::from_mode(0o755)).expect("runnable");

  let spec = dir.path().join("link.json");
  let state = dir.path().join("state.json");

  fs::write(
    &spec,
    json!({
      "node": "u7",
      "ssh": [],
      "root": there,
      "binary": binary,
      "herdr": { "local": dir.path().join("h.sock"), "remote": dir.path().join("rh.sock") },
      "ports": [tool_port, held_port],
      "bridge": dir.path().join("b.sock"),
      "deploy": dir.path(),
      "control": dir.path().join("c.sock"),
      "rewrite": [[here.to_string_lossy(), there.join("data").to_string_lossy()]],
      "push": [{ "local": place_here, "files": ["program.json"], "rewrite": true }],
      "pull": [{ "remote": place_there, "local": place_here, "files": ["running.json"] }],
      "proc": dir.path().join("proc"),
      "state": state,
      "follow": [here],
      "withheld": [],
      "jake": {
        "agent": "u7",
        "connect": [connect, "connect"],
        "hub_env": hub_env,
        "mirror_port": mirror_port,
        "herdr_port": 1
      }
    })
    .to_string(),
  )
  .expect("spec");

  let _link = Link(
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

  wait_for("the connection", || read_json(&state).is_some_and(|read| read["state"] == "connected"));

  // Both ways over the hub's connect program: the program there, its running record here.
  fs::write(place_here.join("program.json"), json!({ "command": { "args": ["x"] } }).to_string())
    .expect("program");
  wait_for("the program on the PC", || place_there.join("program.json").exists());
  fs::write(place_there.join("running.json"), json!({ "pids": [] }).to_string()).expect("running");
  wait_for("the running record here", || place_here.join("running.json").exists());

  // The port the PC holds is said in the link's state, with why; the tool host's is listened on there.
  wait_for("the refused port in the state", || {
    read_json(&state).is_some_and(|read| read["refused"][0]["port"] == held_port)
  });
  assert_eq!(read_json(&state).unwrap()["refused"].as_array().map(Vec::len), Some(1));
  drop(held);

  // A program on the PC reaches the tool host and the home bridge here through the far end; the bridge answers a
  // request it cannot read in its own words.
  let on_pc = std::net::TcpStream::connect(("127.0.0.2", tool_port)).expect("the far end listens");
  on_pc.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
  assert_eq!(ask_line(Box::new(on_pc)), "tool:hi\n");

  let on_pc =
    std::os::unix::net::UnixStream::connect(there.join("data").join("bridge.sock")).expect("bridge");
  on_pc.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
  let said = ask_line(Box::new(on_pc));

  assert!(said.contains("node-link bridge: unreadable request"), "{said:?}");
}

/// A stream either kind of socket gives, read and written alike.
trait ReadWrite: std::io::Read + std::io::Write + Send {}

impl<T: std::io::Read + std::io::Write + Send> ReadWrite for T {}

/// A line read and answered with the stand-in's name ahead of it.
fn answer_line(name: &str, mut stream: Box<dyn ReadWrite>) {
  use std::io::{BufRead, Write};

  let mut line = String::new();

  if std::io::BufReader::new(&mut stream).read_line(&mut line).is_ok() {
    let _ = stream.write_all(format!("{name}:{line}").as_bytes());
  }
}

/// A line asked over a stream and the line answered.
fn ask_line(mut stream: Box<dyn ReadWrite>) -> String {
  use std::io::{BufRead, Write};

  stream.write_all(b"hi\n").expect("asked");
  let mut line = String::new();
  std::io::BufReader::new(stream).read_line(&mut line).expect("answered");
  line
}

#[test]
fn a_link_to_a_node_of_this_machine_starts_its_far_end_under_its_own_words_with_no_ssh() {
  use std::io::{Read, Write};
  use std::os::unix::net::{UnixListener, UnixStream};

  let binary = crate::support::binary::path("node-link");
  let dir = tempfile::tempdir().expect("dir");
  let here = dir.path().join("here");
  let there = dir.path().join("there");
  let link = there.join("link");
  let place_here = here.join("agent-places").join("w");
  let place_there = there.join("data").join("agent-places").join("w");

  for made in [&place_here, &place_there, &there.join("data").join("run"), &link] {
    fs::create_dir_all(made).expect("made");
  }

  // The node's herdr session standing in: one answer to each connection.
  let herdr = dir.path().join("rh.sock");
  let session = UnixListener::bind(&herdr).expect("herdr");

  std::thread::spawn(move || {
    for mut client in session.incoming().flatten() {
      let _ = client.write_all(b"herdr answers\n");
    }
  });

  // The words that would run the far end as the node's own user standing in: the far end's own command line run,
  // and the words it was handed written down.
  let run_as = dir.path().join("run-as");
  let said = dir.path().join("run-as.said");

  fs::write(&run_as, "#!/bin/sh\necho \"$@\" > \"$RUN_AS_SAID\"\nexec \"$@\"\n").expect("run-as");
  fs::set_permissions(&run_as, fs::Permissions::from_mode(0o755)).expect("runnable");

  let spec = dir.path().join("link.json");
  let state = dir.path().join("state.json");
  let herdr_here = dir.path().join("h.sock");
  let bridge = link.join("bridge.sock");

  fs::write(
    &spec,
    json!({
      "node": "account-7",
      "ssh": [],
      "root": there,
      "binary": binary,
      "herdr": { "local": herdr_here, "remote": herdr },
      "ports": [],
      "bridge": bridge,
      "deploy": dir.path(),
      "control": dir.path().join("c.sock"),
      "rewrite": [[here.to_string_lossy(), there.join("data").to_string_lossy()]],
      "push": [{ "local": place_here, "files": ["program.json"], "rewrite": true }],
      "pull": [{ "remote": place_there, "local": place_here, "files": ["running.json"] }],
      "proc": dir.path().join("proc"),
      "state": state,
      "follow": [here],
      "withheld": [],
      "local": { "run_as": [run_as], "herdr": link.join("herdr.sock") }
    })
    .to_string(),
  )
  .expect("spec");

  let _link = Link(
    Command::new(&binary)
      .arg("connect")
      .arg(&spec)
      .env("NODE_LINK_FIRST_WAIT_MS", "50")
      .env("RUN_AS_SAID", &said)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .expect("connect"),
  );

  wait_for("the connection", || read_json(&state).is_some_and(|read| read["state"] == "connected"));

  // The far end went out under the words, handed the bridge the near end holds and the socket it joins to herdr.
  let words = fs::read_to_string(&said).expect("the words");

  assert!(words.contains(" serve --root "), "{words}");
  assert!(words.contains(&format!("--bridge {}", bridge.display())), "{words}");
  assert!(
    words.contains(&format!("--herdr-unix {}", link.join("herdr.sock").display())),
    "{words}"
  );

  // The node's user reaches both sockets in the folder the two share.
  for socket in [&bridge, &link.join("herdr.sock")] {
    wait_for("the shared socket", || socket.exists());
    assert_eq!(fs::metadata(socket).expect("socket").permissions().mode() & 0o777, 0o666);
  }

  // The herdr socket here reaches the node's session through the far end.
  let mut answer = String::new();

  wait_for("herdr through the far end", || {
    UnixStream::connect(&herdr_here).is_ok_and(|mut stream| {
      answer.clear();
      stream.set_read_timeout(Some(Duration::from_secs(5))).is_ok()
        && stream.read_to_string(&mut answer).is_ok()
    }) && answer == "herdr answers\n"
  });

  // Both ways over the far end's standard input and output.
  fs::write(place_here.join("program.json"), json!({ "command": { "args": ["x"] } }).to_string())
    .expect("program");
  wait_for("the program on the node", || place_there.join("program.json").exists());
  fs::write(place_there.join("running.json"), json!({ "pids": [] }).to_string()).expect("running");
  wait_for("the running record here", || place_here.join("running.json").exists());
}

/// One call of the CLI's verbs asked of a home bridge at `socket`, as a node asks it, its input ended at once
/// or held open for as long as the call runs: its exit code, or none where the bridge cannot be reached or the
/// call ended with no exit code. A refused call is answered on its request line alone, so the end of its input
/// may meet a bridge that already closed: as the CLI's asker does, that write's failure ends nothing.
fn ask_bridge(socket: &Path, args: &[&str], input_ends: bool) -> Option<i32> {
  use std::io::{Read, Write};

  let mut stream = std::os::unix::net::UnixStream::connect(socket).ok()?;

  stream.set_read_timeout(Some(Duration::from_secs(10))).expect("a bounded read");
  writeln!(stream, "{}", json!({ "script": "bin/bmo", "args": args })).ok()?;

  if input_ends {
    let _ = stream.write_all(&[0, 0, 0, 0, 0]);
  }

  loop {
    let mut head = [0u8; 5];

    stream.read_exact(&mut head).ok()?;

    let mut body = vec![0; u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize];

    stream.read_exact(&mut body).ok()?;

    if head[0] == node_link::bridge::EXIT {
      return Some(i32::from_be_bytes(body.try_into().expect("an exit code")));
    }
  }
}

/// The link program for `spec`, its far end never reached, so only its home bridge is at work.
fn bridge_only_link(binary: &Path, spec: &Path, ssh: &Path) -> Link {
  Link(
    Command::new(binary)
      .arg("connect")
      .arg(spec)
      .env("NODE_LINK_SSH", ssh)
      .env("NODE_LINK_FIRST_WAIT_MS", "50")
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .expect("connect"),
  )
}

// The home bridge's verbs held by the link program itself (docs/person-nodes.md, 「Who reaches a node」): a
// spec that comes to name verbs while the link runs holds the next call to them; the program killed in the
// middle of a call leaves no run behind; and started again on the same spec it refuses what it refused.
#[test]
fn the_bridge_s_verbs_hold_from_the_next_call_and_across_the_link_killed_mid_call_and_started_again() {
  let binary = crate::support::binary::path("node-link");
  let dir = tempfile::tempdir().expect("dir");
  let deploy = dir.path().join("deploy");
  let ran = dir.path().join("ran");
  let held = dir.path().join("held.pid");
  let bmo = deploy.join(node_link::bridge::VERBS_BINARY);
  let ssh = dir.path().join("ssh");

  fs::create_dir_all(bmo.parent().expect("bin")).expect("bin");
  // The CLI standing in: each call written down, and a held call standing on its input until it ends.
  fs::write(
    &bmo,
    format!(
      "#!/bin/sh\necho \"$@\" >> {}\nif [ \"$2\" = --hold ]; then echo $$ > {}; exec cat > /dev/null; fi\n",
      ran.display(),
      held.display()
    ),
  )
  .expect("bmo");
  fs::write(&ssh, "#!/bin/sh\nexit 1\n").expect("ssh");

  for runnable in [&bmo, &ssh] {
    fs::set_permissions(runnable, fs::Permissions::from_mode(0o755)).expect("runnable");
  }

  let spec = dir.path().join("link.json");
  let socket = dir.path().join("b.sock");
  let write_spec = |verbs: Option<&[&str]>| {
    let moving = dir.path().join("link.json.new");

    fs::write(
      &moving,
      json!({
        "node": "pc",
        "ssh": ["tester@pc"],
        "root": dir.path().join("there"),
        "binary": binary,
        "herdr": { "local": dir.path().join("h.sock"), "remote": dir.path().join("rh.sock") },
        "ports": [],
        "bridge": socket,
        "deploy": deploy,
        "control": dir.path().join("c.sock"),
        "rewrite": [],
        "push": [],
        "pull": [],
        "proc": dir.path().join("proc"),
        "state": dir.path().join("state.json"),
        "follow": [],
        "withheld": [],
        "bridge_verbs": verbs
      })
      .to_string(),
    )
    .expect("spec");
    fs::rename(&moving, &spec).expect("spec in place");
  };

  // A node of the deployment's own asks for any verb.
  write_spec(None);

  let link = bridge_only_link(&binary, &spec, &ssh);

  wait_for("the bridge", || socket.exists());
  assert_eq!(ask_bridge(&socket, &["node", "work"], true), Some(0));

  // An account's record comes to name the node while the link runs: its next call is held to the verbs, and
  // refused on its request line alone, its input still open.
  write_spec(Some(&["agent-status"]));
  assert_eq!(ask_bridge(&socket, &["node", "work"], false), Some(2));
  assert_eq!(ask_bridge(&socket, &["agent-status"], true), Some(0));

  // The link program killed in the middle of a call: the asker gets no exit code and the run ends with it.
  let asker = {
    let socket = socket.clone();

    std::thread::spawn(move || ask_bridge(&socket, &["agent-status", "--hold"], false))
  };

  // The stand-in creates the file before it writes its pid into it, so the run is known once the pid is.
  let held_pid = || fs::read_to_string(&held).map(|pid| pid.trim().to_owned()).unwrap_or_default();

  wait_for("the held run", || !held_pid().is_empty());

  let run = held_pid();

  drop(link);
  assert_eq!(asker.join().expect("asker"), None);
  wait_for("the held run's end", || !Path::new("/proc").join(&run).exists());

  // Started again on the same spec, it refuses what it refused and runs what it ran.
  let _link = bridge_only_link(&binary, &spec, &ssh);

  wait_for("the bridge again", || ask_bridge(&socket, &["agent-status"], true).is_some());
  assert_eq!(ask_bridge(&socket, &["node", "work"], true), Some(2));
  assert_eq!(
    fs::read_to_string(&ran).expect("ran"),
    "node work\nagent-status\nagent-status --hold\nagent-status\n",
    "a refused call runs nothing"
  );
}

// A hub's connect program standing in that takes the link's frames and says nothing back, its output held open so
// the link reads silence rather than an end, counting its starts.
fn silent_connect(dir: &Path) -> std::path::PathBuf {
  let connect = dir.join("jake-hub");

  fs::write(&connect, "#!/bin/sh\necho start >>\"$CONNECT_STARTS\"\nexec cat 3>&1 >/dev/null\n")
    .expect("connect");
  fs::set_permissions(&connect, fs::Permissions::from_mode(0o755)).expect("runnable");
  connect
}

fn jake_spec(dir: &Path, connect: &Path) -> std::path::PathBuf {
  let spec = dir.join("link.json");
  let hub_env = dir.join("hub.env");

  // A control port nothing listens on, so the listens the link asks for reach no hub of this machine's.
  fs::write(&hub_env, "JAKE_CONTROL_PORT=1\nJAKE_CONTROL_TOKEN=t\nJAKE_CONTROL_KEY=k\n")
    .expect("hub settings");
  fs::write(
    &spec,
    json!({
      "node": "u7",
      "ssh": [],
      "root": dir.join("there"),
      "binary": "node-link",
      "herdr": { "local": dir.join("h.sock"), "remote": dir.join("rh.sock") },
      "bridge": dir.join("b.sock"),
      "deploy": dir,
      "control": dir.join("c.sock"),
      "proc": dir.join("proc"),
      "state": dir.join("state.json"),
      "jake": {
        "agent": "u7",
        "connect": [connect, "connect"],
        "hub_env": hub_env,
        "mirror_port": 1,
        "herdr_port": 1
      }
    })
    .to_string(),
  )
  .expect("spec");
  spec
}

#[test]
fn a_connection_the_node_is_not_heard_on_is_dropped_and_dialed_again() {
  let binary = crate::support::binary::path("node-link");
  let dir = tempfile::tempdir().expect("dir");
  let starts = dir.path().join("starts");
  let spec = jake_spec(dir.path(), &silent_connect(dir.path()));
  let _link = Link(
    Command::new(&binary)
      .arg("connect")
      .arg(&spec)
      .env("NODE_LINK_FIRST_WAIT_MS", "50")
      .env("NODE_LINK_UNHEARD_MS", "300")
      .env("CONNECT_STARTS", &starts)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .expect("connect"),
  );

  wait_for("a second dial", || {
    fs::read_to_string(&starts).is_ok_and(|read| read.lines().count() >= 2)
  });

  let state = read_json(&dir.path().join("state.json")).expect("the link's state");

  assert!(
    state["dropped"]
      .as_str()
      .is_some_and(|why| why.contains("nothing was heard from the node")),
    "{state}"
  );
}

#[test]
fn a_connect_program_that_does_not_stand_is_named_by_its_path() {
  let binary = crate::support::binary::path("node-link");
  let dir = tempfile::tempdir().expect("dir");
  let missing = dir.path().join("gone/jake-hub");
  let spec = jake_spec(dir.path(), &missing);
  let _link = Link(
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
  let said = || {
    read_json(&dir.path().join("state.json"))
      .and_then(|state| state["dropped"].as_str().map(str::to_owned))
      .unwrap_or_default()
  };

  wait_for("the reason it could not start", || !said().is_empty());
  assert!(said().contains(&missing.display().to_string()), "{}", said());
}

// A hub's connect program standing in for an agent on a person's PC: refused as the hub refuses a stream to an agent
// that is not attached while the file `AGENT_AWAY` names stands, else joined to the port named on the PC's loopback
// and ended once the far end closes it, as the hub's own ends.
fn returning_connect(dir: &Path) -> std::path::PathBuf {
  let connect = dir.join("jake-hub");

  fs::write(
    &connect,
    "#!/bin/bash\nif [ -e \"$AGENT_AWAY\" ]; then\n  echo 'jake-hub connect: the hub \
     refused the stream (HTTP 409): no agent attached' >&2\n  exit 1\nfi\nexec 3<>/dev/tcp/127.0.0.2/$3\nexec 4<&0\ncat \
     <&4 >&3 &\ncat <&3\nkill $!\n",
  )
  .expect("connect");
  fs::set_permissions(&connect, fs::Permissions::from_mode(0o755)).expect("runnable");
  connect
}

// The spec of a link named `node` to the far end at `mirror_port` on the PC's loopback, its own sockets and state
// under `dir` and the far end's root `there`, pushing the program in its folder `here` to the node's folder under
// the far end's data root.
fn pc_link_spec(
  dir: &Path,
  node: &str,
  connect: &Path,
  mirror_port: u16,
  there: &Path,
) -> std::path::PathBuf {
  let own = dir.join(node);
  let hub_env = dir.join("hub.env");
  let spec = own.join("link.json");

  fs::create_dir_all(own.join("here")).expect("the link's folder");
  fs::write(&hub_env, "JAKE_CONTROL_PORT=1\nJAKE_CONTROL_TOKEN=t\nJAKE_CONTROL_KEY=k\n")
    .expect("hub settings");
  fs::write(
    &spec,
    json!({
      "node": node,
      "ssh": [],
      "root": there,
      "binary": "node-link",
      "herdr": { "local": own.join("h.sock"), "remote": own.join("rh.sock") },
      "bridge": own.join("b.sock"),
      "deploy": dir,
      "control": own.join("c.sock"),
      "proc": own.join("proc"),
      "state": own.join("state.json"),
      "rewrite": [[own.join("here"), there.join("data").join(node)]],
      "push": [{ "local": own.join("here"), "files": ["program.json"] }],
      "jake": {
        "agent": node,
        "connect": [connect, "connect"],
        "hub_env": hub_env,
        "mirror_port": mirror_port,
        "herdr_port": 1
      }
    })
    .to_string(),
  )
  .expect("spec");
  spec
}

// A PC two agents reach the one far end on, as a Windows agent and a Linux one on the same PC both reach its loopback:
// the link whose agent was away comes back to the far end while the other's stands, and stays up when the other link
// is made again, its log saying the tries refused, the link up, and no link down.
#[test]
fn a_link_whose_agent_was_refused_comes_back_and_stays_beside_another_link_to_the_same_far_end() {
  let binary = crate::support::binary::path("node-link");
  let dir = tempfile::tempdir().expect("dir");
  let there = dir.path().join("there");

  fs::create_dir_all(there.join("data").join("run")).expect("made");

  let mirror = std::net::TcpListener::bind("127.0.0.2:0").expect("a port");
  let mirror_port = mirror.local_addr().expect("its address").port();
  drop(mirror);

  let _far = Link(
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
  let connect = returning_connect(dir.path());
  let link = |node: &str, away: &Path| {
    let spec = pc_link_spec(dir.path(), node, &connect, mirror_port, &there);
    let log = fs::File::create(dir.path().join(node).join("log")).expect("the link's log");

    Link(
      Command::new(&binary)
        .arg("connect")
        .arg(&spec)
        .env("NODE_LINK_FIRST_WAIT_MS", "50")
        .env("AGENT_AWAY", away)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .expect("connect"),
    )
  };
  let state = |node: &str| read_json(&dir.path().join(node).join("state.json")).unwrap_or(Value::Null);
  let away = dir.path().join("away");

  // The other agent's link holds the far end.
  let never = dir.path().join("never");
  let other = link("w7", &never);
  wait_for("the other link", || state("w7")["state"] == "connected");

  // The agent is away: the hub refuses its link's streams.
  fs::write(&away, "").expect("away");
  let _returning = link("u7", &away);
  wait_for("a refused stream", || {
    state("u7")["dropped"].as_str().is_some_and(|why| why.contains("409"))
  });

  // The agent attaches again, and its link is up.
  fs::remove_file(&away).expect("back");
  wait_for("the link back", || state("u7")["state"] == "connected");
  let since = state("u7")["since"].clone();

  // The other link is made again, its stream left standing on the PC, and the far end serves its new session.
  drop(other);
  fs::remove_dir_all(dir.path().join("w7").join("proc")).expect("the other link's node files");
  let _other = link("w7", &never);
  wait_for("the other link's new session served", || {
    dir.path().join("w7").join("proc").join("loadavg").exists()
  });

  // The returning link's program still reaches the PC, over the session it came back on.
  fs::write(dir.path().join("u7").join("here").join("program.json"), "{}").expect("program");
  wait_for("the program on the PC", || there.join("data").join("u7").join("program.json").exists());
  assert_eq!(state("u7")["state"], "connected", "{}", state("u7"));
  assert_eq!(state("u7")["since"], since, "{}", state("u7"));

  let log = fs::read_to_string(dir.path().join("u7").join("log")).expect("the link's log");

  assert!(
    log.contains(
      "u7: link not made: the session ended: jake-hub connect: the hub refused the stream (HTTP 409)"
    ),
    "{log}"
  );
  assert!(log.contains("u7: link up\n"), "{log}");
  assert!(!log.contains("link down"), "{log}");
}
