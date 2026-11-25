//! A turn that goes nowhere, read off its run as it streams (docs/supervisors.md, 「A turn that goes
//! nowhere」): the tool calls and results its log takes, judged by repetition first — the same tool failing
//! again and again, calls that repeat themselves and learn nothing — and by a cap on its calls last. No
//! clock cuts a turn. The cap bounds the words a turn takes rather than one run of them: a run starts from
//! what the runs before it on the same words spent, which the room's session keeps.

use policy::fill_marks;
use policy::supervisor_runner::{Limits, SUPERVISOR_RUNNER};

use crate::run_log::lines::Line;

/// What the runs on a turn's words have spent: the tool calls made and the milliseconds run, the wait for
/// the engine not counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Spent {
  pub calls: i64,
  pub ms: i64,
}

fn too_many_calls(calls: i64) -> String {
  fill_marks(&SUPERVISOR_RUNNER.cut.too_many_calls, &[("calls", &calls.to_string())])
}

/// Why a turn whose words already stand at a net is cut before its run starts, or nothing.
pub fn spent_out(limits: &Limits, spent: Spent) -> Option<String> {
  (spent.calls >= limits.calls).then(|| too_many_calls(spent.calls))
}

/// How alike two texts are by their pairs of characters (the Sørensen–Dice coefficient), from 0 to 1, the
/// pairs counted in UTF-16 units.
pub fn likeness(one: &str, other: &str) -> f64 {
  if one == other {
    return 1.0;
  }

  let one: Vec<u16> = one.encode_utf16().collect();
  let other: Vec<u16> = other.encode_utf16().collect();

  if one.len() < 2 || other.len() < 2 {
    return 0.0;
  }

  let mut pairs: std::collections::HashMap<[u16; 2], usize> = std::collections::HashMap::new();

  for pair in one.windows(2) {
    *pairs.entry([pair[0], pair[1]]).or_default() += 1;
  }

  let mut shared = 0;

  for pair in other.windows(2) {
    if let Some(left) = pairs.get_mut(&[pair[0], pair[1]])
      && *left > 0
    {
      shared += 1;
      *left -= 1;
    }
  }

  (2 * shared) as f64 / (one.len() - 1 + other.len() - 1) as f64
}

struct Call {
  tool: String,
  input: String,
  // The earlier calls of the window this one repeats, by their place among the calls.
  repeats: Vec<usize>,
  // What it answered, once its result is in.
  result: Option<String>,
}

/// The guard over one run of a turn.
pub struct Guard<'a> {
  limits: &'a Limits,
  carried: i64,
  calls: Vec<Call>,
  by_reference: std::collections::HashMap<String, usize>,
  failing: Option<(String, usize)>,
  repeating: usize,
}

impl<'a> Guard<'a> {
  /// A guard under the limits, starting from the calls the runs before it on the same words made.
  pub fn new(limits: &'a Limits, carried: i64) -> Self {
    Self {
      limits,
      carried,
      calls: Vec::new(),
      by_reference: std::collections::HashMap::new(),
      failing: None,
      repeating: 0,
    }
  }

  /// The calls the turn's words have taken, the runs before this one's included.
  pub fn made(&self) -> i64 {
    self.carried + self.calls.len() as i64
  }

  /// Why the turn must stop at this line of its run, or nothing where it goes on.
  pub fn see(&mut self, line: &Line) -> Option<String> {
    match line.kind {
      "tool" => self.called(line),
      "tool-result" => self.answered(line),
      _ => None,
    }
  }

  fn called(&mut self, line: &Line) -> Option<String> {
    let tool = line.name.clone().unwrap_or_default();
    let window = self.calls.len().saturating_sub(self.limits.repeat_window);
    let repeats = (window..self.calls.len())
      .filter(|at| {
        let one = &self.calls[*at];

        one.tool == tool && likeness(&one.input, &line.text) >= self.limits.likeness
      })
      .collect();

    self.calls.push(Call {
      tool,
      input: line.text.clone(),
      repeats,
      result: None,
    });

    if let Some(reference) = &line.reference {
      self.by_reference.insert(reference.clone(), self.calls.len() - 1);
    }

    (self.made() >= self.limits.calls).then(|| too_many_calls(self.made()))
  }

  fn answered(&mut self, line: &Line) -> Option<String> {
    let at = *self.by_reference.get(line.reference.as_ref()?)?;
    let failed = line.state == Some("failed");
    let tool = self.calls[at].tool.clone();

    self.failing = match (&self.failing, failed) {
      (_, false) => None,
      (Some((held, count)), true) if *held == tool => Some((tool.clone(), count + 1)),
      (_, true) => Some((tool.clone(), 1)),
    };

    // A repeated call learns nothing where it fails, or answers what a call it repeats answered.
    let call = &self.calls[at];
    let stale = !call.repeats.is_empty()
      && (failed || call.repeats.iter().any(|one| self.calls[*one].result.as_deref() == Some(&line.text)));
    let repeats = !call.repeats.is_empty();

    self.calls[at].result = Some(line.text.clone());
    self.repeating = if stale {
      self.repeating + 1
    } else if repeats {
      self.repeating
    } else {
      0
    };

    if let Some((tool, count)) = &self.failing
      && *count >= self.limits.failures
    {
      return Some(fill_marks(
        &SUPERVISOR_RUNNER.cut.failing,
        &[("tool", tool), ("count", &count.to_string())],
      ));
    }

    (self.repeating >= self.limits.repeats).then(|| {
      fill_marks(
        &SUPERVISOR_RUNNER.cut.repeating,
        &[("tool", &tool), ("count", &self.repeating.to_string())],
      )
    })
  }
}
