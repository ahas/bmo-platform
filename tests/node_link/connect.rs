//! The link's own end: a landing stays inside itself, and the home bridge runs what it is asked.

use std::fs;
use std::path::{Path, PathBuf};

use node_link::bridge;
use node_link::connect::*;
use node_link::frame::{Frame, Pull, encode};
use node_link::spec::{Landing, Spec};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;

#[test]
fn a_landing_takes_no_path_out_of_itself() {
  assert_eq!(landed(Path::new("/l"), "a/b.json"), Some(PathBuf::from("/l/a/b.json")));
  assert_eq!(landed(Path::new("/l"), "../x"), None);
  assert_eq!(landed(Path::new("/l"), "/etc/x"), None);
}

#[test]
fn a_grown_file_is_written_from_the_offset_the_node_names() {
  let dir = tempfile::tempdir().expect("dir");
  let spec = Spec {
    pull: vec![Landing {
      pull: Pull {
        remote: "/r".into(),
        files: vec![],
        tree: true,
        append: true,
        alive: false,
      },
      local: dir.path().to_owned(),
    }],
    ..Spec::default()
  };
  let mut state = State::default();

  apply(
    &spec,
    Frame::Append {
      pull: 0,
      path: "s.jsonl".into(),
      offset: 0,
      data: encode(b"one\n"),
      list: None,
    },
    &mut state,
  );
  apply(
    &spec,
    Frame::Append {
      pull: 0,
      path: "s.jsonl".into(),
      offset: 4,
      data: encode(b"two\n"),
      list: None,
    },
    &mut state,
  );
  assert_eq!(fs::read_to_string(dir.path().join("s.jsonl")).expect("read"), "one\ntwo\n");

  // The node's file replaced: sent again from nothing, and nothing of the old one stays.
  apply(
    &spec,
    Frame::Append {
      pull: 0,
      path: "s.jsonl".into(),
      offset: 0,
      data: encode(b"new\n"),
      list: None,
    },
    &mut state,
  );
  assert_eq!(fs::read_to_string(dir.path().join("s.jsonl")).expect("read"), "new\n");
}

#[tokio::test]
async fn the_bridge_runs_a_deploy_script_with_the_runs_own_variables() {
  let dir = tempfile::tempdir().expect("dir");
  let deploy = dir.path().join("deploy");

  fs::create_dir_all(deploy.join("scripts")).expect("scripts");
  fs::write(
    deploy.join("scripts/echo.ts"),
    "const input = await Bun.stdin.text();\nconsole.log(JSON.stringify({ args: process.argv.slice(2), input, place: process.env.BMO_AGENT_PLACE, node: process.env.BMO_NODE }));\nconsole.error(\"said\");\nprocess.exit(7);\n",
  )
  .expect("script");

  let socket = dir.path().join("bridge.sock");
  let listener = bind(&socket).expect("bind");

  tokio::spawn(async move {
    let (stream, _) = listener.accept().await.expect("accept");

    bridge::answer(stream, deploy, "n1".into(), None, |path| path.replace("/n/data", "/h/data"))
      .await
      .expect("answered");
  });

  let header = json!({ "script": "scripts/echo.ts", "args": ["--show", "/n/data/x"], "env": { "BMO_AGENT_PLACE": "/n/data/agent-places/w" } }).to_string();
  let (out, err, code) = ask_bridge(&socket, &header, b"hello").await;
  let said: Value = serde_json::from_slice(&out).expect("json out");

  assert_eq!(code, 7);
  assert_eq!(String::from_utf8_lossy(&err).trim(), "said");
  assert_eq!(
    said,
    json!({ "args": ["--show", "/h/data/x"], "input": "hello", "place": "/h/data/agent-places/w", "node": "n1" })
  );

  let refused =
    ask_bridge(&socket_again(&dir).await, &json!({ "script": "../x.ts" }).to_string(), b"").await;

  assert_eq!(refused.2, 2);
}

// A person's node asks the bridge for the verbs its spec names alone: a call of one runs, and any other verb, a
// verb named after an option and a script are each refused with exit 2 and run nothing (docs/person-nodes.md,
// 「Who reaches a node」).
#[tokio::test]
async fn a_person_s_node_runs_the_verbs_its_spec_names_alone() {
  use std::os::unix::fs::PermissionsExt;

  let dir = tempfile::tempdir().expect("dir");
  let deploy = dir.path().join("deploy");
  let ran = dir.path().join("ran");
  let bmo = deploy.join(bridge::VERBS_BINARY);

  fs::create_dir_all(bmo.parent().unwrap()).expect("bin");
  fs::create_dir_all(deploy.join("scripts")).expect("scripts");
  fs::write(&bmo, format!("#!/bin/sh\necho \"$@\" >> {}\n", ran.display())).expect("bmo");
  fs::set_permissions(&bmo, fs::Permissions::from_mode(0o755)).expect("mode");
  fs::write(deploy.join("scripts/echo.ts"), "console.log('ran');\n").expect("script");

  let socket = dir.path().join("bridge.sock");
  let listener = bind(&socket).expect("bind");

  tokio::spawn(async move {
    while let Ok((stream, _)) = listener.accept().await {
      let deploy = deploy.clone();
      let verbs = Some(vec!["agent-status".to_owned(), "use".to_owned()]);

      tokio::spawn(bridge::answer(stream, deploy, "pc".into(), verbs, str::to_owned));
    }
  });

  let ask = |script: &str, args: &[&str]| json!({ "script": script, "args": args }).to_string();
  let (_, _, code) = ask_bridge(&socket, &ask("bin/bmo", &["agent-status", "--place", "p"]), b"").await;

  assert_eq!(code, 0);

  for (script, args) in [
    ("bin/bmo", &["node", "work", "setup", "pc"][..]),
    ("bin/bmo", &["--via", "r", "use", "x"][..]),
    ("bin/bmo", &[][..]),
    ("scripts/echo.ts", &[][..]),
  ] {
    let (out, err, code) = ask_bridge(&socket, &ask(script, args), b"").await;

    assert_eq!(code, 2, "{script} {args:?}");
    assert!(out.is_empty(), "{script} {args:?}");
    assert!(String::from_utf8_lossy(&err).contains("agent-status, use"), "{script} {args:?}");
  }

  assert_eq!(
    fs::read_to_string(&ran).expect("ran"),
    "agent-status --place p\n",
    "only the named verb ran"
  );
}

// A second bridge on the same directory, for a refusal.
async fn socket_again(dir: &tempfile::TempDir) -> PathBuf {
  let socket = dir.path().join("bridge-2.sock");
  let listener = bind(&socket).expect("bind");
  let deploy = dir.path().join("deploy");

  tokio::spawn(async move {
    let (stream, _) = listener.accept().await.expect("accept");

    bridge::answer(stream, deploy, "n1".into(), None, str::to_owned)
      .await
      .expect("answered");
  });

  socket
}

/// A bridge asker as the tests drive one: the request line, the input, and the frames read back. A refused call
/// is answered on its request line alone, so its input may meet a bridge that already closed: as the CLI's asker
/// does, a failed write of the input ends nothing and the answer is read.
pub async fn ask_bridge(socket: &Path, header: &str, input: &[u8]) -> (Vec<u8>, Vec<u8>, i32) {
  use tokio::io::AsyncReadExt;

  let mut stream = tokio::net::UnixStream::connect(socket).await.expect("bridge answers");

  stream.write_all(header.as_bytes()).await.expect("header");
  stream.write_all(b"\n").await.expect("line end");

  let input_written = input.is_empty() || bridge::write_frame(&mut stream, bridge::IN, input).await.is_ok();

  if input_written {
    let _ = bridge::write_frame(&mut stream, 0, &[]).await;
  }

  let (mut out, mut err, mut code) = (Vec::new(), Vec::new(), -1);

  loop {
    let mut head = [0u8; 5];

    if stream.read_exact(&mut head).await.is_err() {
      break;
    }

    let length = u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize;
    let mut body = vec![0; length];

    stream.read_exact(&mut body).await.expect("body");

    match head[0] {
      bridge::OUT => out.extend(body),
      bridge::ERR => err.extend(body),
      _ => {
        code = i32::from_be_bytes([body[0], body[1], body[2], body[3]]);
        break;
      }
    }
  }

  (out, err, code)
}

#[test]
fn a_watch_asked_says_how_much_of_each_growing_file_is_held_here() {
  use node_link::frame::Held;

  let dir = tempfile::tempdir().expect("dir");
  let (grown, whole) = (dir.path().join("grown"), dir.path().join("whole"));

  fs::create_dir_all(grown.join("sub")).expect("dir");
  fs::create_dir_all(&whole).expect("dir");
  fs::write(grown.join("sub/s.jsonl"), "one\n").expect("write");
  fs::write(whole.join("running.json"), "{}").expect("write");

  let pull = |remote: &str, append: bool| Pull {
    remote: remote.into(),
    files: vec![],
    tree: true,
    append,
    alive: false,
  };
  let spec = Spec {
    pull: vec![
      Landing {
        pull: pull("/r/whole", false),
        local: whole,
      },
      Landing {
        pull: pull("/r/grown", true),
        local: grown,
      },
    ],
    ..Spec::default()
  };

  let Frame::Watch { pulls, held, list } = watch(&spec, &mut State::default()) else {
    panic!("a watch is asked")
  };

  assert_eq!(pulls, spec.pulls());
  assert_eq!(
    held,
    vec![Held {
      pull: 1,
      path: "sub/s.jsonl".into(),
      len: 4
    }]
  );
  assert!(list.is_some(), "the list is numbered");
}

#[test]
fn a_frame_read_under_an_earlier_watch_list_lands_where_that_list_put_it() {
  let dir = tempfile::tempdir().expect("dir");
  let landing = |name: &str| Landing {
    pull: Pull {
      remote: format!("/r/{name}"),
      files: vec![],
      tree: true,
      append: true,
      alive: false,
    },
    local: dir.path().join(name),
  };
  let before = Spec {
    pull: vec![landing("kept")],
    ..Spec::default()
  };
  // A place came, its pull put first.
  let now = Spec {
    pull: vec![landing("added"), landing("kept")],
    ..Spec::default()
  };
  let mut state = State::default();
  let Frame::Watch { list: earlier, .. } = watch(&before, &mut state) else {
    panic!("a watch is asked")
  };
  let _ = watch(&now, &mut state);
  let append = |pull, list| Frame::Append {
    pull,
    path: "s.jsonl".into(),
    offset: 0,
    data: encode(b"one\n"),
    list,
  };

  apply(&now, append(0, earlier), &mut state);
  assert_eq!(fs::read_to_string(dir.path().join("kept/s.jsonl")).expect("read"), "one\n");
  assert!(
    !dir.path().join("added/s.jsonl").exists(),
    "nothing lands in the pull the new list put first"
  );

  // A far end that numbers no list is read by the spec's own.
  apply(&now, append(0, None), &mut state);
  assert!(dir.path().join("added/s.jsonl").exists());
}
