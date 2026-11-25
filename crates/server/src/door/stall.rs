//! A turn's progress kept as one moment (docs/agent-control.md, 「A run's tool calls are held to a clock」, a job's
//! turn that makes no progress). The call clock judges one call at a time; this keeps, across every call and the
//! record between them, the last moment the turn moved, and says once when it has stood still for the stall span
//! and again when it moves after that.
//!
//! Each read hands it what the clock saw: the record's new lines, a call's output grown with its last line, the
//! processor time a call's tree burned since the read before. A line alike one of the last few of its source,
//! every run of digits taken as one, is no progress, so a wait that keeps saying it still waits and a poll made
//! again and again with the same answer leave the moment where it was. It reads no file and no process itself:
//! the clock reads them, and every moment is handed in.

use std::collections::{HashMap, VecDeque};
use std::hash::{DefaultHasher, Hash as _, Hasher as _};

use policy::tool_clock::TOOL_CLOCK;
use serde_json::Value;

/// The numbers a turn's stall is judged by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StallClock {
  /// How long the turn may make no progress before it is told.
  pub after_ms: i64,
  /// The share of one processor, in percent, above which a tree's processor time is progress.
  pub idle_cpu_percent: u64,
  /// How many last lines of a source a new one is compared with.
  pub recent: usize,
  /// How many last lines of what a call waits on are told.
  pub lines: usize,
  /// How many characters of a call's command and of each line are told.
  pub chars: usize,
}

impl StallClock {
  /// The numbers the policy names.
  pub fn policy() -> Self {
    let stall = &TOOL_CLOCK.stall;

    Self {
      after_ms: i64::try_from(stall.after_seconds).unwrap_or(i64::MAX / 1_000).saturating_mul(1_000),
      idle_cpu_percent: TOOL_CLOCK.call.silent.idle_cpu_percent,
      recent: stall.recent,
      lines: stall.lines,
      chars: stall.chars,
    }
  }
}

/// One running call a stalled turn waits on: its tool and command, and the last lines of what it waits on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Waits {
  pub tool: String,
  pub lines: Vec<String>,
}

/// A turn that has made no progress since `since`, told at `at`, and the calls it waits on, none where the
/// model's own turn is what stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stalled {
  pub since: i64,
  pub at: i64,
  pub waits: Vec<Waits>,
}

/// What became of a turn's progress: it stalled, or it moved again after a stall was told.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StallChange {
  Stalled(Stalled),
  Resumed,
}

/// The key two lines are alike by: the line with every run of digits taken as one and its spaces flattened.
pub fn alike(text: &str) -> u64 {
  let mut hasher = DefaultHasher::new();
  let mut digits = false;

  for word in text.split_whitespace() {
    for one in word.chars() {
      if one.is_ascii_digit() {
        if !digits {
          '#'.hash(&mut hasher);
        }

        digits = true;
      } else {
        digits = false;
        one.hash(&mut hasher);
      }
    }

    digits = false;
    ' '.hash(&mut hasher);
  }

  hasher.finish()
}

// The last few lines of one source, by their keys.
#[derive(Clone, Debug, Default)]
struct Recent(VecDeque<u64>);

impl Recent {
  // Whether the line is unlike every one kept, kept now as the newest either way.
  fn new_line(&mut self, key: u64, size: usize) -> bool {
    let new = !self.0.contains(&key);

    self.0.retain(|kept| *kept != key);
    self.0.push_back(key);

    while self.0.len() > size.max(1) {
      self.0.pop_front();
    }

    new
  }
}

/// A turn's progress over its reads.
#[derive(Debug)]
pub struct Progress {
  clock: StallClock,
  // The last moment the turn moved; none before the first read of its record.
  moved_at: Option<i64>,
  // How many lines of the record have been read.
  read: usize,
  said: Recent,
  wrote: HashMap<String, Recent>,
  sizes: HashMap<String, u64>,
  told: bool,
  // Whether a stall may stand told from before this watch began, so its first progress withdraws it.
  maybe_told: bool,
  changes: VecDeque<StallChange>,
}

impl Progress {
  pub fn new(clock: StallClock) -> Self {
    Self {
      clock,
      moved_at: None,
      read: 0,
      said: Recent::default(),
      wrote: HashMap::new(),
      sizes: HashMap::new(),
      told: false,
      maybe_told: false,
      changes: VecDeque::new(),
    }
  }

  /// A watch that takes up a turn already going, as after a restart: a stall told before it began may stand, so
  /// the turn's first progress is said as a move after one.
  pub fn taken_up(clock: StallClock) -> Self {
    Self {
      maybe_told: true,
      ..Self::new(clock)
    }
  }

  /// The numbers it judges by.
  pub fn clock(&self) -> &StallClock {
    &self.clock
  }

  /// The last moment the turn moved, none before its record was first read.
  pub fn moved_at(&self) -> Option<i64> {
    self.moved_at
  }

  fn moved(&mut self, now: i64) {
    self.moved_at = Some(now);

    if self.told || self.maybe_told {
      self.changes.push_back(StallChange::Resumed);
    }

    self.told = false;
    self.maybe_told = false;
  }

  /// The record read: its lines from `from`, past those already read. A text, a tool call or a tool's result the
  /// model's turn wrote is progress where it is unlike the last few. The first read only learns them, and counts
  /// the turn's last progress from the time of the last such line, else from now.
  pub fn records(&mut self, records: &[Value], from: usize, now: i64) {
    let first = self.moved_at.is_none();
    // The last line read may have been read while it was still being written, so it is read again.
    let start = from.max(self.read.saturating_sub(1)).min(records.len());
    let mut last_at = None;
    let mut moved = false;

    for record in &records[start..] {
      for key in said_keys(record) {
        moved |= self.said.new_line(key, self.clock.recent);
        last_at = stamp_of(record).or(last_at);
      }
    }

    self.read = records.len();

    if first {
      self.moved_at = Some(last_at.unwrap_or(now).min(now));
    } else if moved {
      self.moved(now);
    }
  }

  /// A source of output read at `size`: a call's output, the file the job's own work writes, a subagent's
  /// record. Grown, it is progress where its last line, where `last_line` reads one, is unlike the last few it
  /// wrote; the first read of a source only learns it. The line is read only where the source is new or grew.
  pub fn output(&mut self, source: &str, size: u64, last_line: impl FnOnce() -> Option<String>, now: i64) {
    let was = self.sizes.insert(source.to_owned(), size);
    let grown = was.is_some_and(|was| size > was);

    if !grown && was.is_some() {
      return;
    }

    let recent = self.clock.recent;
    let new = match last_line() {
      Some(line) => self.wrote.entry(source.to_owned()).or_default().new_line(alike(&line), recent),
      None => true,
    };

    if grown && new {
      self.moved(now);
    }
  }

  /// A source no longer read, forgotten.
  pub fn forget(&mut self, source: &str) {
    self.sizes.remove(source);
    self.wrote.remove(source);
  }

  /// A tree that burned `burned_ms` of processor time across `span_ms`: progress where that is above the idle
  /// share of one processor.
  pub fn burned(&mut self, burned_ms: i64, span_ms: i64, now: i64) {
    let limit = i64::try_from(self.clock.idle_cpu_percent).unwrap_or(i64::MAX);

    if span_ms > 0 && burned_ms.saturating_mul(100) > limit.saturating_mul(span_ms) {
      self.moved(now);
    }
  }

  /// The turn judged at `now`: told once it has made no progress for the stall span, unless `known`, the
  /// supervisor already holding what it waits on. `waits` is read only where it is told.
  pub fn judge(&mut self, now: i64, known: bool, waits: impl FnOnce() -> Vec<Waits>) {
    let Some(since) = self.moved_at else { return };

    if self.told || known || now - since < self.clock.after_ms {
      return;
    }

    self.told = true;
    self.changes.push_back(StallChange::Stalled(Stalled {
      since,
      at: now,
      waits: waits(),
    }));
  }

  /// The next change to tell, oldest first.
  pub fn take(&mut self) -> Option<StallChange> {
    self.changes.pop_front()
  }
}

// The keys of what a record line holds of the model's turn: each text, each tool call by its tool and input,
// and each tool's result by what it answered. A thinking block, a line of the program's own and words put into
// the turn hold none.
fn said_keys(record: &Value) -> Vec<u64> {
  let kind = record.get("type").and_then(Value::as_str);
  let Some(blocks) = record.pointer("/message/content").and_then(Value::as_array) else {
    return vec![];
  };

  blocks
    .iter()
    .filter_map(|block| match (kind, block.get("type").and_then(Value::as_str)) {
      (Some("assistant"), Some("text")) => block
        .get("text")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(alike),
      (Some("assistant"), Some("tool_use")) => Some(alike(&format!(
        "{} {}",
        block.get("name").and_then(Value::as_str).unwrap_or_default(),
        block.get("input").unwrap_or(&Value::Null)
      ))),
      (Some("user"), Some("tool_result")) => Some(alike(&match block.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
      })),
      _ => None,
    })
    .collect()
}

fn stamp_of(record: &Value) -> Option<i64> {
  record
    .get("timestamp")
    .and_then(Value::as_str)
    .and_then(|stamp| chrono::DateTime::parse_from_rfc3339(stamp).ok())
    .map(|stamp| stamp.timestamp_millis())
}
