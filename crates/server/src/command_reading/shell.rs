//! A command's text read the way the shell splits it, for the guards in front of a job's shell
//! (docs/live-claude-guard.md). Each simple command comes out as its words with their quoting kept as
//! written, the words its redirects name, and whether something other than the job's own standard
//! input feeds it; the text of every command and process substitution comes out beside them, since the
//! shell runs it as commands of its own, and every heredoc's body beside its opening line, since a body
//! is text a command carries rather than commands. What a script file hides is out of its sight.
//!
//! The reading is brush-parser's tokenizer and word parser, which read the bash grammar a job's zsh line
//! is written in almost always. A line they cannot read — a quote left open, a construct of zsh's alone —
//! reads as no command at all: the shell refuses the first, and the second is out of this reading's
//! sight as a script file is.

use brush_parser::word::{self, WordPiece, WordPieceWithSource};
use brush_parser::{ParserOptions, Token};

/// The shells a command line can hand text to run.
pub const SHELLS: [&str; 4] = ["sh", "bash", "zsh", "dash"];

/// One simple command as the shell splits it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Command {
  /// Its words as written, quoting kept.
  pub words: Vec<String>,
  /// The files its output redirects name, as written.
  pub redirects: Vec<String>,
  /// Those of them a redirect appends to rather than truncates.
  pub appended: Vec<String>,
  /// What its input redirects name: a file it reads, a heredoc's delimiter, a here-string. None of them
  /// is an argument of the program.
  pub inputs: Vec<String>,
  /// Its standard input comes from a pipe, a redirect, a heredoc or a here-string rather than from the
  /// input the shell was started with.
  pub stdin_given: bool,
}

/// The text a command substitution or a process substitution runs, and whether it stood inside double
/// quotes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Substitution {
  pub text: String,
  pub quoted: bool,
}

/// A heredoc: the line it was opened on, which says which program it feeds, and the text it carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heredoc {
  pub opener: String,
  pub text: String,
}

/// A command line read: its simple commands in order, the substitutions standing in their words, and the
/// heredocs it carries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Split {
  pub commands: Vec<Command>,
  pub inner: Vec<Substitution>,
  pub heredocs: Vec<Heredoc>,
}

// What the next word is, after a redirect operator.
enum Pending {
  Output { append: bool },
  Input,
  // A descriptor a `>&` or `<&` duplicates, which names no file.
  Duplicate,
}

const OUTPUTS: [&str; 6] = [">", ">>", ">|", "&>", "&>>", ">&"];
const INPUTS: [&str; 3] = ["<", "<&", "<>"];

// Byte offsets of a text's characters, so a token's place, which brush counts in characters, slices it.
fn offsets(text: &str) -> Vec<usize> {
  text.char_indices().map(|(at, _)| at).chain([text.len()]).collect()
}

fn span_of(token: &Token) -> (usize, usize) {
  match token {
    Token::Word(_, span) | Token::Operator(_, span) => (span.start.index, span.end.index),
  }
}

fn is_operator(token: Option<&Token>, wanted: &str) -> bool {
  matches!(token, Some(Token::Operator(op, _)) if op == wanted)
}

// The whole line a character of the text stands on.
fn line_at(text: &str, at: usize) -> String {
  let start = text[..at].rfind('\n').map_or(0, |found| found + 1);
  let end = text[at..].find('\n').map_or(text.len(), |found| at + found);

  text[start..end].to_owned()
}

// One simple command ended: kept where it holds anything, and whether a pipe feeds the next.
fn end_command(current: &mut Command, read: &mut Split, pipes_on: bool, piped: &mut bool) {
  let mut done = std::mem::take(current);

  done.stdin_given |= *piped;

  if !done.words.is_empty() || !done.redirects.is_empty() || !done.inputs.is_empty() {
    read.commands.push(done);
  }

  *piped = pipes_on;
}

/// The command line read, or nothing read where the tokenizer cannot read it.
pub fn split(text: &str) -> Split {
  let Ok(tokens) = brush_parser::tokenize_str(text) else {
    return Split::default();
  };
  let places = offsets(text);
  let byte = |at: usize| places.get(at).copied().unwrap_or(text.len());
  let mut read = Split::default();
  let mut current = Command::default();
  let mut pending: Option<Pending> = None;
  let mut descriptor: Option<String> = None;
  let mut piped = false;
  let mut at = 0;

  while at < tokens.len() {
    match &tokens[at] {
      Token::Word(word, _) => {
        let next = tokens.get(at + 1);
        let adjacent = next.is_some_and(|next| span_of(next).0 == span_of(&tokens[at]).1);
        let redirect_follows = matches!(next, Some(Token::Operator(op, _))
          if OUTPUTS.contains(&op.as_str()) || INPUTS.contains(&op.as_str()) || op.starts_with("<<"));

        if pending.is_none() && adjacent && redirect_follows && word.chars().all(|ch| ch.is_ascii_digit()) {
          descriptor = Some(word.clone());
        } else {
          match pending.take() {
            Some(Pending::Output { append }) => {
              current.redirects.push(word.clone());

              if append {
                current.appended.push(word.clone());
              }
            }
            Some(Pending::Input) => current.inputs.push(word.clone()),
            Some(Pending::Duplicate) => {}
            None => {
              read.inner.extend(substitutions(word));
              current.words.push(word.clone());
            }
          }
        }

        at += 1;
      }
      Token::Operator(op, span) => {
        let op = op.as_str();
        let process = (op == "<" || op == ">")
          && is_operator(tokens.get(at + 1), "(")
          && tokens.get(at + 1).is_some_and(|open| span_of(open).0 == span.end.index);

        if process {
          let close = closing(&tokens, at + 1);
          let inside_from = byte(span_of(&tokens[at + 1]).1);
          let (inside_to, word_to) = tokens
            .get(close)
            .map_or((text.len(), text.len()), |end| (byte(span_of(end).0), byte(span_of(end).1)));

          read.inner.push(Substitution {
            text: text[inside_from..inside_to].to_owned(),
            quoted: false,
          });
          current.words.push(text[byte(span.start.index)..word_to].to_owned());
          descriptor = None;
          at = close + 1;
          continue;
        }

        if op == "<<" || op == "<<-" {
          let tag = match tokens.get(at + 1) {
            Some(Token::Word(tag, _)) => tag.clone(),
            _ => String::new(),
          };
          let plain = brush_parser::unquote_str(&tag);
          let mut next = at + 2;
          let mut body = String::new();

          if let Some(Token::Word(text, _)) = tokens.get(next)
            && *text != plain
          {
            body = text.strip_suffix('\n').unwrap_or(text).to_owned();
            next += 1;
          }

          if matches!(tokens.get(next), Some(Token::Word(end, _)) if *end == plain) {
            next += 1;
          }

          current.inputs.push(tag);
          current.stdin_given = true;
          read.heredocs.push(Heredoc {
            opener: line_at(text, byte(span.start.index)),
            text: body,
          });
          descriptor = None;
          at = next;
          continue;
        }

        if op == "<<<" {
          current.stdin_given = true;
          pending = Some(Pending::Input);
        } else if INPUTS.contains(&op) {
          if descriptor.as_deref().is_none_or(|fd| fd == "0") {
            current.stdin_given = true;
          }

          pending = Some(if op == "<&" { Pending::Duplicate } else { Pending::Input });
        } else if OUTPUTS.contains(&op) {
          let duplicates = op == ">&"
            && matches!(tokens.get(at + 1), Some(Token::Word(fd, _)) if fd == "-" || fd.chars().all(|ch| ch.is_ascii_digit()));

          pending = Some(if duplicates {
            Pending::Duplicate
          } else {
            Pending::Output {
              append: op.contains(">>"),
            }
          });
        } else {
          end_command(&mut current, &mut read, op == "|" || op == "|&", &mut piped);
          pending = None;
        }

        descriptor = None;
        at += 1;
      }
    }
  }

  end_command(&mut current, &mut read, false, &mut piped);

  read
}

// Where each heredoc's body and closing delimiter stand among the tokens: the token indices of the body
// (where there is one) and of the delimiter, for every `<<` the line holds.
fn heredoc_tokens(tokens: &[Token]) -> Vec<usize> {
  let mut held = vec![];

  for (at, token) in tokens.iter().enumerate() {
    if !matches!(token, Token::Operator(op, _) if op == "<<" || op == "<<-") {
      continue;
    }

    let Some(Token::Word(tag, _)) = tokens.get(at + 1) else {
      continue;
    };
    let plain = brush_parser::unquote_str(tag);
    let mut next = at + 2;

    if matches!(tokens.get(next), Some(Token::Word(text, _)) if *text != plain) {
      held.push(next);
      next += 1;
    }

    if matches!(tokens.get(next), Some(Token::Word(end, _)) if *end == plain) {
      held.push(next);
    }
  }

  held
}

/// The text with every heredoc's body and closing delimiter taken out, the line that opened it kept: what
/// the shell runs as commands, apart from the text a command carries. Text the tokenizer cannot read stands
/// as it is.
pub fn without_heredoc_bodies(text: &str) -> String {
  let Ok(tokens) = brush_parser::tokenize_str(text) else {
    return text.to_owned();
  };
  let places = offsets(text);
  let byte = |at: usize| places.get(at).copied().unwrap_or(text.len());
  let mut kept = String::with_capacity(text.len());
  let mut from = 0;

  for at in heredoc_tokens(&tokens) {
    let (start, end) = span_of(&tokens[at]);

    kept.push_str(&text[from.min(byte(start))..byte(start)]);
    from = byte(end);
  }

  kept.push_str(&text[from.min(text.len())..]);
  kept
}

/// The command as the shell reads its structure: its tokens one space apart, heredoc bodies out and every
/// quoted piece of a word blanked to `''`, so what a call hands a program as text — a replacement written
/// in prose, a report — is neither a loop nor a flag. Text the tokenizer cannot read stands as it is.
pub fn bare(text: &str) -> String {
  let Ok(tokens) = brush_parser::tokenize_str(text) else {
    return text.to_owned();
  };
  let skipped = heredoc_tokens(&tokens);
  let mut kept = vec![];

  for (at, token) in tokens.iter().enumerate() {
    if skipped.contains(&at) {
      continue;
    }

    match token {
      Token::Operator(op, _) => kept.push(op.clone()),
      Token::Word(word, _) => {
        let read = pieces(word);

        if read.is_empty() {
          kept.push(word.clone());
          continue;
        }

        kept.push(
          read
            .iter()
            .map(|piece| match piece.piece {
              WordPiece::SingleQuotedText(_)
              | WordPiece::AnsiCQuotedText(_)
              | WordPiece::DoubleQuotedSequence(_)
              | WordPiece::GettextDoubleQuotedSequence(_) => "''",
              _ => word.get(piece.start_index..piece.end_index).unwrap_or_default(),
            })
            .collect(),
        );
      }
    }
  }

  kept.join(" ")
}

// The token closing the bracket opened at `open`, counted with nesting, or the end where it never closes.
fn closing(tokens: &[Token], open: usize) -> usize {
  let mut depth = 0;

  for (at, token) in tokens.iter().enumerate().skip(open) {
    match token {
      Token::Operator(op, _) if op == "(" => depth += 1,
      Token::Operator(op, _) if op == ")" => {
        depth -= 1;

        if depth == 0 {
          return at;
        }
      }
      _ => {}
    }
  }

  tokens.len()
}

fn pieces(word: &str) -> Vec<WordPieceWithSource> {
  word::parse(word, &ParserOptions::default()).unwrap_or_default()
}

/// The command substitutions standing in a word, in order, each with whether it stood in double quotes.
pub fn substitutions(word: &str) -> Vec<Substitution> {
  fn walk(pieces: &[WordPieceWithSource], quoted: bool, found: &mut Vec<Substitution>) {
    for piece in pieces {
      match &piece.piece {
        WordPiece::CommandSubstitution(text) | WordPiece::BackquotedCommandSubstitution(text) => {
          found.push(Substitution {
            text: text.clone(),
            quoted,
          });
        }
        WordPiece::DoubleQuotedSequence(inner) | WordPiece::GettextDoubleQuotedSequence(inner) => {
          walk(inner, true, found);
        }
        _ => {}
      }
    }
  }

  let mut found = vec![];

  walk(&pieces(word), false, &mut found);
  found
}

/// A word with its quoting taken off and its escapes put in, every expansion and substitution left as
/// written.
pub fn value(word: &str) -> String {
  let read = pieces(word);

  // A word the word parser cannot read stands as written.
  if read.is_empty() {
    return word.to_owned();
  }

  let mut text = String::new();

  for piece in read {
    let source = word.get(piece.start_index..piece.end_index).unwrap_or_default();

    match &piece.piece {
      WordPiece::Text(plain) | WordPiece::SingleQuotedText(plain) | WordPiece::AnsiCQuotedText(plain) => {
        text.push_str(plain);
      }
      WordPiece::DoubleQuotedSequence(_) | WordPiece::GettextDoubleQuotedSequence(_) => {
        let inner = source.strip_prefix('$').unwrap_or(source);

        text.push_str(inner.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')).unwrap_or(inner));
      }
      WordPiece::EscapeSequence(escaped) => text.push_str(escaped.strip_prefix('\\').unwrap_or(escaped)),
      _ => text.push_str(source),
    }
  }

  text
}

/// A word with its outer quoting taken off, for text a shell is handed to run.
pub fn unquoted(word: &str) -> String {
  if word.len() >= 2 && word.starts_with('\'') && word.ends_with('\'') {
    return word[1..word.len() - 1].to_owned();
  }

  if word.len() >= 2 && word.starts_with('"') && word.ends_with('"') {
    let inner = &word[1..word.len() - 1];
    let mut text = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();

    while let Some(ch) = chars.next() {
      match (ch, chars.peek()) {
        ('\\', Some(&next)) if matches!(next, '"' | '$' | '`' | '\\') => {
          text.push(next);
          chars.next();
        }
        _ => text.push(ch),
      }
    }

    return text;
  }

  word.to_owned()
}

/// The last part of a word's path, its quoting taken off.
pub fn basename(word: &str) -> String {
  let bare = unquoted(word);

  bare.rsplit('/').next().unwrap_or(&bare).to_owned()
}

/// A program a simple command runs, with the words after it as written and the runners looked past.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
  pub program: String,
  pub args: Vec<String>,
  pub runners: Vec<String>,
}

// What may stand in front of a program without being it: runners, and the words of the shell's own
// grammar that open a command.
const RUNNERS: [&str; 14] = [
  "env", "command", "exec", "nohup", "time", "sudo", "doas", "nice", "ionice", "stdbuf", "setsid",
  "timeout", "builtin", "busybox",
];
const GRAMMAR: [&str; 11] = ["if", "then", "else", "elif", "do", "while", "until", "!", "{", "}", "time"];

/// An assignment standing before a program, `NAME=value` or `NAME+=value`.
pub fn is_assignment(word: &str) -> bool {
  let name_end = word.find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')).unwrap_or(word.len());
  let rest = &word[name_end..];

  name_end > 0
    && !word.starts_with(|ch: char| ch.is_ascii_digit())
    && (rest.starts_with('=') || rest.starts_with("+="))
}

/// The program a simple command runs and the words after it, looking past assignments, runners and the
/// grammar that opens a command, with the runners it looked past.
pub fn program_of(words: &[String]) -> Option<Run> {
  let mut passed = vec![];
  let mut at = 0;

  while at < words.len() {
    let word = &words[at];

    if is_assignment(word) || GRAMMAR.contains(&word.as_str()) {
      at += 1;
      continue;
    }

    let name = basename(word);

    if RUNNERS.contains(&name.as_str()) {
      at += 1;

      while at < words.len() && words[at].starts_with('-') {
        at += 1;
      }

      // The duration `timeout` takes before its program.
      if name == "timeout"
        && words.get(at).is_some_and(|word| word.starts_with(|ch: char| ch.is_ascii_digit()))
      {
        at += 1;
      }

      passed.push(name);
      continue;
    }

    return Some(Run {
      program: name,
      args: words[at + 1..].to_vec(),
      runners: passed,
    });
  }

  None
}

/// Whether a word is a shell's `-c` flag, alone or in a cluster.
pub fn is_command_flag(word: &str) -> bool {
  word.len() > 1
    && word.starts_with('-')
    && word[1..].chars().all(|ch| ch.is_ascii_alphabetic())
    && word[1..].contains('c')
}

/// The script a shell is handed with `-c`, or nothing.
pub fn shell_script(run: &Run) -> Option<String> {
  if !SHELLS.contains(&run.program.as_str()) {
    return None;
  }

  let at = run.args.iter().position(|arg| is_command_flag(arg))?;

  run.args.get(at + 1).map(|script| unquoted(script))
}

/// The end of a construct opened at `from` by `open`, counted with nesting and single quotes, or the end of
/// the text where it never closes — for a word's own `${…}`, which no tokenizer splits further.
pub fn closing_in(text: &str, from: usize, open: char, close: char) -> usize {
  let mut depth = 0;
  let mut chars = text.char_indices().skip_while(|(at, _)| *at < from);

  while let Some((at, ch)) = chars.next() {
    if ch == '\\' {
      chars.next();
    } else if ch == '\'' && open != '{' {
      for (_, inner) in chars.by_ref() {
        if inner == '\'' {
          break;
        }
      }
    } else if ch == open {
      depth += 1;
    } else if ch == close {
      depth -= 1;

      if depth == 0 {
        return at;
      }
    }
  }

  text.len()
}
