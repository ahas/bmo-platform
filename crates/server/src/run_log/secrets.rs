//! What a credential looks like, and how a line that carries one is written down without it
//! (docs/run-records.md). Masking is done once, where the line is written, so the archive never holds the
//! value; what is masked is the value alone, and the flag, the file and the shape of the command stay.
//!
//! The shapes are the TypeScript side's (`core/util/secrets.ts`), word for word. Some of them look
//! before or after a match, which this process's pattern reader does not, so each such shape is a plain
//! pattern with what it looks at checked beside it.

use std::sync::LazyLock;

use regex::{Captures, Regex};

use super::json::Json;

/// What stands where a credential stood.
pub const MASKED_VALUE: &str = "«masked value»";

// What a shape checks around a match before it is taken.
type Check = fn(&str, &Captures) -> bool;

struct Shape {
  pattern: Regex,
  check: Option<Check>,
  stand_in: fn(&Captures) -> String,
}

fn pattern(text: &str) -> Regex {
  Regex::new(text).expect("a credential shape is a pattern")
}

// The character before a byte position, where there is one.
fn before(text: &str, at: usize) -> Option<char> {
  text[..at].chars().next_back()
}

fn word_char(ch: char) -> bool {
  ch.is_ascii_alphanumeric() || ch == '_'
}

// Whether a match begins a word, as a word is counted in ASCII letters, digits and the underscore.
fn word_starts(text: &str, found: &Captures) -> bool {
  before(text, found.get(0).map_or(0, |whole| whole.start())).is_none_or(|ch| !word_char(ch))
}

fn not_masked(value: &str) -> bool {
  !value.starts_with('«')
}

static SHAPES: LazyLock<Vec<Shape>> = LazyLock::new(|| {
  vec![
    // An authorization header, whichever scheme it carries, the scheme a word of its own.
    Shape {
      pattern: pattern(r#"(?i)(Bearer|Basic|Token)\s+([^\s"'`,;]{8,})"#),
      check: Some(|text, found| {
        let start = found.get(0).map_or(0, |whole| whole.start());

        before(text, start).is_none_or(|ch| !word_char(ch) && ch != '-') && not_masked(&found[2])
      }),
      stand_in: |found| format!("{} {MASKED_VALUE}", &found[1]),
    },
    // A key of the shape the model providers issue.
    Shape {
      pattern: pattern(r"(?i)sk-([a-z]{3,8})-[A-Za-z0-9_-]{6,}"),
      check: Some(word_starts),
      stand_in: |found| format!("sk-{}-{MASKED_VALUE}", &found[1]),
    },
    Shape {
      pattern: pattern(r"(?i)sk-[A-Za-z0-9]{20,}"),
      check: Some(word_starts),
      stand_in: |_| format!("sk-{MASKED_VALUE}"),
    },
    // A password written into an address: scheme://name:password@host.
    Shape {
      pattern: pattern(r#"(?i)([a-z][a-z0-9+.-]{0,31}://[^\s:/@"'`]+:)([^\s@"'`]+)@"#),
      check: Some(|_, found| not_masked(&found[2])),
      stand_in: |found| format!("{}{MASKED_VALUE}@", &found[1]),
    },
    // A flag given its value on the command line, the flag opening a word.
    Shape {
      pattern: pattern(
        r#"(?i)(--?(?:password|passwd|pwd|token|secret|api-?key|apikey|auth|credential)s?[= ])([^\s"'`;&|]+)"#,
      ),
      check: Some(|text, found| {
        let start = found.get(0).map_or(0, |whole| whole.start());
        let value = &found[2];

        before(text, start).is_none_or(char::is_whitespace) && !value.starts_with('-') && not_masked(value)
      }),
      stand_in: |found| format!("{}{MASKED_VALUE}", &found[1]),
    },
    // An assignment: a shell variable, a line of an env file, a query string, a field of a config.
    Shape {
      pattern: pattern(
        r#"(?i)([A-Za-z0-9_.-]*(?:password|passwd|pwd|secret|token|api[_-]?key|apikey|access[_-]?key|credential)s?)(\s*[:=]\s*)("[^"]*"|'[^']*'|[^\s"'`;&|,)}]+)"#,
      ),
      check: Some(|text, found| {
        let start = found.get(0).map_or(0, |whole| whole.start());

        before(text, start).is_none_or(|ch| !(word_char(ch) || ch == '.' || ch == '-'))
          && not_masked(&found[3])
      }),
      stand_in: |found| format!("{}{}{MASKED_VALUE}", &found[1], &found[2]),
    },
    // The console's own account script, whose second argument is the password.
    Shape {
      pattern: pattern(r#"(user\.ts\s+set\s+\S+\s+)([^\s"'`;&|]+)"#),
      check: Some(|_, found| not_masked(&found[2])),
      stand_in: |found| format!("{}{MASKED_VALUE}", &found[1]),
    },
  ]
});

// One shape laid over a text: each match it takes stood in for, a match its check refuses passed over by
// one character, as a pattern that looks around goes on from the next place.
fn replace(shape: &Shape, text: &str) -> String {
  let mut out = String::with_capacity(text.len());
  let mut at = 0;

  while at <= text.len() {
    let Some(found) = shape.pattern.captures_at(text, at) else { break };
    let whole = found.get(0).expect("a match has its whole");

    if shape.check.is_none_or(|check| check(text, &found)) {
      out.push_str(&text[at..whole.start()]);
      out.push_str(&(shape.stand_in)(&found));
      at = if whole.end() > whole.start() { whole.end() } else { whole.end() + 1 };
      continue;
    }

    let next = whole.start() + text[whole.start()..].chars().next().map_or(1, char::len_utf8);

    out.push_str(&text[at..next.min(text.len())]);
    at = next;
  }

  if at < text.len() {
    out.push_str(&text[at..]);
  }

  out
}

/// One piece of text with every credential shape in it masked.
pub fn mask_secret_text(text: &str) -> String {
  SHAPES.iter().fold(text.to_owned(), |masked, shape| replace(shape, &masked))
}

// A name that holds a credential rather than a setting.
static SECRET_NAME: LazyLock<Regex> = LazyLock::new(|| {
  pattern(
    r"(?i)^(?:[a-z0-9_.-]*[_.-])?(?:password|passwd|pwd|secret|token|api[_-]?key|apikey|access[_-]?key|auth|credential)s?$",
  )
});

fn parsed_object(text: &str) -> Option<Json> {
  let trimmed = text.trim_start();

  if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
    return None;
  }

  Json::parse(text)
}

/// A tool's arguments or its result as the log may keep them: JSON masked as the values it holds and
/// written back as JSON, anything else masked as the text it is.
pub fn mask_secrets(text: &str) -> String {
  parsed_object(text).map_or_else(|| mask_secret_text(text), |parsed| masked_within(&parsed).stringify())
}

fn masked_within(value: &Json) -> Json {
  match value {
    Json::String(text) => Json::String(mask_secret_text(text)),
    Json::Array(items) => Json::Array(items.iter().map(masked_within).collect()),
    Json::Object(fields) => Json::Object(
      fields
        .iter()
        .map(|(name, held)| {
          let hides = SECRET_NAME.is_match(name)
            && matches!(held, Json::String(_) | Json::Array(_) | Json::Object(_));

          (
            name.clone(),
            if hides { Json::String(MASKED_VALUE.to_owned()) } else { masked_within(held) },
          )
        })
        .collect(),
    ),
    other => other.clone(),
  }
}

const SECRET_EXTENSIONS: [&str; 6] = [".pem", ".key", ".p12", ".pfx", ".jks", ".keystore"];
const SECRET_STEMS: [&str; 3] = ["credentials", "secret", "secrets"];
const DATA_EXTENSIONS: [&str; 7] = ["", ".json", ".yaml", ".yml", ".toml", ".ini", ".txt"];
const EXAMPLE_ENDINGS: [&str; 3] = [".example", ".sample", ".template"];

fn extension_of(name: &str) -> String {
  match name.rfind('.') {
    Some(dot) if dot > 0 => name[dot..].to_lowercase(),
    _ => String::new(),
  }
}

static KEY_FILE: LazyLock<Regex> = LazyLock::new(|| pattern(r"^id_(rsa|dsa|ecdsa|ed25519)"));

/// Whether a path is a file whose contents are a credential.
pub fn is_secret_path(path: &str) -> bool {
  let parts: Vec<&str> = path.split('/').collect();
  let name = parts.last().copied().unwrap_or_default().to_lowercase();

  if parts[..parts.len().saturating_sub(1)].iter().any(|part| part.to_lowercase() == "env") {
    return true;
  }

  if (name == ".env" || name.starts_with(".env."))
    && !EXAMPLE_ENDINGS.iter().any(|ending| name.ends_with(ending))
  {
    return true;
  }

  let extension = extension_of(&name);

  if SECRET_EXTENSIONS.contains(&extension.as_str()) {
    return true;
  }

  if KEY_FILE.is_match(&name) || name == ".netrc" || name == ".pgpass" {
    return true;
  }

  let stem = if extension.is_empty() { name.as_str() } else { &name[..name.len() - extension.len()] };

  SECRET_STEMS.contains(&stem) && DATA_EXTENSIONS.contains(&extension.as_str())
}

// How long a word may be and still be looked at as a path.
const LONGEST_PATH: usize = 1024;

static WORD_BREAK: LazyLock<Regex> = LazyLock::new(|| pattern(r#"[\s"'`;&|(){}\[\]<>,]+"#));

fn strings(value: &Json, into: &mut Vec<String>) {
  match value {
    Json::String(text) => into.push(text.clone()),
    Json::Array(items) => items.iter().for_each(|item| strings(item, into)),
    Json::Object(fields) => fields.iter().for_each(|(_, held)| strings(held, into)),
    _ => {}
  }
}

/// Whether a tool call reads or writes a file whose contents are a credential.
pub fn reads_secret_file(arguments: &str) -> bool {
  let texts = parsed_object(arguments).map_or_else(
    || vec![arguments.to_owned()],
    |parsed| {
      let mut found = vec![];

      strings(&parsed, &mut found);
      found
    },
  );

  texts
    .iter()
    .flat_map(|text| WORD_BREAK.split(text).map(str::to_owned).collect::<Vec<_>>())
    .any(|word| {
      !word.is_empty()
        && word.contains('/')
        && word.encode_utf16().count() <= LONGEST_PATH
        && is_secret_path(&word)
    })
}
