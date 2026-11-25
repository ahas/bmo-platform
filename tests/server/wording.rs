//! The banned words: the document's lists read out of its own sections, and the terms a text uses outside
//! its quoted spans, each once.

use agency_server::wording::{
  banned_terms_in, banned_terms_line, line_language, parse_glossary, parse_wording,
};

const DOCUMENT: &str = "# Wording\n\n## Banned terms\n\n- en · leverage → use — in prose\n- en · utilize → use\n\n## Uses in the word's own sense\n\n- en · leverage · a lever's leverage — physics\n\n## Allowed uses\n\n- en · utilize · docs/tools.md · utilize the cache — the tool's own name\n";

#[test]
fn the_document_s_lists_are_read_out_of_their_sections() {
  let glossary = parse_wording(DOCUMENT).unwrap();

  assert_eq!(glossary.banned.len(), 2);
  assert_eq!(glossary.banned[0].term, "leverage");
  assert_eq!(glossary.banned[0].instead, "use");
  assert_eq!(glossary.banned[0].when.as_deref(), Some("in prose"));
  assert_eq!(glossary.banned[0].keeps, ["a lever's leverage"]);
  assert_eq!(glossary.allowed.len(), 1);
  assert_eq!(glossary.allowed[0].path, "docs/tools.md");
}

#[test]
fn a_line_in_a_language_no_line_is_told_in_is_a_fault() {
  assert!(parse_glossary("## Banned terms\n- xx · word → other\n").is_err());
  assert!(parse_glossary("## Banned terms\n- en · word → other\n## Uses in the word's own sense\n- en · word · no such phrase — why\n").is_err());
}

#[test]
fn a_term_is_found_outside_quoted_spans_and_outside_its_own_sense() {
  let glossary = parse_glossary(DOCUMENT).unwrap();
  let found =
    |text: &str| banned_terms_in(text, &glossary).into_iter().map(|term| term.term).collect::<Vec<_>>();

  assert_eq!(found("We leverage the cache and utilize it."), ["leverage", "utilize"]);
  assert_eq!(found("Write `leverage` as \"use\"."), Vec::<String>::new());
  assert_eq!(found("It is a lever's leverage."), Vec::<String>::new());
}

#[test]
fn the_terms_line_names_each_beside_what_to_write() {
  let glossary = parse_glossary(DOCUMENT).unwrap();

  assert_eq!(banned_terms_line(&glossary), "\"leverage\" → use; \"utilize\" → use");
}

#[test]
fn a_line_s_language_is_told_by_its_script() {
  assert_eq!(line_language("plain words"), "en");
  assert_eq!(line_language("\u{d55c}\u{ae00}"), "ko");
  assert_eq!(line_language("\u{3072}\u{3089}\u{304c}\u{306a}"), "ja");
}
