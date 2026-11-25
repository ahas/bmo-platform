//! Claude Code's declared advisor pairings (docs/provider-order.md, 「The advisor」): the longest start of a
//! model's name takes it, an advisor must rank at or above the model it advises, and the check made as a run goes
//! out keeps a family word for the catalogue to have judged.

use policy::providers::claude_code::CLAUDE_CODE;

/// the longest start naming a model takes it
#[test]
fn the_longest_start_naming_a_model_takes_it() {
  let advisor = &CLAUDE_CODE.advisor;

  assert!(advisor.pairs("claude-sonnet-5", "claude-haiku-5-5"), "Sonnet 5 takes Haiku 5.5");
  assert!(!advisor.pairs("claude-sonnet-5-5", "claude-haiku-5-5"), "Sonnet 5.5 is not Sonnet 5");
  assert!(advisor.pairs("claude-fable-5", "claude-fable-5-2"));
  assert!(!advisor.pairs("claude-fable-5-2", "claude-fable-5-1"));
}

/// an advisor ranks at or above the model it advises
#[test]
fn an_advisor_ranks_at_or_above_the_model_it_advises() {
  let advisor = &CLAUDE_CODE.advisor;

  assert!(advisor.pairs("claude-sonnet-5", "claude-opus-5-5"));
  assert!(advisor.pairs("claude-opus-5-5", "claude-opus-5-5"));
  assert!(advisor.pairs("claude-opus-5-5", "claude-fable-5-2"));
  assert!(!advisor.pairs("claude-opus-5-5", "claude-sonnet-5-5"), "a Sonnet ranks below Opus 5.5");
  assert!(!advisor.pairs("claude-fable-5-2", "claude-opus-5-5"), "nothing but Fable advises Fable");
  assert!(
    !advisor.pairs("claude-haiku-4-5-20251001", "claude-haiku-4-5-20251001"),
    "Haiku 4.5 advises nobody"
  );
}

/// a model no start names is paired with nothing, a gateway member's name among them
#[test]
fn a_model_no_start_names_is_paired_with_nothing() {
  let advisor = &CLAUDE_CODE.advisor;

  assert!(advisor.pair_of("deepseek-flash").is_none());
  assert!(advisor.pair_of("claude-opus-4-1").is_none());
  assert!(
    !advisor.attaches("flash-next", "opus"),
    "a family word does not reach a model no pairing names"
  );
}

/// the check as a run goes out keeps a family word and drops a model the pairing does not name
#[test]
fn the_check_as_a_run_goes_out_keeps_a_family_word_and_drops_an_unpaired_model() {
  let advisor = &CLAUDE_CODE.advisor;

  assert!(advisor.attaches("claude-sonnet-5", "Opus"), "family words are read without case");
  assert!(advisor.attaches("claude-sonnet-5", "claude-opus-5-5"));
  assert!(!advisor.attaches("claude-opus-5-5", "claude-sonnet-5"));
}
