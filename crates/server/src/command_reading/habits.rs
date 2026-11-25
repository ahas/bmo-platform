//! The shell habits that cost a job its work, refused before the command runs (docs/live-claude-guard.md,
//! 「The same hook refuses the shell habits that stall a job」).
//!
//! Each of these is a command that runs and then never ends, answers nothing, or never runs at all, in the
//! shell a job is given: every call is a fresh `zsh -c` whose standard input the hook closes, whose command
//! line holds the command's own text, and which refuses a glob that matches nothing. The refusal names what
//! the command would have done and the way to write it. Heredoc bodies are the text they are.

use std::collections::HashSet;

use super::ShellCall;
use super::shell::{Command, closing_in, program_of, shell_script, split, unquoted};

/// The least wait a call that runs a known long command may name. A call that names none waits as long as the
/// worker's settings let the tool wait, which is for ever; past a wait it names, the tool moves the command to
/// the background rather than stopping it.
pub const LONG_COMMAND_WAIT_MS: u64 = 300_000;

const DOC: &str =
  "(docs/live-claude-guard.md, 「The same hook refuses the shell habits that stall a job」)";

// The options a program knows: short ones that take a value, attached or as the next word, and long ones
// by how many words they take when none is attached with `=`.
struct Options {
  short: &'static str,
  long: &'static [(&'static str, usize)],
}

const NONE: Options = Options {
  short: "",
  long: &[],
};

// A command's arguments read against the options its program knows: which options stood, and the operands
// left once every option has taken its values.
struct Read {
  seen: HashSet<String>,
  operands: Vec<String>,
}

impl Read {
  fn any(&self, names: &[&str]) -> bool {
    names.iter().any(|name| self.seen.contains(*name))
  }
}

fn read_args(args: &[String], options: &Options) -> Read {
  let mut read = Read {
    seen: HashSet::new(),
    operands: vec![],
  };
  let words: Vec<String> = args.iter().map(|arg| unquoted(arg)).collect();
  let mut at = 0;

  while at < words.len() {
    let word = &words[at];

    if word == "--" {
      read.operands.extend(words[at + 1..].iter().cloned());
      break;
    }

    if let Some(long) = word.strip_prefix("--") {
      let (name, attached) = match long.split_once('=') {
        Some((name, _)) => (name, true),
        None => (long, false),
      };

      read.seen.insert(format!("--{name}"));

      if !attached {
        at += options.long.iter().find(|(known, _)| *known == name).map_or(0, |(_, takes)| *takes);
      }
    } else if word.len() > 1 && word.starts_with('-') {
      let letters: Vec<char> = word[1..].chars().collect();

      for (letter, ch) in letters.iter().enumerate() {
        read.seen.insert(format!("-{ch}"));

        if options.short.contains(*ch) {
          if letter == letters.len() - 1 {
            at += 1;
          }

          break;
        }
      }
    } else {
      read.operands.push(word.clone());
    }

    at += 1;
  }

  read
}

fn only_stdin(files: &[String]) -> bool {
  files.iter().all(|file| file == "-")
}

const EZA: Options = Options {
  short: "LIstw",
  long: &[
    ("level", 1),
    ("ignore-glob", 1),
    ("sort", 1),
    ("time", 1),
    ("color", 1),
    ("colour", 1),
    ("time-style", 1),
    ("width", 1),
  ],
};

// Python reads a program from its standard input unless it is named a script, `-c` or `-m`, which end its own
// options.
fn python_reads(args: &[String]) -> bool {
  let mut at = 0;

  while at < args.len() {
    let word = unquoted(&args[at]);
    let cluster = word
      .strip_prefix('-')
      .filter(|rest| !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_alphabetic()));

    if cluster.is_some_and(|letters| letters.contains(['c', 'm'])) {
      return false;
    }

    if word == "-W" || word == "-X" {
      at += 1;
    } else if word == "-" {
      return true;
    } else if !word.starts_with('-') {
      return false;
    }

    at += 1;
  }

  true
}

// Whether a program run with these arguments reads its standard input, by the program's own rules.
fn reads_stdin(program: &str, args: &[String]) -> bool {
  match program {
    // `ls` is eza on this machine, and eza handed no path with a standard input that is not a terminal reads
    // the names it should list from there.
    "ls" | "eza" => read_args(args, &EZA).operands.is_empty(),
    "cat" | "wc" => only_stdin(&read_args(args, &NONE).operands),
    "head" => only_stdin(
      &read_args(
        args,
        &Options {
          short: "nc",
          long: &[("lines", 1), ("bytes", 1)],
        },
      )
      .operands,
    ),
    "tail" => only_stdin(
      &read_args(
        args,
        &Options {
          short: "ncs",
          long: &[("lines", 1), ("bytes", 1), ("sleep-interval", 1), ("pid", 1)],
        },
      )
      .operands,
    ),
    "sort" => only_stdin(
      &read_args(
        args,
        &Options {
          short: "ktoST",
          long: &[("key", 1), ("field-separator", 1), ("output", 1), ("buffer-size", 1)],
        },
      )
      .operands,
    ),
    "grep" => {
      let read = read_args(
        args,
        &Options {
          short: "efmABCdD",
          long: &[
            ("regexp", 1),
            ("file", 1),
            ("max-count", 1),
            ("after-context", 1),
            ("before-context", 1),
            ("context", 1),
          ],
        },
      );
      let files = if read.any(&["-e", "-f", "--regexp", "--file"]) {
        &read.operands[..]
      } else {
        read.operands.get(1..).unwrap_or_default()
      };

      !read.any(&["-r", "-R", "--recursive", "--dereference-recursive"]) && only_stdin(files)
    }
    "sed" => {
      let read = read_args(
        args,
        &Options {
          short: "efl",
          long: &[("expression", 1), ("file", 1), ("line-length", 1)],
        },
      );
      let files = if read.any(&["-e", "-f", "--expression", "--file"]) {
        &read.operands[..]
      } else {
        read.operands.get(1..).unwrap_or_default()
      };

      only_stdin(files)
    }
    "jq" => {
      let read = read_args(
        args,
        &Options {
          short: "fL",
          long: &[
            ("arg", 2),
            ("argjson", 2),
            ("slurpfile", 2),
            ("rawfile", 2),
            ("indent", 1),
            ("from-file", 1),
          ],
        },
      );
      let files = if read.any(&["-f", "--from-file"]) {
        &read.operands[..]
      } else {
        read.operands.get(1..).unwrap_or_default()
      };

      !read.any(&["-n", "--null-input"]) && only_stdin(files)
    }
    "python" | "python3" => python_reads(args),
    "tr" => true,
    "xargs" => !read_args(
      args,
      &Options {
        short: "aEdILnPs",
        long: &[],
      },
    )
    .any(&["-a", "--arg-file"]),
    _ => false,
  }
}

fn stdin_refusal(program: &str) -> String {
  let fix = if program == "ls" || program == "eza" {
    "Name the path — ls -la ."
  } else {
    "Name the file it reads, or feed it through a pipe or a redirect."
  };

  format!(
    "refused: {program} here is named nothing to read, so it reads its standard input, and a job's standard input is closed: it would answer nothing, which reads as an answer that found nothing. {fix} {DOC}"
  )
}

// The commands known to run past the tool's one-minute wait: the close and the whole suite, the check of a
// change with its builds and tests, the web console's typecheck and build, a push, the question to the
// owner, and a run of the testbed, which waits its turn before it runs. Among the verbs of the `bmo` entry
// point, by a verb's flat name or by its noun and verb; the job unit's own binary by its noun and verb.
const LONG_VERBS: [&str; 2] = ["check", "cargo"];
const LONG_NOUN_VERBS: [(&str, &str); 4] =
  [("job", "close"), ("job", "ask"), ("test", "suite"), ("test", "full")];
// The testbed's verbs that answer at once: everything else it does waits for the machine's turn.
const TESTBED_QUICK: [&str; 2] = ["status", "classify"];
const CONSOLE_STEPS: [&str; 3] = ["typecheck", "build", "generate"];

fn long_command(program: &str, args: &[String]) -> Option<String> {
  let words: Vec<String> = args.iter().map(|arg| unquoted(arg)).collect();
  let at = words.iter().position(|word| !word.starts_with('-'));
  let first = at.map(|at| words[at].as_str());
  let second = at.and_then(|at| words.get(at + 1)).map_or("", String::as_str);
  let steps = |word: &str| CONSOLE_STEPS.contains(&word);

  let binary = program.rsplit('/').next().unwrap_or(program);

  if binary == "bmo"
    && let Some(first) = first.filter(|first| LONG_VERBS.contains(first))
  {
    return Some(format!("bmo {first}"));
  }

  if (binary == "bmo" || binary == "bmo-job")
    && let Some(first) = first
    && LONG_NOUN_VERBS.contains(&(first, second))
  {
    return Some(format!("{binary} {first} {second}"));
  }

  // A run of the testbed, as the `bmo` entry point's unit command or the testbed's own binary.
  if binary == "bmo" && first == Some("testbed") && !TESTBED_QUICK.contains(&second) {
    return Some("bmo testbed".to_owned());
  }

  if binary == "bmo-testbed" && !first.is_some_and(|first| TESTBED_QUICK.contains(&first)) {
    return Some(binary.to_owned());
  }

  if matches!(program, "bun" | "bunx" | "npx") {
    let first = first.unwrap_or("");

    if program == "bun" && first == "test" {
      let rest = &args[at.map_or(args.len(), |at| at + 1)..];
      let read = read_args(
        rest,
        &Options {
          short: "t",
          long: &[("timeout", 1), ("test-name-pattern", 1), ("preload", 1)],
        },
      );

      if read.operands.is_empty() {
        return Some("the whole test suite".into());
      }
    }

    if program == "bun" && first == "run" && steps(second) {
      return Some(format!("bun run {second}"));
    }

    if (first == "nuxi" || first == "nuxt") && steps(second) {
      return Some(format!("{first} {second}"));
    }

    return (first == "vue-tsc").then(|| "vue-tsc".into());
  }

  if (program == "nuxi" || program == "nuxt")
    && let Some(first) = first.filter(|first| steps(first))
  {
    return Some(format!("{program} {first}"));
  }

  if program == "vue-tsc" {
    return Some("vue-tsc".into());
  }

  if program == "git" {
    let read = read_args(
      args,
      &Options {
        short: "Cc",
        long: &[("git-dir", 1), ("work-tree", 1), ("namespace", 1)],
      },
    );

    if read.operands.first().map(String::as_str) == Some("push") {
      return Some("git push".into());
    }
  }

  None
}

fn long_refusal(what: &str, asked_ms: u64) -> String {
  format!(
    "refused: {what} runs for minutes, and this call names a wait of {asked_ms} ms. Past its wait the tool moves the command to the background and answers at once while it runs on, so what depends on it would start before it ends. Run it again naming no timeout, which waits until it ends, or the largest, 2147483647; or with run_in_background, and wait on the file its output goes to before anything that depends on it. {DOC}"
  )
}

// `pkill -f` and `pgrep -f` match whole command lines, and the pattern stands in the command line of the
// very shell running them. A listing shows those lines and is how to look.
fn full_match_refusal(program: &str, args: &[String]) -> Option<String> {
  if program != "pkill" && program != "pgrep" {
    return None;
  }

  let read = read_args(
    args,
    &Options {
      short: "PgGstuUFn",
      long: &[
        ("signal", 1),
        ("parent", 1),
        ("pgroup", 1),
        ("group", 1),
        ("session", 1),
        ("terminal", 1),
        ("euid", 1),
        ("uid", 1),
        ("pidfile", 1),
        ("ns", 1),
        ("nslist", 1),
      ],
    },
  );

  if !read.any(&["-f", "--full"])
    || (program == "pgrep" && read.any(&["-a", "-l", "--list-full", "--list-name"]))
  {
    return None;
  }

  let harm = if program == "pkill" {
    "so it kills that shell and loses every command after it."
  } else {
    "so it always finds that shell, and a test or a wait built on it never falls."
  };

  Some(format!(
    "refused: {program} -f matches whole command lines, and the pattern stands in the command line of the shell running this very call, {harm} Look first with pgrep -af <pattern> and kill the pids that are not $$; keep the pid you started and ask about it with kill -0 $pid; or stop the thing by its systemd unit. {DOC}"
  ))
}

// Every word after an option that takes a pattern, and every `--option=value` word, is a pattern the command
// expands for itself; left unquoted, zsh expands it first, and refuses the whole command where it matches
// nothing.
const PATTERN_OPTIONS: [&str; 13] = [
  "-name",
  "-iname",
  "-path",
  "-ipath",
  "-wholename",
  "-iwholename",
  "-lname",
  "-ilname",
  "--glob",
  "--iglob",
  "--include",
  "--exclude",
  "--exclude-dir",
];

/// Whether a word holds a glob character outside quotes and expansions.
pub fn unquoted_glob(word: &str) -> bool {
  let chars: Vec<(usize, char)> = word.char_indices().collect();
  let mut at = 0;

  while at < chars.len() {
    let (byte, ch) = chars[at];
    let skip_to = |end: usize| chars.iter().position(|(place, _)| *place >= end).unwrap_or(chars.len());

    match ch {
      '\\' => at += 1,
      '\'' | '"' => at = word[byte + 1..].find(ch).map_or(chars.len(), |end| skip_to(byte + 1 + end)),
      '$' if matches!(chars.get(at + 1), Some((_, '{' | '('))) => {
        let open = chars[at + 1].1;
        let close = if open == '{' { '}' } else { ')' };

        at = skip_to(closing_in(word, chars[at + 1].0, open, close));
      }
      '*' | '?' | '[' => return true,
      _ => {}
    }

    at += 1;
  }

  false
}

fn pattern_refusal(program: &str, args: &[String]) -> Option<String> {
  for (at, word) in args.iter().enumerate() {
    let option = if at > 0 { unquoted(&args[at - 1]) } else { String::new() };
    let valued = word
      .strip_prefix("--")
      .and_then(|rest| rest.split_once('='))
      .filter(|(name, _)| {
        name.starts_with(|ch: char| ch.is_ascii_alphabetic())
          && name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
      })
      .map(|(name, value)| (format!("--{name}="), value.to_owned()));
    let pattern = match &valued {
      Some((_, value)) => Some(value.clone()),
      None if PATTERN_OPTIONS.contains(&option.as_str()) || (program == "rg" && option == "-g") => {
        Some(word.clone())
      }
      None => None,
    };

    if let Some(pattern) = pattern.filter(|pattern| unquoted_glob(pattern)) {
      let fix = match &valued {
        Some((prefix, _)) => format!("Quote it: {prefix}'{}'.", unquoted(&pattern)),
        None => format!("Quote it: '{}'.", unquoted(&pattern)),
      };

      return Some(format!(
        "refused: {word} is a pattern the command expands for itself, left unquoted, so zsh expands it first and, where it matches nothing, refuses the whole command without running it — and a && chain ends there. {fix} {DOC}"
      ));
    }
  }

  None
}

fn wait_refusal() -> String {
  format!(
    "refused: a while or until loop that sleeps is a wait on a condition that may never fall — a file that never grows, a pattern that matches the waiter, a process already gone — and the turn holding it never reaches its ending. Wait on the thing itself, bounded: wait on a pid you started, timeout <secs> <command>, flock -w <secs>, a for loop of a fixed number of turns with a give-up that says so, or put the loop under timeout <secs> zsh -c '…'. {DOC}"
  )
}

fn read_habits(text: &str, call: ShellCall, bounded: bool, depth: usize) -> Option<String> {
  if depth > 3 {
    return None;
  }

  let read = split(text);
  let mut loops: Vec<String> = vec![];
  let mut waits = false;
  let mut sleeps = false;

  for command in &read.commands {
    let Command {
      words, stdin_given, ..
    } = command;
    let head = words.first().map_or("", String::as_str);

    if matches!(head, "while" | "until" | "for" | "select") {
      loops.push(head.to_owned());
      waits |= head == "while" || head == "until";
    }

    // A loop fed from elsewhere hands its input to the commands inside it.
    let fed = *stdin_given || loops.iter().any(|kind| kind == "while" || kind == "until");

    if head == "done" {
      loops.pop();
    }

    let Some(run) = program_of(words) else {
      continue;
    };

    if let Some(long) = long_command(&run.program, &run.args)
      && !call.background
      && let Some(asked_ms) = call.timeout_ms.filter(|asked_ms| *asked_ms < LONG_COMMAND_WAIT_MS)
    {
      return Some(long_refusal(&long, asked_ms));
    }

    if !fed && reads_stdin(&run.program, &run.args) {
      return Some(stdin_refusal(&run.program));
    }

    if let Some(refused) =
      full_match_refusal(&run.program, &run.args).or_else(|| pattern_refusal(&run.program, &run.args))
    {
      return Some(refused);
    }

    sleeps |= run.program == "sleep";

    let script = shell_script(&run).or_else(|| {
      (run.program == "eval")
        .then(|| run.args.iter().map(|arg| unquoted(arg)).collect::<Vec<_>>().join(" "))
    });

    if let Some(within) = script.and_then(|script| {
      read_habits(
        &script,
        call,
        bounded || run.runners.iter().any(|runner| runner == "timeout"),
        depth + 1,
      )
    }) {
      return Some(within);
    }
  }

  if waits && sleeps && !bounded {
    return Some(wait_refusal());
  }

  read.inner.iter().find_map(|inner| read_habits(&inner.text, call, bounded, depth + 1))
}

/// Why this call may not run as written, or nothing.
pub fn habit_refusal(command: &str, call: ShellCall) -> Option<String> {
  read_habits(command, call, false, 0)
}
