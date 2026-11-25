//! A removal a job's shell may not run as written (docs/live-claude-guard.md, 「The same hook refuses a
//! removal the CLI would stop on」).
//!
//! Claude Code stops on a confirmation before an `rm` whose target it cannot resolve, whatever the
//! permission mode, and a job's pane has nobody to answer it. The same removal is a real danger on its own:
//! through an empty variable it reaches the root. So a removal target is refused where it holds a command
//! substitution or a parameter expansion that does not stop on an empty value, and the refusal says how to
//! write it. What a script file hides is out of this reading's sight, and the CLI's own confirmation stands
//! behind it.

use std::sync::LazyLock;

use regex::Regex;

use super::shell::{closing_in, program_of, shell_script, split, unquoted};

const REMOVERS: [&str; 2] = ["rm", "rmdir"];

/// What in one removal target the CLI cannot resolve and an empty value would turn into another path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsafe {
  Substitution,
  Expansion,
}

static GUARDED: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*:?\?").expect("a guarded expansion pattern"));

/// What in one removal target is unsafe, or nothing for a target that is safe as written.
pub fn unsafe_target(target: &str) -> Option<Unsafe> {
  let mut quoted = false;
  let mut chars = target.char_indices().peekable();

  while let Some((at, ch)) = chars.next() {
    let next = chars.peek().map(|(_, next)| *next);

    match ch {
      '\\' => {
        chars.next();
      }
      '"' => quoted = !quoted,
      '\'' if !quoted => {
        for (_, inner) in chars.by_ref() {
          if inner == '\'' {
            break;
          }
        }
      }
      '`' => return Some(Unsafe::Substitution),
      '$' if next == Some('(') => return Some(Unsafe::Substitution),
      '$' if next == Some('{') => {
        let close = closing_in(target, at + 1, '{', '}');
        let body = target.get(at + 2..close).unwrap_or_default();

        if !GUARDED.is_match(body) {
          return Some(Unsafe::Expansion);
        }

        while chars.peek().is_some_and(|(place, _)| *place <= close) {
          chars.next();
        }
      }
      // `$$` is never empty, and `$'…'` and `$"…"` are quoting rather than a value.
      '$' if next == Some('$') => {
        chars.next();
      }
      '$' if next.is_some_and(|next| next.is_ascii_alphanumeric() || "_@*#?!-".contains(next)) => {
        return Some(Unsafe::Expansion);
      }
      _ => {}
    }
  }

  None
}

// The targets of a removal: every word after the program that is not an option.
fn targets_of(args: &[String]) -> Vec<&String> {
  let mut options = true;
  let mut targets = vec![];

  for arg in args {
    if options && arg == "--" {
      options = false;
    } else if !options || !arg.starts_with('-') {
      targets.push(arg);
    }
  }

  targets
}

const GUIDANCE: &str = "Write the removal so nothing is left to resolve: name the path outright where you can; where a variable is needed, write it so it stops when empty and quote it — rm -f \"${S:?}\"/*.md; never remove through a command substitution (put its result in a variable first and guard that); write a script that removes things with the Write tool rather than a heredoc; and leave files under ~/.claude alone.";

fn refusal_for(target: &str, unsafe_part: Unsafe) -> String {
  let what = match unsafe_part {
    Unsafe::Substitution => format!("the removal target {target} holds a command substitution"),
    Unsafe::Expansion => {
      format!("the removal target {target} holds a variable that is not guarded against being empty")
    }
  };

  format!(
    "refused: {what}. Claude Code stops on a confirmation before such a removal even in bypass mode, and a job's pane has nobody to answer it; an empty variable would also turn the target into a path at the root. {GUIDANCE} (docs/live-claude-guard.md, 「The same hook refuses a removal the CLI would stop on」)"
  )
}

static NAMES_REMOVAL: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"\brm(dir)?\b").expect("a removal pattern"));

/// Why this command may not run as written, or nothing.
pub fn removal_refusal(command: &str) -> Option<String> {
  refusal_at(command, 0)
}

fn refusal_at(command: &str, depth: usize) -> Option<String> {
  if depth > 3 || !NAMES_REMOVAL.is_match(command) {
    return None;
  }

  let read = split(command);

  for command in &read.commands {
    let Some(run) = program_of(&command.words) else {
      continue;
    };

    if REMOVERS.contains(&run.program.as_str())
      && let Some(refused) = targets_of(&run.args)
        .into_iter()
        .find_map(|target| unsafe_target(target).map(|unsafe_part| refusal_for(target, unsafe_part)))
    {
      return Some(refused);
    }

    if let Some(refused) = shell_script(&run).and_then(|script| refusal_at(&script, depth + 1)) {
      return Some(refused);
    }

    if run.program == "eval"
      && let Some(refused) =
        refusal_at(&run.args.iter().map(|arg| unquoted(arg)).collect::<Vec<_>>().join(" "), depth + 1)
    {
      return Some(refused);
    }
  }

  // A heredoc's body is read too: the CLI stops on a removal it finds there as on one standing in the line.
  read
    .inner
    .iter()
    .map(|inner| inner.text.as_str())
    .chain(read.heredocs.iter().map(|held| held.text.as_str()))
    .find_map(|text| refusal_at(text, depth + 1))
}
