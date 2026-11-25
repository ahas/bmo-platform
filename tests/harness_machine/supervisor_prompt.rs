//! A supervisor's system prompt: the role whole and the persona, the base prompt's values, and the persona read
//! out of the prompt folder its row names rather than one named after it.

use std::path::Path;

use harness_machine::supervisor_prompt::*;
use policy::base_prompts::BASE_PROMPTS;
use policy::prompt_layers::PROMPT_LAYERS;
use policy::supervisors::SUPERVISORS;

// The supervisor every test composes for: its name, and the prompt folder its row names, which differs
// from the name so a prompt read by the name would show.
const PAL: Supervisor = Supervisor {
  name: "tester-pal",
  label: None,
  call_names: &[],
  prompts: Some("pal-prompts"),
};

// A tree of agent documents, each naming what it is, and the roots over it.
fn roots(dir: &Path) -> Roots {
  let write = |path: &str, text: &str| {
    let at = dir.join("assets").join(path);

    std::fs::create_dir_all(at.parent().unwrap()).unwrap();
    std::fs::write(at, format!("{text}\n")).unwrap();
  };

  write("prompts/harness.md", "the platform's harness");
  write("prompts/rules/self-preservation.md", "keep the system");
  write("prompts/platform/environment.md", "the platform's environment");
  write("prompts/platform/preservation.md", "the platform's parts");
  write("prompts/rules/absolute-rules.md", "ABSOLUTE RULES");
  write("prompts/rules/agent-words.md", "agent words");
  write(
    "prompts/agents/pieces/frame.md",
    "frame of {{label}}, called {{call_names}}\n{{tools}}\n{{unknown}}",
  );
  write("prompts/agents/pieces/turn.md", "turn");
  write("prompts/agents/pieces/tools.md", "tools at {{workplace}}");
  write("prompts/agents/pal-prompts/persona.md", "persona file");
  write("prompts/agents/tester-pal/persona.md", "the name's persona, never read");

  Roots {
    assets: dir.join("assets"),
    data: dir.join("data"),
  }
}

#[test]
fn a_supervisor_whose_row_names_no_prompt_folder_is_sent_no_persona() {
  let dir = tempfile::tempdir().unwrap();
  let bare = Supervisor {
    prompts: None,
    ..PAL
  };
  let composed =
    system(&roots(dir.path()), bare, &Composition::default(), None, true, &Layers::default()).unwrap();

  assert!(composed.ends_with("RULEBOOK prompts/rules/agent-words.md:\nagent words"), "{composed}");
}

#[test]
fn a_prompt_folder_holding_no_persona_sends_none() {
  let dir = tempfile::tempdir().unwrap();
  let roots = roots(dir.path());

  assert_eq!(roots.persona(Some("empty-folder"), None).unwrap(), "");
  assert_eq!(roots.persona(Some("pal-prompts"), Some("held")).unwrap(), "held");
}

#[test]
fn a_prompt_folder_that_is_not_one_plain_name_is_refused() {
  let dir = tempfile::tempdir().unwrap();
  let roots = roots(dir.path());

  for folder in ["", "..", "../rules", "pal-prompts/inner", "/etc"] {
    assert!(roots.prompt_folder(folder).is_err(), "{folder:?}");
    assert!(roots.persona(Some(folder), None).is_err(), "{folder:?}");
  }

  assert_eq!(
    roots.prompt_folder("pal-prompts").unwrap(),
    dir.path().join("assets/prompts/agents/pal-prompts")
  );
}

#[test]
fn every_policy_piece_has_its_own_text() {
  let dir = tempfile::tempdir().unwrap();
  let roots = roots(dir.path());

  for name in SUPERVISORS.pieces_role.iter().map(String::as_str).chain(["persona"]) {
    assert!(
      piece(&roots, PAL, name, (&Composition::default(), &Layers::default())).is_ok(),
      "{name}"
    );
  }
}

// On a program that is a harness of its own and with no base chosen, the prompt is the role whole and the
// persona, with no opening of the platform's.
#[test]
fn the_prompt_is_the_rulebook_the_role_whole_then_the_persona() {
  let dir = tempfile::tempdir().unwrap();
  let composed =
    system(&roots(dir.path()), PAL, &Composition::default(), None, false, &Layers::default()).unwrap();

  assert_eq!(
    composed,
    "RULEBOOK prompts/rules/self-preservation.md:\nkeep the system\n\nABSOLUTE RULES\n\nframe of tester-pal, called tester-pal\ntools at the folder your shell starts in\n{{unknown}}\n\nturn\n\nRULEBOOK prompts/rules/agent-words.md:\nagent words\n\npersona file"
  );
}

// The frame tells a supervisor the label it stands as and the call names it is called by
// (docs/supervisors.md, 「What a turn is given」); one holding neither is told its name for both, as the exact
// prompt above shows.
#[test]
fn the_frame_names_the_label_and_the_call_names() {
  let dir = tempfile::tempdir().unwrap();
  let names = ["Dear".to_owned(), "dear".to_owned()];
  let named = Supervisor {
    label: Some("Tester's Dear"),
    call_names: &names,
    ..PAL
  };
  let frame =
    piece(&roots(dir.path()), named, "frame", (&Composition::default(), &Layers::default())).unwrap();

  assert!(frame.starts_with("frame of Tester's Dear, called Dear, dear\n"), "{frame}");
}

// On Claude Code a supervisor that chooses no base opens on the platform's harness opening, read out of the
// asset root, before the role and the persona (docs/supervisors.md, 「What a turn is given」).
#[test]
fn a_supervisor_with_no_base_opens_on_the_platform_s_harness() {
  let dir = tempfile::tempdir().unwrap();
  let roots = roots(dir.path());
  let opened = system(&roots, PAL, &Composition::default(), None, true, &Layers::default()).unwrap();
  let bare = system(&roots, PAL, &Composition::default(), None, false, &Layers::default()).unwrap();

  assert_eq!(roots.harness().unwrap(), "the platform's harness");
  assert_eq!(opened, format!("the platform's harness\n\n{bare}"));
}

#[test]
fn the_base_prompt_is_the_harness_and_the_settings_persona_closes_the_prompt() {
  let dir = tempfile::tempdir().unwrap();
  let roots = roots(dir.path());
  let settings = Composition {
    persona: Some("short".into()),
  };
  let base = Base {
    text: "\nRun in {{cwd}} on {{model}} for {{nobody}}.\n".into(),
    model: "model-0-1".into(),
  };
  let home = dir.path().join("data/supervisor-homes/tester-pal").display().to_string();

  // The chosen base stands in the platform's opening's place, so the opening is not sent beside it.
  assert_eq!(
    system(&roots, PAL, &settings, Some(&base), true, &Layers::default()).unwrap(),
    format!(
      "Run in {home} on model-0-1 for {{{{nobody}}}}.\n\nRULEBOOK prompts/rules/self-preservation.md:\nkeep the system\n\nABSOLUTE RULES\n\nframe of tester-pal, called tester-pal\ntools at the folder your shell starts in\n{{{{unknown}}}}\n\nturn\n\nRULEBOOK prompts/rules/agent-words.md:\nagent words\n\nshort"
    )
  );
}

#[test]
fn every_base_prompt_value_has_a_reading_on_this_side() {
  for name in &BASE_PROMPTS.values {
    assert!(base_value(name, Path::new("/w"), "m").is_some(), "{name}");
  }

  assert!(machine::local_date().len() == 10 && machine::os_version().contains(' '));
}

// The layers between the harness and the role stand in their order (docs/prompt-layers.md, 「The order」): the
// platform's rules where the layers attach them, then the organisation's instructions, then the role.
#[test]
fn the_platform_s_rules_and_the_organisation_s_instructions_stand_between_the_harness_and_the_role() {
  let dir = tempfile::tempdir().unwrap();
  let roots = roots(dir.path());
  let layers = Layers {
    platform: true,
    organisation: organisation(&["Never reach the company's ledger.".to_owned()]),
    ..Layers::default()
  };
  let composed = system(&roots, PAL, &Composition::default(), None, true, &layers).unwrap();
  let bare = system(&roots, PAL, &Composition::default(), None, true, &Layers::default()).unwrap();
  let platform_at = composed
    .find("RULEBOOK prompts/platform/preservation.md:\nthe platform's parts")
    .unwrap();
  let organisation_at = composed.find("Never reach the company's ledger.").unwrap();
  let role_at = composed.find("RULEBOOK prompts/rules/self-preservation.md").unwrap();

  assert!(composed.starts_with("the platform's harness\n\n"), "{composed}");
  assert!(platform_at < organisation_at && organisation_at < role_at, "{composed}");
  assert!(!bare.contains("prompts/platform/"), "{bare}");
  assert!(!bare.contains(&PROMPT_LAYERS.organisation.header), "{bare}");
}

// Every link's text, the outermost first, under the header and the end line; none where no link says any.
#[test]
fn the_organisation_s_instructions_keep_every_link_s_text_outermost_first() {
  let words = &PROMPT_LAYERS.organisation;

  assert_eq!(organisation(&[]), None);
  assert_eq!(organisation(&["  ".to_owned()]), None);
  assert_eq!(
    organisation(&["platform's".to_owned(), String::new(), "company's".to_owned()]).unwrap(),
    format!("{}\n\nplatform's\n\ncompany's\n\n{}", words.header, words.end)
  );
}

// A persona the chain chose none for leaves no persona text: the role ends the prompt.
#[test]
fn a_persona_chosen_none_leaves_no_persona_text() {
  let dir = tempfile::tempdir().unwrap();
  let roots = roots(dir.path());
  let settings = Composition {
    persona: Some("a cheerful pirate".into()),
  };
  let none = Layers {
    persona_none: true,
    ..Layers::default()
  };
  let kept = system(&roots, PAL, &settings, None, false, &Layers::default()).unwrap();
  let left = system(&roots, PAL, &settings, None, false, &none).unwrap();

  assert!(kept.ends_with("a cheerful pirate"), "{kept}");
  assert!(!left.contains("pirate") && !left.contains("persona file"), "{left}");
  assert!(left.ends_with("RULEBOOK prompts/rules/agent-words.md:\nagent words"), "{left}");
}

// The folder a supervisor stands in is named by its path, which stands under the platform's data root, to a
// supervisor the platform's rules reach alone; any other is told it in words (docs/prompt-layers.md).
#[test]
fn only_a_platform_supervisor_is_told_its_folder_s_path() {
  let dir = tempfile::tempdir().unwrap();
  let roots = roots(dir.path());
  let home = dir.path().join("data/supervisor-homes/tester-pal").display().to_string();
  let platform = Layers {
    platform: true,
    ..Layers::default()
  };
  let named = piece(&roots, PAL, "frame", (&Composition::default(), &platform)).unwrap();
  let unnamed = piece(&roots, PAL, "frame", (&Composition::default(), &Layers::default())).unwrap();

  assert!(named.contains(&format!("tools at {home}")), "{named}");
  assert!(!unnamed.contains(&home), "{unnamed}");
  assert!(unnamed.contains(&format!("tools at {}", PROMPT_LAYERS.workplace_unnamed)), "{unnamed}");
}

// The person's instructions stand after the role and before the persona (docs/prompt-layers.md, 「The person's
// instructions」): the text for every agent first, then the agent's own; another agent's own reaches it not.
#[test]
fn the_person_s_instructions_stand_after_the_role_and_before_the_persona() {
  let dir = tempfile::tempdir().unwrap();
  let roots = roots(dir.path());
  let settings = Composition {
    persona: Some("a cheerful pirate".into()),
  };
  let layers = Layers {
    personal: Personal {
      all: Some("Answer in short lines.".into()),
      agents: [
        (PAL.name.to_owned(), "Call me by my first name.".to_owned()),
        ("someone-else".to_owned(), "Speak only of the weather.".to_owned()),
      ]
      .into_iter()
      .collect(),
    },
    ..Layers::default()
  };
  let composed = system(&roots, PAL, &settings, None, false, &layers).unwrap();
  let role_at = composed.find("RULEBOOK prompts/rules/agent-words.md").unwrap();
  let header_at = composed.find(&PROMPT_LAYERS.personal.header).unwrap();
  let all_at = composed.find("Answer in short lines.").unwrap();
  let own_at = composed.find("Call me by my first name.").unwrap();
  let persona_at = composed.find("a cheerful pirate").unwrap();

  assert!(
    role_at < header_at && header_at < all_at && all_at < own_at && own_at < persona_at,
    "{composed}"
  );
  assert!(!composed.contains("weather"), "{composed}");
  assert_eq!(Personal::default().of(PAL.name), None);
  assert!(
    !system(&roots, PAL, &settings, None, false, &Layers::default())
      .unwrap()
      .contains(&PROMPT_LAYERS.personal.header)
  );
}
