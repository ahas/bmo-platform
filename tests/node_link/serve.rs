//! The far end on the node: what it watches, sends and writes.

use std::fs;

use node_link::frame::{Identity, Pull};
use node_link::serve::*;

#[test]
fn a_tree_names_every_file_under_it() {
  let dir = tempfile::tempdir().expect("dir");

  fs::create_dir_all(dir.path().join("a/b")).expect("tree");
  fs::write(dir.path().join("a/b/one.jsonl"), "x").expect("one");
  fs::write(dir.path().join("two.jsonl"), "y").expect("two");

  let mut tree = files_of(&Pull {
    remote: dir.path().to_string_lossy().into(),
    files: vec![],
    tree: true,
    append: true,
    alive: false,
  });
  let flat = files_of(&Pull {
    remote: dir.path().to_string_lossy().into(),
    files: vec![],
    tree: false,
    append: true,
    alive: false,
  });

  tree.sort();
  assert_eq!(tree, vec!["a/b/one.jsonl".to_owned(), "two.jsonl".to_owned()]);
  assert_eq!(flat, vec!["two.jsonl".to_owned()]);
}

#[test]
fn only_a_live_process_of_the_record_is_told_alive() {
  let dir = tempfile::tempdir().expect("dir");
  let own = std::process::id() as i32;
  let birth = process_birth(own).expect("own birth");

  fs::write(
    dir.path().join("running.json"),
    serde_json::json!({ "pid": own, "birth": birth, "loop": { "pid": own, "birth": "0" } }).to_string(),
  )
  .expect("record");

  assert_eq!(alive_in(dir.path()), vec![Identity { pid: own, birth }]);
}

// A place under the far end's data root with a program written, and what its running record names.
fn place_with_program(
  places: &std::path::Path,
  name: &str,
  running: Option<serde_json::Value>,
) -> std::path::PathBuf {
  let dir = places.join(name);

  fs::create_dir_all(&dir).expect("place");
  fs::write(dir.join("program.json"), "{}").expect("program");

  if let Some(running) = running {
    fs::write(dir.join("running.json"), running.to_string()).expect("running");
  }

  dir
}

fn place_pull(places: &std::path::Path, name: &str) -> Pull {
  Pull {
    remote: places.join(name).to_string_lossy().into(),
    files: vec![PLACE_RECORD.to_owned(), "start-failed.json".to_owned()],
    tree: false,
    append: false,
    alive: true,
  }
}

// "a place no node's latest watch list names has its program let go and its process ended; a node heard from before
// keeps its own places"
#[test]
fn a_place_no_node_s_watch_list_names_is_let_go_and_another_node_s_are_kept() {
  let dir = tempfile::tempdir().expect("dir");
  let places = dir.path();
  let mut sleeper = std::process::Command::new("sleep").arg("60").spawn().expect("sleep");
  let pid = sleeper.id() as i32;
  let birth = process_birth(pid).expect("birth");
  let own = place_with_program(places, "u7", None);
  let other = place_with_program(places, "u60", None);
  let stale = place_with_program(places, "u12", Some(serde_json::json!({ "pid": pid, "birth": birth })));
  let mirror = Mirror::default();

  // Only the pull of a place's running record names a place; a transcript pull names none.
  let transcripts = Pull {
    remote: dir.path().join("projects").to_string_lossy().into(),
    files: vec![],
    tree: true,
    append: true,
    alive: false,
  };
  let named = places_named(&[place_pull(places, "u60"), transcripts]);

  assert_eq!(named, ["u60".to_owned()].into());

  // The first node to speak keeps its place; every other place with a program is let go, its process ended.
  let mut let_go = mirror.keep_places("jake-wsl", named, places);

  let_go.sort();
  assert_eq!(let_go, vec!["u12".to_owned(), "u7".to_owned()]);
  assert!(other.join("program.json").exists());
  assert!(!own.join("program.json").exists());
  assert!(!stale.join("program.json").exists());
  assert!(
    sleeper.wait().is_ok_and(|status| !status.success()),
    "the stale place's process was not ended"
  );

  // A program written again for a place another node names stays, and the first node's place stays beside it.
  fs::write(own.join("program.json"), "{}").expect("program again");

  assert!(mirror.keep_places("u4", ["u7".to_owned()].into(), places).is_empty());
  assert!(own.join("program.json").exists() && other.join("program.json").exists());

  // A node's list naming none of its places any more lets them go.
  assert_eq!(
    mirror.keep_places("u4", std::collections::BTreeSet::new(), places),
    vec!["u7".to_owned()]
  );
}

// "a newer mirror session of a node ends the node's one before it, and is ended in turn by the next; another node's
// stands beside them"
#[test]
fn a_newer_mirror_session_ends_its_node_s_one_before_it_and_leaves_another_node_s_standing() {
  let mirror = Mirror::default();
  let mut first = mirror.newest("u7");
  let mut other = mirror.newest("w7");

  assert!(first.try_recv().is_err(), "another node's session ends no session of this node");

  let mut second = mirror.newest("u7");

  assert_eq!(first.try_recv(), Ok(()));
  assert!(second.try_recv().is_err());

  let _third = mirror.newest("u7");

  assert_eq!(second.try_recv(), Ok(()));
  assert!(other.try_recv().is_err(), "a node's sessions end no other node's");
}

/// A far end serving `root` on the loopback, the port it listens on, and the first near end's stream to it, the
/// socket `socket` makes. The port is found free and let go for the far end to bind, so another process may take
/// it meanwhile; a far end that could not bind its port has ended, and it is served again on another. Every wait
/// is a timer's, so the far end's task runs between tries, and the whole is bounded.
async fn served(
  root: &std::path::Path,
  socket: impl Fn() -> tokio::net::TcpSocket,
) -> (u16, tokio::task::JoinHandle<std::io::Result<()>>, tokio::net::TcpStream) {
  for _ in 0..20 {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let serving = tokio::spawn(serve(Serve {
      root: root.to_path_buf(),
      bridge: None,
      herdr_session: None,
      herdr_socket: None,
      listen: Some(format!("127.0.0.1:{port}")),
      herdr_listen: None,
      herdr_unix: None,
    }));

    for _ in 0..500 {
      if serving.is_finished() {
        break;
      }

      if let Ok(stream) = socket().connect(([127, 0, 0, 1], port).into()).await {
        return (port, serving, stream);
      }

      tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    serving.abort();
  }

  panic!("the far end listened on no port of the loopback");
}

// "on a person's PC a near end that connects closes the mirror an earlier near end left standing"
#[tokio::test]
async fn on_a_person_s_pc_a_near_end_that_connects_closes_the_mirror_an_earlier_near_end_left_standing() {
  use node_link::frame::Frame;
  use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

  let root = tempfile::tempdir().unwrap();
  let (port, serving, first) = served(root.path(), || tokio::net::TcpSocket::new_v4().unwrap()).await;
  // A near end that is greeted and opens a mirror session, as a link does: the first one's stream, then one of
  // its own to the far end already listening.
  let near_end = |stream: Option<tokio::net::TcpStream>| async move {
    let stream = match stream {
      Some(stream) => stream,
      None => tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap(),
    };
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut hello = String::new();

    tokio::time::timeout(std::time::Duration::from_secs(30), reader.read_line(&mut hello))
      .await
      .expect("the far end greets in time")
      .unwrap();
    assert!(hello.contains("hello"), "{hello}");
    writer.write_all(Frame::Carry { ports: vec![] }.line().as_bytes()).await.unwrap();
    (reader, writer)
  };

  let (mut left, _left_open) = near_end(Some(first)).await;
  let (_now, _now_open) = near_end(None).await;
  let mut rest = Vec::new();
  let closed = tokio::time::timeout(
    std::time::Duration::from_secs(10),
    tokio::io::AsyncReadExt::read_to_end(&mut left, &mut rest),
  )
  .await;

  assert!(matches!(closed, Ok(Ok(_))), "the earlier near end's mirror is closed: {closed:?}");

  serving.abort();
}

// A far end on a person's PC listening on a port of its own, its root, and a near end greeted on it that names its
// node, as a link's does.
async fn pc_far_end() -> (
  tempfile::TempDir,
  tokio::io::BufReader<tokio::net::tcp::OwnedReadHalf>,
  tokio::net::tcp::OwnedWriteHalf,
) {
  use node_link::frame::Frame;
  use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

  let root = tempfile::tempdir().unwrap();
  // The near end reads slowly, as a hub's stream carries the mirror: little of it is held ahead of the reader.
  let (_, _, stream) = served(root.path(), || {
    let socket = tokio::net::TcpSocket::new_v4().unwrap();

    socket.set_recv_buffer_size(16 * 1024).unwrap();
    socket
  })
  .await;
  let (reader, mut writer) = stream.into_split();
  let mut reader = BufReader::new(reader);
  let mut hello = String::new();

  tokio::time::timeout(std::time::Duration::from_secs(30), reader.read_line(&mut hello))
    .await
    .expect("the far end greets in time")
    .unwrap();
  assert!(hello.contains("hello"), "{hello}");
  writer.write_all(Frame::Near { node: "u7".into() }.line().as_bytes()).await.unwrap();

  (root, reader, writer)
}

// The far end's next frame, bounded so a frame that never comes fails the test rather than holding it.
async fn next_frame(
  reader: &mut tokio::io::BufReader<tokio::net::tcp::OwnedReadHalf>,
) -> node_link::frame::Frame {
  use tokio::io::AsyncBufReadExt;

  loop {
    let mut line = String::new();
    let read = tokio::time::timeout(std::time::Duration::from_secs(30), reader.read_line(&mut line))
      .await
      .expect("the far end said its next frame in time")
      .expect("the mirror reads");

    assert!(read > 0, "the far end closed the mirror");

    if let Some(frame) = node_link::frame::Frame::read(&line) {
      return frame;
    }
  }
}

fn transcripts(dir: &std::path::Path) -> Pull {
  Pull {
    remote: dir.display().to_string(),
    files: vec![],
    tree: true,
    append: true,
    alive: false,
  }
}

async fn ask(writer: &mut tokio::net::tcp::OwnedWriteHalf, frame: node_link::frame::Frame) {
  tokio::io::AsyncWriteExt::write_all(writer, frame.line().as_bytes()).await.unwrap();
}

// "on a person's PC a connection on the bridge is asked home ahead of the transcripts on their way"
#[tokio::test]
async fn on_a_person_s_pc_a_connection_on_the_bridge_is_asked_home_ahead_of_the_transcripts_on_their_way() {
  use node_link::frame::Frame;

  let (root, mut reader, mut writer) = pc_far_end().await;
  let dir = root.path().join("projects");
  // Far more than a dial's twenty seconds of a hub's stream: every chunk of it ahead of the dial would hold the
  // bridge's call past its wait.
  let chunks = 96;

  fs::create_dir_all(&dir).unwrap();
  fs::write(dir.join("s.jsonl"), vec![b'x'; chunks * 512 * 1024]).unwrap();
  ask(
    &mut writer,
    Frame::Watch {
      pulls: vec![transcripts(&dir)],
      held: vec![],
      list: Some(1),
    },
  )
  .await;

  while !matches!(next_frame(&mut reader).await, Frame::Append { .. }) {}

  // A hook's call on the node's bridge socket, taken while the transcript is on its way.
  let _call = tokio::net::UnixStream::connect(root.path().join("data/bridge.sock")).await.unwrap();
  let mut ahead = 0;

  loop {
    match next_frame(&mut reader).await {
      Frame::Dial { .. } => break,
      Frame::Append { .. } => ahead += 1,
      _ => {}
    }
  }

  assert!(ahead < chunks / 2, "the dial waited behind {ahead} of the transcript's {chunks} chunks");
}

// "a watch list asked anew goes on with a pull it still names"
#[tokio::test]
async fn a_watch_list_asked_anew_goes_on_with_a_pull_it_still_names() {
  use node_link::frame::Frame;

  let (root, mut reader, mut writer) = pc_far_end().await;
  let (kept, added) = (root.path().join("kept"), root.path().join("added"));

  fs::create_dir_all(&kept).unwrap();
  fs::create_dir_all(&added).unwrap();
  fs::write(kept.join("a.jsonl"), "one\n").unwrap();
  fs::write(added.join("b.jsonl"), "two\n").unwrap();
  ask(
    &mut writer,
    Frame::Watch {
      pulls: vec![transcripts(&kept)],
      held: vec![],
      list: Some(1),
    },
  )
  .await;

  while !matches!(next_frame(&mut reader).await, Frame::Append { path, .. } if path == "a.jsonl") {}

  // A place came: its pull put first, the one already sent moved down the list.
  ask(
    &mut writer,
    Frame::Watch {
      pulls: vec![transcripts(&added), transcripts(&kept)],
      held: vec![],
      list: Some(2),
    },
  )
  .await;

  while !matches!(next_frame(&mut reader).await, Frame::Append { pull: 0, path, .. } if path == "b.jsonl") {
  }

  // Grown in place, as a transcript grows, never cut short on its way.
  std::io::Write::write_all(
    &mut fs::OpenOptions::new().append(true).open(kept.join("a.jsonl")).unwrap(),
    b"more\n",
  )
  .unwrap();

  let grown = loop {
    if let Frame::Append {
      pull,
      path,
      offset,
      data,
      list,
    } = next_frame(&mut reader).await
      && path == "a.jsonl"
    {
      break (pull, offset, data, list);
    }
  };

  assert_eq!(
    grown,
    (1, 4, node_link::frame::encode(b"more\n"), Some(2)),
    "sent as what it grew by, at its new place, under the list that put it there"
  );
}

// "a new mirror session goes on from what the near end holds of each growing file"
#[tokio::test]
async fn a_new_mirror_session_goes_on_from_what_the_near_end_holds_of_each_growing_file() {
  use node_link::frame::{Frame, Held, encode};

  let (root, mut reader, mut writer) = pc_far_end().await;
  let dir = root.path().join("projects");

  fs::create_dir_all(&dir).unwrap();
  fs::write(dir.join("a.jsonl"), "one\ntwo\n").unwrap();
  fs::write(dir.join("c.jsonl"), "x\n").unwrap();

  let held = vec![
    Held {
      pull: 0,
      path: "a.jsonl".into(),
      len: 4,
    },
    // More than the node's file holds: the near end's copy is of another file, so it is sent from nothing.
    Held {
      pull: 0,
      path: "c.jsonl".into(),
      len: 100,
    },
  ];

  ask(
    &mut writer,
    Frame::Watch {
      pulls: vec![transcripts(&dir)],
      held,
      list: Some(1),
    },
  )
  .await;

  let mut sent = std::collections::BTreeMap::new();

  while sent.len() < 2 {
    if let Frame::Append {
      path, offset, data, ..
    } = next_frame(&mut reader).await
    {
      sent.insert(path, (offset, data));
    }
  }

  assert_eq!(sent["a.jsonl"], (4, encode(b"two\n")));
  assert_eq!(sent["c.jsonl"], (0, encode(b"x\n")));
}

// "a watch list numbered by none is sent from nothing as it changes"
#[tokio::test]
async fn a_watch_list_numbered_by_none_is_sent_from_nothing_as_it_changes() {
  use node_link::frame::Frame;

  let (root, mut reader, mut writer) = pc_far_end().await;
  let (kept, added) = (root.path().join("kept"), root.path().join("added"));

  fs::create_dir_all(&kept).unwrap();
  fs::create_dir_all(&added).unwrap();
  fs::write(kept.join("a.jsonl"), "one\n").unwrap();
  fs::write(added.join("b.jsonl"), "two\n").unwrap();
  ask(
    &mut writer,
    Frame::Watch {
      pulls: vec![transcripts(&kept)],
      held: vec![],
      list: None,
    },
  )
  .await;

  while !matches!(next_frame(&mut reader).await, Frame::Append { path, .. } if path == "a.jsonl") {}

  // An older near end, whose frames name no list, could not tell where a frame on its way belongs.
  ask(
    &mut writer,
    Frame::Watch {
      pulls: vec![transcripts(&added), transcripts(&kept)],
      held: vec![],
      list: None,
    },
  )
  .await;

  let again = loop {
    if let Frame::Append {
      pull,
      path,
      offset,
      list,
      ..
    } = next_frame(&mut reader).await
      && path == "a.jsonl"
    {
      break (pull, offset, list);
    }
  };

  assert_eq!(again, (1, 0, None));
}

// "a program on the node calls bmo, and the far end's bmo hands the whole call to the home bridge"
#[test]
fn the_node_s_bmo_hands_its_whole_call_to_the_home_bridge_through_the_link_beside_it() {
  use std::os::unix::fs::PermissionsExt;

  let dir = tempfile::tempdir().expect("dir");
  // A root whose path a shell would split or unquote, so the script's own quoting is what holds it.
  let root = dir.path().join("work place's root");
  let bin = root.join("deploy").join("bin");
  let link = bin.join("node-link");

  fs::create_dir_all(&bin).expect("bin");
  fs::write(&link, "#!/bin/sh\nfor word in \"$@\"; do printf '[%s]' \"$word\"; done\n").expect("link");
  fs::set_permissions(&link, fs::Permissions::from_mode(0o755)).expect("link mode");
  fs::write(bin.join(VERBS_NAME), "stale").expect("an earlier bmo");

  let placed = place_verbs(&root).expect("placed");

  assert_eq!(placed, bin.join(VERBS_NAME));
  assert_eq!(fs::metadata(&placed).expect("bmo").permissions().mode() & 0o777, 0o755);

  let output = std::process::Command::new(&placed)
    .args(["use", "bmo/task", "--input", "{\"title\": \"a 'quoted' $word\"}"])
    .output()
    .expect("bmo runs");

  assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
  assert_eq!(
    String::from_utf8_lossy(&output.stdout),
    "[home][bin/bmo][use][bmo/task][--input][{\"title\": \"a 'quoted' $word\"}]"
  );
  assert!(
    fs::read_dir(&bin)
      .expect("bin")
      .flatten()
      .all(|entry| !entry.file_name().to_string_lossy().starts_with('.')),
    "no half-written script is left beside it"
  );
}
