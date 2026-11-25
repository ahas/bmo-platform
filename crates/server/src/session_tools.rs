//! What a resident program's session holds of its line's servers, and what its error stream said
//! (docs/agent-control.md, 「dsh's providers: OpenRouter and DeepSeek」; docs/run-records.md, 「Agent log」):
//! the lines a program's kept error stream gained since the last look, handed to the turn's run log, and,
//! once per program started, each server of a chat line that did not attach told to the session as a fact
//! it reads in its next round and written into the run log beside it. Nothing here fails a turn.

use std::path::Path;

use policy::chat_line::CHAT_LINE;
use policy::fill_marks;
use policy::run_lines::RUN_LINES;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::door::turn::Watch;
use crate::lines::chat::LINE as CHAT;
use crate::lines::place::{Place, stderr_file, token_of};
use crate::lines::token;
use crate::outbox::{Keep, Notice};
use crate::places::Ground;

// Where the place keeps how far its error stream has been read.
fn read_mark(place: &Path) -> std::path::PathBuf {
  place.join("stderr.read")
}

// Where the place keeps which program its servers were last looked at for.
fn looked_mark(place: &Path) -> std::path::PathBuf {
  place.join("tools-looked.json")
}

#[derive(Default, Serialize, Deserialize)]
struct ReadTo {
  offset: u64,
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
struct Looked {
  pid: i32,
  birth: Option<String>,
}

/// The whole lines the place's kept error stream gained since the last look, at most `most` of them, the
/// rest passed over; the look moves past every line it read. A file shorter than the last look is one
/// started again, read from its head.
pub fn error_news(place: &Path, most: usize) -> Vec<String> {
  let Ok(bytes) = std::fs::read(stderr_file(place)) else {
    return vec![];
  };
  let mark = read_mark(place);
  let from: ReadTo = agent_host::disk::read_json(&mark).unwrap_or_default();
  let from = if from.offset as usize > bytes.len() { 0 } else { from.offset as usize };
  let Some(end) = bytes[from..].iter().rposition(|byte| *byte == b'\n').map(|at| from + at + 1) else {
    return vec![];
  };
  let lines: Vec<String> = String::from_utf8_lossy(&bytes[from..end])
    .lines()
    .filter(|line| !line.trim().is_empty())
    .take(most)
    .map(str::to_owned)
    .collect();

  let _ = agent_host::disk::publish_bytes(
    &mark,
    serde_json::to_string(&ReadTo { offset: end as u64 }).unwrap_or_default().as_bytes(),
  );

  lines
}

/// The line of a turn's run log that keeps one line of its program's error stream.
pub fn error_line(line: &str) -> String {
  format!("the program's error stream: {line}")
}

/// The fact a session is told of a server its program did not attach.
pub fn absent_fact(server: &str, why: &str) -> String {
  fill_marks(&CHAT_LINE.tools.absent, &[("server", server), ("why", why)])
}

/// The servers of `line` a program did not attach: each one its harness said did not activate, with the
/// cause it gave, and each tool host namespace of the line no client of the session reached where the host
/// saw it reach some other; a host that saw none says nothing, since a host started since the token was
/// issued has seen none.
pub fn absent(
  line: &str,
  inactive: &[(String, String)],
  connected: Option<&[String]>,
) -> Vec<(String, String)> {
  let mut absent: Vec<(String, String)> = inactive.to_vec();

  if let Some(connected) = connected.filter(|connected| !connected.is_empty()) {
    for namespace in RUN_LINES.namespaces(line) {
      if !connected.contains(namespace) && !absent.iter().any(|(server, _)| server == namespace) {
        absent.push((namespace.clone(), CHAT_LINE.tools.unreached.clone()));
      }
    }
  }

  absent
}

/// The instruction a session in a room is told of an absent server by: the fact of it, with no instruction.
pub fn absent_instruction(agent: &str, room: i64, server: &str, why: &str) -> serde_json::Value {
  json!({
    "agentName": agent,
    "room": room,
    "said": { "fact": absent_fact(server, why) },
    "sort": "notice",
    "kind": format!("session-tools:{server}"),
    "summary": fill_marks(&CHAT_LINE.tools.absent_summary, &[("server", server)]),
  })
}

/// After a turn of a chat line's program: once per program started, each server it did not attach owed to
/// its session in the turn's room in agency's outbox, keyed by its kind, the agent and the room, and written into
/// the turn's run log. `inactive` is what the program's harness said of its servers in the error stream's lines
/// just read.
pub async fn look<W: Watch>(
  ground: &Ground,
  place: &Place,
  agent: &str,
  line: &str,
  room: Option<i64>,
  inactive: &[(String, String)],
  watch: &mut W,
) {
  let Some(room) = room.filter(|_| line == CHAT) else {
    return;
  };
  let Some(running) = place.program_alive() else {
    return;
  };
  let this = Looked {
    pid: running.pid,
    birth: running.birth.clone(),
  };
  let mark = looked_mark(&place.path);

  if agent_host::disk::read_json::<Looked>(&mark).is_some_and(|looked| looked == this) {
    return;
  }

  let token = place.program().as_ref().and_then(token_of).map(str::to_owned);
  let connected = match &token {
    Some(token) => token::connected(ground, token).await,
    None => None,
  };

  for (server, why) in absent(line, inactive, connected.as_deref()) {
    watch.note(&absent_fact(&server, &why)).await;

    let fact = Notice::instruction(absent_instruction(agent, room, &server, &why), Keep::Once);
    let key = format!("{}@{agent}:{room}", fact.key);

    if let Err(why) = crate::outbox::owe(&ground.db, ground.vars(), fact.keyed(key)).await {
      eprintln!("{agent}: the session was not owed its {server} server's absence: {why}");
    }
  }

  let _ =
    agent_host::disk::publish_bytes(&mark, serde_json::to_string(&this).unwrap_or_default().as_bytes());
}
