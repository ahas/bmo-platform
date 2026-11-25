//! What a run of `codex exec` leaves in the run log (docs/run-records.md): the lines one event of its stream
//! leaves, read as the TypeScript run log reads them, and the diffs of each file it changed, read off the
//! thread's session record (its rollout) where the run keeps one. Codex's stream carries completed items
//! rather than message blocks, so only a completed message is a line, and a tool's start and its result
//! share the item's id. The stream's change names only the paths it touched; what was done to each is in
//! the record, whose own item carries another id, so a change is matched by the set of its paths, in the
//! order they happened. The program standing in a pane leaves no stream: its turn is read out of the rollout
//! itself, whose response items are the messages, reasoning, calls and outputs.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use super::json::Json;
use super::lines::{Line, called_tool, flatten, upstream_truncated};

// The tool each kind of item is logged as.
const TOOLS: [(&str, &str); 5] = [
  ("command_execution", "Bash"),
  ("mcp_tool_call", "MCP"),
  ("file_change", "Edit"),
  ("web_search", "web_search"),
  ("collab_tool_call", "Agent"),
];

fn text_field<'a>(value: &'a Json, name: &str) -> Option<&'a str> {
  value.get(name).and_then(Json::as_str)
}

// The first of the fields named that the value holds and that is not null.
fn first_held<'a>(value: &'a Json, names: &[&str]) -> Option<&'a Json> {
  names.iter().find_map(|name| value.get(name).filter(|held| **held != Json::Null))
}

/// The lines one event of `codex exec`'s stream leaves.
pub fn codex_lines(event: &Json) -> Vec<Line> {
  if !event.is_object() {
    return vec![];
  }

  let kind = text_field(event, "type");

  if kind == Some("thread.started") {
    let thread = text_field(event, "thread_id");

    return vec![Line {
      source_id: thread.map(str::to_owned),
      ..Line::note(format!("Codex thread {}", thread.unwrap_or_default()))
    }];
  }

  if matches!(kind, Some("turn.failed" | "error")) {
    let said = event
      .get("error")
      .and_then(|error| text_field(error, "message"))
      .or_else(|| text_field(event, "message"))
      .unwrap_or("Codex error");

    return vec![Line {
      state: Some("failed"),
      ..Line::note(said)
    }];
  }

  let Some(item) = event.get("item").filter(|item| item.is_object()) else {
    return vec![];
  };
  let completed = match kind {
    Some("item.completed") => true,
    Some("item.started") => false,
    _ => return vec![],
  };
  let text = text_field(item, "text").unwrap_or_default();
  let reference = text_field(item, "id").map(str::to_owned);
  let item_type = text_field(item, "type").unwrap_or_default();

  if matches!(item_type, "agent_message" | "reasoning") {
    if !completed || text.is_empty() {
      return vec![];
    }

    return vec![Line {
      kind: if item_type == "reasoning" { "thinking" } else { "text" },
      text: text.to_owned(),
      source_id: reference,
      upstream_truncated: upstream_truncated(&[Some(item)]),
      ..Line::default()
    }];
  }

  let Some(name) = TOOLS.iter().find(|(kind, _)| *kind == item_type).map(|(_, name)| *name) else {
    return vec![];
  };

  if !completed {
    let called = (item_type == "mcp_tool_call").then(|| {
      let server = text_field(item, "server").unwrap_or("mcp");
      let tool = text_field(item, "tool").unwrap_or("tool");

      called_tool(&format!("{server}/{tool}"), item.get("arguments").unwrap_or(&Json::Null))
    });
    let input = if item_type == "command_execution" {
      Json::Object(
        item
          .get("command")
          .map(|command| ("command".to_owned(), command.clone()))
          .into_iter()
          .collect(),
      )
    } else {
      called
        .as_ref()
        .map(|called| &called.input)
        .filter(|input| **input != Json::Null)
        .or_else(|| first_held(item, &["arguments", "changes"]))
        .unwrap_or(item)
        .clone()
    };

    return vec![Line {
      kind: "tool",
      name: Some(called.as_ref().map_or_else(|| name.to_owned(), |called| called.name.clone())),
      reference: reference.clone(),
      source_id: reference,
      state: Some("running"),
      upstream_truncated: upstream_truncated(&[Some(item)]),
      text: input.stringify(),
      metadata: called
        .filter(|called| called.through_toolbook)
        .map(|_| Json::Object(vec![("throughToolbook".to_owned(), Json::Bool(true))])),
      ..Line::default()
    }];
  }

  let result = first_held(item, &["aggregated_output", "result", "error", "changes"]).unwrap_or(item);

  vec![Line {
    kind: "tool-result",
    name: Some(name.to_owned()),
    reference: reference.clone(),
    source_id: reference,
    state: Some(result_state(item)),
    upstream_truncated: upstream_truncated(&[Some(item)]),
    metadata: Some(Json::Object(vec![("item".to_owned(), item.clone())])),
    text: result.as_str().map_or_else(|| result.stringify(), str::to_owned),
    ..Line::default()
  }]
}

// How a tool's item ended, by its error, its status and its exit.
fn result_state(item: &Json) -> &'static str {
  let status = text_field(item, "status").unwrap_or_default().to_lowercase();
  let number = |name: &str| match item.get(name) {
    Some(Json::Number(number)) => number.as_f64(),
    _ => None,
  };
  let exit = number("exit_code").or_else(|| number("exitCode"));

  if item.get("error").is_some()
    || ["failed", "error", "errored", "cancelled"].contains(&status.as_str())
    || exit.is_some_and(|exit| exit != 0.0)
  {
    return "failed";
  }

  if ["completed", "succeeded", "success"].contains(&status.as_str()) || exit == Some(0.0) {
    return "succeeded";
  }

  "unverified"
}

/// What one line of a thread's rollout is, where it is one of the turn's own response items (docs/agent-control.md,
/// 「What a resident turn reads」): the item; none for the rest of the rollout.
pub fn rollout_item(record: &Json) -> Option<&Json> {
  (text_field(record, "type") == Some("response_item"))
    .then(|| record.get("payload"))
    .flatten()
    .filter(|item| item.is_object())
}

/// What a tool call of the rollout was handed: a function's arguments as the object they are written as, a custom
/// tool's input as the text it is.
pub fn rollout_input(item: &Json) -> Json {
  match (text_field(item, "arguments"), text_field(item, "input")) {
    (Some(arguments), _) => Json::parse(arguments).unwrap_or_else(|| Json::String(arguments.to_owned())),
    (None, Some(input)) => Json::String(input.to_owned()),
    (None, None) => Json::Object(vec![]),
  }
}

/// The lines one line of a thread's rollout leaves: what the program said and thought, each tool it called, and
/// what each call handed back, tied to the call by its id. Neither the turn's context nor the words put to it is
/// a line, and a call's output carries no mark of failure, so how it ended is left unverified.
pub fn rollout_lines(record: &Json) -> Vec<Line> {
  let Some(item) = rollout_item(record) else {
    return vec![];
  };
  let source_id = text_field(item, "id").map(str::to_owned);
  let call = text_field(item, "call_id").map(str::to_owned);

  match text_field(item, "type").unwrap_or_default() {
    "message" if text_field(item, "role") == Some("assistant") => item
      .get("content")
      .and_then(Json::as_array)
      .unwrap_or_default()
      .iter()
      .enumerate()
      .filter(|(_, block)| text_field(block, "type") == Some("output_text"))
      .filter_map(|(place, block)| {
        text_field(block, "text").filter(|text| !text.is_empty()).map(|text| Line {
          kind: "text",
          text: text.to_owned(),
          source_block: Some(place as i64),
          source_id: source_id.clone(),
          ..Line::default()
        })
      })
      .collect(),
    "reasoning" => {
      let said: Vec<&str> = item
        .get("summary")
        .and_then(Json::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(|part| text_field(part, "text"))
        .filter(|text| !text.is_empty())
        .collect();

      if said.is_empty() {
        return vec![];
      }

      vec![Line {
        kind: "thinking",
        text: said.join("\n\n"),
        source_id,
        ..Line::default()
      }]
    }
    "function_call" | "custom_tool_call" => {
      let called = called_tool(text_field(item, "name").unwrap_or("tool"), &rollout_input(item));

      vec![Line {
        kind: "tool",
        name: Some(called.name.clone()),
        reference: call,
        source_id,
        state: Some("requested"),
        text: called.input.as_str().map_or_else(|| called.input.stringify(), str::to_owned),
        metadata: called
          .through_toolbook
          .then(|| Json::Object(vec![("throughToolbook".to_owned(), Json::Bool(true))])),
        ..Line::default()
      }]
    }
    "function_call_output" | "custom_tool_call_output" => vec![Line {
      kind: "tool-result",
      reference: call,
      source_id,
      state: Some("unverified"),
      upstream_truncated: upstream_truncated(&[Some(item)]),
      text: flatten(item.get("output")),
      ..Line::default()
    }],
    _ => vec![],
  }
}

/// The paths a `file_change` item of the stream names; none where it names them in no form this reads.
pub fn stream_paths(item: &Json) -> Option<Vec<String>> {
  item
    .get("changes")?
    .as_array()?
    .iter()
    .map(|change| text_field(change, "path").map(str::to_owned))
    .collect()
}

fn number(value: usize) -> Json {
  Json::Number(serde_json::Number::from(value as u64))
}

/// The hunks of a unified diff, each with its numbers and its lines as the diff writes them, less the marks
/// that are no line of the file; a diff the reader cannot read has none.
pub fn unified_hunks(diff: &str) -> Vec<Json> {
  let patch = match diffy::Patch::from_str(diff) {
    Ok(patch) => patch,
    Err(why) => {
      eprintln!("a Codex change's diff could not be read: {why}");
      return vec![];
    }
  };

  patch
    .hunks()
    .iter()
    .map(|hunk| {
      let lines = hunk
        .lines()
        .iter()
        .map(|line| {
          let (mark, text) = match line {
            diffy::Line::Context(text) => (' ', *text),
            diffy::Line::Delete(text) => ('-', *text),
            diffy::Line::Insert(text) => ('+', *text),
          };

          Json::String(format!("{mark}{}", text.strip_suffix('\n').unwrap_or(text)))
        })
        .collect();

      Json::Object(vec![
        ("oldStart".to_owned(), number(hunk.old_range().start())),
        ("oldLines".to_owned(), number(hunk.old_range().len())),
        ("newStart".to_owned(), number(hunk.new_range().start())),
        ("newLines".to_owned(), number(hunk.new_range().len())),
        ("lines".to_owned(), Json::Array(lines)),
      ])
    })
    .collect()
}

// What one file's change is, as the log screen draws a Claude Code change: an edit's hunks, which carry no
// type as a Claude Code edit carries none, or the whole content of a file created or deleted. A moved file
// stands under the path it was moved to.
fn file_diff(path: &str, change: &Json) -> Option<Json> {
  let file_path = text_field(change, "move_path").filter(|moved| !moved.is_empty()).unwrap_or(path);
  let (kind, hunks, content) = match (text_field(change, "type"), text_field(change, "unified_diff")) {
    (Some("update"), Some(diff)) => (Json::Null, unified_hunks(diff), Json::Null),
    (Some(kind @ ("add" | "delete")), _) => {
      let content = text_field(change, "content")?;
      let kind = if kind == "add" { "create" } else { "delete" };

      (Json::String(kind.to_owned()), vec![], Json::String(content.to_owned()))
    }
    _ => return None,
  };

  Some(Json::Object(vec![
    ("filePath".to_owned(), Json::String(file_path.to_owned())),
    ("type".to_owned(), kind),
    ("hunks".to_owned(), Json::Array(hunks)),
    ("content".to_owned(), content),
  ]))
}

/// One change as the session record wrote it: the paths it names, and what was done to each.
#[derive(Debug, PartialEq)]
pub struct RecordedChange {
  pub paths: Vec<String>,
  pub diffs: Vec<Json>,
}

/// The changes of one `FileChange` item of the session record; none where the record line is no such item.
pub fn recorded_change(event: &Json) -> Option<RecordedChange> {
  let item = event.get("payload")?.get("item")?;

  if text_field(event, "type") != Some("event_msg") || text_field(item, "type") != Some("FileChange") {
    return None;
  }

  let Json::Object(changes) = item.get("changes")? else { return None };

  Some(RecordedChange {
    paths: changes.iter().map(|(path, _)| path.clone()).collect(),
    diffs: changes.iter().filter_map(|(path, change)| file_diff(path, change)).collect(),
  })
}

// The thread names a record's file may be looked for by.
static THREAD: LazyLock<Regex> = LazyLock::new(|| Regex::new("^[a-zA-Z0-9-]+$").unwrap());

/// A thread's session record, read forward as it grows, with the changes the stream named before the record
/// had written them.
pub struct CodexChanges {
  root: PathBuf,
  path: Option<PathBuf>,
  thread: Option<String>,
  offset: u64,
  carry: Vec<u8>,
  recorded: Vec<RecordedChange>,
  waiting: Vec<Waiting>,
}

/// A result line written before the record held its change: the paths it names, the line's id and the
/// metadata it was written with.
pub struct Waiting {
  pub paths: Vec<String>,
  pub id: i64,
  pub metadata: Json,
}

impl CodexChanges {
  /// The records under Codex's sessions directory.
  pub fn new(root: PathBuf) -> Self {
    Self {
      root,
      path: None,
      thread: None,
      offset: 0,
      carry: vec![],
      recorded: vec![],
      waiting: vec![],
    }
  }

  /// The thread the run is on. A resumed thread's record already holds its past turns, so reading starts at
  /// its present end; a new thread's record may not exist yet and is read from its start once it does.
  pub fn started(&mut self, thread: &str) {
    if !THREAD.is_match(thread) || self.path.is_some() {
      return;
    }

    self.path = self.locate(thread);

    match &self.path {
      Some(path) => self.offset = std::fs::metadata(path).map_or(0, |held| held.len()),
      None => self.thread = Some(thread.to_owned()),
    }
  }

  /// The diffs of the change the stream names by these paths, where the record holds it by now.
  pub fn take(&mut self, paths: &[String]) -> Option<Vec<Json>> {
    self.read();
    self.found(paths)
  }

  /// A result line whose change the record has not written yet, to be filled in once it has.
  pub fn wait(&mut self, waiting: Waiting) {
    self.waiting.push(waiting);
  }

  /// The waiting lines whose changes the record has written since, each with the diffs it is filled in with.
  pub fn settle(&mut self) -> Vec<(Waiting, Vec<Json>)> {
    if self.waiting.is_empty() {
      return vec![];
    }

    self.read();

    let mut settled = vec![];

    for held in std::mem::take(&mut self.waiting) {
      match self.found(&held.paths) {
        Some(diffs) => settled.push((held, diffs)),
        None => self.waiting.push(held),
      }
    }

    settled
  }

  fn found(&mut self, paths: &[String]) -> Option<Vec<Json>> {
    let at = self.recorded.iter().position(|change| same_paths(&change.paths, paths))?;

    // Changes the stream never named before this one are not going to be asked for.
    self.recorded.drain(..=at).next_back().map(|change| change.diffs)
  }

  // The one record named for the thread under the sessions directory.
  fn locate(&self, thread: &str) -> Option<PathBuf> {
    let pattern = format!("{}/**/*{thread}.jsonl", glob::Pattern::escape(&self.root.to_string_lossy()));
    let found: Vec<PathBuf> = glob::glob(&pattern).ok()?.filter_map(Result::ok).collect();

    match found.as_slice() {
      [one] => Some(one.clone()),
      _ => None,
    }
  }

  // What the record wrote since the last read, its whole lines read for changes and a broken last line held
  // back until the rest of it is written.
  fn read(&mut self) {
    if self.path.is_none()
      && let Some(thread) = self.thread.clone()
    {
      self.path = self.locate(&thread);
    }

    let Some(bytes) = self.path.as_deref().and_then(|path| unread(path, self.offset)) else {
      return;
    };

    self.offset += bytes.len() as u64;
    self.carry.extend(bytes);

    let Some(cut) = self.carry.iter().rposition(|byte| *byte == b'\n') else { return };
    let whole: Vec<u8> = self.carry.drain(..=cut).collect();

    for line in String::from_utf8_lossy(&whole).lines() {
      if !line.contains("\"FileChange\"") {
        continue;
      }

      if let Some(change) = Json::parse(line).as_ref().and_then(recorded_change) {
        self.recorded.push(change);
      }
    }
  }
}

// What a file holds past `offset`.
fn unread(path: &Path, offset: u64) -> Option<Vec<u8>> {
  use std::io::{Read, Seek, SeekFrom};

  let mut file = std::fs::File::open(path).ok()?;
  let mut bytes = vec![];

  file.seek(SeekFrom::Start(offset)).ok()?;
  file.read_to_end(&mut bytes).ok()?;
  Some(bytes)
}

fn same_paths(held: &[String], named: &[String]) -> bool {
  held.len() == named.len() && named.iter().all(|path| held.contains(path))
}
