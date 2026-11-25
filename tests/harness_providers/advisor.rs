//! Whether a road may name an advisor (docs/provider-order.md, 「The advisor」): each provider's own answer, Claude
//! Code's read against the catalogue of a login's directory, its family words through the catalogue's aliases.

use std::path::Path;

use harness_providers::claude_code::{ClaudeCode, catalogue};
use harness_providers::deepseek::DeepSeek;
use harness_providers::{Machine, Provider};
use serde_json::json;

// A configuration directory of the test's own, holding a catalogue of these models with Opus's family word
// standing for Opus 5.5.
fn catalogued(dir: &Path, models: &[&str]) {
  let place = dir.join("cache/model-catalog");
  let rows: Vec<_> = models.iter().map(|id| json!({ "id": id, "section": "main" })).collect();
  let file = json!({
    "version": 2, "fetchedAt": 1000, "staleAt": 2000,
    "catalog": { "surface": "cc", "config": {
      "id": "cc", "models": rows,
      "provider_alias_targets": { "opus": { "default": "claude-opus-5-5" }, "sonnet": { "default": "claude-sonnet-5-5" } },
    } },
  });

  std::fs::create_dir_all(&place).unwrap();
  std::fs::write(place.join("a-cc.json"), file.to_string()).unwrap();
}

async fn refusal(dir: Option<&Path>, model: &str, advisor: &str) -> Option<String> {
  let machine = Machine::with_env([], Path::new("/nonexistent"));

  ClaudeCode.advisor_refusal(&machine, dir, model, advisor).await
}

/// the catalogue's aliases are read with the models
#[test]
fn the_catalogues_aliases_are_read_with_the_models() {
  let dir = tempfile::tempdir().unwrap();

  catalogued(dir.path(), &["claude-opus-5-5"]);

  assert_eq!(catalogue(dir.path()).unwrap().aliases["opus"], "claude-opus-5-5");
}

/// a family word is judged as the model the catalogue reads it as
#[tokio::test]
async fn a_family_word_is_judged_as_the_model_the_catalogue_reads_it_as() {
  let dir = tempfile::tempdir().unwrap();

  catalogued(dir.path(), &["claude-sonnet-5", "claude-opus-5-5", "claude-sonnet-5-5"]);

  assert_eq!(refusal(Some(dir.path()), "claude-sonnet-5", "opus").await, None);

  let below = refusal(Some(dir.path()), "claude-opus-5-5", "sonnet").await.unwrap();

  assert!(below.contains("sonnet (claude-sonnet-5-5)"), "{below}");
}

/// a model the catalogue does not hold is refused, and one it holds is judged by its pairing
#[tokio::test]
async fn a_model_the_catalogue_does_not_hold_is_refused() {
  let dir = tempfile::tempdir().unwrap();

  catalogued(dir.path(), &["claude-sonnet-5", "claude-opus-5-5"]);

  assert!(
    refusal(Some(dir.path()), "claude-sonnet-5", "claude-opus-9")
      .await
      .unwrap()
      .contains("holds no model")
  );
  assert_eq!(refusal(Some(dir.path()), "claude-sonnet-5", "claude-opus-5-5").await, None);
}

/// a model no pairing names is refused any advisor
#[tokio::test]
async fn a_model_no_pairing_names_is_refused_any_advisor() {
  assert!(refusal(None, "claude-opus-4-1", "opus").await.unwrap().contains("pairs no advisor"));
}

/// a catalogue that cannot be read keeps a family word, and still judges a model by its own name
#[tokio::test]
async fn a_catalogue_that_cannot_be_read_keeps_a_family_word() {
  let dir = tempfile::tempdir().unwrap();

  assert_eq!(refusal(Some(dir.path()), "claude-opus-5-5", "sonnet").await, None);
  assert!(refusal(Some(dir.path()), "claude-opus-5-5", "claude-sonnet-5-5").await.is_some());
}

/// a provider whose program takes no advisor refuses every one and offers no family word
#[tokio::test]
async fn a_provider_whose_program_takes_no_advisor_refuses_every_one() {
  let machine = Machine::with_env([], Path::new("/nonexistent"));

  assert!(ClaudeCode.takes_advisor());
  assert!(!ClaudeCode.advisor_words().is_empty());
  assert!(!DeepSeek.takes_advisor());
  assert!(DeepSeek.advisor_words().is_empty());
  assert!(
    DeepSeek
      .advisor_refusal(&machine, None, "deepseek-flash", "opus")
      .await
      .unwrap()
      .contains("takes no advisor")
  );
}
