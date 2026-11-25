//! What an agent place's records are read for before a line is written (docs/agent-control.md, 「A record
//! holds no secret」): a saved secret's value, the secret shapes of the base library `bmo-secret-shapes` and
//! agency's own token shapes, each run found written as `[redacted]` in every string a line holds, its keys
//! aside. Nothing here writes a word of what it read.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, OnceLock};

use policy::record_secrets::RECORD_SECRETS;
use regex::Regex;
use serde_json::Value;

use crate::text::{REDACTED, redact_tokens_counted};

// The data root this process was told as it started, and the readings made from it on the first line written.
static DATA_ROOT: OnceLock<PathBuf> = OnceLock::new();
static PROCESS: LazyLock<Readings> =
  LazyLock::new(|| DATA_ROOT.get().map_or_else(Readings::shapes_only, |root| Readings::read(root)));

/// Tells this process the data root whose `env/` holds the saved values; the first call alone counts, and a
/// process never told reads the secret shapes and agency's token shapes alone.
pub fn install(data_root: &Path) {
  let _ = DATA_ROOT.set(data_root.to_path_buf());
}

/// The readings every record this process writes is read through.
pub fn process() -> &'static Readings {
  &PROCESS
}

/// A saved value and the file under `env/` it stands in, named from `env/`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Saved {
  pub value: String,
  pub file: String,
}

impl Saved {
  pub fn new(value: impl Into<String>, file: impl Into<String>) -> Self {
    Self {
      value: value.into(),
      file: file.into(),
    }
  }
}

/// What took a run out: a saved value by the files it stands in, a secret shape by its name, or agency's own
/// token shapes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
  Saved(String),
  Shape(String),
  Agency,
}

impl std::fmt::Display for Source {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Saved(files) => write!(f, "saved value in {files}"),
      Self::Shape(name) => write!(f, "secret shape {name}"),
      Self::Agency => f.write_str("agency token shape"),
    }
  }
}

/// How many runs each source took out, so a false find can be judged without the words.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tally(BTreeMap<Source, usize>);

impl Tally {
  fn add(&mut self, source: Source, runs: usize) {
    *self.0.entry(source).or_default() += runs;
  }

  /// The runs one source took out.
  pub fn of(&self, source: &Source) -> usize {
    self.0.get(source).copied().unwrap_or_default()
  }

  /// One line to a source that took a run out, in the sources' order.
  pub fn lines(&self) -> Vec<String> {
    self.0.iter().map(|(source, runs)| format!("{source}: {runs}")).collect()
  }
}

/// The three readings, in the order they run: the saved values here, the secret shapes and agency's token
/// shapes as they stand.
pub struct Readings {
  saved: Option<Regex>,
  // The files each saved value stands in, joined, by the value.
  saved_files: HashMap<String, String>,
}

impl Readings {
  /// The secret shapes and agency's token shapes alone.
  pub fn shapes_only() -> Self {
    Self::of(vec![])
  }

  /// The readings a machine's data root gives, its saved values those its `env/` holds.
  pub fn read(data_root: &Path) -> Self {
    Self::of(saved_values(&data_root.join("env")))
  }

  /// The readings of saved values.
  pub fn of(saved: Vec<Saved>) -> Self {
    let mut saved_files: HashMap<String, String> = HashMap::new();

    for Saved { value, file } in saved {
      let files = saved_files.entry(value).or_default();

      if !files.split(", ").any(|known| known == file) {
        if !files.is_empty() {
          files.push_str(", ");
        }

        files.push_str(&file);
      }
    }

    Self {
      saved: saved_pattern(saved_files.keys()),
      saved_files,
    }
  }

  /// The words with every secret's run written as `[redacted]`.
  pub fn text<'a>(&self, text: &'a str) -> Cow<'a, str> {
    self.text_tallied(text, &mut Tally::default())
  }

  /// The words with every secret's run written as `[redacted]`, each run counted to the source that took it.
  pub fn text_tallied<'a>(&self, text: &'a str, tally: &mut Tally) -> Cow<'a, str> {
    let mut words = Cow::Borrowed(text);

    if let Some(saved) = &self.saved {
      let replaced = saved.replace_all(&words, |found: &regex::Captures| {
        let files = self.saved_files.get(&found[0]).cloned().unwrap_or_default();

        tally.add(Source::Saved(files), 1);
        REDACTED
      });

      if let Some(replaced) = owned(replaced) {
        words = Cow::Owned(replaced);
      }
    }

    let shaped = bmo_secret_shapes::redact(&words, REDACTED);

    for found in &shaped.found {
      tally.add(Source::Shape(found.shape.to_owned()), found.runs);
    }

    if let Cow::Owned(replaced) = shaped.text {
      words = Cow::Owned(replaced);
    }

    let (replaced, runs) = redact_tokens_counted(&words);

    match owned(replaced) {
      Some(replaced) => {
        tally.add(Source::Agency, runs);
        Cow::Owned(replaced)
      }
      None => words,
    }
  }

  /// Every string a JSON value holds, its keys aside, read as words are; answers whether one changed.
  pub fn value(&self, value: &mut Value) -> bool {
    self.value_tallied(value, &mut Tally::default())
  }

  /// Every string a JSON value holds read as words are, each run counted; answers whether one changed.
  pub fn value_tallied(&self, value: &mut Value, tally: &mut Tally) -> bool {
    match value {
      Value::String(text) => match self.text_tallied(text, tally) {
        Cow::Owned(replaced) => {
          *text = replaced;
          true
        }
        Cow::Borrowed(_) => false,
      },
      Value::Array(items) => {
        items.iter_mut().fold(false, |changed, item| self.value_tallied(item, tally) | changed)
      }
      Value::Object(fields) => fields
        .values_mut()
        .fold(false, |changed, item| self.value_tallied(item, tally) | changed),
      Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
  }
}

// The words a replacement made, or none where it found nothing to replace.
fn owned(words: Cow<'_, str>) -> Option<String> {
  match words {
    Cow::Owned(replaced) => Some(replaced),
    Cow::Borrowed(_) => None,
  }
}

// A line's field, where it reads as one: not where its value is padding alone, which a base64 value ends on.
fn field(line: &str) -> Option<regex::Captures<'_>> {
  FIELD.captures(line).filter(|found| !found[2].trim_start_matches('=').is_empty())
}

// One pattern finding any saved value, the longest first so a value holding another is taken whole.
fn saved_pattern<'a>(saved: impl Iterator<Item = &'a String>) -> Option<Regex> {
  let mut saved: Vec<&String> = saved.collect();

  saved.sort_by(|one, other| other.len().cmp(&one.len()).then_with(|| one.cmp(other)));

  if saved.is_empty() {
    return None;
  }

  let alternatives: Vec<String> = saved.iter().map(|value| regex::escape(value)).collect();

  Regex::new(&alternatives.join("|"))
    .map_err(|why| eprintln!("records: the saved values cannot be read for: {why}"))
    .ok()
}

/// Whether a key names a secret: its last part, split at `_`, `.` and `-` and where a lowercase letter meets an
/// uppercase one, and read in any case, is one of the policy's words, alone or after `api`.
pub fn names_secret(key: &str) -> bool {
  let last = key.rsplit(['_', '.', '-']).next().unwrap_or(key);
  let camel = last
    .char_indices()
    .zip(last.chars().skip(1))
    .filter(|((_, one), next)| one.is_ascii_lowercase() && next.is_ascii_uppercase())
    .last()
    .map_or(0, |((at, one), _)| at + one.len_utf8());
  let last = last[camel..].to_ascii_lowercase();
  let word = last.strip_prefix("api").filter(|rest| !rest.is_empty()).unwrap_or(&last);

  RECORD_SECRETS.secret_words.iter().any(|secret| secret == word)
}

// Whether a saved value counts: long enough to be no ordinary word, and no path.
fn counts(value: &str) -> bool {
  value.chars().count() >= RECORD_SECRETS.least_characters && !value.starts_with(['/', '~'])
}

/// The saved values the files under `env` hold that count as a secret's (docs/agent-control.md, 「A record
/// holds no secret」): a folder's files before its folders, each in the order of their names, a provider's
/// login home and every link passed over, within the policy's bounds; a file that cannot be read gives none.
pub fn saved_values(env: &Path) -> Vec<Saved> {
  let marks: Vec<&str> = harness_providers::providers()
    .iter()
    .filter_map(|provider| provider.login_home_mark())
    .collect();
  let mut walk = Walk {
    env,
    marks: &marks,
    left: RECORD_SECRETS.saved_files_most,
    values: vec![],
  };

  if walk.folder(env).is_break() {
    eprintln!(
      "records: the saved values were read from the first {} files under {} alone",
      RECORD_SECRETS.saved_files_most,
      env.display()
    );
  }

  walk.values.retain(|saved| counts(&saved.value));
  walk.values
}

// The walk through the saved files: what it passes over, how many files it may still meet, and what it found.
struct Walk<'a> {
  env: &'a Path,
  marks: &'a [&'a str],
  left: usize,
  values: Vec<Saved>,
}

impl Walk<'_> {
  // A folder's files, then its folders, each in the order of their names; breaks where the bound is met.
  fn folder(&mut self, dir: &Path) -> std::ops::ControlFlow<()> {
    use std::ops::ControlFlow;

    let Ok(entries) = std::fs::read_dir(dir) else {
      return ControlFlow::Continue(());
    };
    let mut entries: Vec<_> = entries.flatten().collect();

    entries.sort_by_key(std::fs::DirEntry::file_name);

    if entries
      .iter()
      .any(|entry| entry.file_name().to_str().is_some_and(|name| self.marks.contains(&name)))
    {
      return ControlFlow::Continue(());
    }

    let (files, folders): (Vec<_>, Vec<_>) = entries
      .into_iter()
      .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file() || kind.is_dir()))
      .partition(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()));

    for file in files {
      if self.left == 0 {
        return ControlFlow::Break(());
      }

      self.left -= 1;
      self.file(&file.path());
    }

    for folder in folders {
      self.folder(&folder.path())?;
    }

    ControlFlow::Continue(())
  }

  // The values one file holds, where it is small enough to be a saved secret's.
  fn file(&mut self, path: &Path) {
    let small =
      std::fs::metadata(path).is_ok_and(|meta| meta.len() <= RECORD_SECRETS.saved_file_most_bytes);
    let Some(text) = small.then(|| std::fs::read_to_string(path).ok()).flatten() else {
      return;
    };
    let name = path.strip_prefix(self.env).unwrap_or(path).to_string_lossy().into_owned();
    let mut values = vec![];

    match structured(path, &text) {
      Some(read) => under(&read, read.is_string(), &mut values),
      None => line_values(&text, &mut values),
    }

    self.values.extend(values.into_iter().map(|value| Saved::new(value, name.clone())));
  }
}

// A `.toml` or `.json` file as its format reads it, or none for another file and for one that does not read.
fn structured(path: &Path, text: &str) -> Option<Value> {
  match path.extension()?.to_str()? {
    "toml" => toml::from_str::<toml::Value>(text)
      .ok()
      .and_then(|table| serde_json::to_value(table).ok()),
    "json" => serde_json::from_str(text).ok(),
    _ => None,
  }
}

// The strings a read file holds where they stand under a key that names a secret, at any depth.
fn under(value: &Value, secret: bool, values: &mut Vec<String>) {
  match value {
    Value::String(text) if secret => values.push(text.clone()),
    Value::Array(items) => items.iter().for_each(|item| under(item, secret, values)),
    Value::Object(fields) => fields.iter().for_each(|(key, item)| under(item, names_secret(key), values)),
    _ => {}
  }
}

static FIELD: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"^(?:export[ \t]+)?([A-Za-z_][A-Za-z0-9_]*)=(.*)$").expect("the field pattern compiles")
});

// The values a saved file of the console's own form holds: one value, or `NAME=VALUE` lines, also opening on
// `export`, whose value is what follows the line's first `=`, verbatim; a file whose one line is such a field
// is read as the field. A dotenv reader is not used, since it reinterprets a value — a `#`, a quote, a `$` —
// and the value the console saved is the one that may land in a record; the one pair of quotes a value may be
// wrapped in is taken off as a second value beside it.
fn line_values(text: &str, values: &mut Vec<String>) {
  let lines: Vec<&str> = text
    .lines()
    .map(|line| line.trim_end_matches('\r'))
    .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
    .collect();

  if let [only] = lines.as_slice()
    && field(only).is_none()
  {
    values.push(only.trim().to_owned());
  }

  for line in lines {
    let Some(field) = field(line) else { continue };

    if !names_secret(&field[1]) {
      continue;
    }

    let value = &field[2];

    values.push(value.to_owned());

    if let Some(inner) =
      ['"', '\''].iter().find_map(|quote| value.strip_prefix(*quote)?.strip_suffix(*quote))
    {
      values.push(inner.to_owned());
    }
  }
}
