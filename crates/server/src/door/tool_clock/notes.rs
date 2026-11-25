//! What the call clock says of a call (docs/agent-control.md, 「A run's tool calls are held to a clock」):
//! the facts as fields, which a run's log keeps as the line's `toolClock` metadata, and the sentence the
//! unit's own log and the line's words read as. A line filed before the fields were kept is read back into
//! them out of its sentence; the sentence is this unit's own grammar, written here beside the reading, not
//! a general format any library reads.

use serde_json::{Value, json};

/// One thing the clock said of one call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClockNote {
  /// The turn's name the sentence opens with.
  pub run: String,
  /// The call's id in the record; none on a note read back out of its sentence.
  pub call: Option<String>,
  pub tool: String,
  /// The one-line summary of what the call was given.
  pub command: String,
  pub pid: Option<i32>,
  /// How long the call had run; none where a sentence read back never said it.
  pub elapsed_ms: Option<i64>,
  pub event: ClockEvent,
}

/// What befell the call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClockEvent {
  Started,
  /// Still running past the threshold.
  Running,
  /// An ask opened on it, silent and idle for so long.
  Asked {
    silent_ms: i64,
  },
  /// Kept on the supervisor's call for so much longer.
  Kept {
    for_ms: i64,
  },
  AskFailed {
    why: String,
  },
  /// Due for its stall but not stopped: it waits on the job's own question, still waiting for its answer.
  Awaiting,
  Cut(Cut),
  Ended(Outcome),
}

/// What cut a call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cut {
  NoProgress,
  NoProcess,
  Supervisor(String),
  Stop,
}

/// How a call ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
  Completed,
  Failed,
  EndedWithTurn,
}

impl Outcome {
  fn code(self) -> &'static str {
    match self {
      Self::Completed => "completed",
      Self::Failed => "failed",
      Self::EndedWithTurn => "ended-with-turn",
    }
  }

  fn said(self) -> &'static str {
    match self {
      Self::Completed => "completed",
      Self::Failed => "failed",
      Self::EndedWithTurn => "ended with the turn",
    }
  }

  fn of_said(said: &str) -> Option<Self> {
    [Self::Completed, Self::Failed, Self::EndedWithTurn]
      .into_iter()
      .find(|outcome| outcome.said() == said)
  }
}

impl Cut {
  fn code(&self) -> &'static str {
    match self {
      Self::NoProgress => "no-progress",
      Self::NoProcess => "no-process",
      Self::Supervisor(_) => "supervisor",
      Self::Stop => "stop",
    }
  }
}

impl ClockNote {
  /// The note's facts as the run log's metadata keeps them.
  pub fn fields(&self) -> Value {
    let mut clock = json!({
      "event": self.event_code(),
      "call": self.call,
      "tool": self.tool,
      "command": self.command,
      "pid": self.pid,
      "elapsedMs": self.elapsed_ms,
    });
    let extra = match &self.event {
      ClockEvent::Asked { silent_ms } => json!({ "silentMs": silent_ms }),
      ClockEvent::Kept { for_ms } => json!({ "keptMs": for_ms }),
      ClockEvent::AskFailed { why } => json!({ "why": why }),
      ClockEvent::Cut(cut @ Cut::Supervisor(why)) => json!({ "cut": cut.code(), "why": why }),
      ClockEvent::Cut(cut) => json!({ "cut": cut.code() }),
      ClockEvent::Ended(outcome) => json!({ "outcome": outcome.code() }),
      ClockEvent::Started | ClockEvent::Running | ClockEvent::Awaiting => json!({}),
    };

    if let (Some(clock), Value::Object(extra)) = (clock.as_object_mut(), extra) {
      clock.extend(extra);
    }

    json!({ "toolClock": clock })
  }

  fn event_code(&self) -> &'static str {
    match self.event {
      ClockEvent::Started => "started",
      ClockEvent::Running => "running",
      ClockEvent::Asked { .. } => "asked",
      ClockEvent::Kept { .. } => "kept",
      ClockEvent::AskFailed { .. } => "ask-failed",
      ClockEvent::Awaiting => "awaiting",
      ClockEvent::Cut(_) => "cut",
      ClockEvent::Ended(_) => "ended",
    }
  }

  /// The sentence the unit's log and the line's words read.
  pub fn sentence(&self) -> String {
    let run = &self.run;
    let tool = &self.tool;
    let summary = &self.command;
    let words = if summary.is_empty() { tool.clone() } else { format!("{tool} {summary}") };
    let pid = self.pid.map_or_else(|| "-".to_owned(), |pid| pid.to_string());
    let seconds = self.elapsed_ms.unwrap_or_default().max(0) as f64 / 1_000.0;

    match &self.event {
      ClockEvent::Started if summary.is_empty() => {
        format!("{run}: tool {tool} started pid={pid} after {seconds:.1}s")
      }
      ClockEvent::Started => format!("{run}: tool {tool} started pid={pid} after {seconds:.1}s: {summary}"),
      ClockEvent::Running => {
        format!("{run}: tool {tool} still running pid={pid} after {seconds:.0}s: {summary}")
      }
      ClockEvent::Asked { silent_ms } => format!(
        "{run}: tool {words} asked about after {seconds:.0}s, silent and idle for {:.0}s",
        *silent_ms as f64 / 1_000.0
      ),
      ClockEvent::Kept { for_ms } => format!(
        "{run}: tool {words} kept on the supervisor's call for {:.0}s more",
        *for_ms as f64 / 1_000.0
      ),
      ClockEvent::AskFailed { why } => format!("{run}: tool {words} could not be asked about: {why}"),
      ClockEvent::Awaiting => format!(
        "{run}: tool {words} waits on the job's own question after {seconds:.0}s with no progress, so it runs on: pid={pid}"
      ),
      ClockEvent::Cut(Cut::NoProcess) => {
        format!("{run}: tool {words} cut after {seconds:.0}s with no process of its own and no answer")
      }
      ClockEvent::Cut(Cut::NoProgress) => {
        format!("{run}: tool {words} cut after {seconds:.0}s with no progress: pid={pid}")
      }
      ClockEvent::Cut(Cut::Supervisor(why)) => format!(
        "{run}: tool {words} cut on the supervisor's call after {}s: pid={pid}: {why}",
        self.elapsed_ms.unwrap_or_default().max(0) / 1_000
      ),
      ClockEvent::Cut(Cut::Stop) => {
        format!("{run}: tool {words} cut for the stop after {seconds:.0}s: pid={pid}")
      }
      ClockEvent::Ended(outcome) => {
        format!("{run}: tool {words} ended pid={pid} after {seconds:.1}s: {}", outcome.said())
      }
    }
  }

  /// A sentence the clock wrote, read back into its facts; none for any other line. A summary is free text,
  /// so where it holds the words that follow it the reading may split it in the wrong place: such a line is
  /// read as the clock's all the same, its summary short of those words.
  pub fn read(text: &str) -> Option<Self> {
    let (run, rest) = text.split_once(": tool ")?;

    if run.is_empty() || run.contains(char::is_whitespace) {
      return None;
    }

    let (tool, after_tool) = rest.split_once(' ').unwrap_or((rest, ""));

    if let Some(said) = after_tool.strip_prefix("started pid=") {
      let (pid, said) = said.split_once(" after ")?;
      let (seconds, summary) = match said.split_once("s: ") {
        Some((seconds, summary)) => (seconds, summary),
        None => (said.strip_suffix('s')?, ""),
      };

      return Some(Self::of(run, tool, summary, pid_of(pid)?, Some(ms(seconds)?), ClockEvent::Started));
    }

    if let Some(said) = after_tool.strip_prefix("still running pid=") {
      let (pid, said) = said.split_once(" after ")?;
      let (seconds, summary) = said.split_once("s: ")?;

      return Some(Self::of(run, tool, summary, pid_of(pid)?, Some(ms(seconds)?), ClockEvent::Running));
    }

    let words = |words: &str| words.split_once(' ').map_or((words.to_owned(), String::new()), split);

    if let Some((said, tail)) = rest.rsplit_once(" ended pid=") {
      let (pid, tail) = tail.split_once(" after ")?;
      let (seconds, outcome) = tail.split_once("s: ")?;
      let (tool, summary) = words(said);

      return Some(Self::of(
        run,
        &tool,
        &summary,
        pid_of(pid)?,
        Some(ms(seconds)?),
        ClockEvent::Ended(Outcome::of_said(outcome)?),
      ));
    }

    if let Some((said, tail)) = rest.rsplit_once(" asked about after ") {
      let (seconds, silent) = tail.split_once("s, silent and idle for ")?;
      let (tool, summary) = words(said);
      let silent_ms = ms(silent.strip_suffix('s')?)?;

      return Some(Self::of(
        run,
        &tool,
        &summary,
        None,
        Some(ms(seconds)?),
        ClockEvent::Asked { silent_ms },
      ));
    }

    if let Some((said, tail)) = rest.rsplit_once(" kept on the supervisor's call for ") {
      let for_ms = ms(tail.strip_suffix("s more")?)?;
      let (tool, summary) = words(said);

      return Some(Self::of(run, &tool, &summary, None, None, ClockEvent::Kept { for_ms }));
    }

    if let Some((said, why)) = rest.split_once(" could not be asked about: ") {
      let (tool, summary) = words(said);

      return Some(Self::of(
        run,
        &tool,
        &summary,
        None,
        None,
        ClockEvent::AskFailed {
          why: why.to_owned(),
        },
      ));
    }

    if let Some((said, tail)) = rest.split_once(" cut on the supervisor's call after ") {
      let (seconds, tail) = tail.split_once("s: pid=")?;
      let (pid, why) = tail.split_once(": ")?;
      let (tool, summary) = words(said);
      let cut = ClockEvent::Cut(Cut::Supervisor(why.to_owned()));

      return Some(Self::of(run, &tool, &summary, pid_of(pid)?, Some(ms(seconds)?), cut));
    }

    if let Some((said, tail)) = rest.rsplit_once(" cut for the stop after ") {
      let (seconds, pid) = tail.split_once("s: pid=")?;
      let (tool, summary) = words(said);

      return Some(Self::of(
        run,
        &tool,
        &summary,
        pid_of(pid)?,
        Some(ms(seconds)?),
        ClockEvent::Cut(Cut::Stop),
      ));
    }

    let (said, tail) = rest.rsplit_once(" cut after ")?;
    let (tool, summary) = words(said);

    if let Some(seconds) = tail.strip_suffix("s with no process of its own and no answer") {
      return Some(Self::of(
        run,
        &tool,
        &summary,
        None,
        Some(ms(seconds)?),
        ClockEvent::Cut(Cut::NoProcess),
      ));
    }

    let (seconds, pid) = tail.split_once("s with no progress: pid=")?;

    Some(Self::of(
      run,
      &tool,
      &summary,
      pid_of(pid)?,
      Some(ms(seconds)?),
      ClockEvent::Cut(Cut::NoProgress),
    ))
  }

  fn of(
    run: &str,
    tool: &str,
    summary: &str,
    pid: Option<i32>,
    elapsed_ms: Option<i64>,
    event: ClockEvent,
  ) -> Self {
    Self {
      run: run.to_owned(),
      call: None,
      tool: tool.to_owned(),
      command: summary.to_owned(),
      pid,
      elapsed_ms,
      event,
    }
  }
}

fn split((tool, summary): (&str, &str)) -> (String, String) {
  (tool.to_owned(), summary.to_owned())
}

// A process id as a sentence writes it: a number, or `-` where the call had none. A word that is neither is
// no clock sentence.
fn pid_of(said: &str) -> Option<Option<i32>> {
  if said == "-" { Some(None) } else { said.parse().ok().map(Some) }
}

// Seconds as a sentence writes them, whole or to a tenth, as milliseconds.
fn ms(said: &str) -> Option<i64> {
  let seconds: f64 = said.parse().ok()?;

  seconds.is_finite().then(|| (seconds * 1_000.0).round() as i64)
}
