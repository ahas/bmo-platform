//! The prefix a worker session is opened on, read off the machine and reduced to one short word
//! (docs/session-resets.md): the launch line with its roots taken out, every memory file the program
//! would read for a run in the working directory and what those import, less what the settings
//! exclude, the settings, the rules, the MCP configuration, the plugin's manifest and the front matter
//! of each skill it carries. A session whose word no longer matches the prefix as it stands now was
//! opened on something that is gone.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use glob::{MatchOptions, Pattern};
use regex::Regex;
use sha2::{Digest, Sha256};

// The part of a skill's document the program reads before anything asks for the rest. The cut is the
// one the other side's digest makes, so both reduce the same document to the same word. It reads no YAML:
// the digest needs the block's bytes as they were written, which no front matter library hands back.
static FRONT_MATTER: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"(?s)\A---\n(.*?)\n---\n").expect("the front matter mark"));

/// What a memory file that names nothing readable puts into the prefix.
pub const UNRESOLVED_IMPORT: &str = "«import not found»";

/// Everything the prefix is read out of.
pub struct PrefixReading<'a> {
  pub cwd: &'a Path,
  pub args: &'a [String],
  /// The code tree the line's documents are read out of: a release of the deployment, taken out of the
  /// paths so a release moving on its own moves no word.
  pub code_root: &'a Path,
  pub source_root: &'a Path,
  pub data_root: &'a Path,
  pub home: &'a Path,
  pub plugin_dir: &'a Path,
  pub settings: &'a Path,
  pub rules: &'a Path,
  pub mcp_config: &'a Path,
}

fn read_if_there(path: &Path) -> Option<String> {
  std::fs::read(path).ok().map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

// The command's words as a prefix reads them: every word but its advisor's, which stands outside the prefix a
// session's cache keeps, and each root a path starts with named rather than written out.
fn normalised_args(args: &[String], roots: &[(&str, &Path)]) -> String {
  super::claude::without_advisor(args)
    .into_iter()
    .map(|arg| {
      roots
        .iter()
        .map(|(name, root)| (name, root.to_string_lossy()))
        .find(|(_, root)| !root.is_empty() && arg.starts_with(root.as_ref()))
        .map_or_else(|| arg.clone(), |(name, root)| format!("<{name}>{}", &arg[root.len()..]))
    })
    .collect::<Vec<_>>()
    .join(" ")
}

/// The part of a document the program reads before anything asks for the rest.
pub fn front_matter(text: &str) -> String {
  let body = text.replace("\r\n", "\n");

  FRONT_MATTER.captures(&body).map_or_else(|| body.clone(), |found| found[1].to_owned())
}

// The documents a memory file imports, each named alone on a line after an `@`.
fn imports_in(text: &str) -> Vec<String> {
  text
    .replace("\r\n", "\n")
    .split('\n')
    .filter_map(|line| line.trim().strip_prefix('@'))
    .filter(|name| !name.is_empty() && !name.contains(char::is_whitespace))
    .map(str::to_owned)
    .collect()
}

// A path with its `.` and `..` read away, as resolving one does.
fn lexical(path: &Path) -> PathBuf {
  let mut out = PathBuf::new();

  for part in path.components() {
    match part {
      Component::ParentDir => {
        out.pop();
      }
      Component::CurDir => {}
      other => out.push(other),
    }
  }

  out
}

fn imported_file(from: &Path, name: &str, home: &Path) -> PathBuf {
  match name.strip_prefix("~/") {
    Some(rest) => home.join(rest),
    None => lexical(&from.parent().unwrap_or(Path::new("/")).join(name)),
  }
}

// One path as seen from another, with a `..` for every step up.
fn relative(from: &Path, to: &Path) -> String {
  let from: Vec<_> = from.components().collect();
  let to: Vec<_> = to.components().collect();
  let shared = from.iter().zip(&to).take_while(|(one, two)| one == two).count();
  let mut parts: Vec<String> = std::iter::repeat_n("..".to_owned(), from.len() - shared).collect();

  parts.extend(to[shared..].iter().map(|part| part.as_os_str().to_string_lossy().into_owned()));
  parts.join("/")
}

// Every memory file the program would find for a run in this directory: the chain from the directory up
// to the home directory, and the machine's own under its configuration directory.
fn memory_files(cwd: &Path, home: &Path) -> Vec<PathBuf> {
  let mut found = vec![];
  let mut at = lexical(cwd);

  loop {
    found.push(at.join("CLAUDE.md"));
    found.push(at.join("CLAUDE.local.md"));

    let Some(up) = at.parent().map(Path::to_path_buf) else { break };

    if at == home {
      break;
    }

    at = up;
  }

  found.push(home.join(".claude").join("CLAUDE.md"));
  found
}

// The memory files the settings tell the program to skip, as the patterns it matches a file's path
// against.
fn excluded_memory(settings: &Path) -> Vec<Pattern> {
  read_if_there(settings)
    .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    .and_then(|settings| settings.get("claudeMdExcludes").and_then(serde_json::Value::as_array).cloned())
    .unwrap_or_default()
    .iter()
    .filter_map(serde_json::Value::as_str)
    .filter_map(|pattern| Pattern::new(pattern).ok())
    .collect()
}

/// Every source of the prefix, by its key.
pub fn prefix_sources(reading: &PrefixReading) -> Vec<(String, String)> {
  let mut sources = vec![(
    "args".to_owned(),
    normalised_args(
      reading.args,
      &[
        ("code", reading.code_root),
        ("source", reading.source_root),
        ("data", reading.data_root),
        ("home", reading.home),
      ],
    ),
  )];
  let excluded = excluded_memory(reading.settings);
  let skip = MatchOptions {
    require_literal_separator: true,
    ..MatchOptions::new()
  };
  let mut chain: Vec<(PathBuf, bool)> = memory_files(reading.cwd, reading.home)
    .into_iter()
    .filter(|path| !excluded.iter().any(|pattern| pattern.matches_path_with(path, skip)))
    .map(|path| (path, false))
    .collect();
  let mut read = HashSet::new();
  let mut at = 0;

  while at < chain.len() {
    let (path, imported) = chain[at].clone();

    at += 1;

    if !read.insert(path.clone()) {
      continue;
    }

    let Some(text) = read_if_there(&path).or_else(|| imported.then(|| UNRESOLVED_IMPORT.to_owned())) else {
      continue;
    };

    for name in imports_in(&text) {
      chain.push((imported_file(&path, &name, reading.home), true));
    }

    sources.push((format!("memory:{}", relative(reading.home, &path)), text));
  }

  for (key, path) in [
    ("settings", reading.settings.to_path_buf()),
    ("rules", reading.rules.to_path_buf()),
    ("mcp", reading.mcp_config.to_path_buf()),
    ("plugin", reading.plugin_dir.join(".claude-plugin").join("plugin.json")),
  ] {
    if let Some(text) = read_if_there(&path) {
      sources.push((key.to_owned(), text));
    }
  }

  let mut skills: Vec<String> = std::fs::read_dir(reading.plugin_dir.join("skills"))
    .map(|entries| {
      entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
    })
    .unwrap_or_default();

  skills.sort();

  for skill in skills {
    if let Some(text) = read_if_there(&reading.plugin_dir.join("skills").join(&skill).join("SKILL.md")) {
      sources.push((format!("skill:{skill}"), front_matter(&text)));
    }
  }

  sources
}

/// The sources reduced to one short word, in the order of their keys.
pub fn digest_of(mut sources: Vec<(String, String)>) -> String {
  sources.sort_by(|one, two| one.0.cmp(&two.0));

  let mut hash = Sha256::new();

  for (key, text) in &sources {
    hash.update(key.as_bytes());
    hash.update(b" ");
    hash.update(format!("{:x}", Sha256::digest(text.as_bytes())).as_bytes());
    hash.update(b"\n");
  }

  format!("{:x}", hash.finalize())[..32].to_owned()
}

pub fn prefix_digest(reading: &PrefixReading) -> String {
  digest_of(prefix_sources(reading))
}

// The repository files a commit can name that the prefix is read out of: the memory files and what a
// pointer among them imports, the MCP configuration, the platform's harness opening its prompt opens with,
// and the worker line's settings, rules, the result-language piece, the report trailer and the rulebooks it
// carries.
const PREFIX_FILES: [&str; 13] = [
  "CLAUDE.md",
  "CLAUDE.local.md",
  "AGENTS.md",
  "STRUCTURE.md",
  ".mcp.json",
  "prompts/harness.md",
  "prompts/agents/worker/settings.json",
  "prompts/agents/worker/standing-rules.md",
  "prompts/agents/worker/result-languages.md",
  "prompts/agents/worker/trailer.md",
  "prompts/rules/self-preservation.md",
  "prompts/platform/environment.md",
  "prompts/platform/preservation.md",
];

// Directories everything under which shapes the prompt: the settings and skills a run is shown, and the
// co-worker definitions.
const PREFIX_DIRS: [&str; 5] = [
  ".claude/",
  "prompts/agents/worker/plugin/",
  "prompts/agents/coder/plugin/",
  "prompts/agents/producer/plugin/",
  "prompts/agents/worker/coworkers/",
];

static OWN_RULES: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^prompts/agents/[^/]+/own-rules\.md$").expect("the own rules path"));

/// Which of the repository-relative paths given are part of a worker session's prefix, in the order given:
/// what a job's own commits moved under the session it ran in.
pub fn touches_prefix(paths: &[String]) -> Vec<String> {
  paths
    .iter()
    .filter(|path| {
      PREFIX_FILES.contains(&path.as_str())
        || OWN_RULES.is_match(path)
        || PREFIX_DIRS.iter().any(|dir| path.starts_with(dir))
    })
    .cloned()
    .collect()
}
