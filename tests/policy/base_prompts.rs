use policy::base_prompts::*;
use regex::Regex;
use std::path::Path;

#[test]
fn the_rules_read_and_every_value_is_named_once() {
  assert!(BASE_PROMPTS.name_pattern.starts_with('^'));
  assert!(BASE_PROMPTS.values.contains(&"cwd".to_owned()));

  let mut names = BASE_PROMPTS.values.clone();

  names.sort();
  names.dedup();
  assert_eq!(names.len(), BASE_PROMPTS.values.len());
}

#[test]
fn every_seed_is_named_once_and_its_file_stands() {
  let mut names: Vec<&str> = BASE_PROMPTS.seed.iter().map(|seed| seed.name.as_str()).collect();

  names.sort_unstable();
  names.dedup();
  assert_eq!(names.len(), BASE_PROMPTS.seed.len());
  for seed in &BASE_PROMPTS.seed {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts/base").join(&seed.file);

    assert!(file.is_file(), "{} names {}", seed.name, file.display());
    assert!(
      seed.name.len() <= BASE_PROMPTS.name_most && seed.label.chars().count() <= BASE_PROMPTS.label_most
    );
  }
}

// A base prompt is for a harness whose own prompt is the opening its model runs on; a line on Claude Code that
// chooses none opens on the platform's harness opening, so no seed reproduces Claude Code's own prompt
// (docs/base-prompts.md, 「The seeds」).
#[test]
fn dsh_alone_is_seeded() {
  let names: Vec<&str> = BASE_PROMPTS.seed.iter().map(|seed| seed.name.as_str()).collect();

  assert_eq!(names, ["dsh"]);
  assert!(!Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts/base/claude-code.md").exists());
  assert!(Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts/harness.md").is_file());
}

#[test]
fn every_value_a_seed_names_is_one_the_policy_declares() {
  let value = Regex::new(r"\{\{([A-Za-z0-9_]+)\}\}").expect("the value mark");

  for seed in &BASE_PROMPTS.seed {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts/base").join(&seed.file);
    let text = std::fs::read_to_string(&file).expect("a seed's file");

    for found in value.captures_iter(&text) {
      let name = &found[1];

      assert!(
        BASE_PROMPTS.values.iter().any(|value| value == name),
        "{} names {{{{{name}}}}}",
        seed.name
      );
    }
  }
}
