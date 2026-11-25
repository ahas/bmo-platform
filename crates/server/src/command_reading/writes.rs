//! What a job's command or file tool writes into a place the job guard keeps (docs/live-claude-guard.md,
//! 「The same hook keeps a job to its own checkout」, 「The same hook keeps STRUCTURE.md the owner's」): the
//! owner's statement document, another worker's checkout, and the files an umbrella job's own folder tracks. It
//! is a reading of text: a redirect, a program that writes files, an in-place editor, git changing a tree, and a
//! script handed to an interpreter on the command line or by a heredoc, a relative path read from where the shell
//! stands. A command that only reads — `cat`, `grep`, `git diff` — writes nothing.

use std::sync::LazyLock;

use regex::Regex;

use super::reading::{self, expand_assigned, program_of, read_command};
use super::records::{IN_PLACE_EDITORS, INNER_TEXT_DEPTH, WRITER_WORDS, in_place_flag, spellings};
use super::shell::SHELLS;

/// The document only the owner's own request changes, however a path to it is spelled.
pub const STRUCTURE_DOCUMENT: &str = "STRUCTURE.md";

// Programs that write only where their last word or their target-directory flag points, reading every other
// file they are named: a copy out of a place is no write into it.
const COPIERS: [&str; 4] = ["cp", "rsync", "install", "ln"];

// Programs that run a script handed to them inline.
const INTERPRETERS: [&str; 7] = ["python", "python3", "node", "bun", "perl", "ruby", "deno"];

/// A place a command may write into, as a reading of its text: what it is called in a refusal, what names
/// it, the git subcommands that count as changing it, and whether a `cd` into it makes every relative path
/// it. Where the reading knows where the shell stands, a relative word is read from there: whether it lands in
/// the place, and whether git run there changes it.
pub trait WriteTarget {
  fn label(&self) -> String;
  fn names(&self, text: &str) -> bool;
  fn git_writers(&self) -> &'static [&'static str];
  fn follows_cd(&self) -> bool;

  /// Whether a relative word, read from the directory `at`, names the place.
  fn names_from(&self, _word: &str, _at: &str) -> bool {
    false
  }

  /// Whether git run in the directory `at` changes the place.
  fn stands_in(&self, _at: &str) -> bool {
    false
  }
}

/// The statement document.
pub struct Structure;

impl WriteTarget for Structure {
  fn label(&self) -> String {
    STRUCTURE_DOCUMENT.into()
  }

  fn names(&self, text: &str) -> bool {
    text.contains(STRUCTURE_DOCUMENT)
  }

  fn git_writers(&self) -> &'static [&'static str] {
    &["rm", "mv", "checkout", "restore", "apply"]
  }

  fn follows_cd(&self) -> bool {
    false
  }
}

/// Another worker's checkout: its worker, the repository it is a checkout of, and its path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignCheckout {
  pub worker: String,
  pub repository: String,
  pub cwd: String,
}

/// An umbrella's own folder, whose tracked files no job writes (docs/worker-slots.md, 「An umbrella repository」):
/// the umbrella repository, its path, and the files its git tracks, relative to the path, `STRUCTURE.md` left to
/// its own rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedTree {
  pub repository: String,
  pub cwd: String,
  pub tracked: Vec<String>,
}

/// The checkouts a job's writes are judged against: the paths it writes in — its own checkout and those it
/// borrowed — every other worker's checkout on its machine, and every umbrella's own folder there, its own among
/// them where it is an umbrella's job. A path is the deepest checkout's that holds it, since one checkout may
/// stand inside another's directory; an umbrella's folder guards only the files it tracks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Checkouts {
  pub own: Vec<String>,
  pub foreign: Vec<ForeignCheckout>,
  pub sealed: Vec<SealedTree>,
}

impl Checkouts {
  fn paths(&self) -> impl Iterator<Item = &str> {
    self
      .own
      .iter()
      .map(String::as_str)
      .chain(self.foreign.iter().map(|checkout| checkout.cwd.as_str()))
  }
}

// A path's boundary: what may follow a directory's name where the text names that directory itself.
const BOUNDARY: &str = r#"(?:[/\s"';&|)]|$)"#;

fn trimmed(path: &str) -> &str {
  path.trim_end_matches('/')
}

// Whether a path lies inside a directory or is the directory itself.
fn within(path: &str, root: &str) -> bool {
  let root = trimmed(root);

  path == root || path.starts_with(&format!("{root}/"))
}

// A word read from the directory `at`: joined to it where it is relative, then its `.` and `..` parts walked,
// so the path it names is compared as it stands.
fn resolved(at: &str, word: &str) -> String {
  let joined = if word.starts_with('/') { word.to_owned() } else { format!("{}/{word}", trimmed(at)) };
  let mut parts: Vec<&str> = vec![];

  for part in joined.split('/') {
    match part {
      "" | "." => {}
      ".." => {
        parts.pop();
      }
      part => parts.push(part),
    }
  }

  format!("/{}", parts.join("/"))
}

// The deepest of these directories that holds the path, where any does.
fn deepest<'a>(roots: impl Iterator<Item = &'a str>, path: &str) -> Option<&'a str> {
  roots.filter(|root| within(path, root)).max_by_key(|root| trimmed(root).len())
}

// Another worker's checkout as a command may name it: its path as it stands and under the home, each matched
// at a boundary, so a support worktree whose path begins with its base's is never read as the base; and a
// path naming a checkout that stands inside this one is that checkout's, not this one's.
struct Foreign<'a> {
  checkout: &'a ForeignCheckout,
  patterns: Vec<Regex>,
  nested: Vec<Regex>,
  inner: Vec<String>,
}

impl<'a> Foreign<'a> {
  fn of(checkout: &'a ForeignCheckout, checkouts: &Checkouts, home: &str) -> Self {
    let at_boundary = |path: &str, anchor: &str| {
      spellings(path, home)
        .iter()
        .filter_map(|spelling| Regex::new(&format!("{anchor}{}{BOUNDARY}", regex::escape(spelling))).ok())
        .collect::<Vec<_>>()
    };
    let nested = checkouts
      .paths()
      .filter(|path| trimmed(path) != trimmed(&checkout.cwd) && within(trimmed(path), &checkout.cwd))
      .flat_map(|path| at_boundary(path, "^"))
      .collect();

    Self {
      checkout,
      patterns: at_boundary(&checkout.cwd, ""),
      nested,
      inner: inner_paths(checkouts, &checkout.cwd),
    }
  }
}

impl WriteTarget for Foreign<'_> {
  fn label(&self) -> String {
    format!("{} ({}'s checkout)", self.checkout.cwd, self.checkout.worker)
  }

  fn names(&self, text: &str) -> bool {
    self.patterns.iter().any(|pattern| {
      pattern
        .find_iter(text)
        .any(|found| !self.nested.iter().any(|inner| inner.is_match(&text[found.start()..])))
    })
  }

  fn git_writers(&self) -> &'static [&'static str] {
    GIT_TREE_WRITERS
  }

  fn follows_cd(&self) -> bool {
    true
  }

  fn names_from(&self, word: &str, at: &str) -> bool {
    self.holds(&resolved(at, word))
  }

  fn stands_in(&self, at: &str) -> bool {
    self.holds(at)
  }
}

impl Foreign<'_> {
  // Whether a path is this checkout's: inside it, and inside no checkout standing deeper within it.
  fn holds(&self, path: &str) -> bool {
    within(path, &self.checkout.cwd) && !self.inner.iter().any(|inner| within(path, inner))
  }
}

// An umbrella's own folder as a command may name it: each file it tracks, by its whole path under any spelling of
// the home, and the folder itself; and, from where the shell stands, a relative word landing on a tracked file in
// no checkout below the folder, and git run in a directory of the folder's own.
struct Sealed<'a> {
  tree: &'a SealedTree,
  patterns: Vec<Regex>,
  inner: Vec<String>,
}

impl<'a> Sealed<'a> {
  fn of(tree: &'a SealedTree, checkouts: &Checkouts, home: &str) -> Self {
    let root = trimmed(&tree.cwd);
    let spelled = |path: &str, after: &str| {
      spellings(path, home)
        .iter()
        .filter_map(|spelling| Regex::new(&format!("{}{after}", regex::escape(spelling))).ok())
        .collect::<Vec<_>>()
    };
    let mut patterns = spelled(root, r#"(?:[\s"';&|)]|$)"#);

    for file in &tree.tracked {
      patterns.extend(spelled(&format!("{root}/{file}"), BOUNDARY));
    }

    Self {
      tree,
      patterns,
      inner: inner_paths(checkouts, root),
    }
  }

  // Whether git standing at this path works on the umbrella's own repository.
  fn outside_inner(&self, path: &str) -> bool {
    within(path, &self.tree.cwd) && !self.inner.iter().any(|inner| within(path, inner))
  }
}

impl WriteTarget for Sealed<'_> {
  fn label(&self) -> String {
    format!("{} (the umbrella {}'s own folder)", self.tree.cwd, self.tree.repository)
  }

  fn names(&self, text: &str) -> bool {
    self.patterns.iter().any(|pattern| pattern.is_match(text))
  }

  fn git_writers(&self) -> &'static [&'static str] {
    GIT_TREE_WRITERS
  }

  fn follows_cd(&self) -> bool {
    false
  }

  fn names_from(&self, word: &str, at: &str) -> bool {
    let path = resolved(at, word);
    let root = trimmed(&self.tree.cwd);

    self.outside_inner(&path)
      && path
        .strip_prefix(&format!("{root}/"))
        .is_some_and(|file| self.tree.tracked.iter().any(|tracked| tracked == file))
  }

  // Git run here works on the umbrella's own repository where the directory is its folder or one that holds files
  // it tracks: any other directory below the folder is ignored by it, and a repository standing there is that
  // repository's own.
  fn stands_in(&self, at: &str) -> bool {
    let root = trimmed(&self.tree.cwd);
    let at = trimmed(at);

    self.outside_inner(at)
      && (at == root
        || at.strip_prefix(&format!("{root}/")).is_some_and(|folder| {
          self.tree.tracked.iter().any(|file| file.starts_with(&format!("{folder}/")))
        }))
  }
}

// Every path of a checkout standing inside this directory, the directory itself left out.
fn inner_paths(checkouts: &Checkouts, root: &str) -> Vec<String> {
  checkouts
    .paths()
    .filter(|path| trimmed(path) != trimmed(root) && within(trimmed(path), root))
    .map(|path| trimmed(path).to_owned())
    .collect()
}

// The git subcommands that change a working tree.
const GIT_TREE_WRITERS: &[&str] = &[
  "add",
  "commit",
  "rm",
  "mv",
  "checkout",
  "switch",
  "restore",
  "reset",
  "apply",
  "stash",
  "merge",
  "rebase",
  "revert",
  "cherry-pick",
  "pull",
];

// The letters and the words an interpreter carries a script under: python's `-c`, and the evaluate and
// print flags of node, bun, deno, perl and ruby.
const SCRIPT_LETTERS: [char; 3] = ['c', 'e', 'p'];
const SCRIPT_WORDS: [&str; 2] = ["--eval", "--print"];

// Every script an interpreter's command line hands it inline. Perl and Ruby take their flags combined —
// `-ne`, `-pe`, `-nlae` — so a cluster is read letter by letter: a letter that carries a script takes the
// rest of its cluster as that script, or the next word where the cluster ends at it, and the reading goes on
// through the cluster either way. In an in-place editor's cluster an `i` takes everything behind it as the
// backup suffix, so the reading of that cluster ends there.
fn inline_scripts(program: &str, args: &[String]) -> Vec<String> {
  let mut scripts = vec![];
  let suffix_after_i = IN_PLACE_EDITORS.contains(&program);

  for (at, arg) in args.iter().enumerate() {
    if let Some(long) = arg.strip_prefix("--") {
      let (name, given) = match long.split_once('=') {
        Some((name, given)) => (name, Some(given)),
        None => (long, None),
      };

      if SCRIPT_WORDS.contains(&format!("--{name}").as_str()) {
        scripts.push(given.map_or_else(|| args.get(at + 1).cloned().unwrap_or_default(), str::to_owned));
      }

      continue;
    }

    let Some(cluster) = arg.strip_prefix('-') else {
      continue;
    };

    for (letter, ch) in cluster.char_indices() {
      if ch == 'i' && suffix_after_i {
        break;
      }

      if SCRIPT_LETTERS.contains(&ch) {
        let rest = &cluster[letter + ch.len_utf8()..];

        scripts.push(if rest.is_empty() {
          args.get(at + 1).cloned().unwrap_or_default()
        } else {
          rest.to_owned()
        });
      }
    }
  }

  scripts
}

// Whether an interpreter reads its script from where a heredoc or a pipe puts it. A script file standing
// among the arguments is a file of its own, out of this reading's sight; `-` standing there is the
// interpreter's own name for its standard input.
fn reads_script_from_stdin(args: &[String]) -> bool {
  args
    .iter()
    .find(|arg| *arg == "-" || !arg.starts_with('-'))
    .is_none_or(|first| first == "-")
}

// A script's write calls are found by these patterns rather than by parsing the script: the script may be
// Python, JavaScript, Perl or Ruby, no maintained library reads all of them, and the reading asks of it only
// which files its write calls name.
macro_rules! pattern {
  ($name:ident, $text:expr) => {
    static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($text).expect("a write-call pattern"));
  };
}

pattern!(PY_OPEN, r#"\bopen\s*\(\s*([^,()]+?)\s*,\s*(?:mode\s*=\s*)?['"]([rwabxt+]+)['"]"#);
pattern!(PY_WRITING_MODE, r"[wax+]");
pattern!(
  PERL_OPEN_TWO,
  r#"\bopen\s*\(\s*(?:my\s+)?[\w$]+\s*,\s*['"]\s*(?:>>?|\+[<>])\s*([^'"\s][^'"]*)['"]\s*\)"#
);
pattern!(
  PERL_OPEN_THREE,
  r#"\bopen\s*\(\s*(?:my\s+)?[\w$]+\s*,\s*['"](?:>>?|\+[<>])['"]\s*,\s*([^,()]+)"#
);
pattern!(
  WRITE_TEXT,
  r#"([A-Za-z_][\w.]*(?:\(\s*['"][^'"]+['"]\s*\))?)\s*\.\s*write_(?:text|bytes)\s*\("#
);
pattern!(
  WRITE_FILE,
  r"(?:Bun\.write|(?:fs\.)?writeFileSync|(?:fs\.)?writeFile|File\.write|IO\.write|\.writeFile)\s*\(\s*([^,()]+)"
);
pattern!(LITERAL, r#"^(?:[A-Za-z_][\w.]*)?\(?\s*['"]([^'"]+)['"]\s*\)?$"#);
pattern!(NAME, r"^[A-Za-z_]\w*$");
pattern!(QUOTED, r#"['"]([^'"]+)['"]"#);

// The paths an inline script writes, as the script's own text spells them, and whether a write call was made
// whose file could not be read.
struct WriteCalls {
  paths: Vec<String>,
  unresolved: bool,
}

fn write_calls(script: &str) -> WriteCalls {
  let mut calls = WriteCalls {
    paths: vec![],
    unresolved: false,
  };
  let add = |calls: &mut WriteCalls, expr: &str| {
    let named = named_paths(expr, script);

    calls.unresolved |= named.is_empty();
    calls.paths.extend(named);
  };

  // Python's `open(<file>, '<mode>')`, a write where the mode writes, appends or updates.
  for call in PY_OPEN.captures_iter(script) {
    if PY_WRITING_MODE.is_match(&call[2]) {
      add(&mut calls, &call[1]);
    }
  }

  // Perl's `open(F, ">file")`, and its three-argument `open(F, ">", <file>)`.
  for call in PERL_OPEN_TWO.captures_iter(script) {
    calls.paths.push(call[1].to_owned());
  }

  for call in PERL_OPEN_THREE.captures_iter(script) {
    add(&mut calls, &call[1]);
  }

  for call in WRITE_TEXT.captures_iter(script) {
    add(&mut calls, &call[1]);
  }

  for call in WRITE_FILE.captures_iter(script) {
    add(&mut calls, &call[1]);
  }

  calls
}

// A path a write call named: a literal as it stands, or every literal the script gave that name — by an
// assignment, or by the list a loop walks.
fn named_paths(expr: &str, script: &str) -> Vec<String> {
  let trimmed = expr.trim();

  if let Some(literal) = LITERAL.captures(trimmed) {
    return vec![literal[1].to_owned()];
  }

  if !NAME.is_match(trimmed) {
    return vec![];
  }

  let mut found = vec![];
  let given = Regex::new(&format!(r#"\b{trimmed}\s*=\s*(?:[A-Za-z_][\w.]*)?\(?\s*['"]([^'"]+)['"]"#));
  let walked = Regex::new(&format!(r"for\s+{trimmed}\s+in\s*[\[(]([^\])]*)[\])]"));

  if let Ok(given) = given {
    found.extend(given.captures_iter(script).map(|given| given[1].to_owned()));
  }

  if let Ok(walked) = walked {
    for list in walked.captures_iter(script) {
      found.extend(QUOTED.captures_iter(&list[1]).map(|item| item[1].to_owned()));
    }
  }

  found
}

// Whether a script writes into the target: one of its write calls names it, or it names the target and makes
// a write call whose file the reading cannot resolve — refused rather than guessed at. A script that opens
// the target only to read it makes no write call, and is not a write.
fn script_writes_target(script: &str, target: &dyn WriteTarget) -> bool {
  let calls = write_calls(script);

  calls.paths.iter().any(|path| target.names(path)) || (calls.unresolved && target.names(script))
}

// The words a writing program writes at: a copier's destination — the value of its target-directory flag, else
// its last plain word — and every word of any other.
fn written_words<'a>(program: &str, args: &'a [String]) -> Vec<&'a String> {
  if !COPIERS.contains(&program) {
    return args.iter().collect();
  }

  if let Some(at) = args.iter().position(|arg| arg == "-t" || arg == "--target-directory") {
    return args.get(at + 1).into_iter().collect();
  }

  if let Some(target) = args.iter().find(|arg| arg.starts_with("--target-directory=")) {
    return vec![target];
  }

  args.iter().rev().find(|arg| !arg.starts_with('-')).into_iter().collect()
}

// A path spelled relative to wherever the shell stands.
fn relative_path(word: &str) -> bool {
  !word.is_empty() && !word.starts_with(['-', '/', '~', '$', '&'])
}

// The text a shell is handed with `-c`, or the text given to `eval`.
fn handed_text(run: &reading::Run) -> Option<String> {
  if run.program == "eval" {
    Some(run.args.join(" "))
  } else if SHELLS.contains(&run.program.as_str()) {
    reading::shell_text(run).cloned()
  } else {
    None
  }
}

/// Why this command would write into the target, or nothing.
pub fn write_reason(command: &str, target: &dyn WriteTarget) -> Option<String> {
  reason_at(command, target, &Standing::default(), 0)
}

/// Where the shell a command runs in stands, as far as the reading knows: the directory the hook was handed, and
/// the home a `cd` with no word or with `~` goes to.
#[derive(Clone, Debug, Default)]
pub struct Standing {
  pub at: Option<String>,
  pub home: String,
}

impl Standing {
  // Where a `cd` to these words leaves the shell: the directory its first plain word names, read from here where
  // it is relative; the home where it names none; unknown where its word cannot be read.
  fn after_cd(&self, args: &[String]) -> Option<String> {
    let home = (!self.home.is_empty()).then(|| trimmed(&self.home).to_owned());

    match args.iter().find(|arg| !arg.starts_with('-')) {
      None => home,
      Some(word) if word.starts_with('/') => Some(resolved("/", word)),
      Some(word) if word == "~" || word.starts_with("~/") => {
        home.map(|home| resolved(&home, word[1..].trim_start_matches('/')))
      }
      Some(word) if relative_path(word) => self.at.as_deref().map(|at| resolved(at, word)),
      Some(_) => None,
    }
  }
}

fn reason_at(
  original: &str,
  target: &dyn WriteTarget,
  standing: &Standing,
  depth: usize,
) -> Option<String> {
  let command = expand_assigned(original);

  if !target.names(&command) && standing.at.is_none() {
    return None;
  }

  let read = read_command(&command);
  let label = target.label();
  // Whether the shell stands inside the target, after a `cd` into it.
  let mut inside = false;
  // Where the shell stands, followed through each `cd`.
  let mut standing = standing.clone();

  for segment in &read.commands {
    let at = standing.at.clone();
    let named = |word: &String| {
      target.names(word)
        || (inside && relative_path(word))
        || at.as_deref().is_some_and(|at| relative_path(word) && target.names_from(word, at))
    };

    if segment.redirects.iter().any(named) {
      return Some(format!("it writes into {label}"));
    }

    let Some(run) = program_of(&segment.words) else {
      continue;
    };
    let program = run.program.as_str();

    if program == "cd" || program == "pushd" {
      if target.follows_cd() {
        inside = run.args.iter().any(|arg| target.names(arg))
          || (inside && run.args.iter().any(|arg| relative_path(arg)));
      }

      standing.at = standing.after_cd(&run.args);
      continue;
    }

    let arg_named = run.args.iter().any(named);
    // Where git works: the directory its `-C` names, read from where the shell stands, else that.
    let git_dir = match run.args.iter().position(|arg| arg == "-C").and_then(|flag| run.args.get(flag + 1))
    {
      Some(dir) if dir.starts_with('/') => Some(resolved("/", dir)),
      Some(dir) => at.as_deref().map(|at| resolved(at, dir)),
      None => at.clone(),
    };
    let stands_in = git_dir.as_deref().is_some_and(|dir| target.stands_in(dir));

    if WRITER_WORDS.contains(&program) && written_words(program, &run.args).into_iter().any(named) {
      return Some(format!("it copies, moves, creates or removes files in {label}"));
    }

    if arg_named && IN_PLACE_EDITORS.contains(&program) && run.args.iter().any(|arg| in_place_flag(arg)) {
      return Some(format!("it edits {label} in place"));
    }

    if (arg_named || inside || stands_in)
      && program == "git"
      && run.args.iter().any(|arg| target.git_writers().contains(&arg.as_str()))
    {
      return Some(format!("it changes {label} through git"));
    }

    if INTERPRETERS.contains(&program) {
      let scripts = inline_scripts(program, &run.args);
      let writes = if scripts.is_empty() {
        reads_script_from_stdin(&run.args)
          && read.heredocs.iter().any(|held| script_writes_target(&held.text, target))
      } else {
        scripts.iter().any(|script| script_writes_target(script, target))
      };

      if writes {
        return Some(format!("it runs a script that names {label} and writes"));
      }
    }

    if depth < INNER_TEXT_DEPTH
      && let Some(inner) = handed_text(&run)
      && let Some(refused) = reason_at(&inner, target, &standing, depth + 1)
    {
      return Some(refused);
    }
  }

  None
}

/// Why this command would change the statement document, or nothing.
pub fn structure_command_write(command: &str) -> Option<String> {
  write_reason(command, &Structure)
}

/// Why a file tool's call on this path would change the statement document, or nothing.
pub fn structure_file_write(path: &str) -> Option<String> {
  (path == STRUCTURE_DOCUMENT || path.ends_with(&format!("/{STRUCTURE_DOCUMENT}")))
    .then(|| format!("it edits {STRUCTURE_DOCUMENT}"))
}

/// What a refusal of a change to the statement document says.
pub fn structure_refusal(why: &str) -> String {
  format!(
    "refused: {why}. {STRUCTURE_DOCUMENT} is the owner's statement of the system, and only a job whose item the owner asked for himself and whose title or brief names it may change it. Where the code differs from it, report the difference instead (docs/live-claude-guard.md, 「The same hook keeps STRUCTURE.md the owner's」)."
  )
}

/// The checkout a command would write into that is another worker's, with why, or nothing; `standing` is where
/// the shell stands and the home its paths are spelled under.
pub fn foreign_command_write<'a>(
  command: &str,
  checkouts: &'a Checkouts,
  standing: &Standing,
) -> Option<(&'a ForeignCheckout, String)> {
  checkouts.foreign.iter().find_map(|checkout| {
    reason_at(command, &Foreign::of(checkout, checkouts, &standing.home), standing, 0)
      .map(|why| (checkout, why))
  })
}

/// The checkout a file tool's target lies in that is another worker's, or nothing: the deepest checkout
/// holding the path, where that one is another worker's.
pub fn foreign_file_write<'a>(path: &str, checkouts: &'a Checkouts) -> Option<&'a ForeignCheckout> {
  let deepest = deepest(checkouts.paths(), path)?;

  checkouts.foreign.iter().find(|checkout| checkout.cwd == deepest)
}

/// The umbrella whose tracked files a command would write, or whose repository it would change through git, with
/// why, or nothing; `standing` is where the shell stands and the home its paths are spelled under.
pub fn sealed_command_write<'a>(
  command: &str,
  checkouts: &'a Checkouts,
  standing: &Standing,
) -> Option<(&'a SealedTree, String)> {
  checkouts.sealed.iter().find_map(|tree| {
    reason_at(command, &Sealed::of(tree, checkouts, &standing.home), standing, 0).map(|why| (tree, why))
  })
}

/// The umbrella whose tracked file a file tool's call on this path would write, with why, or nothing: the
/// umbrella's folder is the deepest checkout holding the path, and its git tracks the file.
pub fn sealed_file_write<'a>(path: &str, checkouts: &'a Checkouts) -> Option<(&'a SealedTree, String)> {
  checkouts.sealed.iter().find_map(|tree| {
    let root = trimmed(&tree.cwd);
    let file = path.strip_prefix(&format!("{root}/"))?;
    let in_folder = deepest(checkouts.paths().chain([root]), path).map(trimmed) == Some(root);

    (in_folder && tree.tracked.iter().any(|tracked| tracked == file))
      .then(|| (tree, format!("it edits {path}, a file the umbrella {} tracks", tree.repository)))
  })
}

/// What a refusal of a write into an umbrella's own folder says.
pub fn sealed_refusal(tree: &SealedTree, why: &str) -> String {
  format!(
    "refused: {why} — {cwd} is the umbrella {repository}'s own folder, whose files are the owner's and which no job writes. A job's work goes in its own checkout and the checkouts lent to it, which stand below the folder; borrow another with bmo job borrow <repository> (docs/worker-slots.md, 「An umbrella repository」).",
    cwd = tree.cwd,
    repository = tree.repository,
  )
}

/// Whether a command could write at all, as far as its text says: a redirect, or a program that writes files,
/// edits in place, changes a tree through git, runs a script or hands text to a shell. A command that could not is
/// read for no checkout.
pub fn may_write(command: &str) -> bool {
  let read = read_command(&expand_assigned(command));

  read.commands.iter().any(|segment| {
    !segment.redirects.is_empty()
      || program_of(&segment.words).is_some_and(|run| {
        let program = run.program.as_str();

        WRITER_WORDS.contains(&program)
          || IN_PLACE_EDITORS.contains(&program)
          || INTERPRETERS.contains(&program)
          || SHELLS.contains(&program)
          || program == "git"
          || program == "eval"
      })
  })
}

/// What a refusal of a write into another worker's checkout says.
pub fn foreign_refusal(checkout: &ForeignCheckout, why: &str) -> String {
  format!(
    "refused: {why} — {cwd} is {worker}'s checkout, and a job writes only in its own checkout and in one it has borrowed, since what is open in a checkout is parked as its holder's own when that holder stops. To change that repository, borrow one of its checkouts for this job with bmo job borrow {repository}, which hands you a free one (docs/worker-slots.md, 「A job writes only in its own checkout」).",
    cwd = checkout.cwd,
    worker = checkout.worker,
    repository = checkout.repository,
  )
}
