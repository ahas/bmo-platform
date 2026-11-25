use std::path::{Path, PathBuf};

use agency_server::lines::*;
use harness_machine::supervisor_prompt::{self, Composition, Layers, Roots, Supervisor};

#[test]
fn a_mark_is_filled_where_it_is_named_and_left_where_it_is_not() {
  assert_eq!(render("a {{tools}} b {{other}}", &[("tools", "x")]), "a x b {{other}}");
}

// The trees over a code tree, the rest under a scratch directory.
fn trees_over(code: &Path, scratch: &Path) -> Trees {
  Trees {
    code: code.to_path_buf(),
    agents: scratch.join("agents"),
    source: scratch.to_path_buf(),
    data: scratch.join("data"),
    home: scratch.to_path_buf(),
  }
}

// This checkout, which stands as the deploy tree a line is composed from.
fn checkout() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// The platform's harness opening is the code tree's own file, and a whole prompt for Claude Code opens on it
// with the line's own text after it (docs/base-prompts.md, 「The harness layer」).
#[test]
fn the_harness_opening_is_read_out_of_the_code_tree_and_opens_a_whole_prompt() {
  let scratch = tempfile::tempdir().unwrap();
  let trees = trees_over(&checkout(), scratch.path());
  let harness = std::fs::read_to_string(checkout().join("prompts/harness.md")).unwrap();

  assert_eq!(trees.harness_path(), checkout().join("prompts").join("harness.md"));
  assert_eq!(
    harnessed(&trees, "the line's own").unwrap(),
    format!("{}\n\nthe line's own", harness.trim())
  );
}

// A tree that holds no harness opening composes no prompt for Claude Code rather than one without it.
#[test]
fn a_tree_without_the_harness_opening_composes_no_whole_prompt() {
  let scratch = tempfile::tempdir().unwrap();
  let trees = trees_over(scratch.path(), scratch.path());

  assert!(harnessed(&trees, "the line's own").is_err());
}

// A supervisor composed out of the checkout's documents, with no base chosen, opens on the same file the chat
// and the worker lines open on.
#[test]
fn a_supervisor_composed_out_of_the_tree_opens_on_its_harness_opening() {
  let scratch = tempfile::tempdir().unwrap();
  let trees = trees_over(&checkout(), scratch.path());
  let roots = Roots {
    assets: checkout(),
    data: scratch.path().join("data"),
  };
  let supervisor = Supervisor {
    name: "tester-pal",
    label: None,
    call_names: &[],
    prompts: None,
  };
  let composed =
    supervisor_prompt::system(&roots, supervisor, &Composition::default(), None, true, &Layers::default())
      .unwrap();
  let harness = std::fs::read_to_string(trees.harness_path()).unwrap();

  assert!(composed.starts_with(&format!("{}\n\n", harness.trim())), "{composed}");
}
