//! The picture: what herdr holds, kept by applying its events, the watch that keeps it attached, and what a
//! snapshot says of a pane's folder.

use std::collections::HashSet;
use std::time::Duration;

use herdr::picture::{Picture, Watch};
use herdr::{Herdr, Snapshot};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// The run every test names first.
const UNIT: &str = "run-1";

fn pane(id: &str, tab: &str, run: Option<&str>) -> Value {
  json!({ "pane_id": id, "workspace_id": "w1", "tab_id": tab, "tokens": run.map(|run| json!({ "run": run })) })
}

#[test]
fn a_run_comes_with_its_token_and_goes_with_its_pane_its_tab_or_its_workspace() {
  let mut picture = Picture::default();

  picture.apply(&json!({ "event": "pane_created", "data": { "pane": pane("p1", "t1", None) } }));
  assert!(picture.names().is_empty());
  picture.apply(&json!({ "event": "pane_updated", "data": { "pane": pane("p1", "t1", Some(UNIT)) } }));
  picture.apply(&json!({ "event": "pane_updated", "data": { "pane": pane("p2", "t2", Some("run-2")) } }));
  assert_eq!(picture.names(), HashSet::from([UNIT.to_owned(), "run-2".to_owned()]));

  // A move arrives after its old tab was announced closed, carrying itself whole.
  picture.apply(&json!({ "event": "tab_closed", "data": { "tab_id": "t2" } }));
  picture.apply(&json!({ "event": "pane_moved", "data": { "previous_pane_id": "p2", "pane": pane("p3", "t3", Some("run-2")) } }));
  assert!(picture.names().contains("run-2"));

  // A word about a pane already gone does not bring it back.
  picture.apply(&json!({ "event": "pane_exited", "data": { "pane_id": "p1" } }));
  picture.apply(&json!({ "event": "pane_updated", "data": { "pane": pane("p1", "t1", Some(UNIT)) } }));
  assert!(!picture.names().contains(UNIT));

  picture.apply(&json!({ "event": "workspace_closed", "data": { "workspace_id": "w1" } }));
  assert!(picture.names().is_empty());
}

#[tokio::test]
async fn each_attachment_is_counted_once_its_snapshot_stands() {
  let dir = tempfile::tempdir().unwrap();
  let socket = dir.path().join("herdr.sock");
  let listener = tokio::net::UnixListener::bind(&socket).unwrap();

  // The subscription acknowledged and ended at once, so the watch attaches again and again; every
  // snapshot answered with a run in it.
  tokio::spawn(async move {
    while let Ok((stream, _)) = listener.accept().await {
      tokio::spawn(async move {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();

        reader.read_line(&mut line).await.unwrap();

        let asked: Value = serde_json::from_str(&line).unwrap();
        let result = match asked["method"].as_str() {
          Some("session.snapshot") => {
            json!({ "snapshot": { "panes": [pane("p1", "t1", Some("run-x"))] } })
          }
          _ => json!({}),
        };
        let _ = reader
          .into_inner()
          .write_all(format!("{}\n", json!({ "id": asked["id"], "result": result })).as_bytes())
          .await;
      });
    }
  });

  let watch = Watch::start(Herdr::there(socket));
  let mut attachments = watch.attachments();

  tokio::time::timeout(Duration::from_secs(5), attachments.changed())
    .await
    .unwrap()
    .unwrap();
  assert!(*attachments.borrow() >= 1);
  tokio::time::timeout(Duration::from_secs(5), attachments.changed())
    .await
    .unwrap()
    .unwrap();
  assert!(*attachments.borrow() >= 2);
}

#[test]
fn a_snapshot_replaces_everything() {
  let mut picture = Picture::default();
  let snapshot: Snapshot =
    serde_json::from_value(json!({ "panes": [pane("p9", "t9", Some("run-x"))] })).unwrap();

  picture.apply(&json!({ "event": "pane_updated", "data": { "pane": pane("p1", "t1", Some("run-old")) } }));
  picture.install(&snapshot);
  assert_eq!(picture.names(), HashSet::from(["run-x".to_owned()]));
}

#[test]
fn a_pane_whose_folder_was_removed_reads_as_gone() {
  let snapshot: Snapshot = serde_json::from_value(json!({ "panes": [
    { "pane_id": "p1", "cwd": "/srv/releases/one (deleted)" },
    { "pane_id": "p2", "cwd": "/srv/releases/two" },
    { "pane_id": "p3" },
  ] }))
  .unwrap();

  assert_eq!(
    snapshot.panes.iter().map(|pane| pane.cwd_gone()).collect::<Vec<_>>(),
    [true, false, false]
  );
}

#[tokio::test]
async fn the_last_picture_stands_while_the_subscription_is_down_and_goes_once_the_socket_refuses() {
  let dir = tempfile::tempdir().unwrap();
  let socket = dir.path().join("herdr.sock");
  let listener = tokio::net::UnixListener::bind(&socket).unwrap();

  // One attachment whose snapshot holds a run at work in its pane; the subscription then ends and the
  // socket goes away.
  let serving = tokio::spawn(async move {
    for _ in 0..3 {
      let (stream, _) = listener.accept().await.unwrap();
      let mut reader = BufReader::new(stream);
      let mut line = String::new();

      reader.read_line(&mut line).await.unwrap();

      let asked: Value = serde_json::from_str(&line).unwrap();
      let result = match asked["method"].as_str() {
        Some("session.snapshot") => json!({ "snapshot": { "panes": [pane("p1", "t1", Some("run-x"))] } }),
        Some("layout.export") => {
          json!({ "layout": { "root": { "pane_id": "p1", "command": ["claude"] } } })
        }
        _ => json!({}),
      };
      let mut stream = reader.into_inner();

      stream
        .write_all(format!("{}\n", json!({ "id": asked["id"], "result": result })).as_bytes())
        .await
        .unwrap();

      if asked["method"] == "events.subscribe" {
        // Held open until the snapshot is in, then dropped with the listener.
        tokio::time::sleep(Duration::from_millis(300)).await;
      }
    }
  });

  let watch = Watch::start(Herdr::there(socket.clone()));
  let mut attachments = watch.attachments();

  tokio::time::timeout(Duration::from_secs(5), attachments.changed())
    .await
    .unwrap()
    .unwrap();
  assert!(watch.names().contains("run-x"));

  serving.await.unwrap();
  std::fs::remove_file(&socket).unwrap();

  // Dropped, the picture still names the run until a reattach is refused.
  let mut gone = false;

  for _ in 0..50 {
    if watch.names().is_empty() {
      gone = true;
      break;
    }

    tokio::time::sleep(Duration::from_millis(100)).await;
  }

  assert!(gone, "a herdr whose socket refuses holds nothing");
  assert!(!watch.attached());
}
