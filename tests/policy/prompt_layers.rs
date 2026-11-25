//! The prompt layers' policy (docs/prompt-layers.md): the platform's rulebooks each turn shape carries stand in
//! the platform's folder, and the two settings agency declares read as the account service takes a kind.

use std::path::PathBuf;

use policy::chat_line::CHAT_LINE;
use policy::prompt_layers::PROMPT_LAYERS;
use policy::supervisors::SUPERVISORS;

#[test]
fn every_platform_rulebook_a_shape_names_stands_in_the_platform_s_folder() {
  let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("prompts")
    .join(&PROMPT_LAYERS.platform_folder);

  assert_eq!(CHAT_LINE.system.platform, ["environment", "preservation", "operations"]);
  assert_eq!(SUPERVISORS.pieces_platform, ["environment", "preservation"]);

  for name in CHAT_LINE.system.platform.iter().chain(&SUPERVISORS.pieces_platform) {
    assert!(folder.join(format!("{name}.md")).is_file(), "{name}");
  }
}

#[test]
fn the_kinds_read_as_the_account_service_takes_them() {
  let organisation = &PROMPT_LAYERS.organisation.kind;
  let persona = &PROMPT_LAYERS.persona.kind;

  assert_eq!((organisation.shape.as_str(), organisation.mode.as_str()), ("text", "layered"));
  assert_eq!(organisation.levels, ["platform", "unit"]);
  assert_eq!((persona.shape.as_str(), persona.mode.as_str()), ("keys", "nearest"));
  assert_eq!(
    persona.choices,
    [PROMPT_LAYERS.persona.own.as_str(), PROMPT_LAYERS.persona.none.as_str()]
  );
  assert!(PROMPT_LAYERS.ask_seconds > 0);
}
