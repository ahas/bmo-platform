//! What a document the chat agent writes into the archive says of itself (`lang/<language>/archive-docs.yaml`), each line
//! with the `{name}` marks its file names.

use indexmap::IndexMap;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveDocs {
  pub header: Header,
  /// What each kind of document is called, by the kind's name.
  pub kinds: IndexMap<String, String>,
  pub time: Time,
}

/// The lines a document's file opens with under its title.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
  pub kind: String,
  pub request: String,
  pub written: String,
  pub attached: String,
}

/// A moment as the documents tell it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Time {
  pub line: String,
  pub morning: String,
  pub afternoon: String,
}

crate::part!(pub ARCHIVE_DOCS: ArchiveDocs = "archive-docs");
