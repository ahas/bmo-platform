//! What a Claude Code program's own record says about a turn (docs/agent-control.md, 「What a resident
//! turn reads」, 「The session record and usage are not squeezed out of the screen」).
//!
//! A turn is not one line of the record, and a request is not one line either. A turn that uses a tool
//! makes a request for the call and another for the answer; a co-worker writes its own lines into the
//! same file marked as a side chain; and the program writes one response as a line per content block —
//! its thinking, its text, its tool call — with the same message id and the whole response's usage
//! repeated on every one of them. So a turn's cost is every response it added, each counted once, and
//! its answer is the text of the last response that was not a side chain's.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use agent_session::{Opened, TurnAnswer};
use indexmap::IndexMap;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::events::Event;

/// The provider every reading here is filed under.
pub const CLAUDE_CODE: &str = "claude-code";

// How many bytes before the point a record was read to are kept, to know the file under it is still
// the one they came from.
const SEAM_BYTES: u64 = 64;

/// A session's record read as it grows (docs/agent-control.md, 「A session record is read within a
/// bound」). A turn in a pane is watched by looking at its record several times a second, and a
/// session's record runs to tens of megabytes, so the lines already parsed are kept and each look reads
/// only the bytes written since the last whole line it took, and hands back the lines it holds without
/// copying them. What a look answers is what parsing the whole file would: every line that is JSON, in
/// order, a line that is not passed over, and a last line still being written counted only where it
/// already parses. The kept lines are thrown away and the file read again from its start wherever it is
/// not the file they came from: another file under the path, a file shorter than what was taken, or
/// bytes before the point taken that no longer read as they did.
///
/// A record's lines are numbered from its start, but its owner reads nothing before its floor again. A
/// line before the floor stands as null unless a reading of the whole record still turns on it
/// ([`Kept`]), so a look holds the lines from the floor on and a few small lines before it. A line as it
/// was written is held only until its owner has filed it.
#[derive(Debug)]
pub struct Tail {
  path: PathBuf,
  ino: u64,
  offset: u64,
  seam: Vec<u8>,
  records: Vec<Value>,
  // Each record line as it was written, for a reader that keeps its own order of keys; empty once
  // forgotten.
  raw: Vec<String>,
  // How many of the lines came from whole lines of the file; a line after them is the last line still
  // being written, read again by the next look.
  whole: usize,
  floor: usize,
  raw_floor: usize,
  // Where the floor stands whenever the record is read again from its start.
  start: usize,
  // Whether the floor is set to the end of what the first look reads.
  floor_at_end: bool,
  // The line before the floor kept of each kind only the last of which is read.
  last_kept: [Option<usize>; Lasting::KINDS],
}

/// What a [`Tail`] holds whole: the lines it holds parsed, the nulls before its floor not counted, and
/// the bytes of the lines as written it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Held {
  pub records: usize,
  pub raw_bytes: usize,
}

impl Tail {
  /// A reader of the whole record: its floor stands at its start until its owner raises it.
  pub fn new(path: impl Into<PathBuf>) -> Self {
    Self {
      path: path.into(),
      ino: 0,
      offset: 0,
      seam: vec![],
      records: vec![],
      raw: vec![],
      whole: 0,
      floor: 0,
      raw_floor: 0,
      start: 0,
      floor_at_end: false,
      last_kept: [None; Lasting::KINDS],
    }
  }

  /// A reader of what the record gains from now: its first look sets the floor after every whole line
  /// already written, so the lines before it are never held.
  pub fn at_end(path: impl Into<PathBuf>) -> Self {
    Self {
      floor_at_end: true,
      ..Self::new(path)
    }
  }

  /// A reader whose floor stands at line `at` from its first look, so the lines before it are let go as
  /// they are parsed rather than once the whole record has been.
  pub fn from_line(path: impl Into<PathBuf>, at: usize) -> Self {
    Self {
      start: at,
      ..Self::new(path)
    }
  }

  pub fn path(&self) -> &Path {
    &self.path
  }

  /// Every JSON line of the record now, in order, each line before the floor that no reading of the
  /// whole record turns on standing as null; nothing where there is no file.
  pub fn look(&mut self) -> &[Value] {
    // A record not written yet holds nothing before the end the first look reads.
    let Ok(mut file) = std::fs::File::open(&self.path) else {
      self.restart(0);
      self.floor_at_end = false;

      return &self.records;
    };
    let Ok(meta) = file.metadata() else {
      self.floor_at_end = false;

      return &self.records;
    };
    let intact = self.ino == meta.ino()
      && meta.len() >= self.offset
      && read_at(&mut file, self.offset - self.seam.len() as u64, self.seam.len() as u64) == self.seam;

    if !intact {
      self.restart(meta.ino());
    }

    // The last line read unfinished is read again with whatever has been written after it.
    self.records.truncate(self.whole);
    self.raw.truncate(self.whole);

    let fresh = read_at(&mut file, self.offset, meta.len() - self.offset);
    let end = fresh.iter().rposition(|byte| *byte == b'\n');

    if let Some(end) = end {
      if self.floor_at_end {
        self.floor = usize::MAX;
        self.raw_floor = usize::MAX;
      }

      self.take(&String::from_utf8_lossy(&fresh[..=end]));
      self.offset += end as u64 + 1;
      self.seam = read_at(&mut file, self.offset.saturating_sub(SEAM_BYTES), SEAM_BYTES.min(self.offset));
    }

    self.whole = self.records.len();

    if std::mem::take(&mut self.floor_at_end) {
      self.floor = self.whole;
      self.raw_floor = self.whole;
    }

    // A last line with no newline yet is taken only where it already reads whole.
    self.take(&String::from_utf8_lossy(&fresh[end.map_or(0, |end| end + 1)..]));

    &self.records
  }

  /// The lines the last look handed back, as it handed them, with no new reading of the record.
  pub fn lines(&self) -> &[Value] {
    &self.records
  }

  /// The record's lines as they were written, as the last look read them: one for each line it
  /// handed back, empty for one before the floor or already filed.
  pub fn raw(&self) -> &[String] {
    &self.raw
  }

  /// How many lines the record holds now.
  pub fn count(&mut self) -> usize {
    self.look().len()
  }

  /// Raises the floor to `at`: the owner reads no line before it again. Each such line is let go as
  /// [`Tail`] says, and so is its line as written. A floor never comes down, and never passes the last
  /// line still being written.
  pub fn forget_before(&mut self, at: usize) {
    let at = at.min(self.whole);

    self.forget_raw_before(at);

    while self.floor < at {
      self.settle(self.floor);
      self.floor += 1;
    }
  }

  /// Lets go of the lines as written before `at`, once the owner has filed them.
  pub fn forget_raw_before(&mut self, at: usize) {
    let at = at.min(self.whole);

    while self.raw_floor < at {
      self.raw[self.raw_floor] = String::new();
      self.raw_floor += 1;
    }
  }

  /// What the reader holds whole now.
  pub fn held(&self) -> Held {
    Held {
      records: self.records.iter().filter(|record| !record.is_null()).count(),
      raw_bytes: self.raw.iter().map(String::len).sum(),
    }
  }

  fn take(&mut self, text: &str) {
    for line in text.split('\n') {
      let trimmed = line.trim();

      if trimmed.is_empty() {
        continue;
      }

      let Ok(record) = serde_json::from_str::<Value>(trimmed) else { continue };
      let at = self.records.len();

      self.records.push(record);
      self.raw.push(if at < self.raw_floor { String::new() } else { trimmed.to_owned() });

      if at < self.floor {
        self.settle(at);
      }
    }
  }

  // A line before the floor: kept where a reading of the whole record turns on it, else let go.
  fn settle(&mut self, at: usize) {
    match Kept::of(&self.records[at]) {
      Kept::Mark => self.records[at] = queue_mark(&self.records[at]),
      Kept::Last(kind) => {
        if let Some(before) = self.last_kept[kind as usize].replace(at)
          && before != at
        {
          self.records[before] = Value::Null;
        }
      }
      Kept::Not => self.records[at] = Value::Null,
    }
  }

  fn restart(&mut self, ino: u64) {
    self.ino = ino;
    self.offset = 0;
    self.seam.clear();
    self.records.clear();
    self.raw.clear();
    self.whole = 0;
    self.floor = self.start;
    self.raw_floor = self.start;
    self.last_kept = [None; Lasting::KINDS];
  }
}

/// The line a program writes its running account into, by model, as its session changes or it ends.
pub const COST_STATE: &str = "cost-state";

/// How a line before a reader's floor is kept, by what a reading of the whole record still reads of it:
/// whether the foreground rests and what stands queued ([`rests_apart`], [`ends_on_turn_end`]) turn on
/// every queue operation, the last line of the conversation and the last turn's end, and a turn's spend
/// is measured from the last account line before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kept {
  /// Every such line, as its mark alone: a queue operation's operation and whether what it queued is a
  /// notice, the words it queued let go, since a long session queues thousands of them.
  Mark,
  Last(Lasting),
  Not,
}

/// A kind of line of which only the last before a reader's floor is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lasting {
  /// A prompt, an answer, or a tool's call or answer.
  Talk,
  TurnEnd,
  Account,
}

impl Lasting {
  const KINDS: usize = 3;
}

impl Kept {
  pub fn of(record: &Value) -> Self {
    if is_turn_end(record) {
      return Self::Last(Lasting::TurnEnd);
    }

    match kind(record) {
      Some("queue-operation") => Self::Mark,
      Some(COST_STATE) => Self::Last(Lasting::Account),
      Some("user" | "assistant") => Self::Last(Lasting::Talk),
      _ => Self::Not,
    }
  }
}

/// Every JSON line of a record, in order, handed over one at a time with its number among them as a
/// [`Tail`] numbers them; nothing where there is no file. A record runs to tens of megabytes, so it is read
/// a line at a time and no line is held past its turn here.
pub fn each_line(path: &Path, mut seen: impl FnMut(usize, Value)) {
  use std::io::BufRead as _;

  let Ok(file) = std::fs::File::open(path) else { return };
  let mut reader = std::io::BufReader::new(file);
  let mut line = Vec::new();
  let mut at = 0;

  loop {
    line.clear();

    if !matches!(reader.read_until(b'\n', &mut line), Ok(read) if read > 0) {
      return;
    }

    let text = String::from_utf8_lossy(&line);
    let trimmed = text.trim();

    if trimmed.is_empty() {
      continue;
    }

    if let Ok(record) = serde_json::from_str::<Value>(trimmed) {
      seen(at, record);
      at += 1;
    }
  }
}

fn read_at(file: &mut std::fs::File, from: u64, length: u64) -> Vec<u8> {
  let mut bytes = Vec::with_capacity(length as usize);

  if file.seek(SeekFrom::Start(from)).is_ok() {
    let _ = file.take(length).read_to_end(&mut bytes);
  }

  bytes
}

fn text_of<'a>(record: &'a Value, key: &str) -> Option<&'a str> {
  record.get(key).and_then(Value::as_str)
}

fn is_true(record: &Value, key: &str) -> bool {
  record.get(key) == Some(&Value::Bool(true))
}

fn kind(record: &Value) -> Option<&str> {
  text_of(record, "type")
}

fn blocks_of(record: &Value) -> &[Value] {
  record.pointer("/message/content").and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn count(usage: Option<&Value>, name: &str) -> i64 {
  usage
    .and_then(|usage| usage.get(name))
    .and_then(|value| value.as_i64().or_else(|| value.as_f64().map(|value| value as i64)))
    .unwrap_or(0)
}

fn number_of(value: Option<&Value>) -> Option<f64> {
  value.and_then(Value::as_f64).filter(|value| value.is_finite())
}

/// One model's share of what a turn spent.
#[derive(Clone, Debug, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSpend {
  #[serde(default)]
  pub mode: String,
  pub model: String,
  pub tokens: i64,
  pub input_tokens: i64,
  pub output_tokens: i64,
  pub cache_read_tokens: i64,
  pub cache_write_tokens: i64,
  #[serde(default)]
  pub cost: f64,
}

/// Background work a turn launched and walked away from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct LiveTask {
  pub id: String,
  pub description: String,
}

/// A Claude Workflow a turn launched: the call it went under, what it was launched as, and the word its
/// ending carried, where one was reported.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct RunWorkflow {
  pub call: String,
  pub name: String,
  pub status: Option<String>,
}

/// One of the account's windows as the status line reads it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
  pub name: String,
  pub utilization: f64,
  pub resets_at: Option<f64>,
}

/// The account's session window with the weekly one beside it.
#[derive(Clone, Debug, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsage {
  pub utilization: f64,
  pub resets_at: f64,
  pub weekly_utilization: Option<f64>,
  pub weekly_resets_at: Option<f64>,
}

/// A turn's answer in the one reply language every road hands back.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
  pub mode: String,
  pub model: Option<String>,
  pub effort: Option<String>,
  pub text: String,
  pub session_id: Option<String>,
  pub context_ratio: Option<f64>,
  pub context_tokens: Option<i64>,
  pub session_usage: Option<SessionUsage>,
  pub windows: Vec<ReplyWindow>,
  pub spend: Vec<ModelSpend>,
  pub live_tasks: Vec<LiveTask>,
  pub workflows: Vec<RunWorkflow>,
  /// Which registered login answered, where one is known.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub account: Option<String>,
  /// What the last request of the turn carried, and every request in order, where the program's record
  /// counts them one by one.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub last_request_usage: Option<RequestUsage>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub request_usages: Option<Vec<RequestUsage>>,
  /// The answer of each turn of the program the round spanned, in order, which `text` was built from;
  /// none where the road reads no such turns.
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub answers: Vec<TurnAnswer>,
}

/// One of the account's windows a turn came back with: as Claude Code's status line hands it, or as a
/// program that names each window's length and its limit writes it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ReplyWindow {
  Status(UsageWindow),
  Account(AccountWindow),
}

/// One window of an account's limit as a program that names each window's length and its limit writes
/// it, in the shape agency keeps every window in (docs/usage-windows.md): the share of it used, when it
/// resets in epoch seconds, and how long it runs and what its provider calls it where the provider says.
pub type AccountWindow = cache_siren::Window;

impl From<&ReplyWindow> for AccountWindow {
  /// Every window in the one shape it is kept in: the status line's reset rounded to a second, and its
  /// length and limit unnamed.
  fn from(window: &ReplyWindow) -> Self {
    match window {
      ReplyWindow::Status(window) => Self {
        name: window.name.clone(),
        utilization: window.utilization,
        resets_at: window.resets_at.map(|at| at.round() as i64),
        duration_minutes: None,
        limit_name: None,
      },
      ReplyWindow::Account(window) => window.clone(),
    }
  }
}

/// One model a run spent tokens on, under the login it is filed against: empty on a road that holds no
/// logins of agency's.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpentRow {
  pub account: String,
  #[serde(flatten)]
  pub spend: ModelSpend,
}

/// What a run spent, published for the reader that keeps usage (docs/api.md, 「Events」): the agent it is
/// filed under, the run it came from where one is named, when in epoch seconds, and each model it spent
/// tokens on. Agency keeps no usage rows.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSpent {
  pub agent: String,
  pub run: Option<String>,
  pub at: i64,
  pub rows: Vec<SpentRow>,
}

impl Event for UsageSpent {
  const NAME: &'static str = "usage.changed";
}

impl UsageSpent {
  /// What a run spent, each model that spent tokens filed under the login `account_of` names for its
  /// provider; none where it spent nothing.
  pub fn of(
    agent: &str,
    run: Option<&str>,
    at: i64,
    spend: &[ModelSpend],
    account_of: impl Fn(&ModelSpend) -> String,
  ) -> Option<Self> {
    let rows: Vec<SpentRow> = spend
      .iter()
      .filter(|one| one.tokens > 0)
      .map(|one| SpentRow {
        account: account_of(one),
        spend: one.clone(),
      })
      .collect();

    (!rows.is_empty()).then(|| Self {
      agent: agent.to_owned(),
      run: run.map(str::to_owned),
      at,
      rows,
    })
  }
}

/// What one request carried, the fresh input apart from the cache it read and wrote.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestUsage {
  pub input_tokens: i64,
  pub cache_read_tokens: i64,
  pub cache_write_tokens: i64,
  pub output_tokens: i64,
}

/// What a turn added to the record, read as a reply.
#[derive(Clone, Debug, PartialEq)]
pub struct TurnRead {
  pub text: String,
  pub model: Option<String>,
  pub effort: Option<String>,
  pub spend: Vec<ModelSpend>,
  pub context_tokens: Option<i64>,
  /// The program's own error where the turn ended on one instead of an answer.
  pub error: Option<String>,
  /// That error's kind as the program names it — `rate_limit`, `server_error` and the rest of its list —
  /// where it names one.
  pub error_kind: Option<String>,
  pub live_tasks: Vec<LiveTask>,
  pub workflows: Vec<RunWorkflow>,
}

impl TurnRead {
  /// The reply a turn read this way makes, on the session it went in on and with the spend given.
  pub fn reply(&self, session_id: &str, spend: Vec<ModelSpend>) -> Reply {
    Reply {
      mode: CLAUDE_CODE.to_owned(),
      model: self.model.clone(),
      effort: self.effort.clone(),
      text: self.text.clone(),
      session_id: Some(session_id.to_owned()),
      // A record states what one request carried, never a share of a window, so the ratio is left to
      // whoever knows the window.
      context_ratio: None,
      context_tokens: self.context_tokens,
      // The account's windows are nowhere in a record; absent is the truthful answer.
      session_usage: None,
      windows: vec![],
      spend,
      live_tasks: self.live_tasks.clone(),
      workflows: self.workflows.clone(),
      account: None,
      last_request_usage: None,
      request_usages: None,
      answers: Vec::new(),
    }
  }
}

/// The turn that begins at `from`: its cost is every response it added, side chains included, because
/// a co-worker's tokens are that run's tokens; its answer is the last response that was not a side
/// chain's. A response is known by its message id, since the lines it was split into all carry it; a
/// line with no id is a response of its own.
pub fn read_turn(records: &[Value], from: usize) -> TurnRead {
  let turn = records.get(from..).unwrap_or_default();
  let assistants: Vec<&Value> = turn.iter().filter(|record| kind(record) == Some("assistant")).collect();
  let (last, spoken) = last_answer(turn);
  // One entry per response: the last of a response's lines is kept, since it is the one written once
  // the response was whole.
  let mut responses: IndexMap<String, &Value> = IndexMap::new();

  for (at, record) in assistants.iter().enumerate() {
    let key = record
      .pointer("/message/id")
      .and_then(Value::as_str)
      .map_or_else(|| format!("line {at}"), str::to_owned);

    responses.insert(key, record);
  }

  let mut by_model: IndexMap<String, ModelSpend> = IndexMap::new();

  for record in responses.values() {
    let model = record.pointer("/message/model").and_then(Value::as_str).unwrap_or("").to_owned();
    let usage = record.pointer("/message/usage");
    let held = by_model.entry(model.clone()).or_insert_with(|| ModelSpend {
      mode: CLAUDE_CODE.to_owned(),
      model,
      tokens: 0,
      input_tokens: 0,
      output_tokens: 0,
      cache_read_tokens: 0,
      cache_write_tokens: 0,
      // The record carries the four token counts and no price; a figure nobody reported is left at
      // nothing rather than worked out from a table that would go out of date.
      cost: 0.0,
    });
    let input = count(usage, "input_tokens");
    let output = count(usage, "output_tokens");
    let read = count(usage, "cache_read_input_tokens");
    let write = count(usage, "cache_creation_input_tokens");

    held.input_tokens += input;
    held.output_tokens += output;
    held.cache_read_tokens += read;
    held.cache_write_tokens += write;
    held.tokens += input + output + read + write;
  }

  // How much the model was holding on its last request of the turn, which the fold's clock is read
  // against: the sum of what that request carried.
  let context_tokens = last.map(|last| {
    let usage = last.pointer("/message/usage");

    count(usage, "input_tokens")
      + count(usage, "cache_read_input_tokens")
      + count(usage, "cache_creation_input_tokens")
  });
  let (live_tasks, workflows) = left_going(records, from);

  TurnRead {
    model: last
      .and_then(|last| last.pointer("/message/model"))
      .and_then(Value::as_str)
      .map(str::to_owned),
    effort: last.and_then(|last| text_of(last, "effort")).map(str::to_owned),
    spend: by_model.into_values().filter(|held| !held.model.is_empty()).collect(),
    context_tokens,
    error: last.filter(|last| is_true(last, "isApiErrorMessage")).map(|_| spoken.clone()),
    error_kind: last
      .filter(|last| is_true(last, "isApiErrorMessage"))
      .and_then(|last| text_of(last, "error"))
      .map(str::to_owned),
    text: spoken,
    live_tasks,
    workflows,
  }
}

/// The answer of the turn that begins at `from` as it stands: [`read_turn`]'s text, read without the
/// rest of that reading.
pub fn answer_since(records: &[Value], from: usize) -> String {
  last_answer(records.get(from..).unwrap_or_default()).1
}

// The last response of the turn's own, not a side chain's, and its words: the text of every line that
// response was split into.
fn last_answer(turn: &[Value]) -> (Option<&Value>, String) {
  let mine = || {
    turn
      .iter()
      .filter(|record| kind(record) == Some("assistant") && !is_true(record, "isSidechain"))
  };
  let Some(last) = mine().next_back() else { return (None, String::new()) };
  let spoken = match last.pointer("/message/id") {
    None => text_blocks(std::iter::once(last)),
    Some(id) => text_blocks(mine().filter(|record| record.pointer("/message/id") == Some(id))),
  };

  (Some(last), spoken)
}

fn text_blocks<'a>(records: impl Iterator<Item = &'a Value>) -> String {
  records
    .flat_map(blocks_of)
    .filter(|block| text_of(block, "type") == Some("text"))
    .filter_map(|block| text_of(block, "text"))
    .collect::<String>()
    .trim()
    .to_owned()
}

/// Whether the record holds an answer of the run's own, not a co-worker's, written at or after `from`.
pub fn answered_since(records: &[Value], from: usize) -> bool {
  records
    .get(from..)
    .unwrap_or_default()
    .iter()
    .any(|record| kind(record) == Some("assistant") && !is_true(record, "isSidechain"))
}

/// Whether the record holds a prompt put in after `from`: a user line of the main conversation
/// carrying words, never one carrying a tool's answer.
pub fn prompted_since(records: &[Value], from: usize) -> bool {
  records.get(from..).unwrap_or_default().iter().any(|record| {
    if kind(record) != Some("user") || is_true(record, "isSidechain") || is_true(record, "isMeta") {
      return false;
    }

    match record.pointer("/message/content") {
      Some(Value::String(_)) => true,
      Some(Value::Array(parts)) => parts.iter().any(|part| text_of(part, "type") == Some("text")),
      _ => false,
    }
  })
}

/// Whether the record holds the line the program writes once a turn is over, after `from`.
pub fn turn_closed_since(records: &[Value], from: usize) -> bool {
  records.get(from..).unwrap_or_default().iter().any(is_turn_end)
}

/// Whether the last line of the conversation in these records — a prompt, an answer, words queued or a
/// turn's end — is a turn's end: nothing was handed to the program or written by it after its last turn
/// closed.
pub fn ends_on_turn_end(records: &[Value]) -> bool {
  records
    .iter()
    .rev()
    .find(|record| {
      is_turn_end(record) || matches!(kind(record), Some("user" | "assistant" | "queue-operation"))
    })
    .is_some_and(is_turn_end)
}

/// Whether the turn begun at `from` is over on the program's own error: the line a turn ends with has
/// come, and the last answer before it is an error the program wrote in the answer's place. The program
/// can stand at a question of its own after such an end, which says nothing about the turn: the record
/// says it is over.
pub fn ended_on_error(records: &[Value], from: usize) -> bool {
  turn_closed_since(records, from)
    && last_answer(records.get(from..).unwrap_or_default())
      .0
      .is_some_and(|last| is_true(last, "isApiErrorMessage"))
}

/// Whether the turn begun at `from` stands on the program's own error with nothing after it: the last line of
/// the conversation is an error the program wrote in the answer's place. A program at rest on such a line has
/// ended the turn even where it writes no closing line, as on a full window, where it opens its own choice of
/// what to do next instead.
pub fn rests_on_error(records: &[Value], from: usize) -> bool {
  records
    .get(from..)
    .unwrap_or_default()
    .iter()
    .rev()
    .find(|record| {
      matches!(kind(record), Some("user" | "assistant" | "queue-operation"))
        && !is_true(record, "isSidechain")
    })
    .is_some_and(|last| kind(last) == Some("assistant") && is_true(last, "isApiErrorMessage"))
}

/// The error a turn come back to rest ended on, with its kind, out of its reading and the failure its place
/// recorded for it (its words and kind): the program's own error in the answer's place, the kind the
/// place's failure names where the record names none; else, where the record holds no answer yet, the
/// place's failure; none where the turn answered.
pub fn turn_error(
  read: &TurnRead,
  answered: bool,
  failure: Option<(String, Option<String>)>,
) -> Option<(String, Option<String>)> {
  match (&read.error, failure) {
    (Some(error), failure) => {
      Some((error.clone(), read.error_kind.clone().or_else(|| failure.and_then(|(_, kind)| kind))))
    }
    (None, Some(failure)) if !answered => Some(failure),
    _ => None,
  }
}

fn is_turn_end(record: &Value) -> bool {
  kind(record) == Some("system") && text_of(record, "subtype") == Some("turn_duration")
}

/// The words a command of the program's own wrote back, and whether it wrote them as its error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandWords {
  pub text: String,
  pub error: bool,
}

static COMMAND_WORDS: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?s)^\s*<local-command-(stdout|stderr)>(.*)</local-command-(?:stdout|stderr)>\s*$")
    .expect("a pattern")
});
static COLOURS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").expect("a pattern"));

// The words a line of the main conversation holds as plain text: a user line's message, a system line's own.
fn plain_words(record: &Value) -> Option<&str> {
  if is_true(record, "isSidechain") {
    return None;
  }

  match kind(record)? {
    "user" => record.pointer("/message/content")?.as_str(),
    "system" => text_of(record, "content"),
    _ => None,
  }
}

/// What a command of the program's own said back after `from`: the program writes its words once the command
/// is over, on a user or a system line by the outcome (docs/agent-control.md, 「How a command of the program's
/// own ends」).
pub fn command_words(records: &[Value], from: usize) -> Option<CommandWords> {
  records.get(from..).unwrap_or_default().iter().find_map(|record| {
    let said = COMMAND_WORDS.captures(plain_words(record)?)?;

    Some(CommandWords {
      text: COLOURS.replace_all(said[2].trim(), "").into_owned(),
      error: &said[1] == "stderr",
    })
  })
}

/// Whether the program has written the line that opens a command's own lines after `from`, which it does once
/// the command has stopped running, whether or not any words follow.
pub fn command_closing_since(records: &[Value], from: usize) -> bool {
  records
    .get(from..)
    .unwrap_or_default()
    .iter()
    .any(|record| plain_words(record).is_some_and(|said| said.starts_with("<local-command-caveat>")))
}

/// Whether the record holds the compacted marker the fold word leaves, after `from`.
pub fn compacted_since(records: &[Value], from: usize) -> bool {
  records
    .get(from..)
    .unwrap_or_default()
    .iter()
    .any(|record| kind(record) == Some("system") && text_of(record, "subtype") == Some("compact_boundary"))
}

/// Whether the record holds a word pushed at `from`: queued by a working turn, or submitted straight as
/// a prompt by a program at rest.
pub fn word_written_down(records: &[Value], from: usize) -> bool {
  prompted_since(records, from)
    || records.get(from..).unwrap_or_default().iter().any(|record| {
      kind(record) == Some("queue-operation") && text_of(record, "operation") == Some("enqueue")
    })
}

const ACTIVITY_DETAIL_LIMIT: usize = 160;

/// The last line a turn wrote to its record: when it was written, and what it was — the tool a
/// response called with what it was given, words written, thinking, or a tool's answer coming back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activity {
  pub at: Option<i64>,
  pub what: Option<String>,
}

pub fn last_activity(records: &[Value], from: usize) -> Activity {
  let Some(last) = records
    .get(from..)
    .unwrap_or_default()
    .iter()
    .rev()
    .find(|record| matches!(kind(record), Some("assistant" | "user")))
  else {
    return Activity {
      at: None,
      what: None,
    };
  };
  let at = text_of(last, "timestamp")
    .and_then(|stamp| chrono::DateTime::parse_from_rfc3339(stamp).ok())
    .map(|stamp| stamp.timestamp_millis());
  let blocks = blocks_of(last);
  let has = |kind: &str| blocks.iter().any(|block| text_of(block, "type") == Some(kind));
  let what =
    if let Some(tool) = blocks.iter().rev().find(|block| text_of(block, "type") == Some("tool_use")) {
      let input = tool.get("input");
      let detail = ["command", "file_path", "path", "description"]
        .iter()
        .find_map(|field| {
          input
            .and_then(|input| input.get(*field))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        })
        .map_or_else(String::new, |value| {
          let flat = whitespace().replace_all(value, " ");

          format!(" ({})", flat.chars().take(ACTIVITY_DETAIL_LIMIT).collect::<String>())
        });

      Some(format!("called {}{detail}", text_of(tool, "name").unwrap_or("a tool")))
    } else if has("tool_result") {
      Some("a tool's answer came back".to_owned())
    } else if has("text") {
      Some(if kind(last) == Some("assistant") { "wrote words" } else { "was handed words" }.to_owned())
    } else if has("thinking") {
      Some("was thinking".to_owned())
    } else {
      None
    };

  Activity { at, what }
}

fn whitespace() -> &'static Regex {
  static PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("a fixed pattern"));

  &PATTERN
}

// The words Claude Code answers a background launch with, and the notice it writes when that work ends.
// Both are the program's own wording, read here as this repository's reading of that program.
fn launched_wording() -> &'static Regex {
  static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)running in background with ID|moved to the background|launched in background|Async agent launched")
      .expect("a fixed pattern")
  });

  &PATTERN
}

fn notification() -> &'static Regex {
  static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<task-notification>(.*?)</task-notification>").expect("a fixed pattern")
  });

  &PATTERN
}

// Each task notification a line carries, handed over as its body. A notification reaches the record as a
// queued prompt, an attachment or the user line it was taken up as, so every text the line holds is looked
// in; the texts are read where they stand, since a line is read on every look of a turn and writing it out
// again to search it would cost its whole size each time.
fn each_notification(record: &Value, found: &mut dyn FnMut(&str)) {
  match record {
    Value::String(text) if text.contains("<task-notification>") => {
      for body in notification().captures_iter(text) {
        found(body.get(1).map_or("", |body| body.as_str()));
      }
    }
    Value::Array(values) => values.iter().for_each(|value| each_notification(value, found)),
    Value::Object(fields) => fields.values().for_each(|value| each_notification(value, found)),
    _ => {}
  }
}

fn tagged(body: &str, pattern: &Regex) -> Option<String> {
  pattern
    .captures(body)
    .and_then(|found| found.get(1))
    .map(|found| found.as_str().to_owned())
}

fn call_tag() -> &'static Regex {
  static PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<tool-use-id>([^<]+)</tool-use-id>").expect("a fixed pattern"));

  &PATTERN
}

fn status_tag() -> &'static Regex {
  static PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<status>([^<]+)</status>").expect("a fixed pattern"));

  &PATTERN
}

fn tag(name: &str) -> Regex {
  Regex::new(&format!(r"(?s)<{name}>(.*?)</{name}>")).expect("a fixed pattern")
}

/// What a background task's notice says, as Claude Code writes the notice it hands the program: the task,
/// the call that launched it, the status it ended on, its one-line summary, what it handed back and the file
/// its whole output stands in. A notice saying a task was resumed names no status.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct TaskNotice {
  pub task: String,
  pub call: Option<String>,
  pub status: Option<String>,
  pub summary: Option<String>,
  pub result: Option<String>,
  pub output: Option<String>,
}

impl TaskNotice {
  /// Whether the notice ends its task: it names the status the task ended on. A notice that the task was
  /// resumed names none.
  pub fn ended(&self) -> bool {
    self.status.is_some()
  }
}

/// Every notice a session record's line carries that names the status its task ended on, however the line
/// carries it: a queued prompt, an attachment or the user line it was taken up as.
pub fn ended_notices(record: &Value) -> Vec<TaskNotice> {
  let mut found = vec![];

  each_notification(record, &mut |body| {
    if let Some(notice) =
      task_notice(&format!("{NOTICE_MARK}{body}</task-notification>")).filter(TaskNotice::ended)
    {
      found.push(notice);
    }
  });

  found
}

/// Whether a line of a session record may carry a notice, read before the line is parsed.
pub fn may_hold_notice(line: &str) -> bool {
  line.contains(NOTICE_MARK)
}

/// Whether a prompt is a background task's notice the program hands itself.
pub fn is_notice(prompt: &str) -> bool {
  prompt.trim_start().starts_with(NOTICE_MARK)
}

/// The notice a prompt carries, or none where it is no notice or names no task. The notice is the program's
/// own wording, read here as this repository's reading of that program.
pub fn task_notice(prompt: &str) -> Option<TaskNotice> {
  static TAGS: LazyLock<[Regex; 6]> =
    LazyLock::new(|| ["task-id", "tool-use-id", "status", "summary", "result", "output-file"].map(tag));

  if !is_notice(prompt) {
    return None;
  }

  let body = notification()
    .captures(prompt)
    .and_then(|found| found.get(1))
    .map_or(prompt, |body| body.as_str());
  let read = |at: usize| {
    tagged(body, &TAGS[at])
      .map(|text| text.trim().to_owned())
      .filter(|text| !text.is_empty())
  };

  Some(TaskNotice {
    task: read(0)?,
    call: read(1),
    status: read(2),
    summary: read(3),
    result: read(4),
    output: read(5),
  })
}

// The work a turn walked away from. Background work is launched by a call answered at once, with a
// line saying it now runs in the background, and ends later in a task notification naming the call
// and the status it ended with; so what was still going is a launch this turn made with no ending
// after it, and a Claude Workflow is one it walked away from unless its ending said it ran to the end.
// A call that never got an answer was cut off in the middle, which is the same state. Only this turn's
// own launches count.
fn left_going(records: &[Value], from: usize) -> (Vec<LiveTask>, Vec<RunWorkflow>) {
  let turn = records.get(from..).unwrap_or_default();
  let mut asked: IndexMap<String, &Value> = IndexMap::new();
  let mut answers: HashMap<String, String> = HashMap::new();
  let mut endings: HashMap<String, String> = HashMap::new();

  for record in turn {
    for block in blocks_of(record) {
      if text_of(block, "type") == Some("tool_use")
        && let Some(id) = text_of(block, "id")
      {
        asked.insert(id.to_owned(), block);
      }

      if text_of(block, "type") == Some("tool_result")
        && let Some(id) = text_of(block, "tool_use_id")
      {
        answers.insert(id.to_owned(), result_text(block));
      }
    }

    each_notification(record, &mut |body| {
      if let (Some(call), Some(status)) = (tagged(body, call_tag()), tagged(body, status_tag())) {
        endings.insert(call, status);
      }
    });
  }

  let mut live = vec![];
  let mut workflows = vec![];

  for (id, block) in asked {
    let answer = answers.get(&id);
    let launched = answer.is_some_and(|answer| launched_wording().is_match(answer));
    let name = text_of(block, "name");

    if name == Some("Workflow") {
      if answer.is_none() || launched {
        workflows.push(RunWorkflow {
          name: workflow_name(block),
          status: endings.get(&id).cloned(),
          call: id,
        });
      }

      continue;
    }

    let branch = matches!(name, Some("Task" | "Agent"));

    if (branch && answer.is_none()) || (launched && !endings.contains_key(&id)) {
      live.push(LiveTask {
        description: task_description(block),
        id,
      });
    }
  }

  (live, workflows)
}

/// A shell a record put in its program's background that no notice has ended yet: the task the program named
/// it, its command and when it was called.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellLeft {
  pub task: String,
  pub command: String,
  pub at: i64,
}

/// The shells a record put in the background and no notice in it ended, read as a subagent's own record holds
/// them: a shell call answered at once with the task it now runs as, and a notice naming that task with the
/// status it ended on.
pub fn shells_left(records: &[Value]) -> Vec<ShellLeft> {
  static LAUNCHED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)running in background with ID:?\s*([A-Za-z0-9_-]+)").expect("a fixed pattern")
  });
  static TASK: LazyLock<Regex> = LazyLock::new(|| tag("task-id"));
  let mut called: HashMap<String, (String, i64)> = HashMap::new();
  let mut launched: IndexMap<String, ShellLeft> = IndexMap::new();
  let mut ended: std::collections::HashSet<String> = std::collections::HashSet::new();

  for record in records {
    let at = text_of(record, "timestamp")
      .and_then(|stamp| chrono::DateTime::parse_from_rfc3339(stamp).ok())
      .map_or(0, |stamp| stamp.timestamp_millis());

    for block in blocks_of(record) {
      if text_of(block, "type") == Some("tool_use")
        && text_of(block, "name") == Some("Bash")
        && let (Some(id), Some(command)) =
          (text_of(block, "id"), block.get("input").and_then(|input| text_of(input, "command")))
      {
        called.insert(id.to_owned(), (command.to_owned(), at));
      }

      if text_of(block, "type") == Some("tool_result")
        && let Some((command, at)) = text_of(block, "tool_use_id").and_then(|id| called.get(id))
        && let Some(task) = LAUNCHED.captures(&result_text(block)).and_then(|found| found.get(1))
      {
        let task = task.as_str().to_owned();

        launched.insert(
          task.clone(),
          ShellLeft {
            task,
            command: command.clone(),
            at: *at,
          },
        );
      }
    }

    each_notification(record, &mut |body| {
      if let (Some(task), Some(_)) = (tagged(body, &TASK), tagged(body, status_tag())) {
        ended.insert(task.trim().to_owned());
      }
    });
  }

  launched.into_values().filter(|shell| !ended.contains(&shell.task)).collect()
}

fn result_text(block: &Value) -> String {
  match block.get("content") {
    Some(Value::String(text)) => text.clone(),
    Some(Value::Array(parts)) => parts.iter().map(|part| text_of(part, "text").unwrap_or("")).collect(),
    _ => String::new(),
  }
}

fn task_description(block: &Value) -> String {
  let input = block.get("input");
  let said = input.and_then(|input| text_of(input, "description")).filter(|said| !said.is_empty());
  let role = input.and_then(|input| text_of(input, "subagent_type"));

  said.or(role).or_else(|| text_of(block, "name")).unwrap_or("").to_owned()
}

// What a Claude Workflow was launched as: the script names itself in the block every one of them opens
// with, and a script that named itself some other way is reported by its call's own name.
fn workflow_name(block: &Value) -> String {
  static NAMED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"name:\s*['"`]([^'"`]+)['"`]"#).expect("a fixed pattern"));
  let input = block.get("input");
  let script = input.and_then(|input| text_of(input, "script"));

  match script {
    Some(script) => NAMED.captures(script).and_then(|found| found.get(1)).map_or_else(
      || input.and_then(|input| text_of(input, "name")).unwrap_or("").to_owned(),
      |name| name.as_str().to_owned(),
    ),
    None => input.and_then(|input| text_of(input, "name")).unwrap_or("").to_owned(),
  }
}

const INTERRUPTED_MARK: &str = "[Request interrupted by user";
// How the program opens a line of its own that tells the model a background task has moved.
const NOTICE_MARK: &str = "<task-notification>";

fn spoken_text(record: &Value) -> String {
  match record.pointer("/message/content") {
    Some(Value::String(text)) => text.clone(),
    Some(Value::Array(blocks)) => blocks
      .iter()
      .map(|block| {
        if text_of(block, "type") == Some("text") {
          text_of(block, "text").unwrap_or("")
        } else {
          ""
        }
      })
      .collect(),
    _ => String::new(),
  }
}

// Whether a line is words handed to the program — the round's prompt or words pushed into it — rather
// than a tool's answer, the program's own bookkeeping, a background task's notice or an interruption.
fn handed_words(record: &Value) -> bool {
  if kind(record) != Some("user") || is_true(record, "isSidechain") || is_true(record, "isMeta") {
    return false;
  }

  let spoken = spoken_text(record);
  let text = spoken.trim_start();

  !text.is_empty() && !text.starts_with(NOTICE_MARK) && !text.starts_with(INTERRUPTED_MARK)
}

/// How a round reads the turns its program opens on its own, on a background task's finished notice
/// (docs/agent-control.md, 「The round's answer」, 「Background work on a line that carries it apart」).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notices {
  /// The round that launched the work waits for its notice's turn and adds that turn's answer to its own: a
  /// job's turn, and a turn of every line that does not carry its background apart.
  Joined,
  /// A notice's turn is no round's: the round owes it nothing, waits for none and takes nothing from it, and
  /// a notice still queued is not words the round handed over.
  Apart,
}

impl Notices {
  /// How a turn of the line reads notices, as the line declares it.
  pub fn of_line(line: &str) -> Self {
    if policy::run_lines::RUN_LINES.background_apart(line) {
      Self::Apart
    } else {
      Self::Joined
    }
  }
}

/// What a round of the program still has coming, out of the record it wrote since the round went in
/// (docs/agent-control.md, 「Words that arrive while a turn is working」): the words it was handed and
/// has not yet taken, and the turns the round owes — the one it opened with, and one more for every
/// stretch between two end lines in which words were taken off the queue. Words that waited together
/// are taken off together and answered in one turn. A word taken inside a turn is removed rather than
/// taken off, and opens nothing; words pulled back into the input leave nothing queued. A word pushed
/// after the turn had ended is never queued: the program takes it straight as a prompt, so words handed
/// over that open a stretch nothing was taken off the queue for owe a turn of their own. Read with notices
/// apart, a queued notice is no word handed over and one taken off the queue owes the round nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Owed {
  pub queued: u64,
  pub owed: u64,
}

pub fn turns_owed(records: &[Value], notices: Notices) -> Owed {
  // What stands queued, in order, each marked by whether it is a notice: the queue is taken off from its
  // front.
  let mut queue: std::collections::VecDeque<bool> = std::collections::VecDeque::new();
  let mut owed: u64 = 1;
  let mut taken_since_end = false;
  // Right after an end line, until the next words handed over: whether the turn that follows was
  // already counted by what was taken off the queue before the end.
  let mut after_end: Option<bool> = None;

  for record in records {
    if is_turn_end(record) {
      after_end = Some(taken_since_end);
      taken_since_end = false;
      continue;
    }

    if let Some(counted) = after_end
      && handed_words(record)
    {
      if !counted && !taken_since_end {
        owed += 1;
      }

      after_end = None;
      continue;
    }

    if kind(record) != Some("queue-operation") {
      continue;
    }

    let notice = queued_notice(record);

    match text_of(record, "operation") {
      Some("enqueue") => queue.push_back(notice),
      Some("remove") => {
        let at = queue.iter().position(|queued| *queued == notice).unwrap_or(0);

        queue.remove(at);
      }
      Some("popAll") => queue.clear(),
      Some("dequeue") => {
        let taken_notice = queue.pop_front().unwrap_or(false);

        if notices == Notices::Apart && taken_notice {
          continue;
        }

        if !taken_since_end {
          owed += 1;
        }

        taken_since_end = true;
      }
      _ => {}
    }
  }

  let queued = match notices {
    Notices::Joined => queue.len(),
    Notices::Apart => queue.iter().filter(|notice| !**notice).count(),
  };

  Owed {
    queued: queued as u64,
    owed,
  }
}

// Whether a queued entry is a background task's notice rather than words handed over: the program writes the
// notice it queues whole.
fn queued_notice(record: &Value) -> bool {
  text_of(record, "content").is_some_and(|content| content.trim_start().starts_with(NOTICE_MARK))
}

// A queue operation as a reading of the whole record reads it ([`turns_owed`], [`rests_apart`]): what it did
// to the queue, and whether what it queued is a notice.
fn queue_mark(record: &Value) -> Value {
  serde_json::json!({
    "type": "queue-operation",
    "operation": record.get("operation").cloned().unwrap_or(Value::Null),
    "content": if queued_notice(record) { NOTICE_MARK } else { "" },
  })
}

/// Whether the program's foreground is at rest, read off the record alone where a background task of its
/// own keeps its screen reading as working: the last line of the conversation, notices queued or taken up
/// inside a turn passed over, is a turn's end, and no word handed over still waits in its queue.
pub fn rests_apart(records: &[Value]) -> bool {
  let last = records.iter().rev().find(|record| {
    is_turn_end(record)
      || matches!(kind(record), Some("user" | "assistant"))
      || (kind(record) == Some("queue-operation") && !queued_notice(record))
  });

  last.is_some_and(is_turn_end) && turns_owed(records, Notices::Apart).queued == 0
}

/// Where a round of the program stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundState {
  Working,
  Ended,
  Interrupted,
}

/// A round of the program read out of what its record gained since it went in: where it stands, and
/// the answer built turn by turn — a turn opened by words handed over writes the reply again whole and
/// replaces it, and a turn the program opened on a background task's notice adds to it, or, read with
/// notices apart, is passed over — with each turn's answer it was built from, in order. A round with an
/// error in the answer's place answers nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Round {
  pub state: RoundState,
  pub text: String,
  pub answers: Vec<TurnAnswer>,
}

pub fn read_round(records: &[Value], notices: Notices) -> Round {
  // A round read on every look of a turn is read for its answers alone: what else a turn's reading
  // gathers, its spend and the work it left going, is read once, at the turn's end.
  let errored = last_answer(records).0.is_some_and(|last| is_true(last, "isApiErrorMessage"));
  let Owed { queued, owed } = turns_owed(records, notices);
  let mut ends: u64 = 0;
  let mut last_end: Option<usize> = None;
  let mut last_interrupt: Option<usize> = None;
  let mut answer = String::new();
  let mut answers = Vec::new();
  let mut from = 0;

  for (at, record) in records.iter().enumerate() {
    if is_turn_end(record) {
      last_end = Some(at);
      ends += 1;

      let turn = &records[from..=at];
      let text = answer_since(turn, 0);

      let opened = if turn.iter().any(handed_words) { Opened::Handed } else { Opened::Program };

      // Read with notices apart, a turn the program opened on its own is no round's.
      if !text.is_empty() && !(notices == Notices::Apart && opened == Opened::Program) {
        answer = if answer.is_empty() || opened == Opened::Handed {
          text.clone()
        } else {
          format!("{answer}\n\n{text}")
        };
        answers.push(TurnAnswer { text, opened });
      }

      from = at + 1;
    } else if kind(record) == Some("user") && spoken_text(record).starts_with(INTERRUPTED_MARK) {
      last_interrupt = Some(at);
    }
  }

  let interrupted = match (last_interrupt, last_end) {
    (Some(interrupt), Some(end)) => interrupt > end,
    (Some(_), None) => true,
    _ => false,
  };
  // Read with notices apart, the round is over once the last words it handed over are answered: every word
  // taken, and the turn the last of them went into ended. Turns the program opened on its own are not counted.
  let ended = !interrupted
    && queued == 0
    && match notices {
      Notices::Joined => ends >= owed,
      Notices::Apart => records
        .iter()
        .rposition(handed_words)
        .is_some_and(|at| records[at..].iter().any(is_turn_end)),
    };

  Round {
    state: if interrupted {
      RoundState::Interrupted
    } else if ended {
      RoundState::Ended
    } else {
      RoundState::Working
    },
    text: if errored { String::new() } else { answer },
    answers: if errored { Vec::new() } else { answers },
  }
}

/// What the two status readings either side of a turn say about it: its money — the running total's
/// difference, and nothing where the two are not of one session — and, from the reading after it, the
/// account's windows, the context the model is holding, the model and the session.
#[derive(Clone, Debug, PartialEq)]
pub struct Figures {
  pub money: Option<f64>,
  pub windows: Vec<UsageWindow>,
  pub session_usage: Option<SessionUsage>,
  pub context_ratio: Option<f64>,
  pub context_tokens: Option<i64>,
  pub model: Option<String>,
  pub session_id: Option<String>,
}

pub fn status_figures(before: Option<&Value>, after: Option<&Value>) -> Figures {
  let before = before.filter(|reading| reading.is_object() || reading.is_array());
  let after = after.filter(|reading| reading.is_object() || reading.is_array());
  let same_session = matches!((before, after), (Some(before), Some(after)) if before.get("session_id") == after.get("session_id"));
  let total_before = number_of(before.and_then(|reading| reading.pointer("/cost/total_cost_usd")));
  let total_after = number_of(after.and_then(|reading| reading.pointer("/cost/total_cost_usd")));
  let windows: Vec<UsageWindow> = after
    .and_then(|reading| reading.get("rate_limits"))
    .and_then(Value::as_object)
    .map(|limits| {
      limits
        .iter()
        .filter_map(|(name, window)| {
          number_of(window.get("used_percentage")).map(|used| UsageWindow {
            name: name.clone(),
            utilization: used / 100.0,
            resets_at: number_of(window.get("resets_at")),
          })
        })
        .collect()
    })
    .unwrap_or_default();
  let short = windows.iter().find(|window| window.name == "five_hour");
  let weekly = windows.iter().find(|window| window.name == "seven_day");
  let used = number_of(after.and_then(|reading| reading.pointer("/context_window/used_percentage")));
  // What the model is holding, as the status line counts its last request: everything sent to it,
  // read from the cache or written into it.
  let current = after
    .and_then(|reading| reading.pointer("/context_window/current_usage"))
    .filter(|current| !current.is_null());
  let field = |current: &Value, name: &str| number_of(current.get(name)).unwrap_or(0.0);

  Figures {
    money: match (same_session, total_before, total_after) {
      (true, Some(before), Some(after)) => Some((after - before).max(0.0)),
      _ => None,
    },
    session_usage: short.and_then(|short| {
      short.resets_at.map(|resets_at| SessionUsage {
        utilization: short.utilization,
        resets_at,
        weekly_utilization: weekly.map(|weekly| weekly.utilization),
        weekly_resets_at: weekly.and_then(|weekly| weekly.resets_at),
      })
    }),
    context_ratio: used.map(|used| used / 100.0),
    context_tokens: current.map(|current| {
      (field(current, "input_tokens")
        + field(current, "cache_creation_input_tokens")
        + field(current, "cache_read_input_tokens")) as i64
    }),
    model: after
      .and_then(|reading| reading.pointer("/model/id"))
      .and_then(Value::as_str)
      .map(str::to_owned),
    session_id: after.and_then(|reading| text_of(reading, "session_id")).map(str::to_owned),
    windows,
  }
}

/// What a line of the record says about a child run: asked for, started, completed, failed or never
/// resolved, with the evidence that said so, and whether its id names a session of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildEvidence {
  pub id: String,
  pub state: &'static str,
  pub evidence: String,
}

/// Claude Code's own evidence of the child runs a turn opens — a tool_use block, the structured result
/// beside a tool's return, the system lines a task writes. Plain tool text, an asynchronous launch's
/// acknowledgement included, is not evidence that a child finished: only a structured result or a task
/// notification says that. A task announces itself under a name of its own, and only the tool return
/// that launched it joins the two, so the pairing is kept for as long as the run lasts.
#[derive(Debug, Default)]
pub struct ChildReader {
  tasks: HashMap<String, String>,
}

impl ChildReader {
  pub fn read(&mut self, event: &Value, known: impl Fn(&str) -> bool) -> Vec<ChildEvidence> {
    let mut said = vec![];
    let evidence = |id: &str, state: &'static str, evidence: &str| ChildEvidence {
      id: id.to_owned(),
      state,
      evidence: evidence.to_owned(),
    };

    for block in blocks_of(event) {
      if text_of(block, "type") == Some("tool_use")
        && let Some(id) = text_of(block, "id")
        && matches!(text_of(block, "name"), Some("Agent" | "Task" | "Workflow"))
      {
        said.push(evidence(id, "requested", "claude.tool_use"));
      }

      let Some(called) =
        text_of(block, "tool_use_id").filter(|_| text_of(block, "type") == Some("tool_result"))
      else {
        continue;
      };

      if !known(called) {
        continue;
      }

      if is_true(block, "is_error") {
        said.push(evidence(called, "failed", "claude.tool_result.is_error"));
      }

      let result = event.get("toolUseResult");
      let status = result.and_then(|result| text_of(result, "status"));

      if result.and_then(|result| text_of(result, "agentId")).is_some() && status == Some("completed") {
        said.push(evidence(called, "completed", "claude.toolUseResult.completed"));
      } else if status == Some("async_launched") {
        said.push(evidence(called, "started", "claude.toolUseResult.async_launched"));

        let task =
          result.and_then(|result| text_of(result, "taskId").or_else(|| text_of(result, "agentId")));

        if let Some(task) = task {
          self.tasks.insert(task.to_owned(), called.to_owned());
        }
      }
    }

    if kind(event) != Some("system") {
      return said;
    }

    let task = text_of(event, "task_id");
    let Some(id) = text_of(event, "tool_use_id")
      .map(str::to_owned)
      .or_else(|| task.and_then(|task| self.tasks.get(task).cloned()))
      .filter(|id| !id.is_empty())
    else {
      return said;
    };

    if !known(&id) && !matches!(text_of(event, "task_type"), Some("local_agent" | "local_workflow")) {
      return said;
    }

    if let Some(task) = task {
      self.tasks.insert(task.to_owned(), id.clone());
    }

    if text_of(event, "subtype") == Some("task_started") {
      said.push(evidence(&id, "started", "claude.task_started"));
    }

    if text_of(event, "subtype") == Some("task_notification") {
      let status = event.get("status");
      let state = match status.and_then(Value::as_str) {
        Some("completed") => "completed",
        Some("failed") => "failed",
        _ => "unresolved",
      };
      let word = match status {
        Some(Value::String(word)) => word.clone(),
        None => "undefined".to_owned(),
        Some(other) => other.to_string(),
      };

      said.push(evidence(&id, state, &format!("claude.task_notification:{word}")));
    }

    said
  }
}
