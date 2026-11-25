//! A text measured and cut and a bound written as the console does, so a refusal reads the same
//! whichever side said it.

/// A text's length as the console counts it, in UTF-16 units.
pub fn length(text: &str) -> usize {
  text.encode_utf16().count()
}

/// Whether a character is white space as the console's own trim reads it.
pub fn is_space(character: char) -> bool {
  matches!(
    character,
    '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
      ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
  )
}

/// A text with the console's white space taken off both ends, as a note written on either side is read.
pub fn trim(text: &str) -> &str {
  text.trim_matches(is_space)
}

/// The longest head of a text that comes within a number of UTF-16 units, as the console cuts one: a
/// character that would pass the bound is left out whole.
pub fn within(text: &str, units: usize) -> &str {
  let mut spent = 0;
  let end = text
    .char_indices()
    .find(|(_, character)| {
      spent += character.len_utf16();
      spent > units
    })
    .map_or(text.len(), |(at, _)| at);

  &text[..end]
}

/// A bound as the console writes a number, its thousands set apart.
pub fn grouped(number: usize) -> String {
  let digits = number.to_string();
  let mut grouped = String::new();

  for (at, digit) in digits.chars().enumerate() {
    if at > 0 && (digits.len() - at).is_multiple_of(3) {
      grouped.push(',');
    }

    grouped.push(digit);
  }

  grouped
}

/// Names as an English sentence lists them: one alone, two joined by "and", more set apart by commas with "and"
/// before the last.
pub fn listed(names: &[String]) -> String {
  match names {
    [] => String::new(),
    [one] => one.clone(),
    [first @ .., last] => format!("{} and {last}", first.join(", ")),
  }
}

/// What a secret's run is written as in a record (docs/agent-control.md, 「A record holds no secret」).
pub const REDACTED: &str = "[redacted]";

static TOKEN_SHAPED: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
  regex::Regex::new(
    r"\b[0-9a-fA-F]{64}\b|\bsk-[A-Za-z0-9_-]{16,}|(?P<bearer>\bBearer\s+)[A-Za-z0-9._~+/=-]{16,}",
  )
  .expect("the token pattern compiles")
});

/// A text with every value shaped as a token written as `[redacted]`: sixty-four hexadecimal digits standing
/// alone, a key beginning `sk-`, and the word after `Bearer`, which keeps its `Bearer`.
pub fn redact_tokens(text: &str) -> std::borrow::Cow<'_, str> {
  redact_tokens_counted(text).0
}

/// A text with every value shaped as a token written as `[redacted]`, and how many it wrote.
pub fn redact_tokens_counted(text: &str) -> (std::borrow::Cow<'_, str>, usize) {
  let mut runs = 0;
  let words = TOKEN_SHAPED.replace_all(text, |found: &regex::Captures| {
    runs += 1;
    format!("{}{REDACTED}", found.name("bearer").map_or("", |bearer| bearer.as_str()))
  });

  (words, runs)
}
