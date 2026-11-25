//! What a job's shell may not run before the job has opened the live-Claude guard for itself
//! (docs/live-claude-guard.md): a command that writes into the Claude Code CLI's own session records,
//! starts the CLI itself, or carries the flag that forks a session. It is a reading of text: it follows
//! the variables a command assigns itself and the text a command hands to `eval`, to `source` or to a
//! shell to run, but text a script hides in a file is out of its sight, and the guard in the launch code
//! stands behind it for that. The same reading finds the CLI started in print mode, which no opening lets
//! through, and every start of the CLI, which a conversational agent's shell guard refuses.

use std::sync::LazyLock;

use regex::Regex;

use super::reading::{self, Run, expand_assigned, program_of, read_command, shortly};
use super::shell::SHELLS;

/// The CLI's own session records, however the command spells the way there. Each login keeps its records
/// in the `projects` directory under its own configuration directory; the machine's own spelling and the
/// variable that names another are always among them, and the configuration directories of the accounts
/// are handed in, because which accounts there are is the store's and this reading of text never opens
/// it. Handed none, the records are the machine's spellings alone.
pub struct CliRecords {
  /// Whether a text names the records anywhere.
  pub named: Regex,
  /// Whether a literal path leads into a configuration directory.
  pub home: Regex,
}

/// A directory as a command may spell it: as it stands, and under the home as `~`, `$HOME` and `${HOME}`.
pub fn spellings(root: &str, home: &str) -> Vec<String> {
  let bare = root.trim_end_matches('/');
  let under = (!home.is_empty())
    .then(|| bare.strip_prefix(&format!("{home}/")))
    .flatten()
    .map(|rest| format!("/{rest}"));

  match under {
    None => vec![bare.to_owned()],
    Some(under) => vec![
      bare.to_owned(),
      format!("~{under}"),
      format!("$HOME{under}"),
      format!("${{HOME}}{under}"),
    ],
  }
}

impl CliRecords {
  pub fn of(config_dirs: &[String], home: &str) -> Self {
    let mut places: Vec<String> = vec![];

    for spelling in config_dirs.iter().flat_map(|dir| spellings(dir, home)) {
      if !spelling.is_empty() && !places.contains(&spelling) {
        places.push(spelling);
      }
    }

    let places: Vec<String> = places.iter().map(|place| regex::escape(place)).collect();
    let named = [r"\.claude/projects".to_owned(), "CLAUDE_CONFIG_DIR".to_owned()]
      .into_iter()
      .chain(places.iter().map(|place| format!("{place}/projects")))
      .collect::<Vec<_>>()
      .join("|");
    let home = [r"\.claude".to_owned()]
      .into_iter()
      .chain(places.iter().map(|place| format!("{place}(?:/|$)")))
      .collect::<Vec<_>>()
      .join("|");

    Self {
      named: Regex::new(&named).expect("a records pattern"),
      home: Regex::new(&home).expect("a configuration pattern"),
    }
  }
}

// Programs that write, move, copy, link or remove files, and the editors that edit a file in place.
pub const WRITER_WORDS: [&str; 13] = [
  "cp", "mv", "ln", "rm", "rsync", "tee", "install", "truncate", "dd", "touch", "mkdir", "rmdir", "unlink",
];
pub const IN_PLACE_EDITORS: [&str; 3] = ["sed", "perl", "ruby"];

// Programs that run text as commands of the shell they stand in.
const SOURCERS: [&str; 2] = ["source", "."];

/// How many levels of text handed on as commands are read — text given to `eval` or to a shell's `-c`, or
/// read as commands where a substitution stands for the program — before the reading stops.
pub const INNER_TEXT_DEPTH: usize = 3;

/// Whether a flag asks for the file itself to be edited: `-i` with its optional backup suffix, or the long
/// spelling of it.
pub fn in_place_flag(arg: &str) -> bool {
  static FLAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-[A-Za-z]*i").expect("an -i pattern"));

  FLAG.is_match(arg) || arg == "--in-place" || arg.starts_with("--in-place=")
}

// A redirect whose target is plainly somewhere else: a descriptor, the null device, or a literal path that
// does not lead into the CLI's directory. Anything spelled with a variable could be anywhere, so it counts
// as a write wherever the records are named in the same command.
fn harmless_target(target: &str, records: &CliRecords) -> bool {
  if target.is_empty() || target.starts_with('&') || target == "/dev/null" || target == "-" {
    return true;
  }

  !target.contains('$')
    && !target.contains('`')
    && !records.named.is_match(target)
    && !records.home.is_match(target)
}

// Whether a simple command runs text it is handed as commands: `source`, a shell reading its standard input
// or a process substitution, or a program word that is itself a variable.
fn reads_text(run: &Run) -> bool {
  if SOURCERS.contains(&run.program.as_str()) || run.program.starts_with('$') {
    return true;
  }

  if !SHELLS.contains(&run.program.as_str()) {
    return false;
  }

  match run.args.iter().find(|arg| !arg.starts_with('-')) {
    None => true,
    Some(script) => run.args.iter().any(|arg| arg == "-s") || script == "/dev/stdin",
  }
}

static FORK: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"(^|\s)--fork-session(\s|=|$)").expect("a fork pattern"));
// An argv spelled out in code with the CLI's name as one of its quoted words — a spawn written into an
// inline script — starts the CLI however it is run. A backquoted name counts only inside a list's
// brackets: standing in prose it is a markdown code span naming the program, not an argument of one.
static SPELLED: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r#""claude"\s*[,\])]|'claude'\s*[,\])]|[\[,]\s*`claude`\s*[,\])]"#)
    .expect("a spelled argv pattern")
});
// Words put into a pane's session through herdr, spelled out in code.
static HERDR_PROMPT: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"herdrCall\s*\(\s*["'`]agent\.prompt["'`]"#).expect("a herdr pattern"));
static HERDR_DRIVE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"agent|prompt|layout").expect("a herdr pattern"));

// The words that start the CLI in print mode, where it answers one prompt and ends with no pane to watch.
const PRINT_FLAGS: [&str; 2] = ["-p", "--print"];

// What the guard reads a command to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Deed {
  /// It forks a session.
  Fork,
  /// It starts the CLI: by its program word, or by an argv spelled out in code.
  Launch,
  /// It starts the CLI by its program word with a print-mode flag.
  PrintLaunch,
  /// It puts words into a pane's session, or drives a pane.
  Pane,
  /// It writes among the CLI's session records.
  RecordWrite,
}

// One deed read in a command, with the reason and the piece of the command it was read in.
struct Found {
  deed: Deed,
  said: String,
}

/// Why this command may not run in a job that has not opened the guard, or nothing. The answer names the
/// reason and the piece of the command it was read in.
pub fn command_refusal(original: &str, records: &CliRecords) -> Option<String> {
  found_at(original, records, &|_| true, 0).map(|found| found.said)
}

/// Where this command starts the CLI, in any form the guard reads one, or nothing: the reason with the
/// piece of the command it was read in.
pub fn claude_launch(original: &str) -> Option<String> {
  let records = CliRecords::of(&[], "");

  found_at(original, &records, &|deed| matches!(deed, Deed::Launch | Deed::PrintLaunch), 0)
    .map(|found| found.said)
}

/// Why this command may not run whatever the job has opened, or nothing: it starts the CLI in print mode,
/// which no agent of this system runs (docs/agents.md, 「No print mode」).
pub fn print_mode_refusal(original: &str) -> Option<String> {
  let records = CliRecords::of(&[], "");
  let found = found_at(original, &records, &|deed| deed == Deed::PrintLaunch, 0)?;

  Some(format!(
    "refused: {}, Claude Code's print mode, which no agent of this system runs (docs/agents.md, 「No print mode」).",
    found.said
  ))
}

// The first deed of those sought read in the command, read on past every other.
fn found_at(
  original: &str,
  records: &CliRecords,
  sought: &dyn Fn(Deed) -> bool,
  depth: usize,
) -> Option<Found> {
  let command = expand_assigned(original);
  let found = |deed: Deed, said: String| sought(deed).then_some(Found { deed, said });
  let within = |found: Found, said: String| Found {
    deed: found.deed,
    said,
  };

  if FORK.is_match(&command)
    && let Some(found) = found(Deed::Fork, "it forks a Claude Code session".into())
  {
    return Some(found);
  }

  if let Some(spelled) = SPELLED.find(&command)
    && let Some(found) = found(
      Deed::Launch,
      format!(
        "it spells out a command line that starts the Claude Code CLI, read in {}",
        shortly(spelled.as_str())
      ),
    )
  {
    return Some(found);
  }

  if HERDR_PROMPT.is_match(&command)
    && let Some(found) =
      found(Deed::Pane, "it puts words into a pane's Claude Code session through herdr".into())
  {
    return Some(found);
  }

  let names_records = records.named.is_match(&command);
  let read = read_command(&command);
  // Where any part of the command runs text as commands, every word of it may be that text, and why it is
  // read that way is what the refusal has to say.
  let mut text_run = (depth < INNER_TEXT_DEPTH && read.substituted_program)
    .then(|| "a command substitution stands where the program does".to_owned());

  for segment in &read.commands {
    let written = segment.redirects.iter().find(|target| !harmless_target(target, records));

    if names_records
      && let Some(written) = written
      && let Some(found) = found(
        Deed::RecordWrite,
        format!("it writes into the Claude Code CLI's session records, read in {}", shortly(written)),
      )
    {
      return Some(found);
    }

    let Some(run) = program_of(&segment.words) else {
      continue;
    };

    if run.program == "claude" {
      let print = run.args.iter().find(|arg| PRINT_FLAGS.contains(&arg.as_str()));
      let started = match print {
        Some(flag) => found(
          Deed::PrintLaunch,
          format!("it starts the Claude Code CLI, read in {} with {}", shortly(&run.word), shortly(flag)),
        ),
        None => {
          found(Deed::Launch, format!("it starts the Claude Code CLI, read in {}", shortly(&run.word)))
        }
      };

      if let Some(found) = started {
        return Some(found);
      }
    }

    if names_records
      && WRITER_WORDS.contains(&run.program.as_str())
      && let Some(found) = found(
        Deed::RecordWrite,
        format!(
          "it copies, moves, links or removes files among the Claude Code CLI's session records, read in {}",
          shortly(&run.word)
        ),
      )
    {
      return Some(found);
    }

    if names_records
      && IN_PLACE_EDITORS.contains(&run.program.as_str())
      && run.args.iter().any(|arg| in_place_flag(arg))
      && let Some(found) = found(
        Deed::RecordWrite,
        format!(
          "it edits the Claude Code CLI's session records in place, read in {}",
          shortly(&run.word)
        ),
      )
    {
      return Some(found);
    }

    if run.program == "herdr"
      && run.args.iter().any(|arg| HERDR_DRIVE.is_match(arg))
      && let Some(found) =
        found(Deed::Pane, format!("it drives a herdr pane, read in {}", shortly(&run.word)))
    {
      return Some(found);
    }

    if run.program == "eval" && depth < INNER_TEXT_DEPTH {
      let inner = run.args.join(" ");

      if let Some(refused) = found_at(&inner, records, sought, depth + 1) {
        let said = format!("{}, inside the text {} given to eval", refused.said, shortly(&inner));

        return Some(within(refused, said));
      }
    }

    if text_run.is_none() && depth < INNER_TEXT_DEPTH && reads_text(&run) {
      text_run = Some(format!("{} runs the text it is handed as commands", shortly(&run.word)));
    }

    // A shell handed a command string reads that string as commands of its own.
    if SHELLS.contains(&run.program.as_str())
      && depth < INNER_TEXT_DEPTH
      && let Some(inner) = reading::shell_text(&run)
      && let Some(refused) = found_at(inner, records, sought, depth + 1)
    {
      let said = format!("{}, inside the text {} given to {}", refused.said, shortly(inner), run.program);

      return Some(within(refused, said));
    }
  }

  let text_run = text_run?;

  for word in read.commands.iter().flat_map(|segment| &segment.words) {
    if word == original {
      continue;
    }

    if let Some(refused) = found_at(word, records, sought, depth + 1) {
      let said = format!(
        "{}, inside {}, which is read as a command because {text_run}",
        refused.said,
        shortly(word)
      );

      return Some(within(refused, said));
    }
  }

  None
}
