//! Where a run's time went, gathered as its lines are read (docs/run-records.md): each tool call timed
//! from the line that made it to the line that answered it, by the clock it is read on, and summed by
//! the tool. A co-worker's call is named by the kind it opened and what it was asked. A branch's own
//! lines say nothing of the run's own time.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::json::Json;
use super::lines::{BRANCH_TOOLS, called_tool};
use crate::now_ms;

/// One span of where a job's time or spending went, as the run log and the runner's clock state it: its
/// kind and name, the seconds and times it took, and the tokens it spent where it is a model's.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSpan {
  pub kind: String,
  pub name: String,
  pub seconds: f64,
  pub times: i64,
  pub tokens: Option<i64>,
  pub input_tokens: Option<i64>,
  pub output_tokens: Option<i64>,
  pub cache_read_tokens: Option<i64>,
  pub cache_write_tokens: Option<i64>,
}

// The tool a workflow is launched with, whose time the workflow's own branches account for.
const WORKFLOW_TOOL: &str = "Workflow";

// How much of a co-worker's instruction names it.
const NAMED_INSTRUCTION: usize = 60;

struct Open {
  kind: &'static str,
  name: String,
  at: i64,
}

/// The tally one run keeps.
#[derive(Default)]
pub struct SpanTally {
  open: HashMap<String, Open>,
  totals: Vec<JobSpan>,
}

impl SpanTally {
  /// One line of the record seen now.
  pub fn see(&mut self, event: &Json) {
    if event.get("parent_tool_use_id").and_then(Json::as_str).is_some() {
      return;
    }

    let blocks = event
      .get("message")
      .and_then(|message| message.get("content"))
      .and_then(Json::as_array)
      .unwrap_or_default();

    match event.get("type").and_then(Json::as_str) {
      Some("assistant") => self.calls(blocks),
      Some("user") => self.answers(blocks),
      _ => {}
    }
  }

  /// Every span closed so far.
  pub fn spans(&self) -> Vec<JobSpan> {
    self.totals.clone()
  }

  fn calls(&mut self, blocks: &[Json]) {
    let empty = Json::Object(vec![]);

    for block in blocks {
      let called = called_tool(
        block.get("name").and_then(Json::as_str).unwrap_or_default(),
        block.get("input").unwrap_or(&empty),
      );
      let name = called.name.clone();
      let id = block.get("id").and_then(Json::as_str);

      if block.get("type").and_then(Json::as_str) != Some("tool_use")
        || name.is_empty()
        || name == WORKFLOW_TOOL
      {
        continue;
      }

      let Some(id) = id else { continue };
      let branch = BRANCH_TOOLS.contains(&name.as_str());

      self.open.insert(
        id.to_owned(),
        Open {
          kind: if branch { "agent" } else { "tool" },
          name: if branch { coworker_name(&name, &called.input) } else { name },
          at: now_ms(),
        },
      );
    }
  }

  fn answers(&mut self, blocks: &[Json]) {
    for block in blocks {
      let Some(id) = block.get("tool_use_id").and_then(Json::as_str) else { continue };

      if block.get("type").and_then(Json::as_str) != Some("tool_result") {
        continue;
      }

      let Some(call) = self.open.remove(id) else { continue };
      let seconds = (now_ms() - call.at) as f64 / 1000.0;

      match self.totals.iter_mut().find(|held| held.kind == call.kind && held.name == call.name) {
        Some(held) => {
          held.seconds += seconds;
          held.times += 1;
        }
        None => self.totals.push(JobSpan {
          kind: call.kind.to_owned(),
          name: call.name,
          seconds,
          times: 1,
          tokens: None,
          input_tokens: None,
          output_tokens: None,
          cache_read_tokens: None,
          cache_write_tokens: None,
        }),
      }
    }
  }
}

/// A co-worker's span named by the kind it opened and what it was asked.
pub fn coworker_name(tool: &str, input: &Json) -> String {
  let head = match input.get("subagent_type").and_then(Json::as_str) {
    Some(kind) if !kind.is_empty() => format!("{tool}({kind})"),
    _ => tool.to_owned(),
  };

  match input.get("description").and_then(Json::as_str) {
    Some(said) if !said.is_empty() => format!("{head} · {}", clip(said)),
    _ => head,
  }
}

fn clip(text: &str) -> String {
  let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
  let units: Vec<u16> = flat.encode_utf16().collect();

  if units.len() <= NAMED_INSTRUCTION {
    flat
  } else {
    format!("{}…", String::from_utf16_lossy(&units[..NAMED_INSTRUCTION]))
  }
}

/// The spans of two tallies of one job as one: a span both kept is summed.
pub fn merged(first: &[JobSpan], then: &[JobSpan]) -> Vec<JobSpan> {
  let mut all: Vec<JobSpan> = first.to_vec();

  for span in then {
    match all.iter_mut().find(|held| held.kind == span.kind && held.name == span.name) {
      Some(held) => {
        held.seconds += span.seconds;
        held.times += span.times;
        held.tokens = match (held.tokens, span.tokens) {
          (held, None) => held,
          (held, Some(more)) => Some(held.unwrap_or(0) + more),
        };
      }
      None => all.push(span.clone()),
    }
  }

  all
}
