//! The client: where the socket is, how a frame reads, and one call out and back.

use std::path::PathBuf;
use std::time::Duration;

use herdr::{
  AGENT_BLOCKED, Fault, Herdr, LINE_CAP_BYTES, LayoutNode, ReadSource, TOO_LARGE, command_of_pane,
  read_frame, socket_path,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[test]
fn the_socket_is_under_the_configuration_home_and_nowhere_without_one() {
  assert_eq!(socket_path(Some("/x"), Some("/h")), Some(PathBuf::from("/x/herdr/herdr.sock")));
  assert_eq!(socket_path(Some(""), Some("/h")), Some(PathBuf::from("/h/.config/herdr/herdr.sock")));
  assert_eq!(socket_path(None, None), None);
}

#[test]
fn a_frame_is_a_result_or_what_herdr_said_instead() {
  assert_eq!(read_frame(r#"{"id":"1","result":{"a":1}}"#), Ok(json!({ "a": 1 })));
  assert_eq!(
    read_frame(r#"{"id":"1","error":{"code":"agent_blocked","message":"waiting"}}"#),
    Err(Fault {
      code: AGENT_BLOCKED.to_owned(),
      message: "waiting".to_owned()
    })
  );
  assert_eq!(read_frame("nonsense").unwrap_err().code, "unreadable");
}

#[test]
fn a_panes_command_is_found_in_its_tabs_tree() {
  let root: LayoutNode = serde_json::from_value(json!({
    "first": { "pane_id": "p1", "command": [] },
    "second": { "pane_id": "p2", "command": ["bun", "run.ts"] },
  }))
  .unwrap();

  assert_eq!(command_of_pane(Some(&root), "p2"), ["bun", "run.ts"]);
  assert!(command_of_pane(Some(&root), "p1").is_empty());
  assert!(command_of_pane(None, "p1").is_empty());
}

#[tokio::test]
async fn a_call_is_one_line_out_and_one_line_back_and_a_socket_that_is_not_there_is_unreachable() {
  let dir = tempfile::tempdir().unwrap();
  let socket = dir.path().join("herdr.sock");
  let listener = tokio::net::UnixListener::bind(&socket).unwrap();

  tokio::spawn(async move {
    let (stream, _) = listener.accept().await.unwrap();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    reader.read_line(&mut line).await.unwrap();

    let asked: Value = serde_json::from_str(&line).unwrap();
    let mut stream = reader.into_inner();

    stream
      .write_all(
        format!("{}\n", json!({ "id": asked["id"], "result": { "method": asked["method"] } })).as_bytes(),
      )
      .await
      .unwrap();
  });

  let herdr = Herdr::there(socket.clone());

  assert_eq!(
    herdr.call("pane.close", json!({ "pane_id": "p" }), None).await,
    Ok(json!({ "method": "pane.close" }))
  );
  assert_eq!(
    Herdr::there(dir.path().join("none.sock"))
      .call("x", json!({}), None)
      .await
      .unwrap_err()
      .code,
    "unreachable"
  );
}

#[tokio::test]
async fn a_request_past_herdrs_line_is_refused_and_never_sent() {
  let dir = tempfile::tempdir().unwrap();
  let socket = dir.path().join("herdr.sock");
  let listener = tokio::net::UnixListener::bind(&socket).unwrap();
  let text = "x".repeat(LINE_CAP_BYTES);
  let refused = Herdr::there(socket)
    .call("agent.prompt", json!({ "text": text }), None)
    .await
    .unwrap_err();

  assert_eq!(refused.code, TOO_LARGE);
  assert!(refused.message.contains("agent.prompt"));
  assert!(tokio::time::timeout(Duration::ZERO, listener.accept()).await.is_err());
}

#[tokio::test]
async fn a_call_herdr_answers_late_is_waited_for_and_written_down_as_a_silence_meanwhile() {
  let dir = tempfile::tempdir().unwrap();
  let socket = dir.path().join("herdr.sock");
  let silences = dir.path().join("silences.jsonl");
  let listener = tokio::net::UnixListener::bind(&socket).unwrap();

  tokio::spawn(async move {
    let (stream, _) = listener.accept().await.unwrap();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    reader.read_line(&mut line).await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;

    let asked: Value = serde_json::from_str(&line).unwrap();

    reader
      .into_inner()
      .write_all(format!("{}\n", json!({ "id": asked["id"], "result": { "late": true } })).as_bytes())
      .await
      .unwrap();
  });

  let herdr = Herdr::here(socket, silences.clone());

  assert_eq!(
    herdr.call("agent.list", json!({}), Some(Duration::from_millis(50))).await,
    Ok(json!({ "late": true }))
  );
  assert!(std::fs::read_to_string(&silences).unwrap().contains("agent.list"));
}

// "a node's herdr that never answers is answered silent past its ceiling, and the call goes out once"
#[tokio::test]
async fn a_node_s_herdr_that_never_answers_is_answered_silent_past_its_ceiling() {
  let dir = tempfile::tempdir().unwrap();
  let socket = dir.path().join("herdr.sock");
  let listener = tokio::net::UnixListener::bind(&socket).unwrap();
  let held = tokio::spawn(async move {
    let mut kept = Vec::new();

    while let Ok((stream, _)) = listener.accept().await {
      kept.push(stream);
    }
  });
  let silent = Herdr::there(socket)
    .within(Duration::from_millis(200))
    .call("pane.read", json!({ "pane_id": "p" }), Some(Duration::from_millis(50)))
    .await
    .unwrap_err();

  held.abort();
  assert_eq!(silent.code, herdr::SILENT);
  assert!(silent.message.contains("pane.read"), "{}", silent.message);
}

// "a pane is read by the source words herdr takes"
#[tokio::test]
async fn a_pane_is_read_by_the_source_words_herdr_takes() {
  let dir = tempfile::tempdir().unwrap();
  let socket = dir.path().join("herdr.sock");
  let listener = tokio::net::UnixListener::bind(&socket).unwrap();

  // herdr's own sources: any other word is refused as an unknown variant, as herdr refuses it.
  tokio::spawn(async move {
    loop {
      let (stream, _) = listener.accept().await.unwrap();
      let mut reader = BufReader::new(stream);
      let mut line = String::new();

      reader.read_line(&mut line).await.unwrap();

      let asked: Value = serde_json::from_str(&line).unwrap();
      let source = asked["params"]["source"].as_str().unwrap_or_default().to_owned();
      let answer = if ["visible", "recent", "recent_unwrapped", "detection"].contains(&source.as_str()) {
        json!({ "id": asked["id"], "result": { "read": { "text": format!("{source} {}", asked["params"]["lines"]) } } })
      } else {
        json!({ "id": asked["id"], "error": { "code": "invalid_request", "message": format!("unknown variant `{source}`") } })
      };

      reader.into_inner().write_all(format!("{answer}\n").as_bytes()).await.unwrap();
    }
  });

  let herdr = Herdr::there(socket);

  assert_eq!(
    herdr.read_pane("p", ReadSource::RecentUnwrapped, Some(200)).await,
    Ok("recent_unwrapped 200".to_owned())
  );
  assert_eq!(herdr.read_pane("p", ReadSource::Visible, None).await, Ok("visible null".to_owned()));
  assert_eq!(
    serde_json::to_value([ReadSource::Visible, ReadSource::Recent, ReadSource::RecentUnwrapped]).unwrap(),
    json!(["visible", "recent", "recent_unwrapped"])
  );
}
