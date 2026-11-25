//! What one line of a Claude Code program's record leaves in the run log (docs/run-records.md). A record's
//! assistant and user lines are the stream's assistant and user events, so a turn read out of its record
//! is logged as a stream would be: what the run said and thought, each tool it called, and what each call
//! handed back.

use harness_providers::toolbook;

use crate::door::tool_clock::ClockNote;

use super::json::Json;

/// One line before it is written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
  pub kind: &'static str,
  pub text: String,
  pub name: Option<String>,
  pub reference: Option<String>,
  pub parent: Option<String>,
  pub source: Option<String>,
  pub source_event: Option<i64>,
  pub source_block: Option<i64>,
  pub source_id: Option<String>,
  pub state: Option<&'static str>,
  pub upstream_truncated: bool,
  pub metadata: Option<Json>,
}

/// The states a line stands in: a call asked, a step under way, and how one ended.
pub const LINE_STATES: [&str; 5] = ["requested", "running", "succeeded", "failed", "unverified"];

impl Line {
  /// A note in the bot's own words.
  pub fn note(text: impl Into<String>) -> Self {
    Self {
      kind: "note",
      text: text.into(),
      ..Self::default()
    }
  }

  /// A note of the call clock's: its sentence, and its facts kept as the line's `toolClock`
  /// (docs/agent-control.md, 「A run's tool calls are held to a clock」).
  pub fn clock(note: &ClockNote) -> Self {
    Self {
      metadata: serde_json::from_value(note.fields()).ok(),
      ..Self::note(note.sentence())
    }
  }
}

/// The tools that open a branch: a co-worker, or a search's pooled branch.
pub const BRANCH_TOOLS: [&str; 3] = ["Task", "Agent", "mcp__web__research"];

/// A call as the run log records it: the tool it reached and what that tool was handed.
pub struct CalledTool {
  pub name: String,
  pub input: Json,
  pub through_toolbook: bool,
}

/// The tool a call reached: a toolbook proxy call answered as the inner call, in the same road's
/// spelling; every other call as it was made.
pub fn called_tool(name: &str, input: &Json) -> CalledTool {
  let inner = input.get("tool").and_then(Json::as_str);
  let arguments = input.get("arguments").filter(|arguments| arguments.is_object());

  match (inner.and_then(|inner| toolbook::reached(name, inner)), arguments) {
    (Some(reached), Some(arguments)) => CalledTool {
      name: reached,
      input: arguments.clone(),
      through_toolbook: true,
    },
    _ => CalledTool {
      name: name.to_owned(),
      input: input.clone(),
      through_toolbook: false,
    },
  }
}

fn tool_name(called: &CalledTool) -> String {
  match called.input.get("subagent_type").and_then(Json::as_str) {
    Some(kind) if BRANCH_TOOLS.contains(&called.name.as_str()) && !kind.is_empty() => {
      format!("{}({kind})", called.name)
    }
    _ => called.name.clone(),
  }
}

pub(super) fn upstream_truncated(values: &[Option<&Json>]) -> bool {
  values.iter().flatten().any(|value| {
    value.is_object()
      && ["truncated", "is_truncated", "isTruncated", "output_truncated", "outputTruncated"]
        .iter()
        .any(|name| value.get(name).and_then(Json::as_bool) == Some(true))
  })
}

// A tool's answer as the text it reads as.
pub(super) fn flatten(content: Option<&Json>) -> String {
  match content {
    None | Some(Json::Null) => String::new(),
    Some(Json::String(text)) => text.clone(),
    Some(Json::Array(parts)) => parts.iter().map(|part| flatten(Some(part))).collect::<Vec<_>>().join("\n"),
    Some(block @ Json::Object(_)) => block
      .get("text")
      .and_then(Json::as_str)
      .map_or_else(|| block.stringify(), str::to_owned),
    Some(other) => other.stringify(),
  }
}

fn text_field<'a>(value: &'a Json, name: &str) -> Option<&'a str> {
  value.get(name).and_then(Json::as_str)
}

/// The lines one event of a run's stream leaves: a record line's, and beside them the run's start with its
/// model and tools and its end with how it came out, each tied to the branch it came out of.
pub fn stream_lines(event: &Json) -> Vec<Line> {
  let parent = text_field(event, "parent_tool_use_id").map(str::to_owned);
  let tied = |line: Line| Line {
    parent: parent.clone(),
    ..line
  };

  match text_field(event, "type") {
    Some("system") if text_field(event, "subtype") == Some("init") => {
      let tools: Vec<&str> = event
        .get("tools")
        .and_then(Json::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(Json::as_str)
        .collect();
      let parts: Vec<String> = [
        text_field(event, "model").unwrap_or_default().to_owned(),
        if tools.is_empty() { String::new() } else { format!("tools: {}", tools.join(", ")) },
      ]
      .into_iter()
      .filter(|part| !part.is_empty())
      .collect();

      vec![tied(Line {
        kind: "start",
        text: parts.join(" · "),
        ..Line::default()
      })]
    }
    // A compaction the program made of its own session, how it was set off and how full the session was
    // before it, which is how a job's compactions are counted long after its record is swept.
    Some("system") if text_field(event, "subtype") == Some("compact_boundary") => {
      let meta = event.get("compact_metadata");
      let trigger = meta.and_then(|meta| text_field(meta, "trigger")).unwrap_or("auto");
      let before = meta.and_then(|meta| meta.get("pre_tokens")).map(Json::stringify);

      vec![tied(Line {
        kind: "compact",
        text: before
          .map_or_else(|| trigger.to_owned(), |before| format!("{trigger} · {before} tokens before")),
        ..Line::default()
      })]
    }
    Some("result") => {
      let failed = event.get("is_error").and_then(Json::as_bool) == Some(true);
      let outcome = if failed { "error" } else { "done" };
      let result = text_field(event, "result").map(crate::text::trim).unwrap_or_default();

      vec![tied(Line {
        kind: "end",
        text: if result.is_empty() { outcome.to_owned() } else { format!("{outcome}\n{result}") },
        state: Some(if failed { "failed" } else { "succeeded" }),
        ..Line::default()
      })]
    }
    _ => record_lines(event),
  }
}

/// The lines one record line leaves, each tied to the branch it came out of.
pub fn record_lines(event: &Json) -> Vec<Line> {
  if !event.is_object() {
    return vec![];
  }

  let parent = text_field(event, "parent_tool_use_id").map(str::to_owned);
  let message = event.get("message");
  let blocks = message
    .and_then(|message| message.get("content"))
    .and_then(Json::as_array)
    .unwrap_or_default();
  let source_id = message
    .and_then(|message| text_field(message, "id"))
    .or_else(|| text_field(event, "uuid"))
    .map(str::to_owned);
  let lines = match text_field(event, "type") {
    Some("assistant") => assistant_lines(
      blocks,
      source_id,
      event.get("isApiErrorMessage").and_then(Json::as_bool) == Some(true),
    ),
    Some("user") => result_lines(event, blocks, source_id),
    _ => vec![],
  };

  let request = match text_field(event, "type") {
    Some("assistant") => message.and_then(request_of),
    _ => None,
  };

  lines
    .into_iter()
    .map(|line| Line {
      parent: parent.clone(),
      metadata: with_request(line.metadata, request.as_ref()),
      ..line
    })
    .collect()
}

// The token counts a record's usage may state for one request.
const USAGE_COUNTS: [&str; 4] = [
  "input_tokens",
  "output_tokens",
  "cache_read_input_tokens",
  "cache_creation_input_tokens",
];

// What an assistant message says of the request it answers (docs/api.md, 「log-lines」): its id and the usage
// the provider stated, each count where the record states it; none where it states no usage.
fn request_of(message: &Json) -> Option<Json> {
  let usage = message.get("usage").filter(|usage| usage.is_object())?;
  let counts: Vec<(String, Json)> = USAGE_COUNTS
    .iter()
    .filter_map(|name| match usage.get(name) {
      Some(Json::Number(count)) => Some(((*name).to_owned(), Json::Number(count.clone()))),
      _ => None,
    })
    .collect();
  let mut request = vec![];

  if let Some(id) = text_field(message, "id") {
    request.push(("id".to_owned(), Json::String(id.to_owned())));
  }

  request.push(("usage".to_owned(), Json::Object(counts)));

  Some(Json::Object(request))
}

// A line's metadata with the request its message answers beside what it already kept.
fn with_request(metadata: Option<Json>, request: Option<&Json>) -> Option<Json> {
  let Some(request) = request else { return metadata };
  let mut kept = match metadata {
    Some(Json::Object(kept)) => kept,
    Some(other) => return Some(other),
    None => vec![],
  };

  kept.push(("request".to_owned(), request.clone()));

  Some(Json::Object(kept))
}

fn assistant_lines(blocks: &[Json], source_id: Option<String>, failed: bool) -> Vec<Line> {
  let mut lines = vec![];
  let state = failed.then_some("failed");

  for (place, block) in blocks.iter().enumerate() {
    let source_block = Some(place as i64);

    match text_field(block, "type") {
      Some("text") if text_field(block, "text").is_some() => lines.push(Line {
        kind: "text",
        text: text_field(block, "text").unwrap_or_default().to_owned(),
        source_block,
        source_id: source_id.clone(),
        state,
        ..Line::default()
      }),
      Some("thinking") if text_field(block, "thinking").is_some_and(|text| !text.is_empty()) => {
        lines.push(Line {
          kind: "thinking",
          text: text_field(block, "thinking").unwrap_or_default().to_owned(),
          source_block,
          source_id: source_id.clone(),
          state,
          ..Line::default()
        })
      }
      Some("tool_use") => {
        let empty = Json::Object(vec![]);
        let called =
          called_tool(text_field(block, "name").unwrap_or_default(), block.get("input").unwrap_or(&empty));

        lines.push(Line {
          kind: "tool",
          name: Some(tool_name(&called)),
          reference: text_field(block, "id").map(str::to_owned),
          source_block,
          source_id: source_id.clone(),
          state: Some("requested"),
          upstream_truncated: upstream_truncated(&[Some(block)]),
          text: called.input.stringify(),
          metadata: called
            .through_toolbook
            .then(|| Json::Object(vec![("throughToolbook".to_owned(), Json::Bool(true))])),
          ..Line::default()
        });
      }
      _ => {}
    }
  }

  lines
}

fn result_lines(event: &Json, blocks: &[Json], source_id: Option<String>) -> Vec<Line> {
  let mut lines = vec![];
  let result = event.get("toolUseResult");

  for (place, block) in blocks.iter().enumerate() {
    if text_field(block, "type") != Some("tool_result") {
      continue;
    }

    let mut metadata = vec![];

    if let Some(result) = result {
      metadata.push(("toolUseResult".to_owned(), result.clone()));
    }

    if let Some(error) = block.get("is_error").and_then(Json::as_bool) {
      metadata.push(("isError".to_owned(), Json::Bool(error)));
    }

    lines.push(Line {
      kind: "tool-result",
      reference: text_field(block, "tool_use_id").map(str::to_owned),
      source_block: Some(place as i64),
      source_id: source_id.clone(),
      state: Some(result_state(block, result)),
      upstream_truncated: upstream_truncated(&[Some(event), Some(block), result]),
      metadata: (!metadata.is_empty()).then_some(Json::Object(metadata)),
      text: flatten(block.get("content")),
      ..Line::default()
    });
  }

  lines
}

fn result_state(block: &Json, result: Option<&Json>) -> &'static str {
  match block.get("is_error").and_then(Json::as_bool) {
    Some(true) => return "failed",
    Some(false) => return "succeeded",
    None => {}
  }

  let Some(structured) = result.filter(|result| result.is_object()) else {
    return "unverified";
  };
  let status = text_field(structured, "status").unwrap_or_default().to_lowercase();
  let flag = |name: &str| structured.get(name).and_then(Json::as_bool);

  if flag("is_error") == Some(true)
    || flag("isError") == Some(true)
    || flag("success") == Some(false)
    || structured.get("error").is_some()
    || ["failed", "error", "errored", "cancelled"].contains(&status.as_str())
  {
    return "failed";
  }

  let kind = text_field(structured, "type").unwrap_or_default().to_lowercase();
  let applied = structured.get("structuredPatch").and_then(Json::as_array).is_some()
    || (text_field(structured, "filePath").is_some()
      && ["create", "update", "delete", "write"].contains(&kind.as_str()));

  if flag("success") == Some(true)
    || applied
    || ["completed", "succeeded", "success"].contains(&status.as_str())
  {
    return "succeeded";
  }

  if status == "running" || status == "async_launched" { "running" } else { "unverified" }
}
