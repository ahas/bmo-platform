//! The ceilings on an answer's tokens a tier mapping may name (docs/model-tiers.md).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerCeilings {
  pub choices: Vec<i64>,
}

impl AnswerCeilings {
  pub fn holds(&self, ceiling: i64) -> bool {
    self.choices.contains(&ceiling)
  }
}

crate::part!(pub ANSWER_CEILINGS: AnswerCeilings = "answer-ceilings.toml");
