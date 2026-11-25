//! Whether a turn's final answer is a tool call the model wrote as text instead of making it
//! (docs/agent-control.md, 「What a resident turn reads」): such an answer made no call, ran nothing, and is
//! no report. The judgement is broad, since each model and template breaks a call its own way: any
//! tool-call markup or control token outside what the answer quotes as code, or a lone JSON object shaped
//! like a call.

use std::ops::Range;
use std::sync::LazyLock;

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};
use regex::Regex;
use serde_json::Value;

// An opening or closing tag of the markup a model wraps a call in, its name ending where a tag's name ends,
// so a longer word that begins the same way is none.
static CALL_MARKUP: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?i)</?(?:tool_calls?|function_calls|invoke|function|parameter)(?:[\s>=/]|$)")
    .expect("the call markup")
});

// A model's control token written out, between plain bars or the full-width ones some templates use.
static CONTROL_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"<\|[^|\s<>]{1,64}\|>|<\x{FF5C}[^\x{FF5C}\s<>]{1,64}\x{FF5C}>").expect("a control token")
});

/// Whether the answer is a tool call written as text rather than a call made.
pub fn text_tool_call(text: &str) -> bool {
  let bare = outside_quoted_code(text);

  CALL_MARKUP.is_match(&bare) || CONTROL_TOKEN.is_match(&bare) || lone_call_object(text)
}

// The answer with its fenced code blocks and inline code spans taken out, as a markdown reader finds them:
// what an answer quotes as code is a report showing markup, not a call. An indented block is left in, since
// a call written as text can come indented.
fn outside_quoted_code(text: &str) -> String {
  let mut bare = String::with_capacity(text.len());
  let mut from = 0;

  for quoted in quoted_code(text) {
    if quoted.start >= from {
      bare.push_str(&text[from..quoted.start]);
      bare.push('\n');
      from = quoted.end;
    }
  }
  bare.push_str(&text[from..]);

  bare
}

fn quoted_code(text: &str) -> Vec<Range<usize>> {
  Parser::new(text)
    .into_offset_iter()
    .filter(|(event, _)| {
      matches!(event, Event::Code(_) | Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_))))
    })
    .map(|(_, range)| range)
    .collect()
}

// Whether the whole answer is one JSON object shaped like a call: a name beside its arguments, or such an
// object under `function`.
fn lone_call_object(text: &str) -> bool {
  serde_json::from_str::<Value>(text.trim()).is_ok_and(|value| value.is_object() && call_shaped(&value))
}

fn call_shaped(value: &Value) -> bool {
  let named = value.get("name").is_some_and(Value::is_string);
  let argued = value.get("arguments").is_some() || value.get("parameters").is_some();

  (named && argued) || value.get("function").is_some_and(call_shaped)
}
