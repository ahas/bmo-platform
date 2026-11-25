use policy::providers::*;

#[test]
fn every_provider_reads_and_names_the_lines_its_program_carries() {
  for declared in [
    &claude_code::CLAUDE_CODE.declared,
    &codex::CODEX.declared,
    &openrouter::OPENROUTER.declared,
    &registered::REGISTERED.declared,
  ] {
    assert!(!declared.label.is_empty());
    assert!(declared.carries("chat"), "{} carries the chat", declared.label);
    assert!(
      declared.effort.launch == Launch::Either || declared.effort.fault.is_some(),
      "{} says why",
      declared.label
    );
  }
}

#[test]
fn a_launch_rule_holds_what_each_provider_declares() {
  assert_eq!(codex::CODEX.declared.effort.fault(None), None);
  assert_eq!(codex::CODEX.declared.effort.fault(Some("max")), None);
  assert_eq!(openrouter::OPENROUTER.declared.effort.fault(Some("low")), None);
  assert_eq!(openrouter::OPENROUTER.declared.effort.fault(None), None);
  assert_eq!(registered::REGISTERED.declared.effort.fault(Some("thinking")), None);
  assert_eq!(
    claude_code::CLAUDE_CODE.declared.effort.fault(None),
    Some("This harness requires explicit effort")
  );
  assert_eq!(claude_code::CLAUDE_CODE.declared.effort.vocabulary().first(), Some(&None));
}

#[test]
fn deepseek_carries_the_search_line_and_claude_code_alone_the_browse_line() {
  assert!(deepseek::DEEPSEEK.declared.carries("search"));
  assert!(!deepseek::DEEPSEEK.declared.carries("browse"));
  assert!(claude_code::CLAUDE_CODE.declared.carries("browse"));
}

#[test]
fn the_declared_media_rules_read_the_models_the_road_carries() {
  // (docs/provider-order.md, 「The media a model reads」.)
  assert_eq!(
    claude_code::CLAUDE_CODE.declared.media_of("Claude-Sonnet-5"),
    [Media::Image, Media::Pdf],
    "the name is read without regard to case"
  );
  assert_eq!(codex::CODEX.declared.media_of("gpt-5.1-codex-max"), [Media::Image]);
  assert_eq!(openrouter::OPENROUTER.declared.media_of("anthropic/claude-opus-4.5"), [Media::Image]);
  assert!(openrouter::OPENROUTER.declared.media_of("x-ai/grok-4").is_empty());
  assert!(deepseek::DEEPSEEK.declared.media_of("deepseek-chat").is_empty());
  // The registered road judges the engine a registration's row names; the file itself holds no rule by
  // a registration's own alias.
  assert_eq!(registered::REGISTERED.declared.media_of("qwen3.8-flash-next-iq3_s"), [Media::Image]);
  assert_eq!(registered::REGISTERED.declared.media_of("claude-opus-4.5"), [Media::Image, Media::Pdf]);
  assert!(registered::REGISTERED.declared.media_of("HY-MT2-7B-Q6_K").is_empty());
}

#[test]
fn the_longest_model_start_wins_and_an_unnamed_model_reads_nothing() {
  let declared: Declared = serde_json::from_value(serde_json::json!({
    "label": "Test road",
    "lines": ["chat"],
    "caps_answer_tokens": false,
    "owners_engine": false,
    "effort": { "words": ["low"], "none_held": true, "launch": "either" },
    "reads": [
      { "model": "qwen", "media": ["image"] },
      { "model": "qwen3.8-flash", "media": ["image", "video"] }
    ]
  }))
  .expect("a declaration with media rules reads");

  assert_eq!(declared.media_of("qwen3.8-flash-next"), [Media::Image, Media::Video]);
  assert_eq!(declared.media_of("qwen-3"), [Media::Image]);
  assert!(declared.media_of("other").is_empty());
}
