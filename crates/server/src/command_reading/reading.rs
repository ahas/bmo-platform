//! The guard's reading of a command for what it writes and what it starts (docs/live-claude-guard.md):
//! each simple command's words with their quoting taken off, the commands every unquoted substitution
//! runs read as simple commands of their own, the variables the command assigns itself put in where it
//! uses them, and the program each command runs.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use super::shell::{self, Heredoc};

/// One simple command with its quoting taken off.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Segment {
  pub words: Vec<String>,
  pub redirects: Vec<String>,
  pub appended: Vec<String>,
  pub inputs: Vec<String>,
}

/// A command line read for the guard.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reading {
  pub commands: Vec<Segment>,
  /// Whether a command substitution stands where a program word does, which makes its output the program
  /// and every word of the command something that may be run as one.
  pub substituted_program: bool,
  pub heredocs: Vec<Heredoc>,
}

// The words a compound command opens with, which stand before a program without being one.
const KEYWORDS: [&str; 9] = ["then", "do", "else", "elif", "if", "while", "until", "{", "!"];

/// The command read: the commands its unquoted substitutions run come before the command they stand in,
/// as the shell runs them.
pub fn read_command(text: &str) -> Reading {
  let split = shell::split(text);
  let mut reading = Reading {
    heredocs: split.heredocs,
    ..Reading::default()
  };

  for inner in split.inner.iter().filter(|inner| !inner.quoted) {
    let read = read_command(&inner.text);

    reading.commands.extend(read.commands);
    reading.heredocs.extend(read.heredocs);
    reading.substituted_program |= read.substituted_program;
  }

  for command in split.commands {
    let values: Vec<String> = command.words.iter().map(|word| shell::value(word)).collect();
    let program = values
      .iter()
      .position(|word| !(shell::is_assignment(word) || KEYWORDS.contains(&word.as_str())));

    if program.is_some_and(|at| command.words[at].starts_with("$(") || command.words[at].starts_with('`')) {
      reading.substituted_program = true;
    }

    reading.commands.push(Segment {
      words: values,
      redirects: command.redirects.iter().map(|word| shell::value(word)).collect(),
      appended: command.appended.iter().map(|word| shell::value(word)).collect(),
      inputs: command.inputs.iter().map(|word| shell::value(word)).collect(),
    });
  }

  reading
}

// A variable the command gives a value to itself, and a use of one. Each Bash call is a fresh shell, so a
// variable the command did not assign is the environment's, and one it did is read as the shell would.
static ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(
    r#"(?:^|[\s;&|(])(?:export\s+|local\s+|readonly\s+|typeset\s+|declare\s+)?([A-Za-z_][A-Za-z0-9_]*)=("[^"]*"|'[^']*'|[^\s;&|)]*)"#,
  )
  .expect("an assignment pattern")
});
static VARIABLE_USE: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}|\$([A-Za-z_][A-Za-z0-9_]*)").expect("a variable pattern")
});

/// The command with the variables it assigns itself put in where it uses them. A value may name another
/// such variable, so the reading is repeated a few times.
pub fn expand_assigned(text: &str) -> String {
  let mut values: HashMap<String, String> = HashMap::new();

  for found in ASSIGNMENT.captures_iter(text) {
    let raw = &found[2];

    // A value a substitution makes is not known from the text.
    if raw.starts_with("$(") || raw.starts_with('`') {
      continue;
    }
    let bare = [('"', '"'), ('\'', '\'')]
      .iter()
      .find_map(|(open, close)| {
        (raw.len() >= 2 && raw.starts_with(*open) && raw.ends_with(*close))
          .then(|| raw[1..raw.len() - 1].to_owned())
      })
      .unwrap_or_else(|| raw.to_owned());

    values.insert(found[1].to_owned(), bare);
  }

  let mut expanded = text.to_owned();

  for _ in 0..3 {
    if values.is_empty() {
      break;
    }

    let next = VARIABLE_USE
      .replace_all(&expanded, |found: &regex::Captures| {
        let name = found.get(1).or_else(|| found.get(2)).map_or("", |name| name.as_str());

        values.get(name).cloned().unwrap_or_else(|| found[0].to_owned())
      })
      .into_owned();

    if next == expanded {
      break;
    }

    expanded = next;
  }

  expanded
}

/// A simple command: the program's name, the word it was spelled as, and the words after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
  pub program: String,
  pub word: String,
  pub args: Vec<String>,
}

// What may stand in front of a program without being the program: the commands whose whole work is to run
// the next word.
const RUNNERS: [&str; 15] = [
  "env", "command", "exec", "nohup", "time", "sudo", "nice", "ionice", "stdbuf", "setsid", "xargs",
  "timeout", "flock", "builtin", "unbuffer",
];

fn basename(word: &str) -> &str {
  word.rsplit('/').next().unwrap_or(word)
}

fn is_program_word(word: &str) -> bool {
  let name = basename(word);

  name == "claude" || shell::SHELLS.contains(&name) || RUNNERS.contains(&name)
}

/// The program a simple command runs, with the words after it, looking past assignments, the words a
/// compound command opens with, and runners.
pub fn program_of(words: &[String]) -> Option<Run> {
  let mut at = 0;

  while at < words.len() {
    let word = &words[at];

    if shell::is_assignment(word) || KEYWORDS.contains(&word.as_str()) {
      at += 1;
      continue;
    }

    let name = basename(word);

    if RUNNERS.contains(&name) {
      at += 1;

      // A runner's own flags and the one value some of them take (a duration, a lock file).
      while at < words.len() && words[at].starts_with('-') {
        at += 1;
      }

      if (name == "timeout" || name == "flock") && at < words.len() && !is_program_word(&words[at]) {
        at += 1;
      }

      continue;
    }

    return Some(Run {
      program: name.to_owned(),
      word: word.clone(),
      args: words[at + 1..].to_vec(),
    });
  }

  None
}

/// The piece of a command a refusal points at, short enough to stand in the reason: a job reads which word
/// was judged instead of guessing at the rule and writing around it.
pub fn shortly(text: &str) -> String {
  let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
  let cut = if flat.chars().count() > 60 {
    format!("{}…", flat.chars().take(60).collect::<String>())
  } else {
    flat
  };

  serde_json::to_string(&cut).unwrap_or_default()
}

/// The text a shell is handed with `-c`, among a run's words.
pub fn shell_text(run: &Run) -> Option<&String> {
  let at = run.args.iter().position(|arg| shell::is_command_flag(arg))?;

  run.args.get(at + 1)
}
