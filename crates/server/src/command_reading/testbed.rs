//! A heavy test or build command a job starts outside the testbed, refused with the line that runs it there
//! (docs/live-claude-guard.md, 「The same hook sends a heavy command to the testbed」). Whether a command is
//! heavy is the testbed's answer: its languages claim their own heavy commands, and its `classify` verb reads
//! the line.

use serde::Deserialize;

/// What the testbed's `classify --json` answers for one shell line (the testbed's `docs/boundary.md`,
/// 「classify」).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Classified {
  pub heavy: bool,
  pub language: Option<String>,
  pub command: Option<Vec<String>>,
  pub instead: Option<String>,
}

/// Why the command may not run as written, with the line that runs it in the testbed, or nothing.
pub fn testbed_refusal(classified: &Classified) -> Option<String> {
  if !classified.heavy {
    return None;
  }

  let words = classified
    .command
    .as_ref()
    .map_or_else(|| "this command".to_owned(), |words| words.join(" "));
  let claims = classified
    .language
    .as_ref()
    .map(|language| format!(" ({language})"))
    .unwrap_or_default();
  let instead = classified.instead.as_deref().unwrap_or("the testbed's own run");

  Some(format!(
    "refused: {words}{claims} is a heavy command, and a job runs a heavy command in the testbed, which waits for the machine's turn. Run instead, naming no timeout or the largest, 2147483647: {instead} (docs/live-claude-guard.md, 「The same hook sends a heavy command to the testbed」)"
  ))
}
