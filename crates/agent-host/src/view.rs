//! An agent's view file, which its head pane follows (docs/agent-control.md, 「The host」): the lines
//! the host knows from the envelopes, appended, and the file kept to its newest part.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset};

use crate::disk::{private_dir_below, publish_bytes};
use crate::envelope::{Cause, Outcome};
use crate::layout::Layout;

// A file grown past this is cut back to its newest `KEPT` bytes.
pub const CEILING: u64 = 512 * 1024;
pub const KEPT: usize = 256 * 1024;

// How much of a failed execution's error stream its end line carries under it.
const FAILURE_LINES: usize = 3;
pub const FAILURE_WIDTH: usize = 300;

// The hour and minute in Seoul, which keeps no daylight saving.
fn clock(at_ms: u64) -> String {
  let seoul = FixedOffset::east_opt(9 * 3600).expect("nine hours is an offset");

  DateTime::from_timestamp_millis(at_ms as i64)
    .map(|at| at.with_timezone(&seoul).format("%H:%M").to_string())
    .unwrap_or_default()
}

/// An execution beginning, headed by its title and name.
pub fn began_line(title: &str, name: &str, since_ms: u64) -> String {
  format!("── {} {title} ({name}) started", clock(since_ms))
}

/// An execution ended, as its outcome says.
pub fn ended_line(title: &str, at_ms: u64, outcome: &Outcome) -> String {
  let how = match outcome.cause {
    Cause::Exited if outcome.exit_code == 0 => "ended".to_owned(),
    Cause::Exited => format!("ended · exit code {}", outcome.exit_code),
    other => other.as_str().to_owned(),
  };

  format!("── {} {title} {how}", clock(at_ms))
}

/// What an execution that ended badly said on its error stream, under its end line: its last few lines
/// that hold anything, each cut to a line's length. Its directory goes once its row is closed, so this is
/// where the reason it gave stays readable.
pub fn failure_lines(outcome: &Outcome, stderr: &str) -> Vec<String> {
  if outcome.cause == Cause::Exited && outcome.exit_code == 0 {
    return vec![];
  }

  let said: Vec<&str> = stderr.lines().map(str::trim).filter(|line| !line.is_empty()).collect();

  said[said.len().saturating_sub(FAILURE_LINES)..]
    .iter()
    .map(|line| {
      let cut: String = line.chars().take(FAILURE_WIDTH).collect();

      if cut.len() < line.len() { format!("   {cut}…") } else { format!("   {cut}") }
    })
    .collect()
}

/// The agent at rest, since its last execution ended where that is known.
pub fn resting_line(since_ms: Option<u64>) -> String {
  since_ms.map_or_else(|| "resting".to_owned(), |since| format!("resting · since {}", clock(since)))
}

/// The view files of one data root.
pub struct Views {
  // The executions directory, which a view is never written without.
  base: PathBuf,
  dir: PathBuf,
}

impl Views {
  pub fn new(layout: &Layout) -> Self {
    Self {
      base: layout.executions.clone(),
      dir: layout.views(),
    }
  }

  /// Where an agent's view stands: its name, where that is a plain word, and a hash of it otherwise.
  pub fn path(&self, agent: &str) -> PathBuf {
    self.dir.join(crate::layout::name_component(agent))
  }

  /// Lines appended to an agent's view, the file cut back once it has grown past its ceiling.
  pub fn write(&self, agent: &str, lines: &[String]) -> io::Result<()> {
    // Made below the executions directory only, so a data root taken away is not made again by a
    // late line.
    private_dir_below(&self.base, &self.dir)?;

    let path = self.path(agent);
    let mut file = OpenOptions::new().create(true).append(true).mode(0o600).open(&path)?;
    let mut text = lines.join("\n");

    text.push('\n');
    file.write_all(text.as_bytes())?;

    if file.metadata()?.len() > CEILING {
      trim(&path)?;
    }

    Ok(())
  }
}

// The file replaced whole by its newest part, cut just after a newline so no line is left half.
fn trim(path: &Path) -> io::Result<()> {
  let bytes = fs::read(path)?;
  let from = bytes.len().saturating_sub(KEPT);
  let start = bytes[from..].iter().position(|byte| *byte == b'\n').map_or(from, |at| from + at + 1);

  publish_bytes(path, &bytes[start..])
}
