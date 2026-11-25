//! What a search leaves for a person to read (docs/archive.md): its document's head, result and trail, the
//! words each part and field of a result is written as, and the result a page brought in whole is written
//! as, each line with the `{name}` marks its file names.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Head {
  pub untitled: String,
  pub query: String,
  pub topic: String,
  pub keywords: String,
  pub rule: String,
  pub option: String,
  pub depth: String,
  pub status: String,
  pub started: String,
  pub finished: String,
  pub runs: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Result {
  pub heading: String,
  pub empty: String,
  pub branch: String,
  pub sources: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trail {
  pub heading: String,
  pub summary: String,
  pub own: String,
  pub run: String,
  pub asked: String,
  pub opened: String,
  pub found: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reading {
  pub address: String,
  pub map: String,
  pub street_view: String,
  pub menu: String,
  pub dish: String,
  pub price: String,
  pub notes: String,
  pub released: String,
  pub newer: String,
  pub stock: String,
  pub product_page: String,
  pub deal: String,
  pub usual: String,
  pub review: String,
  pub untitled: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Import {
  pub answered: String,
  pub topic: String,
  pub missing: String,
  pub kept: String,
  pub moved: String,
  pub cited: String,
  pub cited_text: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Search {
  pub archive_name: String,
  pub import_query: String,
  pub head: Head,
  pub result: Result,
  pub trail: Trail,
  /// What each fixed part of a body is called, by its label.
  pub parts: BTreeMap<String, String>,
  /// What each line of an entry is written as, by its label, beside the pieces a deal, a review and a kept
  /// picture are made of.
  pub fields: BTreeMap<String, String>,
  /// What the console's reading of a finding draws it with.
  pub reading: Reading,
  pub import: Import,
}

impl Search {
  /// One field's words by its name; a name the pack lacks is a fault of the build, which the pack's test meets.
  pub fn field(&self, name: &str) -> &str {
    self.fields.get(name).map_or("", String::as_str)
  }
}

crate::part!(pub SEARCH: Search = "search");
