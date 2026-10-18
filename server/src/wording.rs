//! The document wording file bmo-artifact reads (docs/language-pack.md, 「What other units read」): the pack's
//! parts its keys stand in, written out under the groups bmo-artifact reads them by.

use serde_json::{Map, Value};

use crate::lang::{LANGUAGE, read};

/// The file's text: every part bmo-artifact's 「Wording」 table names, under its group, and nothing else.
pub fn document_wording() -> Result<String, String> {
  let parts = [
    ("archive_docs", "archive-docs", crate::part_text!("archive-docs")),
    ("search", "search", crate::part_text!("search")),
  ];
  let file = parts
    .into_iter()
    .map(|(group, part, text)| (group.to_owned(), read::<Value>(&format!("{LANGUAGE}/{part}.yaml"), text)))
    .collect::<Map<String, Value>>();
  let text = serde_saphyr::to_string(&Value::Object(file))
    .map_err(|why| format!("the document wording does not write: {why}"))?;

  Ok(format!(
    "# The document wording file bmo-artifact reads, written from bmo-web's language pack ({LANGUAGE}).\n{text}"
  ))
}
