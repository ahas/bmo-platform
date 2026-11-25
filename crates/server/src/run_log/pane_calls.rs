//! A pane program's tool calls (docs/run-records.md, 「Agent log」): the terminal client's record says a call began by a
//! `progress` line naming the tool and the state dsh gave it, and that it settled by one naming the state it
//! settled in. Each call is filed as a `tool` line under the tool's name, carrying the summary of its
//! arguments the record keeps, and a `tool-result` line saying how it ended, how long it took and how large
//! its result was, as a stream's calls are, so a turn on that road reads back with its calls. A line that
//! names its call by `toolCallId` is matched to that call; a record that names none, an older one, is matched
//! first in, first out, under a reference made of the turn's id and the call's place in it.

use std::collections::VecDeque;

use super::json::Json;
use super::lines::Line;

// The name the terminal client writes where an update names no tool, as a call's settling does.
const UNNAMED: &str = "tool";

// The states a call stands in before it settles, in dsh's words.
const PENDING: &str = "pending";
const STARTED: [&str; 2] = [PENDING, "in_progress"];

#[derive(Debug)]
struct Open {
  // The call's id where the record names one.
  id: Option<String>,
  reference: String,
  name: String,
  at: Option<i64>,
  pending: bool,
}

/// The calls of one run's pane program begun and not yet settled, and how many it has made.
#[derive(Debug, Default)]
pub struct PaneCalls {
  open: VecDeque<Open>,
  made: u64,
}

fn number(event: &Json, name: &str) -> Option<i64> {
  match event.get(name)? {
    Json::Number(number) => number.as_i64(),
    _ => None,
  }
}

impl PaneCalls {
  /// The lines one `progress` line of the record stands for beside itself: a call's start, its ending, or
  /// none.
  pub fn lines(&mut self, event: &Json) -> Vec<Line> {
    if event.get("phase").and_then(Json::as_str) != Some("tool") {
      return vec![];
    }

    let name = event.get("tool").and_then(Json::as_str).unwrap_or(UNNAMED);
    let state = event.get("state").and_then(Json::as_str);
    let at = number(event, "at");
    let id = event.get("toolCallId").and_then(Json::as_str).filter(|id| !id.is_empty());

    match (state, id) {
      (Some("completed" | "failed"), _) => self.settled(id, state == Some("failed"), event, at),
      (None, Some(id)) => self.begun_by_id(id, name, true, event, at),
      (None, None) => self.begun(name, true, event, at),
      (Some(state), Some(id)) if STARTED.contains(&state) => {
        self.begun_by_id(id, name, state == PENDING, event, at)
      }
      (Some(state), None) if STARTED.contains(&state) => self.begun(name, state == PENDING, event, at),
      _ => vec![],
    }
  }

  // A call named by its id begun: a new one, unless that call is already open, now under way.
  fn begun_by_id(
    &mut self,
    id: &str,
    name: &str,
    pending: bool,
    event: &Json,
    at: Option<i64>,
  ) -> Vec<Line> {
    if let Some(open) = self.open.iter_mut().find(|open| open.id.as_deref() == Some(id)) {
      open.pending = pending;

      return vec![];
    }

    self.made += 1;
    self.open.push_back(Open {
      id: Some(id.to_owned()),
      reference: id.to_owned(),
      name: name.to_owned(),
      at,
      pending,
    });

    vec![Self::call_line(name, id, event)]
  }

  // A call begun in a record that names no call: a new one, unless it is the last one asked still waiting
  // to start, now under way.
  fn begun(&mut self, name: &str, pending: bool, event: &Json, at: Option<i64>) -> Vec<Line> {
    if name == UNNAMED {
      return vec![];
    }

    if let Some(last) = self.open.back_mut()
      && last.pending
      && last.name == name
    {
      last.pending = pending;

      return vec![];
    }

    self.made += 1;

    let turn = event.get("id").and_then(Json::as_str).unwrap_or_default();
    let reference = format!("{turn}:{}", self.made);

    self.open.push_back(Open {
      id: None,
      reference: reference.clone(),
      name: name.to_owned(),
      at,
      pending,
    });

    vec![Self::call_line(name, &reference, event)]
  }

  // A call's own line: its tool, its reference, and the summary of its arguments the record keeps, cut where
  // the record says it cut them.
  fn call_line(name: &str, reference: &str, event: &Json) -> Line {
    Line {
      kind: "tool",
      name: Some(name.to_owned()),
      reference: Some(reference.to_owned()),
      state: Some("requested"),
      text: event.get("input").and_then(Json::as_str).unwrap_or_default().to_owned(),
      upstream_truncated: event.get("inputCut").and_then(Json::as_bool).unwrap_or(false),
      ..Line::default()
    }
  }

  // The call settled — the one its id names, else the oldest open — with how long it ran where both moments
  // are known and how large its result was where the record says.
  fn settled(&mut self, id: Option<&str>, failed: bool, event: &Json, at: Option<i64>) -> Vec<Line> {
    let place = match id {
      Some(id) => self.open.iter().position(|open| open.id.as_deref() == Some(id)),
      None => (!self.open.is_empty()).then_some(0),
    };
    let Some(open) = place.and_then(|place| self.open.remove(place)) else {
      return vec![];
    };
    let word = if failed { "failed" } else { "completed" };
    let mut text = match open.at.zip(at) {
      Some((began, ended)) => format!("{word} in {} ms", (ended - began).max(0)),
      None => word.to_owned(),
    };

    if let Some(bytes) = number(event, "resultBytes") {
      text.push_str(&format!(", {bytes} bytes"));
    }

    let metadata = event.get("bashEditDiff").map(|diff| {
      Json::Object(vec![(
        "toolUseResult".to_owned(),
        Json::Object(vec![("bashEditDiff".to_owned(), diff.clone())]),
      )])
    });

    vec![Line {
      kind: "tool-result",
      reference: Some(open.reference),
      state: Some(if failed { "failed" } else { "succeeded" }),
      text,
      metadata,
      ..Line::default()
    }]
  }

  /// How many calls the run has made.
  pub fn made(&self) -> u64 {
    self.made
  }
}
