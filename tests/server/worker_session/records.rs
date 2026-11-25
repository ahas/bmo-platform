//! A downgrade read off what a run spent, and the model a kept session is marked with.

use agency_server::door::record::ModelSpend;
use agency_server::worker_session::records::{downgrades, fallen_model};

fn spent(model: &str, tokens: i64) -> ModelSpend {
  ModelSpend {
    mode: "claude-code".into(),
    model: model.into(),
    tokens,
    input_tokens: 0,
    output_tokens: 0,
    cache_read_tokens: 0,
    cache_write_tokens: 0,
    cost: 0.0,
  }
}

#[test]
fn a_downgrade_is_a_sibling_of_the_launched_family_that_spent_something() {
  let spend = [
    spent("claude-opus-5", 10),
    spent("claude-opus-4-7", 5),
    spent("claude-haiku-5", 3),
    spent("claude-opus-4", 0),
  ];

  assert_eq!(downgrades(&spend, "claude-opus-5"), vec!["claude-opus-4-7".to_owned()]);
  assert!(downgrades(&spend, "opus").is_empty());
  assert_eq!(fallen_model("claude-opus-5", "claude-opus-5", &[]), None);
  assert_eq!(
    fallen_model("claude-opus-5", "claude-opus-4-7", &[]).as_deref(),
    Some("claude-opus-4-7")
  );
}
